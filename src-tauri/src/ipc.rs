//! The IPC surface: one thin adapter per command, and nothing else (plan section 3).
//!
//! Every handler here does three things, in this order:
//!
//!   1. decides which *service surface* the calling window may drive - [`library`] hands over the
//!      privileged one and refuses everybody else's window with the message naming that window,
//!      [`palette`] hands over the palette's, which is the whole of what a window other than the
//!      library may reach;
//!   2. calls the service, which returns a value plus the [`Effects`] it wants the runtime to carry
//!      out; and
//!   3. dispatches those effects ([`crate::effects`]): events to the windows named by their variant,
//!      window actions, and one tray refresh.
//!
//! There is deliberately no business rule in this file. What a call means - the revision check that
//! makes a stale pin fail, the vault authorisation a backup goes through, the order the reload event
//! and the vault event go out in - lives in `dev-console_core::services`, so the same answers hold
//! for the native (Rune) shell that replaces this file, and so there is one implementation of each
//! rule to audit rather than one per UI. The `#[cfg(test)]` suites in `src-tauri/tests` drive these
//! commands through Tauri's own invoke path, so what is pinned here is the wire contract: the
//! command names, the argument names, the response shapes and the error strings the webview reads.

use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

use crate::error::{AppError, AppResult};
use crate::models::{
    AppStateInfo, BackupOutcome, CopyRequest, CopyResult, DeleteRequest, EntryDetails, EntryMeta,
    GetRequest, ImportProjectRequest, KindRequest, ListRequest, OpenLibraryRequest, PinRequest,
    RunRequest, SaveRequest, SavedEntry, ScanProjectRequest, SearchRequest, SearchResponse,
    Settings, VaultState,
};
use crate::services::{Effects, FinderActions, LibraryActions, Surface};
use crate::state::AppState;
use crate::{autostart, effects};
use crate::{execution, history, projects};

fn state<R: Runtime>(app: &AppHandle<R>) -> Arc<AppState> {
    app.state::<Arc<AppState>>().inner().clone()
}

fn version<R: Runtime>(app: &AppHandle<R>) -> String {
    app.package_info().version.to_string()
}

/// Which window is calling, as the services name it. The label is the only identity a webview has
/// here, and `Surface::from_label` is what turns it into "library", "finder" or "somebody else".
fn surface<R: Runtime>(window: &WebviewWindow<R>) -> Surface {
    Surface::from_label(window.label())
}

/// The privileged surface, or the refusal naming the window that tried.
///
/// Every command that can change data, decrypt, execute or export goes through this, so the guard is
/// one function instead of one `if` per handler - and a handler that forgets it is a visible bug in
/// the window-permission test rather than a plausible-looking omission.
fn library<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) -> AppResult<LibraryActions> {
    surface(window).require_library()?;
    Ok(LibraryActions::new(state(app), &version(app)))
}

/// The palette surface, which is also what answers the commands *both* windows may call: the shared
/// read model, the copy path, the vault lock, and a window's own hide or quit. Building the palette's
/// actions rather than the library's for those is what keeps "the palette cannot reach a privileged
/// operation" true by construction: no handler for a shared command holds a privileged value at all.
fn palette<R: Runtime>(app: &AppHandle<R>) -> FinderActions {
    FinderActions::new(state(app), &version(app))
}

/// Runs a call that produces effects, then applies them whether or not the call succeeded: a refused
/// unlock has still locked the vault away from the windows and has to say so.
fn with_effects<T, R: Runtime>(
    app: &AppHandle<R>,
    call: impl FnOnce(&mut Effects) -> AppResult<T>,
) -> AppResult<T> {
    let mut effects = Effects::default();
    let value = call(&mut effects);
    effects::finish(value, effects::dispatch(app, effects))
}

// ---------------------------------------------------------------------------------------------
// The read model both windows may use
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn app_state<R: Runtime>(app: AppHandle<R>) -> AppStateInfo {
    palette(&app).app_state()
}

#[tauri::command]
pub fn search_entries<R: Runtime>(app: AppHandle<R>, request: SearchRequest) -> SearchResponse {
    palette(&app).search_entries(request)
}

/// `Ctrl+P`, or clicking the chip: forget the project the palette was ranked against, including a
/// detection that has not answered yet.
#[tauri::command]
pub fn clear_finder_context<R: Runtime>(app: AppHandle<R>) -> AppResult<()> {
    palette(&app).clear_finder_context()
}

