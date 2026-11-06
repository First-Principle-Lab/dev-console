use std::sync::Arc;

use crate::backup::{Archive, ArchiveSeal, BackupManager, RecoveryKey};
use crate::clipboard::Clipboard;
use crate::context::FinderContext;
use crate::error::{AppError, AppResult};
use crate::models::{CopyResult, EntryMeta, EntryRecord, Kind};
use crate::recovery::{ApplyCrash, PendingReplacement, RecoveryReport, ReplacementJournal};
use crate::repository::Repository;
use crate::settings::SettingsStore;
use crate::vault::{self as vault, VaultService};

/// The single owner of persistence and vault state. Both windows talk to this through
/// commands; neither window owns the database or the vault.
pub struct AppState {
    pub repo: Repository,
    pub vault: VaultService,
    pub settings: SettingsStore,
    pub clipboard: Clipboard,
    pub backups: BackupManager,
    pub recovery: RecoveryKey,
    /// Durable state for a library replacement in flight; see [`crate::recovery`].
    pub journal: ReplacementJournal,
    pub quitting: std::sync::atomic::AtomicBool,
    /// The project behind the focused terminal, re-detected on every Finder showing and
    /// consulted only while ranking. Never persisted.
    finder_context: std::sync::Mutex<Option<FinderContext>>,
}

impl AppState {
    /// Replaces the project context. Called by the detection thread after every showing
    /// (with `None` when detection found nothing, was disabled, or the previous context is
    /// stale), so what ranking sees always matches the freshest answer.
    pub fn set_finder_context(&self, context: Option<FinderContext>) {
        *self
            .finder_context
            .lock()
            .expect("finder context lock poisoned") = context;
    }

    pub fn finder_context(&self) -> Option<FinderContext> {
        self.finder_context
            .lock()
            .expect("finder context lock poisoned")
            .clone()
    }
}

/// Clipboard owner id for a recovery-credential copy. It identifies *which* copy the app owns
/// without ever carrying the credential itself.
pub const RECOVERY_COPY_ID: &str = "recovery-credential";

/// Stages at which applying an archive can be made to fail on purpose. Only used by the
/// fault-injection tests; production code calls [`AppState::apply_archive`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyFault {
    /// Before anything is written.
    Vault,
    /// After the vault write, instead of the command-table write.
    Commands,
    /// The command-table write fails **and** the compensating vault rollback fails with
    /// it - the only honest-error path.
    VaultRollback,
}

impl AppState {
    pub fn bootstrap() -> AppResult<Arc<AppState>> {
        let db = crate::paths::database_path()?;
        let vault_file = crate::paths::vault_path()?;
        let lock_flag = crate::paths::lock_flag_path()?;
        let settings = crate::paths::settings_path()?;
        let state = AppState {
            repo: Repository::open(&db)?,
            vault: VaultService::new(vault_file, lock_flag, vault::Credential::FromKeyring),
            settings: SettingsStore::load(settings)?,
            clipboard: Clipboard::new(),
            backups: BackupManager::new(crate::paths::backup_dir()?, crate::backup::DEFAULT_KEEP)?,
            recovery: RecoveryKey::load_or_create(crate::paths::recovery_key_path()?)?,
            journal: ReplacementJournal::new(crate::paths::interrupted_apply_dir()?)?,
            quitting: std::sync::atomic::AtomicBool::new(false),
            finder_context: std::sync::Mutex::new(None),
        };
        // A replacement that was interrupted between its two writes is settled here, inside
        // bootstrap. Nothing is exposed until this returns: the caller cannot manage the state,
        // build a window or serve the CLI socket before it, so no window can read a library
        // whose commands and secrets belong to different versions.
        let unreconciled = match state.reconcile_interrupted_replacement() {
            Ok(Some(report)) => {
                eprintln!("dev-console: {}", report.message());
                None
            }
            Ok(None) => None,
            Err(error) => {
                eprintln!(
                    "dev-console: an interrupted library replacement could not be reconciled \
                     ({error})"
                );
                Some(error.to_string())
            }
        };
        // Opens the vault when the keyring is available and no manual lock is set. A manual
        // lock, a locked keyring, or a damaged vault all leave secret features unavailable;
        // commands stay usable in every one of those cases.
        state.vault.bootstrap();
        if let Some(reason) = unreconciled {
            // Fail closed: the journal is kept, commands stay usable, and every secret path
            // refuses until the reconciliation can run again (it is retried on unlock).
            state.vault.quarantine(&reason);
        }
        Ok(Arc::new(state))
    }

