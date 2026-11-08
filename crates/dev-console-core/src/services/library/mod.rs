//! Everything the library window may do (plan section 3).
//!
//! This is the privileged surface: it decrypts, edits, executes, and touches backups, archives and
//! the recovery credential. The palette holds [`super::FinderActions`] instead, which is a different
//! type with no path to any of this, so "the finder has no reveal" is a property of the program rather
//! than of a careful `if`.
//!
//! The surface is spread over this file and the five submodules below, one per seam of the library's
//! own UI: the shared read model and the window commands live here; the entry list, the inspector and
//! the editor live in `entries`; execution in `execution`; the settings panel and history suggestions
//! in `settings`; the project and environment panels in `projects`; and the backup, archive and
//! recovery panel in `archive`. Every submodule holds an `impl LibraryActions` block of one type -
//! the split is along the screen edges, not along any rule the type enforces, and the surface is
//! still one value handed to one window.
//!
//! [`LIBRARY_OPERATIONS`] is the machine-readable list, checked against every `impl` block of this
//! module by `the_library_surface_declares_every_method_it_has` and against the IPC command set by
//! `the_two_surfaces_together_cover_every_ipc_command_exactly_once`.

mod archive;
mod entries;
mod execution;
mod projects;
mod settings;

use std::sync::Arc;

use crate::error::AppResult;
use crate::models::{
    AppStateInfo, CopyRequest, CopyResult, SearchRequest, SearchResponse, VaultState,
};
use crate::state::AppState;

use super::{
    copy_entry, lock_vault, search_entries, unlock_vault, Effects, Surface, WindowCommand,
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
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Fixture;

    #[test]
    fn the_library_surface_declares_every_method_it_has() {
        // The surface is spread over this file and five submodules, so the declaration is checked
        // against every one of them: a method added to any `impl LibraryActions` block - or to a new
        // submodule that skipped this list - fails the build until the inventory and the scan both
        // know about it.
        let scanned = [
            include_str!("mod.rs"),
            include_str!("archive.rs"),
            include_str!("entries.rs"),
            include_str!("execution.rs"),
            include_str!("projects.rs"),
            include_str!("settings.rs"),
        ]
        .concat();
        let mut declared = crate::services::public_methods(&scanned);
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
}
