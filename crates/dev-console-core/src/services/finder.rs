//! The palette's service surface: search, copy, context dismissal and navigation, and nothing else
//! (plan section 3).
//!
//! This is the whole capability of the finder window. It is deliberately a separate value type from
//! [`super::LibraryActions`] rather than a flag or a mode: there is no call that turns one into the
//! other, so a handler written against the palette cannot reach a privileged operation even by
//! accident, and a reviewer can read this file's method list and see the boundary.
//!
//! [`FINDER_OPERATIONS`] is the machine-readable version of that list. It is checked at compile time
//! against the capability families in [`super`], and
//! `the_finder_surface_declares_every_method_it_has` proves the inventory matches the `impl` block
//! below by reading this file.

use std::sync::Arc;

use crate::error::AppResult;
use crate::models::{
    AppStateInfo, CopyRequest, CopyResult, Kind, SearchRequest, SearchResponse, VaultState,
};
use crate::state::AppState;

use super::{search_entries, Effects, Surface, WindowCommand};

/// Every operation the palette may perform - and so every operation this file may grow without a
/// fresh look at the plan's boundary. See the compile-time check in `mod.rs`.
pub const FINDER_OPERATIONS: &[&str] = &[
    "allow_close",
    "app_state",
    "clear_finder_context",
    "copy_entry",
    "get_theme",
    "hide_self",
    "lock_vault",
    "open_library",
    "quit_app",
    "search_entries",
    "unlock_vault",
];

/// What the palette window may do.
///
/// The IPC layer builds this from the calling window's label; the privileged surface is a different
/// type, so no `if` in this file decides whether a call is allowed.
#[derive(Clone)]
pub struct FinderActions {
    state: Arc<AppState>,
    version: Arc<str>,
}

impl FinderActions {
    pub fn new(state: Arc<AppState>, version: &str) -> FinderActions {
        FinderActions {
            state,
            version: version.into(),
        }
    }

    /// What every window shows: lock state, version, the backup directory, and what the palette is
    /// currently ranked against.
    pub fn app_state(&self) -> AppStateInfo {
        super::app_state_info(&self.state, &self.version)
    }

    /// Ranking only. The rows carry metadata - name, type, tags, description - and never a command's
    /// full record or a secret's value: [`AppState::searchable_metadata`] is the decrypting step and
    /// it refuses to run while the vault is locked.
    pub fn search_entries(&self, request: SearchRequest) -> SearchResponse {
        search_entries(&self.state, request)
    }

    /// `Ctrl+P`, or clicking the chip: forget the detected project for as long as this palette stays
    /// open. A detection still in flight is invalidated too, so the chip cannot reappear a moment
    /// after the user got rid of it; the next open looks again.
    pub fn clear_finder_context(&self) -> AppResult<()> {
        self.state.cancel_finder_detection();
        Ok(())
    }

    /// Copy a *saved* entry. Both windows may copy; nothing here edits or executes.
    pub fn copy_entry(&self, request: CopyRequest) -> AppResult<CopyResult> {
        super::copy_entry(&self.state, request)
    }

    /// Lock the vault. The palette has always been able to do this, and the order inside is the
    /// security-relevant part: secrets stop being readable before any app-owned clipboard copy goes
    /// away.
    pub fn lock_vault(&self, effects: &mut Effects) -> AppResult<VaultState> {
        super::lock_vault(&self.state, effects)
    }

    pub fn unlock_vault(&self, effects: &mut Effects) -> AppResult<VaultState> {
        super::unlock_vault(&self.state, effects)
    }

    /// The saved theme, which every window mirrors. Full settings remain library-only.
    pub fn get_theme(&self) -> String {
        self.state.settings.get().theme
    }

    /// `Escape` in the palette. Hiding is not quitting: the resident app keeps its tray item.
    pub fn hide_self(&self, surface: Surface) -> WindowCommand {
        WindowCommand::Hide { surface }
    }

    /// Closing the palette. While the resident app is quitting, the close is the last step of that
    /// quit - the shipped handler never asked which window answered, and a palette that hid instead
    /// would leave a quit with nothing left to finish it. Otherwise it is a plain hide: the palette
    /// has nothing to answer for, because unsaved edits only exist in the library's editor.
    pub fn allow_close(&self, surface: Surface) -> WindowCommand {
        super::allow_close(&self.state, surface)
    }