/// Codex usage for the CLI Proxy tab (read-only, library surface only). The scan and the network
/// call run off the main thread; the token is read and dropped inside `cliproxy`.
#[tauri::command]
pub async fn cliproxy_usage<R: Runtime>(
    window: WebviewWindow<R>,
    since: Option<String>,
    until: Option<String>,
) -> AppResult<crate::cliproxy::CliproxyUsageResponse> {
    surface(&window).require_library()?;
    tauri::async_runtime::spawn_blocking(move || crate::cliproxy::cliproxy_usage(since, until))
        .await
        .map_err(|error| AppError::Other(format!("usage scan failed: {error}")))?
}

#[tauri::command]
pub fn get_theme<R: Runtime>(app: AppHandle<R>) -> String {
    palette(&app).get_theme()
}

/// Copying is allowed from either window; the value never crosses back into a window, so what the
/// palette may not do - read a secret - stays true.
#[tauri::command]
pub fn copy_entry<R: Runtime>(app: AppHandle<R>, request: CopyRequest) -> AppResult<CopyResult> {
    palette(&app).copy_entry(request)
}

// ---------------------------------------------------------------------------------------------
// Entries: library only
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn list_entries<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: ListRequest,
) -> AppResult<Vec<EntryMeta>> {
    library(&app, &window)?.list_entries(request)
}

/// The type-ahead for the filter row. `AppState::types` decides that a store it cannot read simply
/// contributes nothing, so the only thing this call can fail with is the window guard.
#[tauri::command]
pub fn types_for<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: KindRequest,
) -> AppResult<Vec<String>> {
    Ok(library(&app, &window)?.types_for(request))
}

#[tauri::command]
pub fn get_entry<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: GetRequest,
) -> AppResult<EntryDetails> {
    library(&app, &window)?.get_entry(request)
}

/// The one call that hands a decrypted value to a window, and only ever to the library.
#[tauri::command]
pub fn reveal_secret<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: GetRequest,
) -> AppResult<EntryDetails> {
    library(&app, &window)?.reveal_secret(request)
}

#[tauri::command]
pub fn save_entry<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: SaveRequest,
) -> AppResult<SavedEntry> {
    let actions = library(&app, &window)?;
    with_effects(&app, |effects| {
        actions.save_entry(request, effects).map(|saved| saved.0)
    })
}

#[tauri::command]
pub fn set_pinned<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: PinRequest,
) -> AppResult<SavedEntry> {
    let actions = library(&app, &window)?;
    with_effects(&app, |effects| actions.set_pinned(request, effects))
}

#[tauri::command]
pub fn delete_entry<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: DeleteRequest,
) -> AppResult<()> {
    let actions = library(&app, &window)?;
    with_effects(&app, |effects| actions.delete_entry(request, effects))
}

/// Copies the command output panel, capped at what execution was allowed to capture.
#[tauri::command]
pub fn copy_command_output<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    text: String,
) -> AppResult<CopyResult> {
    library(&app, &window)?.copy_command_output(text)
}

/// Runs a saved command off the async runtime's worker pool, as it has always been: a command that
/// needs a terminal spawns one and returns immediately, and anything else is waited on here with the
/// timeout and the output cap the core applies.
#[tauri::command]
pub async fn run_command<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: RunRequest,
) -> AppResult<execution::RunResult> {
    let actions = library(&app, &window)?;
    tauri::async_runtime::spawn_blocking(move || actions.run_command(request))
        .await
        .map_err(|error| AppError::Execution(format!("command task failed: {error}")))?
}

/// An http(s) link from a URL field, and nothing else. The rule lives with the services so the
/// native shell refuses the same set of links.
#[tauri::command]
pub fn open_url<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    url: String,
) -> AppResult<()> {
    library(&app, &window)?.open_url(url)
}

// ---------------------------------------------------------------------------------------------
// The vault, which both windows may drive
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn lock_vault<R: Runtime>(app: AppHandle<R>) -> AppResult<VaultState> {
    let actions = palette(&app);
    with_effects(&app, |effects| actions.lock_vault(effects))
}

#[tauri::command]
pub fn unlock_vault<R: Runtime>(app: AppHandle<R>) -> AppResult<VaultState> {
    let actions = palette(&app);
    with_effects(&app, |effects| actions.unlock_vault(effects))
}

// ---------------------------------------------------------------------------------------------
// Windows: handoffs, hiding, and the quit
// ---------------------------------------------------------------------------------------------

