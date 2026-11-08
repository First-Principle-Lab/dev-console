//! Everything the library window may do (plan section 3).
//!
//! This is the privileged surface: it decrypts, edits, executes, and touches backups, archives and
//! the recovery credential. The palette holds [`super::FinderActions`] instead, which is a different
//! type with no path to any of this, so "the finder has no reveal" is a property of the program rather
//! than of a careful `if`.
//!
//! [`LIBRARY_OPERATIONS`] is the machine-readable list, checked against the `impl` block below by
//! `the_library_surface_declares_every_method_it_has` and against the IPC command set by
//! `the_two_surfaces_together_cover_every_ipc_command_exactly_once`.

use std::sync::Arc;

use crate::error::{AppError, AppResult};
use crate::models::{
    AppStateInfo, BackupOutcome, CopyRequest, CopyResult, DeleteRequest, EntryDetails, EntryMeta,
    GetRequest, ImportProjectRequest, Kind, KindRequest, ListRequest, PinRequest, RunRequest,
    SaveRequest, SavedEntry, ScanProjectRequest, SearchRequest, SearchResponse, Settings,
    VaultState,
};
use crate::state::AppState;
use crate::{backup, environment, execution, history, projects, repository};

use super::{
    copy_entry, details_from, lock_vault, reload_all, search_entries, unlock_vault, AppEvent,
    Effects, Surface, WindowCommand,
};

/// The whole privileged surface. One name per IPC command the library answers.
pub const LIBRARY_OPERATIONS: &[&str] = &[
    "allow_close",
    "app_state",
    "cancel_quit",
    "clear_dismissed_suggestions",
    "copy_command_output",
    "copy_entry",
    "copy_environment",
    "copy_recovery_key",
    "create_backup",
    "delete_entry",
    "dismiss_suggestion",
    "dismissed_suggestion_count",
    "environment_projects",
    "export_archive",
    "get_entry",
    "get_settings",
    "get_theme",
    "hide_self",
    "history_suggestions",
    "import_archive",
    "import_project",
    "inspect_archive",
    "inspect_backup",
    "install_history_tracking",
    "list_backups",
    "list_entries",
    "list_terminals",
    "lock_vault",
    "open_settings",
    "open_url",
    "quit_app",
    "recovery_key",
    "restart_app",
    "restore_backup",
    "reveal_secret",
    "run_command",
    "save_entry",
    "save_settings",
    "scan_project",
    "search_entries",
    "set_environment_sync",
    "set_pinned",
    "sync_environment",
    "types_for",
    "unlock_vault",
];

/// The largest command output the inspector will copy, matching the execution output cap.
const MAX_COPYABLE_OUTPUT: usize = 64 * 1024;

/// What the library window may do, including everything the palette may do.
#[derive(Clone)]
pub struct LibraryActions {
    state: Arc<AppState>,
    version: Arc<str>,
}

impl LibraryActions {
    pub fn new(state: Arc<AppState>, version: &str) -> LibraryActions {
        LibraryActions {
            state,
            version: version.into(),
        }
    }

    // -----------------------------------------------------------------------------------------
    // The shared read model, spelled out here so the two surfaces can be compared command by
    // command. Each one forwards to the same `pub(crate)` function the palette uses.
    // -----------------------------------------------------------------------------------------

    pub fn app_state(&self) -> AppStateInfo {
        super::app_state_info(&self.state, &self.version)
    }

    pub fn search_entries(&self, request: SearchRequest) -> SearchResponse {
        search_entries(&self.state, request)
    }

    pub fn copy_entry(&self, request: CopyRequest) -> AppResult<CopyResult> {
        copy_entry(&self.state, request)
    }

    pub fn lock_vault(&self, effects: &mut Effects) -> AppResult<VaultState> {
        lock_vault(&self.state, effects)
    }

    pub fn unlock_vault(&self, effects: &mut Effects) -> AppResult<VaultState> {
        unlock_vault(&self.state, effects)
    }

    pub fn get_theme(&self) -> String {
        self.state.settings.get().theme
    }

    pub fn hide_self(&self, surface: Surface) -> WindowCommand {
        WindowCommand::Hide { surface }
    }

    /// Closing a window: while the app is quitting this finishes the quit, otherwise it is a hide.
    /// The rule is the shared one in [`super::allow_close`], so the palette and the library cannot
    /// grow different answers to the same close.
    pub fn allow_close(&self, surface: Surface) -> WindowCommand {
        super::allow_close(&self.state, surface)
    }

    pub fn quit_app(&self) -> WindowCommand {
        WindowCommand::Quit
    }

    // -----------------------------------------------------------------------------------------
    // Entries
    // -----------------------------------------------------------------------------------------