    /// The keyboard-less way to leave the resident app. The quit is still answered for: the desktop
    /// runtime hides both windows and the library gets its chance to report unsaved edits.
    pub fn quit_app(&self) -> WindowCommand {
        WindowCommand::Quit
    }

    /// The palette's `Enter`: present the library, and tell it which entry to open or that a draft is
    /// starting. This is the one command the *library* window is refused, and the refusal names the
    /// calling window exactly as the label comparison it replaces did.
    pub fn open_library(
        &self,
        surface: Surface,
        kind: Kind,
        entry_id: Option<String>,
    ) -> AppResult<WindowCommand> {
        surface.require_finder()?;
        Ok(WindowCommand::ShowLibrary { kind, entry_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CopyRequest, SearchRequest};
    use crate::services::fixtures::Fixture;
    use crate::services::Delivery;

    #[test]
    fn the_finder_surface_declares_every_method_it_has() {
        let mut declared = crate::services::public_methods(include_str!("finder.rs"));
        declared.sort_unstable();
        let mut expected = FINDER_OPERATIONS.to_vec();
        expected.sort_unstable();
        assert_eq!(
            declared, expected,
            "declare a new palette operation in FINDER_OPERATIONS as well"
        );
    }

    #[test]
    fn the_palette_may_open_the_library_and_the_library_may_not() {
        let fixture = Fixture::new("open-library");
        let actions = FinderActions::new(fixture.state.clone(), "9.9.9");
        let opened = actions
            .open_library(Surface::Finder, Kind::Secret, Some("secret-1".into()))
            .expect("the palette may ask for the library");
        assert_eq!(
            opened,
            WindowCommand::ShowLibrary {
                kind: Kind::Secret,
                entry_id: Some("secret-1".into())
            }
        );
        assert_eq!(
            actions
                .open_library(Surface::Library, Kind::Command, None)
                .unwrap_err()
                .to_string(),
            "this window is not allowed to call library"
        );
        // The third case the old label comparison handled: a window this app did not create.
        assert_eq!(
            actions
                .open_library(Surface::Other("webview-9".into()), Kind::Command, None)
                .unwrap_err()
                .to_string(),
            "this window is not allowed to call webview-9"
        );
    }

    #[test]
    fn closing_or_hiding_the_palette_leaves_the_resident_app_running() {
        let fixture = Fixture::new("hide");
        let actions = FinderActions::new(fixture.state.clone(), "9.9.9");
        assert_eq!(
            actions.hide_self(Surface::Finder),
            WindowCommand::Hide {
                surface: Surface::Finder
            }
        );
        assert_eq!(
            actions.allow_close(Surface::Finder),
            WindowCommand::Hide {
                surface: Surface::Finder
            },
            "a resident app is not quit by dismissing its palette"
        );
        assert_eq!(actions.quit_app(), WindowCommand::Quit);

        // A quit that is already under way is a different question, and the palette is not what
        // holds it open: the shipped `allow_close` finished the quit whichever window called it.
        fixture
            .state
            .quitting
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            actions.allow_close(Surface::Finder),
            WindowCommand::FinishQuit,
            "the last window to answer for a quit is what ends it"
        );
        assert_eq!(
            actions.hide_self(Surface::Finder),
            WindowCommand::Hide {
                surface: Surface::Finder
            },
            "an `Escape` is still only a hide, quitting or not"
        );
    }

    #[test]
    fn the_palette_sees_the_same_shared_read_model_as_the_library() {
        let fixture = Fixture::new("app-state");
        fixture.set_theme("omarchy-ristretto");
        let actions = FinderActions::new(fixture.state.clone(), "9.9.9");
        let state = actions.app_state();
        assert_eq!(state.version, "9.9.9");
        assert_eq!(state.backup_dir, fixture.backup_dir());
        assert_eq!(actions.get_theme(), "omarchy-ristretto");
        assert_eq!(state.vault.unlocked, fixture.state.vault.state().unlocked);
    }

    #[test]
    fn a_search_carries_the_request_id_and_generation_it_must_be_rendered_at() {
        // The staleness guard the palette needs: a response is only drawn when its `request_id` is
        // still the newest and its `vault_generation` still matches, so a late answer from before a
        // lock cannot reappear afterwards.
        let fixture = Fixture::new("search");
        fixture.seed_command("git-push", "Alias", "git push", &["git"]);
        let actions = FinderActions::new(fixture.state.clone(), "9.9.9");

        let response = actions.search_entries(SearchRequest {
            query: "push".into(),
            request_id: 41,
            limit: None,
        });
        assert_eq!(response.request_id, 41);
        assert_eq!(response.vault_generation, fixture.generation());
        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].id, "git-push");
        assert_eq!(
            response.results[0].completion, None,
            "an exact query has nothing to complete"
        );