    /// Settles a leftover replacement journal, if there is one. The decision is made from the
    /// database marker that the replacement wrote inside its own transaction, never by
    /// comparing contents:
    ///
    /// * marker matches the journal's operation id - the command transaction committed, and
    ///   since the vault write precedes it both stores already hold the archive's version, so
    ///   the replacement is completed (only the bookkeeping was missing),
    /// * anything else - the transaction never committed, so both stores go back to the
    ///   version recorded in the journal, which is a complete version that was live moments
    ///   before.
    ///
    /// An undecryptable or damaged journal is returned as an error and deliberately left in
    /// place; the caller must then keep the secret store shut rather than guess.
    pub fn reconcile_interrupted_replacement(&self) -> AppResult<Option<RecoveryReport>> {
        if !self.journal.has_pending() {
            return Ok(None);
        }
        let seal = ArchiveSeal::new(&self.vault, &self.recovery)?;
        let Some(pending) = self.journal.read(&seal)? else {
            return Ok(None);
        };
        let marker = self.repo.apply_operation_marker()?;
        if marker.as_deref() == Some(pending.operation_id.as_str()) {
            self.journal.finish()?;
            self.repo.clear_apply_operation_marker()?;
            return Ok(Some(RecoveryReport::Completed));
        }
        let secrets = pending.vault.records.len();
        // Deliberately not gated on `authorise()`: this is the backend repairing its own two
        // stores, and it has to be able to do that even while the user's manual lock is set.
        self.vault.restore(pending.vault)?;
        let commands = self.repo.replace_all_commands(&pending.commands)?;
        self.journal.finish()?;
        self.repo.clear_apply_operation_marker()?;
        Ok(Some(RecoveryReport::RolledBack { commands, secrets }))
    }

    /// Collects everything into an archive. Requires an unlocked vault: a backup taken while
    /// the vault is closed would look like a successful backup while silently containing no
    /// secrets at all.
    pub fn build_archive(&self) -> AppResult<Archive> {
        self.vault.authorise()?;
        let commands = self
            .repo
            .all_command_records()?
            .into_iter()
            .map(|(record, _, _)| record)
            .collect();
        Ok(Archive::new(commands, self.vault.snapshot()?))
    }

    /// The seal every archive goes through: vault identity + recovery credential.
    pub fn archive_seal(&self) -> AppResult<ArchiveSeal> {
        ArchiveSeal::new(&self.vault, &self.recovery)
    }

    /// The recovery credential, behind the same vault authorisation as every other secret
    /// operation. It decrypts every archive this profile has ever sealed, so a locked vault
    /// must not be able to hand it out - the window check alone is not enough.
    pub fn recovery_credential(&self) -> AppResult<(&str, &std::path::Path)> {
        self.vault.authorise()?;
        Ok((self.recovery.secret(), self.recovery.path()))
    }

    /// Copies the recovery credential through the native clipboard path used for saved
    /// secrets: the configured expiry applies, the owned copy is cleared when the vault locks,
    /// and the value itself is never returned to the caller, so nothing can log it or show it
    /// in a notification.
    pub fn copy_recovery_credential(&self) -> AppResult<CopyResult> {
        let (secret, _) = self.recovery_credential()?;
        let expiry = self.settings.get().clipboard_expiry_seconds;
        self.clipboard
            .copy(Kind::Secret, RECOVERY_COPY_ID, secret, expiry)
    }

    /// Applies an already-validated archive to both stores.
    ///
    /// Order: journal (durable), vault, command table (one transaction carrying the journal's
    /// operation id), then the journal is removed. If the command table write fails, the vault
    /// is rolled back to the snapshot and the error says only what actually holds. If that
    /// rollback fails too, the journal is deliberately left behind so the next start repairs
    /// the pair, and the error says the truth instead of falsely claiming nothing changed.
    pub fn apply_archive(&self, archive: &Archive) -> AppResult<(usize, usize)> {
        self.apply_archive_inner(archive, None, None)
    }