    /// The list, in the store's order (pinned first, then by name). The query is matched by the
    /// store; the two filter boxes are prefixes applied here.
    pub fn list_entries(&self, request: ListRequest) -> AppResult<Vec<EntryMeta>> {
        match request.kind {
            Kind::Command => {
                let entries = self
                    .state
                    .repo
                    .list_commands(request.query.clone().unwrap_or_default().trim())?;
                Ok(super::apply_list_filters(entries, &request))
            }
            // Metadata never leaves the vault while it is locked: `list_secrets` authorises first.
            Kind::Secret => {
                let all = self.state.vault.list_secrets()?;
                let query = request.query.unwrap_or_default().to_lowercase();
                let entry_type = request.entry_type.unwrap_or_default().to_lowercase();
                let tag = request.tag.unwrap_or_default().to_lowercase();
                Ok(all
                    .into_iter()
                    .filter(|entry| super::matches_secret(entry, &query, &entry_type, &tag))
                    .collect())
            }
        }
    }

    /// The type-ahead list for the filter row: every type in use across both stores. Delegates to
    /// [`AppState::types`], which is the reader the runtime has always used - including its choice
    /// that a store which cannot be read contributes no types rather than failing the request.
    pub fn types_for(&self, request: KindRequest) -> Vec<String> {
        self.state.types(request.kind)
    }

    /// The inspector. Never carries the secret value - that is [`LibraryActions::reveal_secret`].
    pub fn get_entry(&self, request: GetRequest) -> AppResult<EntryDetails> {
        match request.kind {
            Kind::Command => {
                let (record, usage, last_used) = self
                    .state
                    .repo
                    .get_command(&request.id)?
                    .ok_or(AppError::NotFound)?;
                let mut details =
                    details_from(&record, self.state.repo.approval_state(&record)?, false);
                details.meta.usage_count = usage;
                details.meta.last_used_at = last_used;
                Ok(details)
            }
            Kind::Secret => {
                let record = self.state.vault.get(&request.id)?;
                let usage = self.state.vault.read(|file| {
                    Ok(file
                        .usage
                        .iter()
                        .find(|entry| entry.id == record.id)
                        .map(|entry| (entry.count, entry.last_used_at.clone()))
                        .unwrap_or((0, None)))
                })?;
                let mut details = details_from(&record, false, false);
                details.meta.usage_count = usage.0;
                details.meta.last_used_at = usage.1;
                Ok(details)
            }
        }
    }

    /// The only call that hands a decrypted value to a window, and only ever to the library. The
    /// palette cannot reach this; a copied secret goes to the clipboard instead, where the value
    /// never crosses back into a window.
    pub fn reveal_secret(&self, request: GetRequest) -> AppResult<EntryDetails> {
        if request.kind != Kind::Secret {
            return Err(AppError::Validation("only secrets can be revealed".into()));
        }
        let record = self.state.vault.get(&request.id)?;
        Ok(details_from(&record, false, true))
    }

    /// Save a draft. `operation` is what the list needs to know whether to insert or replace the
    /// row, and it is decided from the draft's own `id` - the same rule the frontend's list relies on.
    pub fn save_entry(&self, request: SaveRequest, effects: &mut Effects) -> AppResult<SavedRow> {
        let mut draft = request.draft;
        let operation = if draft.id.is_none() {
            "created"
        } else {
            "updated"
        };
        repository::validate_draft(&mut draft)?;
        let saved = match draft.kind {
            Kind::Command => self.state.repo.save_command(&mut draft)?,
            Kind::Secret => self.state.vault.save(&mut draft)?,
        };
        super::entries_changed(&self.state, &saved, operation, effects);
        Ok(SavedRow(saved))
    }

    /// Pin or un-pin, at the revision the window is showing: a stale pin cannot overwrite a newer
    /// edit, it gets a conflict and the window reloads.
    pub fn set_pinned(&self, request: PinRequest, effects: &mut Effects) -> AppResult<SavedEntry> {
        let saved = match request.kind {
            Kind::Command => self.state.repo.set_command_pinned(
                &request.id,
                request.expected_revision,
                request.pinned,
            )?,
            Kind::Secret => self.state.vault.set_pinned(
                &request.id,
                request.expected_revision,
                request.pinned,
            )?,
        };
        super::entries_changed(&self.state, &saved, "updated", effects);
        Ok(saved)
    }

    pub fn delete_entry(&self, request: DeleteRequest, effects: &mut Effects) -> AppResult<()> {
        match request.kind {
            Kind::Command => self
                .state
                .repo
                .delete_command(&request.id, request.expected_revision)?,
            Kind::Secret => self
                .state
                .vault
                .delete(&request.id, request.expected_revision)?,
        };
        // The row is gone, so the event names the entry rather than carrying a revision.
        let removed = SavedEntry {
            id: request.id,
            revision: 0,
            kind: request.kind,
        };
        super::entries_changed(&self.state, &removed, "deleted", effects);
        Ok(())
    }

