//! Backups, archives and the recovery credential (plan section 3).
//!
//! Part of [`LibraryActions`]. Every archive path here goes through the state's own
//! `build_archive`/`archive_seal` readers rather than reading the two stores itself: the
//! authorisation at the top of those readers is the point - a sealed archive written from a closed
//! vault would look like a successful backup while containing no secrets at all, and a restore of it
//! would then quietly empty the library.

use crate::backup;
use crate::error::{AppError, AppResult};
use crate::models::{BackupOutcome, CopyResult};

use crate::services::LibraryActions;
use crate::services::{reload_all, Effects};

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

impl LibraryActions {
    // -----------------------------------------------------------------------------------------
    // Backups, archives and the recovery credential
    // -----------------------------------------------------------------------------------------

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

    /// Collecting an archive for a backup or an export. Delegates to [`AppState::build_archive`]
    /// rather than reading the two stores itself, because the authorisation at the top of that
    /// reader is the point: an archive of a closed vault would look like a successful backup while
    /// containing no secrets at all.
    fn build_archive(&self) -> AppResult<backup::Archive> {
        self.state.build_archive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::AppEvent;
    use crate::testing::Fixture;

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
    fn a_bulk_reload_announces_the_list_before_the_vault() {
        // The three paths that replace rows in bulk - restore, archive import, project import - all
        // go through `reload_all`, whose order is the contract. A no-op import must not churn the list.
        let fixture = Fixture::new("reload-order");
        let mut effects = Effects::default();
        crate::services::reload_all(&fixture.state, &mut effects);
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
}