/// The palette's `Enter`: hand the entry to the library window. Refused from the library itself, and
/// the refusal names the window that asked, which is how the frontend reports it.
#[tauri::command]
pub fn open_library<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: OpenLibraryRequest,
) -> AppResult<()> {
    let command = palette(&app).open_library(surface(&window), request.kind, request.entry_id)?;
    effects::dispatch(&app, effects::window_command(command))
}

/// `Escape`. Each window may hide itself; nothing else about the resident app changes.
#[tauri::command]
pub fn hide_self<R: Runtime>(app: AppHandle<R>, window: WebviewWindow<R>) -> AppResult<()> {
    let command = palette(&app).hide_self(surface(&window));
    effects::dispatch(&app, effects::window_command(command))
}

/// Emitted when a window asks to close; the frontend resolves unsaved edits first and then calls
/// this, which is the only path that actually hides a window on close - or finishes a quit that the
/// close was the last answer to.
#[tauri::command]
pub fn allow_close<R: Runtime>(app: AppHandle<R>, window: WebviewWindow<R>) -> AppResult<()> {
    let command = palette(&app).allow_close(surface(&window));
    effects::dispatch(&app, effects::window_command(command))
}

/// The user chose to stay: the quit's countdown is cancelled. Library only, because it is the
/// library's editor that gets asked to answer for a quit.
#[tauri::command]
pub fn cancel_quit<R: Runtime>(app: AppHandle<R>, window: WebviewWindow<R>) -> AppResult<()> {
    let command = library(&app, &window)?.cancel_quit();
    effects::dispatch(&app, effects::window_command(command))
}

/// The keyboard-less way to leave the resident app. Either window may ask; the library still gets
/// its chance to answer for unsaved edits first.
#[tauri::command]
pub fn quit_app<R: Runtime>(app: AppHandle<R>) -> AppResult<()> {
    let command = palette(&app).quit_app();
    effects::dispatch(&app, effects::window_command(command))
}

/// Relaunch into a visible library, which is how an installed update is entered.
#[tauri::command]
pub fn restart_app<R: Runtime>(app: AppHandle<R>, window: WebviewWindow<R>) -> AppResult<()> {
    let command = library(&app, &window)?.restart_app();
    effects::dispatch(&app, effects::window_command(command))
}

// ---------------------------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn get_settings<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<Settings> {
    // A plain value inside the `Result`, as it was: the store cannot fail by the time the app is
    // running, so the only error this call can carry is the window guard.
    Ok(library(&app, &window)?.get_settings())
}

/// Saving settings tells every window about the new palette and then asks the desktop to put the
/// autostart entry in place - in that order, because a window repainting from the theme event should
/// not be held up by a desktop-integration write, and a failed write is still a saved file.
#[tauri::command]
pub fn save_settings<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    settings: Settings,
) -> AppResult<Settings> {
    let actions = library(&app, &window)?;
    let saved = with_effects(&app, |effects| actions.save_settings(settings, effects))?;
    if let Err(error) = autostart::apply(saved.autostart) {
        return Err(AppError::Other(format!(
            "settings saved but autostart failed: {error}"
        )));
    }
    Ok(saved)
}

/// Opens the in-window settings panel (the tray's action, reachable from the library too).
#[tauri::command]
pub fn open_settings<R: Runtime>(app: AppHandle<R>, window: WebviewWindow<R>) -> AppResult<()> {
    let actions = library(&app, &window)?;
    with_effects(&app, |effects| {
        actions.open_settings(effects);
        Ok(())
    })
}

#[tauri::command]
pub fn list_terminals<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<Vec<execution::TerminalOption>> {
    Ok(library(&app, &window)?.list_terminals())
}

// ---------------------------------------------------------------------------------------------
// Shell history suggestions
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub async fn history_suggestions<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    limit: Option<usize>,
) -> AppResult<Vec<history::Suggestion>> {
    let actions = library(&app, &window)?;
    tauri::async_runtime::spawn_blocking(move || actions.history_suggestions(limit))
        .await
        .map_err(|error| AppError::Other(format!("history scan failed: {error}")))?
}

#[tauri::command]
pub fn dismiss_suggestion<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    key: String,
) -> AppResult<()> {
    library(&app, &window)?.dismiss_suggestion(key)
}

