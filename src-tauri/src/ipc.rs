use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::repository::validate_draft;
use crate::state::AppState;
use crate::{execution, search, tray, windows};

/// Frontend windows are untrusted. Every privileged operation is re-checked here against the
/// calling window label, so the restriction does not depend on hidden buttons or on the
/// capability file alone.
fn require_library<R: Runtime>(window: &WebviewWindow<R>) -> AppResult<()> {
    match window.label() {
        windows::LIBRARY => Ok(()),
        other => Err(AppError::Forbidden(other.to_string())),
    }
}

fn state<R: Runtime>(app: &AppHandle<R>) -> Arc<AppState> {
    app.state::<Arc<AppState>>().inner().clone()
}

fn emit_entries_changed<R: Runtime>(
    app: &AppHandle<R>,
    payload: &SavedEntry,
    operation: &'static str,
) {
    let event = EntriesChanged {
        kind: payload.kind,
        id: payload.id.clone(),
        revision: payload.revision,
        operation,
        vault_generation: app.state::<Arc<AppState>>().vault.generation(),
    };
    let _ = app.emit("entries-changed", event);
}

fn emit_vault_state<R: Runtime>(app: &AppHandle<R>) {
    let vault = state(app).vault.state();
    let _ = app.emit("vault-state-changed", vault);
}

#[tauri::command]
pub fn app_state<R: Runtime>(app: AppHandle<R>, _window: WebviewWindow<R>) -> AppStateInfo {
    AppStateInfo {
        vault: state(&app).vault.state(),
        version: app.package_info().version.to_string(),
        authenticated_unlock_available: crate::auth::mechanism_available(),
        clipboard_exclusion: crate::clipboard::exclusion_status(),
        backup_dir: state(&app).backups.dir().to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub fn search_entries<R: Runtime>(
    app: AppHandle<R>,
    _window: WebviewWindow<R>,
    request: SearchRequest,
) -> SearchResponse {
    let state = state(&app);
    let entries = state.searchable_metadata(None);
    let results = search::rank(
        &entries,
        &request.query,
        request.limit.unwrap_or(search::DEFAULT_LIMIT),
    );
    SearchResponse {
        request_id: request.request_id,
        vault_generation: state.vault.generation(),
        results,
    }
}

#[tauri::command]
pub fn list_entries<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: ListRequest,
) -> AppResult<Vec<EntryMeta>> {
    require_library(&window)?;
    let state = state(&app);
    match request.kind {
        Kind::Command => {
            let mut entries = state
                .repo
                .list_commands(&request.query.clone().unwrap_or_default())?;
            if let Some(entry_type) = &request.entry_type {
                entries.retain(|e| e.entry_type.eq_ignore_ascii_case(entry_type));
            }
            if let Some(tag) = &request.tag {
                let tag = tag.to_lowercase();
                entries.retain(|e| e.tags.iter().any(|t| t.to_lowercase().starts_with(&tag)));
            }
            Ok(entries)
        }
        Kind::Secret => {
            // Metadata never leaves the vault while it is locked.
            let all = state.vault.list_secrets()?;
            let query = request.query.unwrap_or_default().to_lowercase();
            let entry_type = request.entry_type.unwrap_or_default().to_lowercase();
            let tag = request.tag.unwrap_or_default().to_lowercase();
            Ok(all
                .into_iter()
                .filter(|e| {
                    query.is_empty()
                        || e.name.to_lowercase().contains(&query)
                        || e.entry_type.to_lowercase().contains(&query)
                        || e.description.to_lowercase().contains(&query)
                        || e.tags.iter().any(|t| t.to_lowercase().contains(&query))
                })
                .filter(|e| {
                    entry_type.is_empty() || e.entry_type.to_lowercase().starts_with(&entry_type)
                })
                .filter(|e| {
                    tag.is_empty() || e.tags.iter().any(|t| t.to_lowercase().starts_with(&tag))
                })
                .collect())
        }
    }
}

#[tauri::command]
pub fn types_for<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: KindRequest,
) -> AppResult<Vec<String>> {
    require_library(&window)?;
    Ok(state(&app).types(request.kind))
}