        let empty = actions.search_entries(SearchRequest {
            query: "nothing-matches".into(),
            request_id: 42,
            limit: Some(10),
        });
        assert!(empty.results.is_empty());
        assert_eq!(
            empty.request_id, 42,
            "the id is echoed for the staleness check"
        );
    }

    #[test]
    fn dismissing_the_project_context_invalidates_a_detection_still_in_flight() {
        let fixture = Fixture::new("context");
        fixture.set_finder_context("/work/api", "api");
        let actions = FinderActions::new(fixture.state.clone(), "9.9.9");
        assert_eq!(
            actions
                .app_state()
                .finder_context
                .map(|context| context.root),
            Some("/work/api".into()),
            "the palette learns what it is ranked against from the same read model"
        );

        // A detection thread answering for this token is now answering for nothing.
        let token = fixture.state.begin_finder_detection();
        actions.clear_finder_context().expect("dismissal");
        assert!(
            !fixture.state.finish_finder_detection(token, None),
            "an answer to a superseded detection must not be applied"
        );
    }

    #[test]
    fn locking_reports_the_new_state_once_and_asks_for_a_tray_refresh() {
        let fixture = Fixture::new("lock");
        let actions = FinderActions::new(fixture.state.clone(), "9.9.9");
        assert!(
            fixture.state.vault.state().unlocked,
            "the fixture starts unlocked"
        );

        let mut effects = Effects::default();
        let vault = actions.lock_vault(&mut effects).expect("lock");
        assert!(!vault.unlocked);
        assert!(
            vault.manual_lock,
            "a palette lock is a persistent manual lock"
        );
        assert_eq!(effects.events.len(), 1);
        assert_eq!(effects.events[0].name(), "vault-state-changed");
        assert_eq!(effects.events[0].delivery(), Delivery::All);
        assert!(effects.tray_refresh);
        assert!(effects.windows.is_empty());

        // And what the palette could have listed is gone with it.
        let response = actions.search_entries(SearchRequest {
            query: String::new(),
            request_id: 7,
            limit: None,
        });
        assert!(
            response.results.is_empty(),
            "secret metadata never leaves a locked vault, not even ranked"
        );
        assert!(
            response.vault_generation > 0,
            "the generation the empty answer was read at is reported for the staleness check"
        );
    }

    #[test]
    fn unlocking_reports_the_state_and_never_returns_stale_metadata() {
        let fixture = Fixture::new("unlock");
        let actions = FinderActions::new(fixture.state.clone(), "9.9.9");
        let mut effects = Effects::default();
        let vault = actions.unlock_vault(&mut effects).expect("unlock");
        assert!(vault.unlocked);
        assert_eq!(effects.events.len(), 1);
        assert_eq!(effects.events[0].name(), "vault-state-changed");
        assert!(effects.tray_refresh);
    }

    #[test]
    fn copying_a_stale_revision_is_refused_before_the_clipboard_is_touched() {
        let fixture = Fixture::new("copy-conflict");
        fixture.seed_command("git-push", "Alias", "git push", &["git"]);
        let actions = FinderActions::new(fixture.state.clone(), "9.9.9");
        assert_eq!(
            actions
                .copy_entry(CopyRequest {
                    kind: Kind::Command,
                    id: "git-push".into(),
                    expected_revision: 99,
                })
                .unwrap_err()
                .to_string(),
            "this entry was changed elsewhere; reload it before saving"
        );
        assert_eq!(
            actions
                .copy_entry(CopyRequest {
                    kind: Kind::Command,
                    id: "missing".into(),
                    expected_revision: 0,
                })
                .unwrap_err()
                .to_string(),
            "entry not found"
        );
    }
}
