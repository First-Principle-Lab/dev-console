use std::sync::Arc;

use crate::clipboard::Clipboard;
use crate::error::AppResult;
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
    pub quitting: std::sync::atomic::AtomicBool,
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
            quitting: std::sync::atomic::AtomicBool::new(false),
        };
        // Opens the vault when the keyring is available and no manual lock is set. A manual
        // lock, a locked keyring, or a damaged vault all leave secret features unavailable;
        // commands stay usable in every one of those cases.
        state.vault.bootstrap();
        Ok(Arc::new(state))
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