#[tauri::command]
pub fn clear_dismissed_suggestions<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<usize> {
    library(&app, &window)?.clear_dismissed_suggestions()
}

#[tauri::command]
pub fn dismissed_suggestion_count<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<usize> {
    library(&app, &window)?.dismissed_suggestion_count()
}

#[tauri::command]
pub fn install_history_tracking<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<String> {
    library(&app, &window)?.install_history_tracking()
}

// ---------------------------------------------------------------------------------------------
// Projects and environments
// ---------------------------------------------------------------------------------------------

/// Read a project folder without saving anything from it.
#[tauri::command]
pub fn scan_project<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: ScanProjectRequest,
) -> AppResult<projects::ProjectScan> {
    library(&app, &window)?.scan_project(request)
}

#[tauri::command]
pub fn import_project<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: ImportProjectRequest,
) -> AppResult<projects::ImportOutcome> {
    let actions = library(&app, &window)?;
    with_effects(&app, |effects| actions.import_project(request, effects))
}

#[tauri::command]
pub fn environment_projects<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<Vec<crate::environment::ProjectMeta>> {
    library(&app, &window)?.environment_projects()
}

#[tauri::command]
pub async fn sync_environment<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    path: String,
) -> AppResult<crate::environment::SyncResult> {
    let actions = library(&app, &window)?;
    tauri::async_runtime::spawn_blocking(move || actions.sync_environment(path))
        .await
        .map_err(|_| AppError::Other("Environment sync could not complete.".into()))?
}

#[tauri::command]
pub fn set_environment_sync<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    root: String,
    enabled: bool,
) -> AppResult<()> {
    library(&app, &window)?.set_environment_sync(root, enabled)
}

/// Copies a snapshot's content. It is a secret, so it goes to the clipboard and never into a window.
#[tauri::command]
pub fn copy_environment<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    id: String,
) -> AppResult<CopyResult> {
    library(&app, &window)?.copy_environment(id)
}

// ---------------------------------------------------------------------------------------------
// Backups, archives and the recovery credential
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn create_backup<R: Runtime>(app: AppHandle<R>, window: WebviewWindow<R>) -> AppResult<String> {
    library(&app, &window)?.create_backup()
}

#[tauri::command]
pub fn list_backups<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<Vec<crate::backup::BackupInfo>> {
    library(&app, &window)?.list_backups()
}

#[tauri::command]
pub fn restore_backup<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    name: String,
) -> AppResult<BackupOutcome> {
    let actions = library(&app, &window)?;
    with_effects(&app, |effects| actions.restore_backup(name, effects))
}

/// Review-before-apply for a generated backup: date and counts, no side effects.
#[tauri::command]
pub fn inspect_backup<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    name: String,
) -> AppResult<crate::backup::ArchiveSummary> {
    library(&app, &window)?.inspect_backup(name)
}

/// Review-before-apply for an exported archive file: date and counts, no side effects.
#[tauri::command]
pub fn inspect_archive<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    path: String,
) -> AppResult<crate::backup::ArchiveSummary> {
    library(&app, &window)?.inspect_archive(path)
}

/// The wire shape the settings panel reads for the recovery credential. It is built here rather than
/// exported by the core on purpose: the core's `RecoveryCredential` is a value that must not be
/// printed (see its `Debug`), and turning it into JSON is this layer's job.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKeyInfo {
    pub secret: String,
    pub path: String,
}

/// The recovery credential, so the owner can store it somewhere independent of this machine. Library
/// window **and** an unlocked vault - the reader the service uses enforces both.
#[tauri::command]
pub fn recovery_key<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<RecoveryKeyInfo> {
    let credential = library(&app, &window)?.recovery_key()?;
    Ok(RecoveryKeyInfo {
        secret: credential.secret,
        path: credential.path,
    })
}

/// Copies the recovery credential the way a secret is always copied: natively, with the configured
/// expiry, and never back into a window where it could be logged or screenshotted.
#[tauri::command]
pub fn copy_recovery_key<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<CopyResult> {
    library(&app, &window)?.copy_recovery_key()
}

#[tauri::command]
pub fn export_archive<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    path: String,
    overwrite: Option<bool>,
) -> AppResult<u64> {
    library(&app, &window)?.export_archive(path, overwrite)
}

#[tauri::command]
pub fn import_archive<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    path: String,
) -> AppResult<BackupOutcome> {
    let actions = library(&app, &window)?;
    with_effects(&app, |effects| actions.import_archive(path, effects))
}