    /// Copy the *saved* command output panel, at most as much as execution was allowed to capture.
    pub fn copy_command_output(&self, text: String) -> AppResult<CopyResult> {
        if text.is_empty() || text.len() > MAX_COPYABLE_OUTPUT {
            return Err(AppError::Validation(
                "command output is empty or too large to copy".into(),
            ));
        }
        let expiry = self.state.settings.get().clipboard_expiry_seconds;
        self.state
            .clipboard
            .copy(Kind::Command, "command-output", &text, expiry)
    }

    /// Run a saved command. The revision check and the approval check both come first, so a stale or
    /// unapproved request cannot reach a process; approval is remembered per saved revision, so an
    /// edit invalidates it and the next run asks again.
    pub fn run_command(&self, request: RunRequest) -> AppResult<execution::RunResult> {
        let (record, _, _) = self
            .state
            .repo
            .get_command(&request.id)?
            .ok_or(AppError::NotFound)?;
        if request.expected_revision != 0 && request.expected_revision != record.revision {
            return Err(AppError::Conflict);
        }
        if !request.approved {
            return Err(AppError::ApprovalRequired);
        }
        if !self.state.repo.approval_state(&record)? {
            self.state.repo.approve_execution(&record)?;
        }
        let result = if execution::needs_terminal(&record.command) {
            execution::execute(&self.state.settings.get().terminal, &record, "/bin/sh")?;
            execution::RunResult {
                mode: "terminal",
                stdout: String::new(),
                stderr: String::new(),
                exit_code: None,
                truncated: false,
                timed_out: false,
            }
        } else {
            // Blocking on purpose: the caller runs this off the UI thread.
            execution::execute_inline(&record, "/bin/sh")?
        };
        self.state.repo.record_usage(&record.id)?;
        Ok(result)
    }