fn details_from(record: &EntryRecord, approved: bool, include_value: bool) -> EntryDetails {
    EntryDetails {
        meta: record.meta(0, None),
        command: if record.kind == Kind::Command {
            Some(record.command.clone())
        } else {
            None
        },
        working_directory: record.working_directory.clone(),
        notes: record.notes.clone(),
        fields: record.fields.clone(),
        value: include_value.then(|| record.value.clone()),
        execution_approved: approved,
    }
}

#[tauri::command]
pub fn get_entry<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: GetRequest,
) -> AppResult<EntryDetails> {
    require_library(&window)?;
    let state = state(&app);
    match request.kind {
        Kind::Command => {
            let (record, usage, last) = state
                .repo
                .get_command(&request.id)?
                .ok_or(AppError::NotFound)?;
            let mut details = details_from(&record, state.repo.approval_state(&record)?, false);
            details.meta.usage_count = usage;
            details.meta.last_used_at = last;
            Ok(details)
        }
        Kind::Secret => {
            let record = state.vault.get(&request.id)?;
            let usage = state.vault.read(|file| {
                Ok(file
                    .usage
                    .iter()
                    .find(|u| u.id == record.id)
                    .map(|u| (u.count, u.last_used_at.clone()))
                    .unwrap_or((0, None)))
            })?;
            let mut details = details_from(&record, false, false);
            details.meta.usage_count = usage.0;
            details.meta.last_used_at = usage.1;
            Ok(details)
        }
    }
}

/// Explicit reveal. The value only ever leaves through this call or a native copy.
#[tauri::command]
pub fn reveal_secret<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: GetRequest,
) -> AppResult<EntryDetails> {
    require_library(&window)?;
    let state = state(&app);
    if request.kind != Kind::Secret {
        return Err(AppError::Validation("only secrets can be revealed".into()));
    }
    let record = state.vault.get(&request.id)?;
    Ok(details_from(&record, false, true))
}

#[tauri::command]
pub fn save_entry<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: SaveRequest,
) -> AppResult<SavedEntry> {
    require_library(&window)?;
    let state = state(&app);
    let mut draft = request.draft;
    validate_draft(&mut draft)?;
    let saved = match draft.kind {
        Kind::Command => state.repo.save_command(&mut draft)?,
        Kind::Secret => state.vault.save(&mut draft)?,
    };
    emit_entries_changed(
        &app,
        &saved,
        if draft.id.is_none() {
            "created"
        } else {
            "updated"
        },
    );
    Ok(saved)
}

#[tauri::command]
pub fn delete_entry<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: DeleteRequest,
) -> AppResult<()> {
    require_library(&window)?;
    let state = state(&app);
    match request.kind {
        Kind::Command => state
            .repo
            .delete_command(&request.id, request.expected_revision)?,
        Kind::Secret => state.vault.delete(&request.id, request.expected_revision)?,
    };
    emit_entries_changed(
        &app,
        &SavedEntry {
            id: request.id,
            revision: 0,
            kind: request.kind,
        },
        "deleted",
    );
    Ok(())
}

/// Copies the *saved* record, never a draft, and re-checks vault access while holding the
/// vault lock so a queued copy cannot complete after a lock was acknowledged.
#[tauri::command]
pub fn copy_entry<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: CopyRequest,
) -> AppResult<CopyResult> {
    let _ = &window; // both windows may copy; privileged editing/execution is library-only
    let state = state(&app);
    let expiry = state.settings.get().clipboard_expiry_seconds;
    match request.kind {
        Kind::Command => {
            let (record, _, _) = state
                .repo
                .get_command(&request.id)?
                .ok_or(AppError::NotFound)?;
            if request.expected_revision != 0 && request.expected_revision != record.revision {
                return Err(AppError::Conflict);
            }
            let result =
                state
                    .clipboard
                    .copy(Kind::Command, &record.id, &record.command, expiry)?;
            state.repo.record_usage(&record.id)?;
            Ok(result)
        }
        Kind::Secret => {
            let result = state.vault.read(|file| {
                let record = file
                    .records
                    .iter()
                    .find(|r| r.id == request.id && r.kind == Kind::Secret)
                    .ok_or(AppError::NotFound)?;
                if request.expected_revision != 0 && request.expected_revision != record.revision {
                    return Err(AppError::Conflict);
                }
                // The native copy happens inside the vault guard.
                state
                    .clipboard
                    .copy(Kind::Secret, &record.id, &record.value, expiry)
            })?;
            let _ = state.vault.record_usage(&request.id);
            Ok(result)
        }
    }
}

