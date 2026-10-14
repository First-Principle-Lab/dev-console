use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::repository::validate_draft;
use crate::state::AppState;
use crate::{execution, search, windows};

/// Frontend windows are untrusted. Every privileged operation is re-checked here against the
/// calling window label, so the restriction does not depend on hidden buttons or on the
/// capability file alone.
fn require_library(window: &WebviewWindow) -> AppResult<()> {
    match window.label() {
        windows::LIBRARY => Ok(()),
        other => Err(AppError::Forbidden(other.to_string())),
    }
}

fn state(app: &AppHandle) -> Arc<AppState> {
    app.state::<Arc<AppState>>().inner().clone()
}

fn emit_entries_changed(app: &AppHandle, payload: &SavedEntry, operation: &'static str) {
    let event = EntriesChanged {
        kind: payload.kind,
        id: payload.id.clone(),
        revision: payload.revision,
        operation,
        vault_generation: app.state::<Arc<AppState>>().vault.generation(),
    };
    let _ = app.emit("entries-changed", event);
}

fn emit_vault_state(app: &AppHandle) {
    let vault = state(app).vault.state();
    let _ = app.emit("vault-state-changed", vault);
}

#[tauri::command]
pub fn app_state(app: AppHandle, _window: WebviewWindow) -> AppStateInfo {
    AppStateInfo {
        vault: state(&app).vault.state(),
        version: app.package_info().version.to_string(),
        authenticated_unlock_available: crate::auth::mechanism_available(),
        clipboard_exclusion: crate::clipboard::exclusion_status(),
    }
}

#[tauri::command]
pub fn search_entries(
    app: AppHandle,
    _window: WebviewWindow,
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
pub fn list_entries(
    app: AppHandle,
    window: WebviewWindow,
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
pub fn types_for(
    app: AppHandle,
    window: WebviewWindow,
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
pub fn get_entry(
    app: AppHandle,
    window: WebviewWindow,
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
            let usage = state.vault.read(|_, usage| {
                Ok(usage
                    .iter()
                    .find(|(id, _, _)| id == &record.id)
                    .map(|(_, c, l)| (*c, l.clone()))
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
pub fn reveal_secret(
    app: AppHandle,
    window: WebviewWindow,
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
pub fn save_entry(
    app: AppHandle,
    window: WebviewWindow,
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
pub fn delete_entry(
    app: AppHandle,
    window: WebviewWindow,
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
pub fn copy_entry(
    app: AppHandle,
    window: WebviewWindow,
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
            let result = state.vault.read(|records, _| {
                let record = records
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
pub fn run_command(app: AppHandle, window: WebviewWindow, request: RunRequest) -> AppResult<()> {
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
pub fn lock_vault(app: AppHandle, _window: WebviewWindow) -> AppResult<VaultState> {
    let state = state(&app);
    // Order matters: secrets stop being readable first, then any app-owned clipboard copy.
    state.vault.lock()?;
    state.clipboard.clear_owned();
    emit_vault_state(&app);
    Ok(state.vault.state())
}

#[tauri::command]
pub fn unlock_vault(app: AppHandle, _window: WebviewWindow) -> AppResult<VaultState> {
    let state = state(&app);
    state.vault.unlock()?;
    emit_vault_state(&app);
    Ok(state.vault.state())
}

#[tauri::command]
pub fn open_library(
    app: AppHandle,
    window: WebviewWindow,
    request: OpenLibraryRequest,
) -> AppResult<()> {
    if window.label() != windows::FINDER {
        return Err(AppError::Forbidden(window.label().to_string()));
    }
    windows::show_library_with(&app, request.kind, request.entry_id)
}

#[tauri::command]
pub fn hide_self(window: WebviewWindow) -> AppResult<()> {
    windows::hide(&window)
}

/// Emitted when a window asks to close; the frontend resolves unsaved edits first and then
/// calls this, which is the only path that actually hides a window on close.
#[tauri::command]
pub fn allow_close(window: WebviewWindow, app: AppHandle) -> AppResult<()> {
    if state(&app)
        .quitting
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        return windows::finish_quit(&app);
    }
    windows::hide(&window)
}

#[tauri::command]
pub fn cancel_quit(app: AppHandle, window: WebviewWindow) -> AppResult<()> {
    require_library(&window)?;
    windows::cancel_quit(&app);
    Ok(())
}

/// Opens the in-window settings panel (tray action).
#[tauri::command]
pub fn open_settings(app: AppHandle, window: WebviewWindow) -> AppResult<()> {
    require_library(&window)?;
    let _ = window.emit("open-settings", ());
    let _ = app;
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) -> AppResult<()> {
    windows::request_quit(&app)
}

#[tauri::command]
pub fn get_settings(app: AppHandle, window: WebviewWindow) -> AppResult<Settings> {
    require_library(&window)?;
    Ok(state(&app).settings.get())
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    window: WebviewWindow,
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