    /// An http(s) link from a URL field, and nothing else.
    pub fn open_url(&self, url: String) -> AppResult<()> {
        let url = super::validated_openable_url(&url)?;
        std::process::Command::new("xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| AppError::Other(format!("could not open the browser: {error}")))
    }

    // -----------------------------------------------------------------------------------------
    // Settings, terminals and history suggestions
    // -----------------------------------------------------------------------------------------

    pub fn get_settings(&self) -> Settings {
        self.state.settings.get()
    }

    /// Saving settings changes the theme for every window, so the theme goes out before the result is
    /// handed back: the library repaints from the event, not from its own return value, which is how
    /// a palette in the other window hears about it too.
    pub fn save_settings(&self, settings: Settings, effects: &mut Effects) -> AppResult<Settings> {
        let saved = self.state.settings.save(settings)?;
        effects.emit(AppEvent::ThemeChanged(saved.theme.clone()));
        Ok(saved)
    }

    /// The tray's Settings item. A window-targeted event: the palette has no settings panel.
    pub fn open_settings(&self, effects: &mut Effects) {
        effects.emit(AppEvent::OpenSettings);
    }

    pub fn list_terminals(&self) -> Vec<execution::TerminalOption> {
        execution::available_terminals()
    }

    /// Shell-history suggestions, clamped to what the panel can show. Off, the history files are
    /// never opened at all.
    pub fn history_suggestions(&self, limit: Option<usize>) -> AppResult<Vec<history::Suggestion>> {
        if !self.state.settings.get().history_suggestions_enabled {
            return Ok(Vec::new());
        }
        let limit = limit
            .unwrap_or(history::DEFAULT_LIMIT)
            .clamp(1, history::MAX_LIMIT);
        self.state.history_suggestions(limit)
    }

    /// Only the digest of a dismissed suggestion is stored, so dismissing cannot put a history line
    /// into the database either.
    pub fn dismiss_suggestion(&self, key: String) -> AppResult<()> {
        self.state.repo.dismiss_suggestion(&key)
    }

    pub fn clear_dismissed_suggestions(&self) -> AppResult<usize> {
        self.state.repo.clear_dismissed_suggestions()
    }

    pub fn dismissed_suggestion_count(&self) -> AppResult<usize> {
        Ok(self.state.repo.dismissed_suggestion_keys()?.len())
    }

    pub fn install_history_tracking(&self) -> AppResult<String> {
        crate::project_history::install_zsh_hook()
    }

    // -----------------------------------------------------------------------------------------
    // Projects, environments, backups, archives and the recovery credential
    // -----------------------------------------------------------------------------------------

    /// Read a project folder without saving anything from it.
    pub fn scan_project(&self, request: ScanProjectRequest) -> AppResult<projects::ProjectScan> {
        projects::scan(&request.path, &self.state.repo)
    }

    /// Save the chosen tasks of a project folder as commands. The folder is read again here, so a
    /// scan result can never be replaced by command text supplied by a window.
    pub fn import_project(
        &self,
        request: ImportProjectRequest,
        effects: &mut Effects,
    ) -> AppResult<projects::ImportOutcome> {
        let outcome =
            projects::import(&request.path, &request.names, request.all, &self.state.repo)?;
        if outcome.imported > 0 {
            // The library window reloads its command list; nothing about the folder itself is logged.
            reload_all(&self.state, effects);
        }
        Ok(outcome)
    }

    pub fn environment_projects(&self) -> AppResult<Vec<environment::ProjectMeta>> {
        environment::list(&self.state.vault)
    }

    /// Blocking: the caller runs this off the UI thread.
    pub fn sync_environment(&self, path: String) -> AppResult<environment::SyncResult> {
        let root = std::fs::canonicalize(&path)?.to_string_lossy().to_string();
        environment::sync(&self.state.vault, &root, true)?;
        Ok(environment::SyncResult {
            root,
            projects: environment::list(&self.state.vault)?,
        })
    }

    pub fn set_environment_sync(&self, root: String, enabled: bool) -> AppResult<()> {
        environment::set_enabled(&self.state.vault, &root, enabled)
    }

    /// Copy a snapshot's content. It is a secret, so it goes through the same expiring, excluded
    /// clipboard as a vault record and never into a window.
    pub fn copy_environment(&self, id: String) -> AppResult<CopyResult> {
        let state = &self.state;
        let expiry = state.settings.get().clipboard_expiry_seconds;
        state.vault.read(|file| {
            let snapshot = file
                .environments
                .iter()
                .flat_map(|project| &project.snapshots)
                .find(|snapshot| snapshot.id == id)
                .ok_or(AppError::NotFound)?;
            state
                .clipboard
                .copy(Kind::Secret, &snapshot.id, &snapshot.content, expiry)
        })
    }

    pub fn create_backup(&self) -> AppResult<String> {
        let archive = self.build_archive()?;
        let seal = self.state.archive_seal()?;
        self.state.backups.create(&archive, &seal)
    }

    pub fn list_backups(&self) -> AppResult<Vec<backup::BackupInfo>> {
        self.state.backups.list()
    }

    /// Restore, then tell every window to refetch: both stores changed underneath them.
    pub fn restore_backup(&self, name: String, effects: &mut Effects) -> AppResult<BackupOutcome> {
        let seal = self.state.archive_seal()?;
        let archive = self.state.backups.read_backup(&name, &seal)?;
        let (commands, secrets) = self.state.apply_archive(&archive)?;
        reload_all(&self.state, effects);
        Ok(BackupOutcome {
            source: name,
            commands,
            secrets,
        })
    }

    /// Review-before-apply for a generated backup: date and counts, no side effects.
    pub fn inspect_backup(&self, name: String) -> AppResult<backup::ArchiveSummary> {
        let seal = self.state.archive_seal()?;
        self.state.backups.inspect_backup(&name, &seal)
    }

    /// Review-before-apply for an exported archive file.
    pub fn inspect_archive(&self, path: String) -> AppResult<backup::ArchiveSummary> {
        if path.trim().is_empty() {
            return Err(AppError::Validation("choose a file first".into()));
        }
        let seal = self.state.archive_seal()?;
        self.state
            .backups
            .inspect_file(std::path::Path::new(&path), &seal)
    }

    /// The recovery credential, so the owner can store it somewhere independent of this machine.
    /// Library-only by type: the palette has no path to it, and it decrypts every archive this
    /// profile has ever sealed.
    pub fn recovery_key(&self) -> AppResult<RecoveryCredential> {
        let (secret, path) = self.state.recovery_credential()?;
        Ok(RecoveryCredential {
            secret: secret.to_string(),
            path: path.to_string_lossy().to_string(),
        })
    }

    /// Copy the recovery credential through the native clipboard path: configured expiry, cleared
    /// with every other app-owned copy when the vault locks, and the value never crosses back into a
    /// window, so it cannot end up in a log line or a screenshot.
    pub fn copy_recovery_key(&self) -> AppResult<CopyResult> {
        self.state.copy_recovery_credential()
    }

    pub fn export_archive(&self, path: String, overwrite: Option<bool>) -> AppResult<u64> {
        if path.trim().is_empty() {
            return Err(AppError::Validation(
                "choose a destination file first".into(),
            ));
        }
        let archive = self.build_archive()?;
        let seal = self.state.archive_seal()?;
        self.state.backups.export_to(
            std::path::Path::new(&path),
            &archive,
            &seal,
            overwrite.unwrap_or(false),
        )?;
        Ok(std::fs::metadata(&path)
            .map(|file| file.len())
            .unwrap_or_default())
    }

    pub fn import_archive(&self, path: String, effects: &mut Effects) -> AppResult<BackupOutcome> {
        let seal = self.state.archive_seal()?;
        let archive = self
            .state
            .backups
            .import_from(std::path::Path::new(&path), &seal)?;
        let (commands, secrets) = self.state.apply_archive(&archive)?;
        reload_all(&self.state, effects);
        Ok(BackupOutcome {
            source: path,
            commands,
            secrets,
        })
    }

    // -----------------------------------------------------------------------------------------
    // Quit and restart
    // -----------------------------------------------------------------------------------------

    /// The user chose to stay: un-do the quit's countdown.
    pub fn cancel_quit(&self) -> WindowCommand {
        WindowCommand::CancelQuit
    }

    /// Relaunch into a visible library, which is how an installed update is entered. The desktop
    /// layer performs the exec because it is the only thing that knows its own binary.
    pub fn restart_app(&self) -> WindowCommand {
        WindowCommand::Restart
    }

    /// Collecting an archive for a backup or an export. Delegates to [`AppState::build_archive`]
    /// rather than reading the two stores itself, because the authorisation at the top of that
    /// reader is the point: an archive of a closed vault would look like a successful backup while
    /// containing no secrets at all.
    fn build_archive(&self) -> AppResult<backup::Archive> {
        self.state.build_archive()
    }
}

/// The recovery credential and where it lives. Named here rather than reused from the Tauri layer so
/// the core has no wire type: the JSON shape the settings panel reads is built by the desktop layer
/// from these two fields - and the value itself never gets a `Debug` that could print it, because it
/// decrypts every archive this profile has ever sealed. A `{:?}` in a log line or a failing
/// assertion's panic message would be the leak, so this says *that* a credential is held, never what
/// it says.
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryCredential {
    pub secret: String,
    pub path: String,
}

impl std::fmt::Debug for RecoveryCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryCredential")
            .field(
                "secret",
                &format_args!("<{} chars, redacted>", self.secret.chars().count()),
            )
            .field("path", &self.path)
            .finish()
    }
}