    pub fn apply_archive_with_fault(
        &self,
        archive: &Archive,
        fault: Option<ApplyFault>,
    ) -> AppResult<(usize, usize)> {
        self.apply_archive_inner(archive, fault, None)
    }

    /// Test-only entry point that kills the process at a real replacement boundary, so the
    /// recovery path can be proven against an actually interrupted run.
    pub fn apply_archive_with_crash(
        &self,
        archive: &Archive,
        point: ApplyCrash,
    ) -> AppResult<(usize, usize)> {
        self.apply_archive_inner(archive, None, Some(point))
    }

    fn apply_archive_inner(
        &self,
        archive: &Archive,
        fault: Option<ApplyFault>,
        crash: Option<ApplyCrash>,
    ) -> AppResult<(usize, usize)> {
        self.vault.authorise()?;
        if self.journal.has_pending() {
            return Err(AppError::Other(
                "a previous library replacement has not been reconciled yet; Dev Console settles \
                 it at the next start"
                    .into(),
            ));
        }
        let previous_vault = self.vault.snapshot()?;
        let previous_commands: Vec<EntryRecord> = self
            .repo
            .all_command_records()?
            .into_iter()
            .map(|(record, _, _)| record)
            .collect();
        if fault == Some(ApplyFault::Vault) {
            return Err(AppError::Other(
                "injected fault: archive not applied".into(),
            ));
        }
        let operation_id = uuid::Uuid::new_v4().simple().to_string();
        let pending = PendingReplacement::new(
            operation_id.clone(),
            previous_commands,
            previous_vault.clone(),
        );
        let seal = self.archive_seal()?;
        // Durably recorded before either store moves: this is what makes a killed process
        // recoverable instead of guesswork.
        self.journal.begin(&seal, &pending)?;
        if let Err(error) = self.vault.restore(archive.secrets.clone()) {
            // Nothing moved, so the journal is already stale. Removing it is best effort: if
            // that fails too the next start settles a replacement that never happened.
            if let Err(journal_error) = self.journal.finish() {
                eprintln!(
                    "dev-console: the archive was not applied ({error}) and its journal could not \
                     be removed either ({journal_error}); the next start reconciles it"
                );
            }
            return Err(error);
        }
        if crash == Some(ApplyCrash::AfterVaultWrite) {
            crate::recovery::abort_process(ApplyCrash::AfterVaultWrite);
        }
        let commands_result = if matches!(
            fault,
            Some(ApplyFault::Commands) | Some(ApplyFault::VaultRollback)
        ) {
            Err(AppError::Other(
                "injected fault: command table write failed".into(),
            ))
        } else {
            self.repo
                .replace_all_commands_marked(&archive.commands, Some(&operation_id))
        };
        if crash == Some(ApplyCrash::AfterCommandCommit) && commands_result.is_ok() {
            crate::recovery::abort_process(ApplyCrash::AfterCommandCommit);
        }
        match commands_result {
            Ok(commands) => {
                // Journal first, marker second: a marker that outlives its journal is inert,
                // while a journal without its marker would make the next start undo a
                // replacement that actually completed. Neither bookkeeping failure is a failed
                // import - both stores already hold the archive - so it is reported honestly
                // and left for the next start, which recognises the matching marker.
                match self.journal.finish() {
                    Ok(()) => {
                        if let Err(error) = self.repo.clear_apply_operation_marker() {
                            eprintln!(
                                "dev-console: the archive was applied but its marker could not \
                                 be cleared ({error}); the next start finishes it"
                            );
                        }
                    }
                    Err(error) => eprintln!(
                        "dev-console: the archive was applied, but its journal could not be \
                         removed ({error}); the next start completes the bookkeeping"
                    ),
                }
                Ok((commands, archive.secrets.records.len()))
            }
            Err(error) => {
                let rollback = if fault == Some(ApplyFault::VaultRollback) {
                    Err(AppError::Other("injected fault: rollback failed".into()))
                } else {
                    self.vault.restore(previous_vault)
                };
                match rollback {
                    Ok(()) => {
                        let note = match self.journal.finish() {
                            Ok(()) => String::new(),
                            Err(journal_error) => format!(
                                "; its recovery journal could not be removed ({journal_error}), \
                                 so the next start will settle it"
                            ),
                        };
                        Err(AppError::Other(format!(
                            "the archive was not applied ({error}); the command table was never \
                             written and the vault was restored to its previous content{note}"
                        )))
                    }
                    Err(rollback_error) => Err(AppError::Other(format!(
                        "the archive failed ({error}) AND rolling the vault back failed \
                         ({rollback_error}); the command table is unchanged, but the secret \
                         vault may hold the archive's content - Dev Console will reconcile the \
                         two stores at the next start, or restore a known-good backup from {}",
                        self.backups.dir().display()
                    ))),
                }
            }
        }
    }

