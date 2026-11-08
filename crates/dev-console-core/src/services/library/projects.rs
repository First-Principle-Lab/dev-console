//! Project folders and environment snapshots (plan section 3).
//!
//! Part of [`LibraryActions`]. A folder is always re-read before anything is saved, so a scan result
//! can never be replaced by command text supplied by a window; a snapshot's content is a secret and
//! goes through the same expiring, excluded clipboard as a vault record, never into a window.

use crate::environment;
use crate::error::{AppError, AppResult};
use crate::models::{CopyResult, ImportProjectRequest, Kind, ScanProjectRequest};
use crate::projects;

use crate::services::LibraryActions;
use crate::services::{reload_all, Effects};

impl LibraryActions {
    // -----------------------------------------------------------------------------------------
    // Projects and environments
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Fixture;

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
}