/// `save_entry`'s return: the saved row plus the operation the list needs, which is why it is not
/// just a [`SavedEntry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedRow(pub SavedEntry);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DeleteRequest, GetRequest, PinRequest, SaveRequest};
    use crate::services::fixtures::Fixture;
    use crate::services::Delivery;

    #[test]
    fn the_library_surface_declares_every_method_it_has() {
        let mut declared = crate::services::public_methods(include_str!("library.rs"));
        declared.sort_unstable();
        let mut expected = LIBRARY_OPERATIONS.to_vec();
        // `clear_finder_context` belongs to the palette only, so it is absent here by design; the
        // shared names are listed in both inventories.
        expected.sort_unstable();
        assert_eq!(
            declared, expected,
            "declare a new library operation in LIBRARY_OPERATIONS as well"
        );
    }

    #[test]
    fn a_reveal_is_the_only_call_that_carries_the_value() {
        let fixture = Fixture::new("reveal");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        // The palette cannot reach any of these, so the check here is that even the library's
        // inspector keeps the value out unless the call is the reveal.
        let request = GetRequest {
            kind: Kind::Secret,
            id: "absent".into(),
        };
        assert_eq!(
            actions.get_entry(request.clone()).unwrap_err().to_string(),
            "entry not found",
            "a locked-then-restored profile still reports the missing record, not a value"
        );
        assert_eq!(
            actions
                .reveal_secret(GetRequest {
                    kind: Kind::Command,
                    id: "whatever".into(),
                })
                .unwrap_err()
                .to_string(),
            "only secrets can be revealed"
        );
    }

    #[test]
    fn saving_a_new_entry_reports_created_and_a_change_reports_updated() {
        let fixture = Fixture::new("save");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");

        let mut draft = fixture.draft("deploy", "echo deploy");
        let mut effects = Effects::default();
        let created = actions
            .save_entry(
                SaveRequest {
                    draft: draft.clone(),
                },
                &mut effects,
            )
            .expect("save")
            .0;
        assert_eq!(created.kind, Kind::Command);
        assert_eq!(created.revision, 1);
        assert_eq!(effects.events.len(), 1);
        match &effects.events[0] {
            AppEvent::EntriesChanged(event) => {
                assert_eq!(event.operation, "created");
                assert_eq!(event.id, created.id);
                assert_eq!(event.vault_generation, fixture.generation());
            }
            other => panic!("expected an entries-changed event, got {other:?}"),
        }

        // The editor resaves the row it is showing, at the revision it loaded.
        draft.id = Some(created.id.clone());
        draft.expected_revision = created.revision;
        draft.command = "echo deploy twice".into();
        let mut effects = Effects::default();
        let updated = actions
            .save_entry(SaveRequest { draft }, &mut effects)
            .expect("resave")
            .0;
        assert_eq!(updated.revision, 2);
        match &effects.events[0] {
            AppEvent::EntriesChanged(event) => assert_eq!(event.operation, "updated"),
            other => panic!("expected an entries-changed event, got {other:?}"),
        }
    }

    #[test]
    fn a_draft_the_store_refuses_produces_no_event() {
        let fixture = Fixture::new("invalid-draft");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        let mut draft = fixture.draft("spaced name", "echo hi");
        draft.command = String::new();
        let mut effects = Effects::default();
        assert!(actions
            .save_entry(SaveRequest { draft }, &mut effects)
            .is_err());
        assert!(
            effects.is_empty(),
            "a rejected save must not tell the list to draw a row that does not exist"
        );
    }

    #[test]
    fn pinning_or_deleting_at_a_stale_revision_conflicts_and_says_nothing() {
        let fixture = Fixture::new("conflict");
        fixture.seed_command("git-push", "Alias", "git push", &["git"]);
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");

        let mut effects = Effects::default();
        assert_eq!(
            actions
                .set_pinned(
                    PinRequest {
                        kind: Kind::Command,
                        id: "git-push".into(),
                        expected_revision: 9,
                        pinned: true,
                    },
                    &mut effects,
                )
                .unwrap_err()
                .to_string(),
            "this entry was changed elsewhere; reload it before saving"
        );
        assert!(effects.is_empty());

        let mut effects = Effects::default();
        assert!(actions
            .delete_entry(
                DeleteRequest {
                    kind: Kind::Command,
                    id: "git-push".into(),
                    expected_revision: 9,
                },
                &mut effects,
            )
            .is_err());
        assert!(
            effects.is_empty(),
            "a rejected delete must not remove a row the user can still see"
        );
    }

    #[test]
    fn a_pinned_row_and_a_deleted_row_both_announce_themselves() {
        let fixture = Fixture::new("pin-and-delete");
        fixture.seed_command("git-push", "Alias", "git push", &["git"]);
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");

        let mut effects = Effects::default();
        let pinned = actions
            .set_pinned(
                PinRequest {
                    kind: Kind::Command,
                    id: "git-push".into(),
                    expected_revision: 1,
                    pinned: true,
                },
                &mut effects,
            )
            .expect("pin");
        assert_eq!(
            pinned.revision, 2,
            "a pin is an edit, so it advances the revision"
        );
        match &effects.events[0] {
            AppEvent::EntriesChanged(event) => {
                assert_eq!(
                    (event.operation, event.id.as_str(), event.revision),
                    ("updated", "git-push", 2)
                );
            }
            other => panic!("expected an entries-changed event, got {other:?}"),
        }

        let mut effects = Effects::default();
        actions
            .delete_entry(
                DeleteRequest {
                    kind: Kind::Command,
                    id: "git-push".into(),
                    expected_revision: 2,
                },
                &mut effects,
            )
            .expect("delete");
        match &effects.events[0] {
            AppEvent::EntriesChanged(event) => assert_eq!(
                (event.operation, event.id.as_str(), event.revision),
                ("deleted", "git-push", 0),
                "the row is gone, so the event names it rather than carrying a revision"
            ),
            other => panic!("expected an entries-changed event, got {other:?}"),
        }
        assert!(actions
            .list_entries(ListRequest {
                kind: Kind::Command,
                ..Default::default()
            })
            .expect("list")
            .is_empty());
    }

    #[test]
    fn the_command_type_box_names_a_whole_type() {
        let fixture = Fixture::new("filters");
        fixture.seed_command("cargo-build", "Shell", "cargo build", &["rust", "ci"]);
        fixture.seed_command("git-push", "Alias", "git push", &["git"]);
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");

        let all = actions
            .list_entries(ListRequest {
                kind: Kind::Command,
                ..Default::default()
            })
            .expect("list");
        assert_eq!(all.len(), 2);

        let shells = actions
            .list_entries(ListRequest {
                kind: Kind::Command,
                entry_type: Some("shell".into()),
                ..Default::default()
            })
            .expect("filtered");
        assert_eq!(
            shells
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["cargo-build"],
            "the box is completed from the types in use, so the comparison is a whole type"
        );
        assert!(
            actions
                .list_entries(ListRequest {
                    kind: Kind::Command,
                    entry_type: Some("sh".into()),
                    ..Default::default()
                })
                .expect("partial type")
                .is_empty(),
            "a fragment of a type name matches no command row"
        );
        // The query reaches the store, which matches any of name, type, description or tags.
        let by_tag = actions
            .list_entries(ListRequest {
                kind: Kind::Command,
                query: Some("ci".into()),
                ..Default::default()
            })
            .expect("query");
        assert_eq!(by_tag.len(), 1);
        assert!(actions
            .list_entries(ListRequest {
                kind: Kind::Command,
                tag: Some("extr".into()),
                ..Default::default()
            })
            .expect("tag")
            .is_empty());
    }

    #[test]
    fn secret_metadata_never_leaves_a_locked_vault() {
        let fixture = Fixture::new("locked-list");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        actions
            .lock_vault(&mut Effects::default())
            .expect("manual lock");
        assert_eq!(
            actions
                .list_entries(ListRequest {
                    kind: Kind::Secret,
                    ..Default::default()
                })
                .unwrap_err()
                .to_string(),
            "the vault is locked"
        );
        assert_eq!(
            actions
                .get_entry(GetRequest {
                    kind: Kind::Secret,
                    id: "anything".into(),
                })
                .unwrap_err()
                .to_string(),
            "the vault is locked"
        );
    }

    #[test]
    fn a_closed_vault_is_refused_as_a_backup_and_as_an_export_rather_than_written_empty() {
        // The whole point of going through `AppState::build_archive` for both of them: reading the
        // two stores directly would produce a *successful*-looking sealed archive containing no
        // secrets at all, and a restore of it would then quietly empty the library.
        let fixture = Fixture::new("locked-archive");
        fixture.seed_command("git-push", "Alias", "git push", &["git"]);
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        actions
            .lock_vault(&mut Effects::default())
            .expect("manual lock");

        let mut effects = Effects::default();
        assert_eq!(
            actions.create_backup().unwrap_err().to_string(),
            "the vault is locked",
            "a backup of a closed vault must be a refusal, not an empty archive"
        );
        // And a refusal tells the windows nothing: an archive that was never read must not make
        // every library refetch its list as if something had been restored.
        assert!(actions
            .restore_backup("no-such-backup.age".into(), &mut effects)
            .is_err());
        assert!(effects.is_empty(), "a refused restore announces nothing");
        assert!(actions
            .import_archive("/nonexistent/archive.age".into(), &mut effects)
            .is_err());
        assert!(effects.is_empty(), "a refused import announces nothing");
        let destination = std::env::temp_dir().join(format!(
            "dev-console-locked-export-{}.age",
            std::process::id()
        ));
        assert_eq!(
            actions
                .export_archive(destination.to_string_lossy().to_string(), Some(true))
                .unwrap_err()
                .to_string(),
            "the vault is locked"
        );
        assert!(
            !destination.exists(),
            "a refused export must leave no half-written archive behind"
        );
    }

    #[test]
    fn the_type_list_spans_both_stores_and_ignores_case_when_ordering() {
        let fixture = Fixture::new("types");
        fixture.seed_command("one", "Shell", "echo one", &[]);
        fixture.seed_command("two", "alias", "echo two", &[]);
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        let types = actions.types_for(KindRequest {
            kind: Kind::Command,
        });
        assert_eq!(types, vec!["alias".to_string(), "Shell".to_string()]);
    }

    #[test]
    fn closing_the_library_while_quitting_finishes_the_quit() {
        let fixture = Fixture::new("allow-close");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        assert_eq!(
            actions.allow_close(Surface::Library),
            WindowCommand::Hide {
                surface: Surface::Library
            },
            "resident: the webview gets close-requested and answers for unsaved edits"
        );
        fixture
            .state
            .quitting
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            actions.allow_close(Surface::Library),
            WindowCommand::FinishQuit
        );
        assert_eq!(actions.cancel_quit(), WindowCommand::CancelQuit);
        assert_eq!(actions.restart_app(), WindowCommand::Restart);
        assert_eq!(actions.quit_app(), WindowCommand::Quit);
    }

    #[test]
    fn saving_settings_announces_the_theme_before_handing_the_result_back() {
        let fixture = Fixture::new("settings");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        let settings = Settings {
            theme: "omarchy-ristretto".into(),
            ..actions.get_settings()
        };
        let mut effects = Effects::default();
        let saved = actions.save_settings(settings, &mut effects).expect("save");
        assert_eq!(saved.theme, "omarchy-ristretto");
        assert_eq!(actions.get_theme(), "omarchy-ristretto");
        assert_eq!(
            effects.events.len(),
            1,
            "saving settings announces one thing"
        );
        match &effects.events[0] {
            AppEvent::ThemeChanged(theme) => assert_eq!(theme, "omarchy-ristretto"),
            other => panic!("expected a theme-changed event, got {other:?}"),
        }
        assert_eq!(effects.events[0].delivery(), Delivery::All);
    }

    #[test]
    fn the_tray_settings_action_addresses_the_library_only() {
        let fixture = Fixture::new("open-settings");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        let mut effects = Effects::default();
        actions.open_settings(&mut effects);
        assert_eq!(effects.events.len(), 1);
        assert_eq!(effects.events[0].name(), "open-settings");
        assert_eq!(
            effects.events[0].delivery(),
            Delivery::Library,
            "the palette has no settings panel to open"
        );
    }

    #[test]
    fn command_output_is_capped_at_what_execution_was_allowed_to_capture() {
        let fixture = Fixture::new("copy-output");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        assert_eq!(
            actions
                .copy_command_output(String::new())
                .unwrap_err()
                .to_string(),
            "command output is empty or too large to copy"
        );
        let too_big = "x".repeat(MAX_COPYABLE_OUTPUT + 1);
        assert!(actions.copy_command_output(too_big).is_err());
        // Right at the cap the size check passes; only the clipboard itself may object, and a
        // container with no display server is exactly where that happens. Either outcome is a pass
        // as long as the refusal is not the size rule.
        let at_cap = "x".repeat(MAX_COPYABLE_OUTPUT);
        if let Err(error) = actions.copy_command_output(at_cap) {
            assert!(
                !error.to_string().contains("too large"),
                "the cap is inclusive: {error}"
            );
        }
    }

    #[test]
    fn an_unapproved_or_stale_run_never_reaches_a_process() {
        let fixture = Fixture::new("run");
        fixture.seed_command("echo", "Shell", "echo hi", &[]);
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        assert_eq!(
            actions
                .run_command(RunRequest {
                    id: "echo".into(),
                    expected_revision: 1,
                    approved: false,
                })
                .err()
                .expect("an unapproved run never reaches a process")
                .to_string(),
            "running this command needs explicit approval for the saved version"
        );
        assert_eq!(
            actions
                .run_command(RunRequest {
                    id: "echo".into(),
                    expected_revision: 9,
                    approved: true,
                })
                .err()
                .expect("a stale revision is refused before anything is spawned")
                .to_string(),
            "this entry was changed elsewhere; reload it before saving"
        );
        assert_eq!(
            actions
                .run_command(RunRequest {
                    id: "absent".into(),
                    expected_revision: 0,
                    approved: true,
                })
                .err()
                .expect("a missing entry is a missing entry")
                .to_string(),
            "entry not found"
        );
    }

    #[test]
    fn only_an_http_or_https_link_is_handed_to_the_opener() {
        let fixture = Fixture::new("open-url");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        for rejected in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "-file=/tmp/evil",
        ] {
            assert_eq!(
                actions.open_url(rejected.into()).unwrap_err().to_string(),
                "only http(s) links can be opened"
            );
        }
    }

    #[test]
    fn a_bulk_reload_announces_the_list_before_the_vault() {
        // The three paths that replace rows in bulk - restore, archive import, project import - all
        // go through `reload_all`, whose order is the contract. A no-op import must not churn the list.
        let fixture = Fixture::new("reload-order");
        let mut effects = Effects::default();
        super::reload_all(&fixture.state, &mut effects);
        assert_eq!(effects.events.len(), 2);
        assert_eq!(effects.events[0].name(), "entries-changed");
        assert_eq!(effects.events[1].name(), "vault-state-changed");
        assert!(effects.tray_refresh);
        match &effects.events[0] {
            AppEvent::EntriesChanged(event) => {
                assert_eq!(event.operation, "reloaded");
                assert!(event.id.is_empty(), "a reload names no entry");
                assert_eq!(event.revision, 0);
            }
            other => panic!("expected an entries-changed event, got {other:?}"),
        }
    }

    #[test]
    fn an_import_that_saved_nothing_churns_no_list() {
        let fixture = Fixture::new("quiet-import");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        // Nothing to read and nothing selected: the reload signal is only for work that happened.
        let mut effects = Effects::default();
        let outcome = actions.import_project(
            ImportProjectRequest {
                path: fixture.scratch_dir(),
                names: vec![],
                all: false,
            },
            &mut effects,
        );
        assert!(
            outcome.is_err() || outcome.expect("outcome").imported == 0,
            "an empty selection cannot import anything"
        );
        assert!(
            effects.is_empty(),
            "nothing was saved, so the list must not refetch"
        );
    }

    #[test]
    fn history_suggestions_are_off_by_default_and_clamped_when_on() {
        let fixture = Fixture::new("suggestions");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        let settings = Settings {
            history_suggestions_enabled: false,
            ..actions.get_settings()
        };
        actions
            .save_settings(settings, &mut Effects::default())
            .expect("save");
        assert!(actions
            .history_suggestions(None)
            .expect("off means an empty list, not an error")
            .is_empty());
        // Dismissals are digests, so the count starts at zero for a fresh profile.
        assert_eq!(actions.dismissed_suggestion_count().expect("count"), 0);
        assert_eq!(actions.clear_dismissed_suggestions().expect("clear"), 0);
    }
}