    /// Metadata only. Secret metadata is included solely while the vault is unlocked, and a
    /// locked vault contributes nothing at all.
    pub fn searchable_metadata(&self, kind: Option<Kind>) -> Vec<EntryMeta> {
        let mut entries: Vec<EntryMeta> = Vec::new();
        if kind.is_none() || kind == Some(Kind::Command) {
            if let Ok(commands) = self.repo.list_commands("") {
                entries.extend(commands);
            }
        }
        if kind.is_none() || kind == Some(Kind::Secret) {
            if let Ok(secrets) = self.vault.list_secrets() {
                entries.extend(secrets);
            }
        }
        // Belt and braces on the "working directories are a command-only concept" rule:
        // ranking consults the field for every entry that reaches it.
        for entry in &mut entries {
            if entry.kind == Kind::Secret {
                entry.working_directory.clear();
            }
        }
        entries
    }

    pub fn types(&self, kind: Kind) -> Vec<String> {
        let mut types = self.repo.types(kind).unwrap_or_default();
        if kind == Kind::Secret {
            if let Ok(mut secret_types) = self.vault.types() {
                types.append(&mut secret_types);
            }
        }
        types.sort_by_key(|a| a.to_lowercase());
        types.dedup();
        types
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EntryDraft, ExtraField};
    use crate::vault::{Credential, VaultService};

    struct TempState {
        base: std::path::PathBuf,
        key: String,
        state: Arc<AppState>,
    }

