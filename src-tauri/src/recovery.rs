//! Durable recovery state for a whole-library replacement (backup restore / archive import).
//!
//! Applying an archive touches two stores that cannot share one transaction: the encrypted
//! vault file and the SQLite command table. The compensating rollback in
//! [`crate::state::AppState::apply_archive`] covers ordinary errors, but it cannot run if the
//! process is killed between the vault write and the command commit, which would leave the
//! secrets on one version of the library and the commands on another.
//!
//! The journal below makes that window decidable:
//!
//! 1. before either store is touched, a sealed journal holding the **previous** command table
//!    and the **previous** vault content is written durably, carrying an operation id,
//! 2. the command-table transaction writes that same operation id into `apply_state` in the
//!    same transaction as its rows,
//! 3. on success the journal is removed first, then the marker.
//!
//! At startup, a leftover journal is therefore answered from the database alone: the marker
//! matches the journal's id (the command commit happened, so both stores already hold the
//! archive's version - only the bookkeeping is missing) or it does not (the commit never
//! happened - both stores are rolled back to the journal's version). Anything unreadable is
//! kept and reported, never silently deleted.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::backup::Seal;
use crate::error::{AppError, AppResult};
use crate::models::{EntryRecord, Kind};
use crate::repository::now_rfc3339;
use crate::vault::VaultFile;

const JOURNAL_NAME: &str = "pending-replace.age";

/// What an interrupted replacement has to roll back to, plus the id its command transaction
/// would have stamped. Sealed with the archive credential before it reaches the disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingReplacement {
    pub operation_id: String,
    pub started_at: String,
    pub commands: Vec<EntryRecord>,
    pub vault: VaultFile,
}

impl PendingReplacement {
    pub fn new(operation_id: String, commands: Vec<EntryRecord>, vault: VaultFile) -> Self {
        PendingReplacement {
            operation_id,
            started_at: now_rfc3339(),
            commands,
            vault,
        }
    }

    /// Checks the two things a journal must never get wrong: it has to be attributable to one
    /// operation, and its command section must not have been fed secret records (which would
    /// put values into a file whose whole purpose is to restore the *command* half).
    fn validate(&self) -> AppResult<()> {
        if self.operation_id.trim().is_empty() {
            return Err(AppError::Other(
                "the interrupted replacement has no operation id".into(),
            ));
        }
        for record in &self.commands {
            if record.kind != Kind::Command {
                return Err(AppError::Other(
                    "the interrupted replacement journal holds a non-command entry".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Outcome of settling a leftover journal, phrased so it can be shown or logged verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryReport {
    /// The command commit had happened, so the replacement was already complete on disk and
    /// only its bookkeeping was left.
    Completed,
    /// The command commit never happened; both stores are back on the pre-import version.
    RolledBack { commands: usize, secrets: usize },
}

impl RecoveryReport {
    pub fn message(&self) -> String {
        match self {
            RecoveryReport::Completed => {
                "an interrupted library replacement had already committed its command table; \
                 it was completed rather than undone"
                    .to_string()
            }
            RecoveryReport::RolledBack { commands, secrets } => format!(
                "an interrupted library replacement was undone before it could half-apply: \
                 the command table ({commands} entries) and the secret vault ({secrets} \
                 secrets) are back on the version from before it started"
            ),
        }
    }
}

/// The single journal slot for this profile.
pub struct ReplacementJournal {
    dir: PathBuf,
}

impl ReplacementJournal {
    pub fn new(dir: PathBuf) -> AppResult<ReplacementJournal> {
        Ok(ReplacementJournal {
            dir: crate::paths::ensure_dir(&dir)?,
        })
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(JOURNAL_NAME)
    }

    pub fn has_pending(&self) -> bool {
        self.path().exists()
    }

    /// Writes the journal atomically, 0600, sealed, and syncs it plus the directory before
    /// the caller is allowed to touch either store.
    pub fn begin(&self, seal: &dyn Seal, pending: &PendingReplacement) -> AppResult<()> {
        pending.validate()?;
        let bytes = seal.seal(&serde_json::to_vec(pending)?)?;
        crate::backup::write_private_new(&self.path(), &bytes)?;
        sync_dir(&self.dir)
    }

    /// Reads and opens the journal. An unreadable or undecryptable journal is an error: the
    /// caller must keep it and refuse to treat the two stores as trustworthy together.
    pub fn read(&self, seal: &dyn Seal) -> AppResult<Option<PendingReplacement>> {
        let path = self.path();
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(|_| {
            AppError::Other("the interrupted replacement journal could not be read".into())
        })?;
        let pending: PendingReplacement =
            serde_json::from_slice(&seal.open(&bytes)?).map_err(|_| {
                AppError::Other("the interrupted replacement journal is not readable".into())
            })?;
        pending.validate()?;
        Ok(Some(pending))
    }

    /// Removes the journal. Only ever called once the two stores are known to describe the
    /// same complete version.
    pub fn finish(&self) -> AppResult<()> {
        match std::fs::remove_file(self.path()) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(AppError::Other(e.to_string())),
        }
        sync_dir(&self.dir)
    }
}

/// Best-effort directory sync so a rename survives a power loss, not just a process kill.
fn sync_dir(dir: &Path) -> AppResult<()> {
    match std::fs::File::open(dir) {
        Ok(handle) => {
            handle.sync_all().ok();
            Ok(())
        }
        Err(_) => Ok(()),
    }
}

/// A crash point inside a library replacement, used only by the interruption tests. Production
/// code always passes `None`; the point exists so a test can kill the real process between the
/// two writes and prove the next start recovers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyCrash {
    /// Right after the vault write landed, before the command transaction started.
    AfterVaultWrite,
    /// Right after the command transaction committed, before the journal was removed.
    AfterCommandCommit,
}

/// Terminates this process the way a real crash does: no destructors, no cleanup handlers, so
/// nothing can quietly tidy up the half-applied state on the way out.
pub fn abort_process(point: ApplyCrash) -> ! {
    eprintln!("dev-console: simulated crash at {:?}", point);
    std::process::abort();
}