#[tauri::command]
pub fn run_command<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: RunRequest,
) -> AppResult<()> {
    require_library(&window)?;
    let state = state(&app);
    let (record, _, _) = state
        .repo
        .get_command(&request.id)?
        .ok_or(AppError::NotFound)?;
    if request.expected_revision != 0 && request.expected_revision != record.revision {
        return Err(AppError::Conflict);
    }
    if !request.approved {
        return Err(AppError::ApprovalRequired);
    }
    if !state.repo.approval_state(&record)? {
        state.repo.approve_execution(&record)?;
    }
    execution::execute(&state.settings.get().terminal, &record, "/bin/sh")?;
    state.repo.record_usage(&record.id)?;
    Ok(())
}

#[tauri::command]
pub fn lock_vault<R: Runtime>(
    app: AppHandle<R>,
    _window: WebviewWindow<R>,
) -> AppResult<VaultState> {
    let state = state(&app);
    // Order matters: secrets stop being readable first, then any app-owned clipboard copy.
    state.vault.lock()?;
    state.clipboard.clear_owned();
    emit_vault_state(&app);
    let vault = state.vault.state();
    let _ = tray::refresh_menu(&app);
    Ok(vault)
}

#[tauri::command]
pub fn unlock_vault<R: Runtime>(
    app: AppHandle<R>,
    _window: WebviewWindow<R>,
) -> AppResult<VaultState> {
    let state = state(&app);
    state.vault.unlock()?;
    emit_vault_state(&app);
    let vault = state.vault.state();
    let _ = tray::refresh_menu(&app);
    Ok(vault)
}

#[tauri::command]
pub fn open_library<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    request: OpenLibraryRequest,
) -> AppResult<()> {
    if window.label() != windows::FINDER {
        return Err(AppError::Forbidden(window.label().to_string()));
    }
    windows::show_library_with(&app, request.kind, request.entry_id)
}

#[tauri::command]
pub fn hide_self<R: Runtime>(window: WebviewWindow<R>) -> AppResult<()> {
    windows::hide(&window)
}

/// Emitted when a window asks to close; the frontend resolves unsaved edits first and then
/// calls this, which is the only path that actually hides a window on close.
#[tauri::command]
pub fn allow_close<R: Runtime>(window: WebviewWindow<R>, app: AppHandle<R>) -> AppResult<()> {
    if state(&app)
        .quitting
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        return windows::finish_quit(&app);
    }
    windows::hide(&window)
}

#[tauri::command]
pub fn cancel_quit<R: Runtime>(app: AppHandle<R>, window: WebviewWindow<R>) -> AppResult<()> {
    require_library(&window)?;
    windows::cancel_quit(&app);
    Ok(())
}

/// Opens the in-window settings panel (tray action).
#[tauri::command]
pub fn open_settings<R: Runtime>(app: AppHandle<R>, window: WebviewWindow<R>) -> AppResult<()> {
    require_library(&window)?;
    let _ = window.emit("open-settings", ());
    let _ = app;
    Ok(())
}

#[tauri::command]
pub fn quit_app<R: Runtime>(app: AppHandle<R>) -> AppResult<()> {
    windows::request_quit(&app)
}

#[tauri::command]
pub fn get_settings<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<Settings> {
    require_library(&window)?;
    Ok(state(&app).settings.get())
}