    impl TempState {
        fn new(tag: &str) -> TempState {
            let base = std::env::temp_dir().join(format!(
                "dev-console-state-{tag}-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&base).expect("temp base");
            let (identity, key) = crate::vault::generate_identity();
            drop(identity);
            let state = Arc::new(TempState::build_state(&base, &key));
            state.vault.bootstrap();
            TempState { base, key, state }
        }

        /// Exactly the fields a real process owns for one profile, with nothing opened yet.
        fn build_state(base: &std::path::Path, key: &str) -> AppState {
            AppState {
                repo: Repository::open(&base.join("commands.sqlite3")).expect("repo"),
                vault: VaultService::new(
                    base.join("vault.age"),
                    base.join("manual.lock"),
                    Credential::Provided {
                        key: key.to_string(),
                    },
                ),
                settings: SettingsStore::load(base.join("settings.json")).expect("settings"),
                clipboard: Clipboard::new(),
                backups: BackupManager::new(base.join("backups"), 4).expect("backups"),
                recovery: RecoveryKey::load_or_create(base.join("recovery.agekey"))
                    .expect("recovery"),
                journal: ReplacementJournal::new(base.join("pending-replace")).expect("journal"),
                quitting: std::sync::atomic::AtomicBool::new(false),
                finder_context: std::sync::Mutex::new(None),
            }
        }

        /// A brand new process over the same files: the startup reconciliation runs in the
        /// same position it has in `AppState::bootstrap`, before the vault is opened.
        fn restart(&self) -> (Arc<AppState>, Option<RecoveryReport>) {
            let state = Arc::new(TempState::build_state(&self.base, &self.key));
            let report = state
                .reconcile_interrupted_replacement()
                .expect("the journal left behind must be decidable");
            state.vault.bootstrap();
            (state, report)
        }

        /// A second service over the same files, i.e. a restart reading from disk.
        fn reopen_vault(&self) -> VaultService {
            let vault = VaultService::new(
                self.base.join("vault.age"),
                self.base.join("manual.lock"),
                Credential::Provided {
                    key: self.key.clone(),
                },
            );
            vault.bootstrap();
            vault
        }

        fn erase(&self) {
            std::fs::remove_dir_all(&self.base).ok();
        }
    }

    fn command(id: &str, name: &str) -> crate::models::EntryRecord {
        crate::models::EntryRecord {
            id: id.into(),
            revision: 1,
            kind: Kind::Command,
            name: name.into(),
            entry_type: "Shell".into(),
            tags: vec![],
            description: String::new(),
            notes: String::new(),
            fields: vec![],
            pinned: false,
            command: "echo hi".into(),
            working_directory: String::new(),
            value: String::new(),
            created_at: crate::repository::now_rfc3339(),
            updated_at: crate::repository::now_rfc3339(),
        }
    }

    fn archive(tag: &str) -> Archive {
        let mut draft = EntryDraft {
            kind: Kind::Secret,
            id: Some(format!("secret-{tag}")),
            expected_revision: 0,
            name: format!("token-{tag}"),
            entry_type: "API key".into(),
            tags: vec![],
            description: String::new(),
            notes: format!("notes-{tag}"),
            fields: vec![ExtraField {
                id: "f".into(),
                label: "L".into(),
                value: format!("field-{tag}"),
                display: crate::models::FieldDisplay::Text,
            }],
            pinned: false,
            command: String::new(),
            working_directory: String::new(),
            value: format!("value-{tag}"),
        };
        // Build the VaultFile by going through the vault API on a scratch service is overkill;
        // the snapshot shape is plain data.
        let secrets = crate::vault::VaultFile {
            version: crate::vault::VAULT_VERSION,
            records: vec![{
                crate::models::EntryRecord {
                    id: draft.id.take().unwrap(),
                    revision: 1,
                    kind: Kind::Secret,
                    name: draft.name.clone(),
                    entry_type: draft.entry_type.clone(),
                    tags: draft.tags.clone(),
                    description: draft.description.clone(),
                    notes: draft.notes.clone(),
                    fields: draft.fields.clone(),
                    pinned: false,
                    command: String::new(),
                    working_directory: String::new(),
                    value: draft.value.clone(),
                    created_at: crate::repository::now_rfc3339(),
                    updated_at: crate::repository::now_rfc3339(),
                }
            }],
            usage: vec![],
        };
        Archive::new(
            vec![command(&format!("cmd-{tag}"), &format!("cmd-{tag}"))],
            secrets,
        )
    }

    fn seed(t: &TempState) {
        t.state
            .repo
            .replace_all_commands(&[command("cmd-prev", "cmd-prev")])
            .expect("seed commands");
        t.state
            .vault
            .restore(archive("prev").secrets)
            .expect("seed vault");
    }

    #[test]
    fn a_full_apply_updates_both_stores() {
        let t = TempState::new("apply-ok");
        seed(&t);
        let next = archive("next");
        let (commands, secrets) = t.state.apply_archive(&next).expect("apply");
        assert_eq!((commands, secrets), (1, 1));
        assert_eq!(
            t.state.repo.all_command_records().expect("cmds")[0].0.name,
            "cmd-next"
        );
        assert_eq!(
            t.reopen_vault().list_secrets().expect("secrets")[0].name,
            "token-next"
        );
        t.erase();
    }

    #[test]
    fn a_fault_before_any_write_changes_nothing() {
        let t = TempState::new("apply-fault-vault");
        seed(&t);
        let error = t
            .state
            .apply_archive_with_fault(&archive("next"), Some(ApplyFault::Vault))
            .expect_err("injected fault");
        assert!(error.to_string().contains("not applied"));
        assert_eq!(
            t.state.repo.all_command_records().expect("cmds")[0].0.name,
            "cmd-prev"
        );
        assert_eq!(
            t.reopen_vault().list_secrets().expect("secrets")[0].name,
            "token-prev"
        );
        t.erase();
    }

    #[test]
    fn a_command_table_fault_rolls_the_vault_back_on_disk_and_in_memory() {
        let t = TempState::new("apply-fault-commands");
        seed(&t);
        let error = t
            .state
            .apply_archive_with_fault(&archive("next"), Some(ApplyFault::Commands))
            .expect_err("injected fault");
        let message = error.to_string();
        // The claim is only made because this test proves both halves of it.
        assert!(
            message.contains("vault was restored to its previous content"),
            "{message}"
        );
        assert!(
            !message.contains("may hold"),
            "must not warn of divergence after a clean rollback"
        );
        // Command store: the write never committed.
        let commands = t.state.repo.all_command_records().expect("cmds");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].0.name, "cmd-prev");
        // Vault: memory AND disk are back to the previous content.
        assert_eq!(
            t.state.vault.list_secrets().expect("mem")[0].name,
            "token-prev"
        );
        let on_disk = t.reopen_vault();
        let secrets = on_disk.list_secrets().expect("disk");
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].name, "token-prev");
        t.erase();
    }

    /// Leaves exactly the on-disk state a killed process leaves: a sealed journal describing
    /// the version being replaced, the vault already on the archive, and the command
    /// transaction either committed (`commit_commands`) or never committed.
    fn leave_journal(t: &TempState, incoming: &Archive, commit_commands: bool) -> String {
        let state = &t.state;
        let operation_id = uuid::Uuid::new_v4().simple().to_string();
        let previous_commands: Vec<EntryRecord> = state
            .repo
            .all_command_records()
            .expect("cmds")
            .into_iter()
            .map(|(record, _, _)| record)
            .collect();
        let pending = PendingReplacement::new(
            operation_id.clone(),
            previous_commands,
            state.vault.snapshot().expect("snapshot"),
        );
        let seal = state.archive_seal().expect("seal");
        state.journal.begin(&seal, &pending).expect("journal");
        state
            .vault
            .restore(incoming.secrets.clone())
            .expect("vault write landed");
        if commit_commands {
            state
                .repo
                .replace_all_commands_marked(&incoming.commands, Some(&operation_id))
                .expect("command transaction committed");
        }
        operation_id
    }

    fn command_names(state: &AppState) -> Vec<String> {
        state
            .repo
            .list_commands("")
            .expect("cmds")
            .into_iter()
            .map(|entry| entry.name)
            .collect()
    }

    fn secret_names(state: &AppState) -> Vec<String> {
        state
            .vault
            .list_secrets()
            .expect("secrets")
            .into_iter()
            .map(|entry| entry.name)
            .collect()
    }

    #[test]
    fn a_replacement_killed_before_the_command_commit_is_undone_on_the_next_start() {
        let t = TempState::new("apply-crash-before-commit");
        seed(&t);
        leave_journal(&t, &archive("next"), false);
        // The half-applied state the crash actually leaves behind.
        assert_eq!(command_names(&t.state), vec!["cmd-prev".to_string()]);
        assert_eq!(secret_names(&t.state), vec!["token-next".to_string()]);

        let (after, report) = t.restart();
        assert_eq!(
            report,
            Some(RecoveryReport::RolledBack {
                commands: 1,
                secrets: 1
            })
        );
        // Commands and secrets describe one complete version again: the pre-import one.
        assert_eq!(command_names(&after), vec!["cmd-prev".to_string()]);
        assert_eq!(secret_names(&after), vec!["token-prev".to_string()]);
        assert_eq!(
            t.reopen_vault().list_secrets().expect("disk")[0].name,
            "token-prev",
            "the repair reached the vault file, not just memory"
        );
        assert!(!after.journal.has_pending(), "the journal is retired");
        assert_eq!(after.repo.apply_operation_marker().expect("marker"), None);
        t.erase();
    }

    #[test]
    fn a_replacement_whose_commit_landed_is_completed_instead_of_undone() {
        let t = TempState::new("apply-crash-after-commit");
        seed(&t);
        let operation_id = leave_journal(&t, &archive("next"), true);
        assert_eq!(
            t.state.repo.apply_operation_marker().expect("m"),
            Some(operation_id)
        );

        let (after, report) = t.restart();
        assert_eq!(report, Some(RecoveryReport::Completed));
        assert_eq!(command_names(&after), vec!["cmd-next".to_string()]);
        assert_eq!(secret_names(&after), vec!["token-next".to_string()]);
        assert!(!after.journal.has_pending());
        assert_eq!(after.repo.apply_operation_marker().expect("marker"), None);
        t.erase();
    }

    #[test]
    fn a_journal_left_by_a_failed_rollback_repairs_the_pair_on_the_next_start() {
        let t = TempState::new("apply-rollback-then-recover");
        seed(&t);
        let error = t
            .state
            .apply_archive_with_fault(&archive("next"), Some(ApplyFault::VaultRollback))
            .expect_err("injected fault");
        assert!(error.to_string().contains("reconcile"), "{error}");
        assert!(
            t.state.journal.has_pending(),
            "a rollback this incomplete must leave its journal behind"
        );
        // The vault may hold the archive while the command table does not: unreconciled.
        assert_eq!(secret_names(&t.state), vec!["token-next".to_string()]);
        assert_eq!(command_names(&t.state), vec!["cmd-prev".to_string()]);

        let (after, report) = t.restart();
        assert_eq!(
            report,
            Some(RecoveryReport::RolledBack {
                commands: 1,
                secrets: 1
            })
        );
        assert_eq!(command_names(&after), vec!["cmd-prev".to_string()]);
        assert_eq!(secret_names(&after), vec!["token-prev".to_string()]);
        t.erase();
    }

    #[test]
    fn a_pending_replacement_blocks_the_next_one() {
        let t = TempState::new("apply-journal-busy");
        seed(&t);
        leave_journal(&t, &archive("next"), false);
        let error = t
            .state
            .apply_archive(&archive("other"))
            .expect_err("two replacements cannot overlap");
        assert!(error.to_string().contains("reconciled"), "{error}");
        t.erase();
    }

    #[test]
    fn the_recovery_journal_is_sealed_and_private() {
        use std::os::unix::fs::PermissionsExt;
        let t = TempState::new("apply-journal-privacy");
        seed(&t);
        leave_journal(&t, &archive("next"), false);
        let path = t.state.journal.path();
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "the journal file is user-only");
        let dir_mode = std::fs::metadata(path.parent().expect("dir"))
            .expect("dir meta")
            .permissions()
            .mode();
        assert_eq!(
            dir_mode & 0o777,
            0o700,
            "the journal directory is user-only"
        );
        let bytes = std::fs::read(&path).expect("journal bytes");
        assert!(
            bytes.starts_with(b"age"),
            "the journal is an age container, not JSON on disk"
        );
        for marker in [
            "token-prev",
            "value-prev",
            "notes-prev",
            "field-prev",
            "cmd-prev",
            "echo hi",
        ] {
            assert!(
                !bytes.windows(marker.len()).any(|w| w == marker.as_bytes()),
                "plaintext marker {marker:?} leaked into the recovery journal"
            );
        }
        t.erase();
    }

    #[test]
    fn the_recovery_marker_commits_in_the_same_transaction_as_its_rows() {
        let t = TempState::new("apply-marker-atomicity");
        seed(&t);
        let mut first = command("cmd-dup", "cmd-dup");
        let second = command("cmd-dup", "cmd-dup-renamed");
        first.updated_at = "not-a-timestamp".into();
        // The second insert collides on the primary key, so the whole transaction, marker
        // included, has to be rolled back.
        let error = t
            .state
            .repo
            .replace_all_commands_marked(&[first, second], Some("op-atomic"))
            .expect_err("duplicate id must fail");
        assert!(matches!(error, AppError::Storage(_) | AppError::Other(_)));
        assert_eq!(t.state.repo.apply_operation_marker().expect("m"), None);
        assert_eq!(command_names(&t.state), vec!["cmd-prev".to_string()]);
        // A successful marked replacement is what the startup recovery then compares against.
        t.state
            .repo
            .replace_all_commands_marked(&[command("cmd-marked", "cmd-marked")], Some("op-ok"))
            .expect("marked write");
        assert_eq!(
            t.state.repo.apply_operation_marker().expect("m"),
            Some("op-ok".to_string())
        );
        t.state
            .repo
            .clear_apply_operation_marker()
            .expect("clear marker");
        assert_eq!(t.state.repo.apply_operation_marker().expect("m"), None);
        t.erase();
    }
    #[test]
    fn a_failed_rollback_is_reported_as_divergence_not_as_intact() {
        let t = TempState::new("apply-fault-rollback");
        seed(&t);
        let error = t
            .state
            .apply_archive_with_fault(&archive("next"), Some(ApplyFault::VaultRollback))
            .expect_err("injected fault");
        let message = error.to_string();
        assert!(
            message.contains("rolling the vault back failed"),
            "{message}"
        );
        assert!(message.contains("known-good backup"), "{message}");
        assert!(
            !message.contains("previous data is intact"),
            "the honest error must never claim intactness it could not achieve"
        );
        // The command table (transactional) is still the previous one.
        assert_eq!(
            t.state.repo.all_command_records().expect("cmds")[0].0.name,
            "cmd-prev"
        );
        t.erase();
    }
}
