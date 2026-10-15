use std::sync::Arc;

use crate::backup::{Archive, ArchiveSeal, BackupManager, RecoveryKey};
use crate::clipboard::Clipboard;
use crate::error::{AppError, AppResult};
use crate::models::{EntryMeta, Kind};
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
    pub quitting: std::sync::atomic::AtomicBool,
}

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
            quitting: std::sync::atomic::AtomicBool::new(false),
        };
        // Opens the vault when the keyring is available and no manual lock is set. A manual
        // lock, a locked keyring, or a damaged vault all leave secret features unavailable;
        // commands stay usable in every one of those cases.
        state.vault.bootstrap();
        Ok(Arc::new(state))
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

    /// Applies an already-validated archive to both stores.
    ///
    /// Order: snapshot, vault (atomic rename), then the command table (one transaction). If
    /// the command table write fails, the vault is rolled back to the snapshot and the error
    /// says only what actually holds. If that rollback fails too, the error says the truth -
    /// the secret store may hold archive content and a known-good backup should be restored -
    /// instead of falsely claiming nothing changed.
    pub fn apply_archive(&self, archive: &Archive) -> AppResult<(usize, usize)> {
        self.apply_archive_with_fault(archive, None)
    }

    pub fn apply_archive_with_fault(
        &self,
        archive: &Archive,
        fault: Option<ApplyFault>,
    ) -> AppResult<(usize, usize)> {
        self.vault.authorise()?;
        let previous_vault = self.vault.snapshot()?;
        if fault == Some(ApplyFault::Vault) {
            return Err(AppError::Other(
                "injected fault: archive not applied".into(),
            ));
        }
        self.vault.restore(archive.secrets.clone())?;
        let commands_result = if matches!(
            fault,
            Some(ApplyFault::Commands) | Some(ApplyFault::VaultRollback)
        ) {
            Err(AppError::Other(
                "injected fault: command table write failed".into(),
            ))
        } else {
            self.repo.replace_all_commands(&archive.commands)
        };
        match commands_result {
            Ok(commands) => Ok((commands, archive.secrets.records.len())),
            Err(error) => {
                let rollback = if fault == Some(ApplyFault::VaultRollback) {
                    Err(AppError::Other("injected fault: rollback failed".into()))
                } else {
                    self.vault.restore(previous_vault)
                };
                match rollback {
                    Ok(()) => Err(AppError::Other(format!(
                        "the archive was not applied ({error}); the command table was never \
                         written and the vault was restored to its previous content"
                    ))),
                    Err(rollback_error) => Err(AppError::Other(format!(
                        "the archive failed ({error}) AND rolling the vault back failed \
                         ({rollback_error}); the command table is unchanged, but the secret \
                         vault may hold the archive's content - restore a known-good backup \
                         from {}",
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
            let state = Arc::new(AppState {
                repo: Repository::open(&base.join("commands.sqlite3")).expect("repo"),
                vault: {
                    let vault = VaultService::new(
                        base.join("vault.age"),
                        base.join("manual.lock"),
                        Credential::Provided { key: key.clone() },
                    );
                    vault.bootstrap();
                    vault
                },
                settings: SettingsStore::load(base.join("settings.json")).expect("settings"),
                clipboard: Clipboard::new(),
                backups: BackupManager::new(base.join("backups"), 4).expect("backups"),
                recovery: RecoveryKey::load_or_create(base.join("recovery.agekey"))
                    .expect("recovery"),
                quitting: std::sync::atomic::AtomicBool::new(false),
            });
            TempState { base, key, state }
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