#[tauri::command]
pub fn save_settings<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    settings: Settings,
) -> AppResult<Settings> {
    require_library(&window)?;
    let state = state(&app);
    let saved = state.settings.save(settings)?;
    if let Err(error) = crate::autostart::apply(saved.autostart) {
        return Err(AppError::Other(format!(
            "settings saved but autostart failed: {error}"
        )));
    }
    Ok(saved)
}

/// Emits one refresh signal after a bulk operation (backup restore or archive import).
fn emit_reload_all<R: Runtime>(app: &AppHandle<R>) {
    emit_entries_changed(
        app,
        &SavedEntry {
            id: String::new(),
            revision: 0,
            kind: Kind::Command,
        },
        "reloaded",
    );
    emit_vault_state(app);
    let _ = tray::refresh_menu(app);
}

#[tauri::command]
pub fn create_backup<R: Runtime>(app: AppHandle<R>, window: WebviewWindow<R>) -> AppResult<String> {
    require_library(&window)?;
    let state = state(&app);
    let archive = state.build_archive()?;
    let seal = state.archive_seal()?;
    state.backups.create(&archive, &seal)
}

#[tauri::command]
pub fn list_backups<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<Vec<crate::backup::BackupInfo>> {
    require_library(&window)?;
    state(&app).backups.list()
}

#[tauri::command]
pub fn restore_backup<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    name: String,
) -> AppResult<BackupOutcome> {
    require_library(&window)?;
    let state = state(&app);
    let seal = state.archive_seal()?;
    let archive = state.backups.read_backup(&name, &seal)?;
    let (commands, secrets) = state.apply_archive(&archive)?;
    emit_reload_all(&app);
    Ok(BackupOutcome {
        source: name,
        commands,
        secrets,
    })
}

/// Review-before-apply for a generated backup: date and counts, no side effects.
#[tauri::command]
pub fn inspect_backup<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    name: String,
) -> AppResult<crate::backup::ArchiveSummary> {
    require_library(&window)?;
    let state = state(&app);
    let seal = state.archive_seal()?;
    state.backups.inspect_backup(&name, &seal)
}

/// Review-before-apply for an exported archive file: date and counts, no side effects.
#[tauri::command]
pub fn inspect_archive<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    path: String,
) -> AppResult<crate::backup::ArchiveSummary> {
    require_library(&window)?;
    if path.trim().is_empty() {
        return Err(AppError::Validation("choose a file first".into()));
    }
    let state = state(&app);
    let seal = state.archive_seal()?;
    state
        .backups
        .inspect_file(std::path::Path::new(&path), &seal)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKeyInfo {
    pub secret: String,
    pub path: String,
}

/// The recovery credential, so the owner can store it somewhere independent of this
/// machine. Library window only; the UI keeps it behind an explicit reveal with warnings.
#[tauri::command]
pub fn recovery_key<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> AppResult<RecoveryKeyInfo> {
    require_library(&window)?;
    let state = state(&app);
    Ok(RecoveryKeyInfo {
        secret: state.recovery.secret().to_string(),
        path: state.recovery.path().to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn export_archive<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    path: String,
    overwrite: Option<bool>,
) -> AppResult<u64> {
    require_library(&window)?;
    if path.trim().is_empty() {
        return Err(AppError::Validation(
            "choose a destination file first".into(),
        ));
    }
    let state = state(&app);
    let archive = state.build_archive()?;
    let seal = state.archive_seal()?;
    state.backups.export_to(
        std::path::Path::new(&path),
        &archive,
        &seal,
        overwrite.unwrap_or(false),
    )?;
    Ok(std::fs::metadata(&path)
        .map(|m| m.len())
        .unwrap_or_default())
}

#[tauri::command]
pub fn import_archive<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    path: String,
) -> AppResult<BackupOutcome> {
    require_library(&window)?;
    let state = state(&app);
    let seal = state.archive_seal()?;
    let archive = state
        .backups
        .import_from(std::path::Path::new(&path), &seal)?;
    let (commands, secrets) = state.apply_archive(&archive)?;
    emit_reload_all(&app);
    Ok(BackupOutcome {
        source: path,
        commands,
        secrets,
    })
}
