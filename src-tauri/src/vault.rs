//! Encrypted secret vault.
//!
//! Format: the complete secret set (records plus usage) is serialised to JSON and encrypted
//! with the `age` crate (X25519 recipient, audited AEAD file format) into a single file,
//! written atomically with user-only permissions. There is no plaintext mirror anywhere:
//! decrypted records exist only in this process, only while the vault is unlocked.
//!
//! The vault key is a randomly generated `age` identity kept in the Linux Secret Service
//! (see [crate::keyring]). If that provider is missing or locked, secret features report
//! themselves unavailable; the app never falls back to plaintext storage and never
//! regenerates a key over an existing vault.
//!
//! Manual locking is persistent: the flag file survives finder opens, app restarts and
//! login, and clearing it requires real system authentication through [crate::auth].

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use age::secrecy::ExposeSecret;
use age::x25519::Identity;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::models::{EntryDraft, EntryMeta, EntryRecord, Kind, SavedEntry, VaultState};
use crate::repository::validate_draft;
use crate::settings::atomic_write;
use crate::{auth, keyring};

pub const VAULT_VERSION: u32 = 1;

/// Where the vault key comes from.
#[derive(Debug, Clone)]
pub enum Credential {
    /// Production path: read (and on first run create) the key in the user's keyring.
    FromKeyring,
    /// Explicit key. Used by tests and by the restore tool only; the application startup
    /// path always uses [`Credential::FromKeyring`], and there is no automatic fallback.
    Provided { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: String,
    pub count: i64,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultFile {
    pub version: u32,
    pub records: Vec<EntryRecord>,
    pub usage: Vec<UsageRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    Unlocked,
    Locked { manual: bool },
    Unavailable(String),
}

/// Encrypts to several recipients at once. Backups use this so the machine keyring and a
/// separately held recovery key can both open the same archive.
pub fn encrypt_to(identities: &[&Identity], plaintext: &[u8]) -> AppResult<Vec<u8>> {
    let recipients: Vec<age::x25519::Recipient> =
        identities.iter().map(|i| i.to_public()).collect();
    let refs: Vec<&dyn age::Recipient> = recipients
        .iter()
        .map(|r| r as &dyn age::Recipient)
        .collect();
    let encryptor = age::Encryptor::with_recipients(refs.into_iter())
        .map_err(|e| AppError::Validation(format!("could not start encryption: {e}")))?;
    use std::io::Write;
    let mut output = Vec::new();
    let mut writer = encryptor
        .wrap_output(&mut output)
        .map_err(|e| AppError::Other(e.to_string()))?;
    writer
        .write_all(plaintext)
        .map_err(|e| AppError::Other(e.to_string()))?;
    writer
        .finish()
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(output)
}

/// Decrypts with any one identity. Returns a typed error for a wrong key or damaged input.
pub fn decrypt_with(identity: &Identity, ciphertext: &[u8]) -> AppResult<Vec<u8>> {
    age::decrypt(identity, ciphertext).map_err(|e| {
        AppError::VaultUnavailable(format!(
            "the vault could not be decrypted ({e}); the stored key does not match this file, \
             or the file is damaged"
        ))
    })
}

pub fn parse_identity(secret: &str) -> AppResult<Identity> {
    Identity::from_str(secret)
        .map_err(|_| AppError::VaultUnavailable("the stored vault key is not valid".into()))
}

pub fn generate_identity() -> (Identity, String) {
    let identity = Identity::generate();
    let secret = identity.to_string().expose_secret().to_string();
    (identity, secret)
}

pub struct VaultService {
    path: PathBuf,
    lock_flag: PathBuf,
    credential: Credential,
    file: Mutex<VaultFile>,
    status: Mutex<Status>,
    manual_lock: AtomicBool,
    generation: AtomicU64,
}

impl VaultService {
    pub fn new(path: PathBuf, lock_flag: PathBuf, credential: Credential) -> VaultService {
        let manual = lock_flag.exists();
        VaultService {
            path,
            lock_flag,
            credential,
            file: Mutex::new(VaultFile::default()),
            status: Mutex::new(Status::Locked { manual }),
            manual_lock: AtomicBool::new(manual),
            generation: AtomicU64::new(1),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn is_unlocked(&self) -> bool {
        matches!(
            &*self.status.lock().expect("vault status lock poisoned"),
            Status::Unlocked
        )
    }

    pub fn manual_lock(&self) -> bool {
        self.manual_lock.load(Ordering::SeqCst)
    }

    fn set_status(&self, status: Status) {
        *self.status.lock().expect("vault status lock poisoned") = status;
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    fn status(&self) -> Status {
        self.status
            .lock()
            .expect("vault status lock poisoned")
            .clone()
    }

    /// Startup path. A persistent manual lock means the vault is *not* opened even though the
    /// keyring is available; nothing here can silently undo it.
    pub fn bootstrap(&self) {
        if self.manual_lock() {
            self.clear_decrypted();
            self.set_status(Status::Locked { manual: true });
            return;
        }
        match self.open() {
            Ok(()) => self.set_status(Status::Unlocked),
            Err(error) => {
                self.clear_decrypted();
                self.set_status(Status::Unavailable(error.to_string()));
            }
        }
    }

    /// Reads (or on first run creates) the key, then loads and decrypts the vault.
    fn open(&self) -> AppResult<()> {
        let loaded = self.open_staged()?;
        *self.file.lock().expect("vault file lock poisoned") = loaded;
        Ok(())
    }

    /// Decrypts into a value without touching the live state, so a failed open can never leave
    /// half-loaded secrets behind or release a manual lock.
    fn open_staged(&self) -> AppResult<VaultFile> {
        let identity = self.resolve_identity()?;
        let loaded = if self.path.exists() {
            let bytes = std::fs::read(&self.path).map_err(|e| AppError::Other(e.to_string()))?;
            let plaintext = decrypt_with(&identity, &bytes)?;
            let file: VaultFile = serde_json::from_slice(&plaintext)
                .map_err(|_| AppError::VaultUnavailable("the vault file is damaged".into()))?;
            if file.version > VAULT_VERSION {
                return Err(AppError::VaultUnavailable(
                    "the vault was written by a newer version of Dev Console".into(),
                ));
            }
            file
        } else {
            let empty = VaultFile {
                version: VAULT_VERSION,
                records: Vec::new(),
                usage: Vec::new(),
            };
            self.persist(&empty)?;
            empty
        };
        Ok(loaded)
    }

    fn resolve_identity(&self) -> AppResult<Identity> {
        match &self.credential {
            Credential::Provided { key } => parse_identity(key),
            Credential::FromKeyring => {
                match keyring::read_key().map_err(AppError::VaultUnavailable)? {
                    Some(secret) => parse_identity(&secret),
                    None => {
                        // First run: create a random credential, store it, and only then create
                        // the vault. If storing fails we never create an unreachable vault.
                        let (identity, secret) = generate_identity();
                        keyring::write_key(&secret).map_err(AppError::VaultUnavailable)?;
                        Ok(identity)
                    }
                }
            }
        }
    }

    fn persist(&self, file: &VaultFile) -> AppResult<()> {
        let json = serde_json::to_vec(&VaultFile {
            version: VAULT_VERSION,
            ..file.clone()
        })?;
        let identity = self.resolve_identity()?;
        let ciphertext = encrypt_to(&[&identity], &json)?;
        // Atomic: a crash or a full disk can never leave a truncated or empty vault behind.
        atomic_write(&self.path, &ciphertext)
    }

    /// Re-checked for every secret operation, including requests queued before a lock.
    pub fn authorise(&self) -> AppResult<()> {
        if self.manual_lock() {
            return Err(AppError::Locked);
        }
        match self.status() {
            Status::Unlocked => Ok(()),
            Status::Locked { .. } => Err(AppError::Locked),
            Status::Unavailable(reason) => Err(AppError::VaultUnavailable(reason)),
        }
    }

    pub fn read<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&VaultFile) -> AppResult<T>,
    {
        self.authorise()?;
        let file = self.file.lock().expect("vault file lock poisoned");
        f(&file)
    }

    /// Applies a mutation, persists it, and only then makes it visible. A failed write leaves
    /// both the file and the in-memory state on the previous revision.
    pub fn write<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&mut VaultFile) -> AppResult<T>,
    {
        self.authorise()?;
        let mut guard = self.file.lock().expect("vault file lock poisoned");
        let mut candidate = guard.clone();
        let result = f(&mut candidate)?;
        self.persist(&candidate)?;
        *guard = candidate;
        self.generation.fetch_add(1, Ordering::SeqCst);
        Ok(result)
    }

    /// Shuts the secret store without touching the persistent manual-lock flag. Used when an
    /// interrupted library replacement could not be reconciled: neither window may receive
    /// secrets that could belong to a version the command table does not match.
    pub fn quarantine(&self, reason: &str) {
        self.clear_decrypted();
        self.set_status(Status::Unavailable(reason.to_string()));
    }

    pub fn clear_decrypted(&self) {
        self.file
            .lock()
            .expect("vault file lock poisoned")
            .records
            .clear();
        self.file
            .lock()
            .expect("vault file lock poisoned")
            .usage
            .clear();
    }

    /// Manual lock: persists the flag, drops decrypted state and advances the generation so
    /// in-flight responses carrying secret metadata are discarded by both windows.
    pub fn lock(&self) -> AppResult<()> {
        self.set_manual_lock(true)
    }

    pub fn set_manual_lock(&self, locked: bool) -> AppResult<()> {
        if locked {
            if !self.lock_flag.exists() {
                if let Some(parent) = self.lock_flag.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::write(&self.lock_flag, b"locked\n")
                    .map_err(|e| AppError::Other(e.to_string()))?;
                crate::paths::set_file_private(&self.lock_flag)?;
            }
            self.manual_lock.store(true, Ordering::SeqCst);
            self.clear_decrypted();
            self.set_status(Status::Locked { manual: true });
            Ok(())
        } else {
            if self.lock_flag.exists() {
                std::fs::remove_file(&self.lock_flag)
                    .map_err(|e| AppError::Other(e.to_string()))?;
            }
            self.manual_lock.store(false, Ordering::SeqCst);
            Ok(())
        }
    }

    /// Explicit unlock. A manual lock is only cleared by verified system authentication;
    /// reading the credential from an already-unlocked keyring is never enough.
    pub fn unlock(&self) -> AppResult<()> {
        self.unlock_with(|| {
            if !auth::mechanism_available() {
                return Err(AppError::VaultUnavailable(
                    auth::unavailable_reason().into(),
                ));
            }
            auth::authenticate(
                "Dev Console is asking you to confirm your identity to unlock the secret vault.",
            )
        })
    }

    /// Ordering matters and is deliberately strict:
    ///
    /// 1. authentication must succeed,
    /// 2. the vault must then actually open and decrypt,
    /// 3. only after that may the persistent manual-lock flag be removed.
    ///
    /// If decryption, keyring access or the flag update fails, both the flag file and the
    /// in-memory lock stay exactly as they were, so a failed unlock can never silently
    /// downgrade a manual lock into an auto-unlocking vault.
    pub fn unlock_with<F>(&self, authenticate: F) -> AppResult<()>
    where
        F: FnOnce() -> AppResult<()>,
    {
        let was_manual = self.manual_lock();
        if was_manual {
            authenticate()?;
        }

        let staged = match self.open_staged() {
            Ok(file) => file,
            Err(error) => {
                self.clear_decrypted();
                self.set_status(if was_manual {
                    Status::Locked { manual: true }
                } else {
                    Status::Unavailable(error.to_string())
                });
                return Err(error);
            }
        };

        if was_manual {
            if let Err(error) = self.set_manual_lock(false) {
                // The vault opened but the flag could not be cleared: stay locked rather than
                // run unlocked against a flag that still says otherwise.
                self.clear_decrypted();
                self.set_status(Status::Locked { manual: true });
                return Err(error);
            }
        }

        *self.file.lock().expect("vault file lock poisoned") = staged;
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.set_status(Status::Unlocked);
        Ok(())
    }

    pub fn state(&self) -> VaultState {
        let keyring_available = keyring::is_available();
        match self.status() {
            Status::Unlocked => VaultState {
                unlocked: true,
                manual_lock: self.manual_lock(),
                keyring_available,
                keyring_unlocked: true,
                vault_available: true,
                generation: self.generation(),
                authentication_required: false,
                reason: None,
            },
            Status::Locked { manual } => VaultState {
                unlocked: false,
                manual_lock: manual,
                keyring_available,
                // A manual lock is not a keyring problem; anything else is.
                keyring_unlocked: manual || keyring_available,
                vault_available: manual,
                generation: self.generation(),
                authentication_required: manual,
                reason: Some(if manual {
                    "manually locked".into()
                } else {
                    "locked".into()
                }),
            },
            Status::Unavailable(reason) => VaultState {
                unlocked: false,
                manual_lock: self.manual_lock(),
                keyring_available,
                keyring_unlocked: false,
                vault_available: false,
                generation: self.generation(),
                authentication_required: self.manual_lock(),
                reason: Some(reason),
            },
        }
    }

    pub fn list_secrets(&self) -> AppResult<Vec<EntryMeta>> {
        self.read(|file| {
            Ok(file
                .records
                .iter()
                .map(|record| {
                    let (count, last) = usage_of(file, &record.id);
                    record.meta(count, last)
                })
                .collect())
        })
    }

    pub fn types(&self) -> AppResult<Vec<String>> {
        self.read(|file| {
            let mut types: Vec<String> =
                file.records.iter().map(|r| r.entry_type.clone()).collect();
            types.sort_by_key(|t| t.to_lowercase());
            types.dedup();
            Ok(types)
        })
    }

    pub fn get(&self, id: &str) -> AppResult<EntryRecord> {
        self.read(|file| {
            file.records
                .iter()
                .find(|r| r.id == id)
                .cloned()
                .ok_or(AppError::NotFound)
        })
    }

    pub fn save(&self, draft: &mut EntryDraft) -> AppResult<SavedEntry> {
        validate_draft(draft)?;
        if draft.kind != Kind::Secret {
            return Err(AppError::Validation(
                "this vault only stores secrets".into(),
            ));
        }
        let now = crate::repository::now_rfc3339();
        self.write(|file| match draft.id.clone() {
            None => {
                let record = record_from(draft, 1, now.clone(), now);
                let saved = SavedEntry {
                    id: record.id.clone(),
                    revision: 1,
                    kind: Kind::Secret,
                };
                file.records.push(record);
                Ok(saved)
            }
            Some(id) => {
                let index = file
                    .records
                    .iter()
                    .position(|r| r.id == id)
                    .ok_or(AppError::NotFound)?;
                if file.records[index].revision != draft.expected_revision {
                    return Err(AppError::Conflict);
                }
                let created = file.records[index].created_at.clone();
                let revision = file.records[index].revision + 1;
                let mut record = record_from(draft, revision, created, now);
                record.id = id.clone();
                file.records[index] = record;
                Ok(SavedEntry {
                    id,
                    revision,
                    kind: Kind::Secret,
                })
            }
        })
    }

    pub fn delete(&self, id: &str, expected_revision: i64) -> AppResult<()> {
        self.write(|file| {
            let index = file
                .records
                .iter()
                .position(|r| r.id == id)
                .ok_or(AppError::NotFound)?;
            if expected_revision != 0 && file.records[index].revision != expected_revision {
                return Err(AppError::Conflict);
            }
            file.records.remove(index);
            file.usage.retain(|u| u.id != id);
            Ok(())
        })
    }

    pub fn set_pinned(
        &self,
        id: &str,
        expected_revision: i64,
        pinned: bool,
    ) -> AppResult<SavedEntry> {
        let now = crate::repository::now_rfc3339();
        self.write(|file| {
            let record = file
                .records
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or(AppError::NotFound)?;
            if record.revision != expected_revision {
                return Err(AppError::Conflict);
            }
            record.pinned = pinned;
            record.revision += 1;
            record.updated_at = now;
            Ok(SavedEntry {
                id: id.to_string(),
                revision: record.revision,
                kind: Kind::Secret,
            })
        })
    }

    pub fn record_usage(&self, id: &str) -> AppResult<()> {
        let now = crate::repository::now_rfc3339();
        self.write(|file| {
            match file.usage.iter_mut().find(|u| u.id == id) {
                Some(entry) => {
                    entry.count += 1;
                    entry.last_used_at = Some(now);
                }
                None => file.usage.push(UsageRecord {
                    id: id.to_string(),
                    count: 1,
                    last_used_at: Some(now),
                }),
            }
            Ok(())
        })
    }

    /// Whole-vault view for backup. Callers must already be authorised.
    pub fn snapshot(&self) -> AppResult<VaultFile> {
        self.read(|file| Ok(file.clone()))
    }

    /// Encrypts bytes with this vault's own credential, for backups and exports.
    pub fn seal(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        let identity = self.resolve_identity()?;
        encrypt_to(&[&identity], plaintext)
    }

    /// The vault's current age identity. Only meaningful once the vault is usable; callers
    /// must already be authorised (backups and exports are unlocked-only paths).
    pub fn identity(&self) -> AppResult<Identity> {
        self.resolve_identity()
    }

    /// Encrypts to the vault identity **and** additional recipients (the recovery credential
    /// is the only production extra). Archives sealed here open with either credential.
    pub fn seal_to_recipients(&self, plaintext: &[u8], extra: &[&Identity]) -> AppResult<Vec<u8>> {
        let identity = self.resolve_identity()?;
        let mut recipients: Vec<&Identity> = Vec::with_capacity(extra.len() + 1);
        recipients.push(&identity);
        recipients.extend_from_slice(extra);
        encrypt_to(&recipients, plaintext)
    }

    /// Opens something this vault sealed earlier. A foreign key fails here, and a failure
    /// never modifies anything.
    pub fn open_sealed(&self, ciphertext: &[u8]) -> AppResult<Vec<u8>> {
        let identity = self.resolve_identity()?;
        decrypt_with(&identity, ciphertext)
    }

    /// Installs a decrypted snapshot as the new vault content (restore).
    pub fn restore(&self, file: VaultFile) -> AppResult<()> {
        let incoming = std::mem::take(&mut *self.file.lock().expect("vault file lock poisoned"));
        let replacement = VaultFile {
            version: VAULT_VERSION,
            ..file
        };
        match self.persist(&replacement) {
            Ok(()) => {
                *self.file.lock().expect("vault file lock poisoned") = replacement.clone();
                self.generation.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(error) => {
                // Restore failed: keep the previous content instead of an empty vault.
                *self.file.lock().expect("vault file lock poisoned") = incoming;
                Err(error)
            }
        }
    }
}

fn usage_of(file: &VaultFile, id: &str) -> (i64, Option<String>) {
    file.usage
        .iter()
        .find(|u| u.id == id)
        .map(|u| (u.count, u.last_used_at.clone()))
        .unwrap_or((0, None))
}

fn record_from(
    draft: &EntryDraft,
    revision: i64,
    created_at: String,
    updated_at: String,
) -> EntryRecord {
    EntryRecord {
        id: draft
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        revision,
        kind: Kind::Secret,
        name: draft.name.clone(),
        entry_type: draft.entry_type.clone(),
        tags: draft.tags.clone(),
        description: draft.description.clone(),
        notes: draft.notes.clone(),
        fields: draft.fields.clone(),
        pinned: draft.pinned,
        command: String::new(),
        working_directory: String::new(),
        value: draft.value.clone(),
        created_at,
        updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ExtraField;

    struct Harness {
        dir: std::path::PathBuf,
        key: String,
    }

    impl Harness {
        fn new() -> Harness {
            let dir = std::env::temp_dir().join(format!("dc-vault-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).expect("temp dir");
            let (_, key) = generate_identity();
            Harness { dir, key }
        }

        fn vault(&self) -> VaultService {
            VaultService::new(
                self.dir.join("secrets.vault"),
                self.dir.join("manual-lock"),
                Credential::Provided {
                    key: self.key.clone(),
                },
            )
        }

        fn unlocked(&self) -> VaultService {
            let vault = self.vault();
            vault.bootstrap();
            assert!(
                vault.is_unlocked(),
                "vault should auto-open with a valid key"
            );
            vault
        }

        fn erase(&self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn secret(name: &str, value: &str) -> EntryDraft {
        EntryDraft {
            kind: Kind::Secret,
            id: None,
            expected_revision: 0,
            name: name.into(),
            entry_type: "API key".into(),
            tags: vec!["work".into()],
            description: "notes about the key".into(),
            notes: "rotated quarterly".into(),
            fields: vec![ExtraField {
                id: "f1".into(),
                label: "Account".into(),
                value: "svc-build@example.test".into(),
                display: crate::models::FieldDisplay::Text,
            }],
            pinned: false,
            command: String::new(),
            working_directory: String::new(),
            value: value.into(),
        }
    }

    #[test]
    fn secrets_round_trip_through_the_encrypted_file() {
        let h = Harness::new();
        let vault = h.unlocked();
        let saved = vault
            .save(&mut secret("Registry token", "synthetic-value-1"))
            .expect("save");
        assert_eq!(saved.revision, 1);

        // A brand new service reading the same file must see the same record.
        let reopened = h.unlocked();
        let record = reopened.get(&saved.id).expect("get");
        assert_eq!(record.value, "synthetic-value-1");
        assert_eq!(record.name, "Registry token");
        assert_eq!(record.fields[0].value, "svc-build@example.test");
        h.erase();
    }

    #[test]
    fn pinning_a_secret_preserves_its_value_and_rejects_stale_revisions() {
        let h = Harness::new();
        let vault = h.unlocked();
        let saved = vault
            .save(&mut secret("Registry token", "synthetic-value-1"))
            .expect("save");
        let pinned = vault
            .set_pinned(&saved.id, saved.revision, true)
            .expect("pin");
        assert_eq!(pinned.revision, 2);
        assert!(vault.set_pinned(&saved.id, saved.revision, false).is_err());
        let reopened = h.unlocked();
        let record = reopened.get(&saved.id).expect("get");
        assert!(record.pinned);
        assert_eq!(record.value, "synthetic-value-1");
        h.erase();
    }

    #[test]
    fn nothing_sensitive_appears_in_the_vault_file() {
        let h = Harness::new();
        let vault = h.unlocked();
        vault
            .save(&mut secret("UNIQUE-NAME-marker", "UNIQUE-VALUE-marker"))
            .expect("save");
        let bytes = std::fs::read(vault.path()).expect("vault file");
        for marker in [
            "UNIQUE-NAME-marker",
            "UNIQUE-VALUE-marker",
            "svc-build@example.test",
            "rotated quarterly",
            "API key",
        ] {
            assert!(
                !bytes.windows(marker.len()).any(|w| w == marker.as_bytes()),
                "plaintext marker leaked into the vault file"
            );
        }
        // And the file really is an age container.
        assert!(bytes.starts_with(b"age"));
        h.erase();
    }

    #[test]
    fn a_wrong_key_is_rejected_without_touching_the_file() {
        let h = Harness::new();
        let vault = h.unlocked();
        vault
            .save(&mut secret("Registry token", "synthetic-value-1"))
            .expect("save");
        let before = std::fs::read(vault.path()).expect("bytes");

        let other = VaultService::new(
            vault.path().to_path_buf(),
            h.dir.join("manual-lock"),
            Credential::Provided {
                key: generate_identity().1,
            },
        );
        other.bootstrap();
        assert!(!other.is_unlocked());
        assert!(matches!(other.status(), Status::Unavailable(_)));
        assert_eq!(
            std::fs::read(vault.path()).expect("bytes after failure"),
            before,
            "a failed unlock must not rewrite the vault"
        );
        assert!(other.list_secrets().is_err());
        h.erase();
    }

    #[test]
    fn truncated_ciphertext_is_rejected_without_replacing_the_vault() {
        let h = Harness::new();
        let vault = h.unlocked();
        vault
            .save(&mut secret("Registry token", "synthetic-value-1"))
            .expect("save");
        let good = std::fs::read(vault.path()).expect("bytes");
        std::fs::write(vault.path(), &good[..good.len() / 2]).expect("truncate");

        let reopened = h.vault();
        reopened.bootstrap();
        assert!(!reopened.is_unlocked());
        assert_eq!(
            std::fs::read(vault.path()).expect("bytes"),
            good[..good.len() / 2]
        );
        h.erase();
    }

    #[test]
    fn manual_lock_survives_a_restart_and_blocks_every_secret_operation() {
        let h = Harness::new();
        let vault = h.unlocked();
        let saved = vault
            .save(&mut secret("Registry token", "synthetic-value-1"))
            .expect("save");
        vault.lock().expect("lock");
        assert!(vault.list_secrets().is_err());
        assert!(vault.get(&saved.id).is_err());
        assert!(vault.save(&mut secret("late", "x")).is_err());

        // Simulated restart: the flag file alone must keep the vault shut.
        let after_restart = h.vault();
        after_restart.bootstrap();
        assert!(!after_restart.is_unlocked());
        assert!(after_restart.manual_lock());
        let state = after_restart.state();
        assert!(state.manual_lock && state.authentication_required && !state.unlocked);
        // Unlocking without a verified authentication mechanism fails closed. On a host
        // where the polkit helper genuinely is installed, unlock() may legitimately succeed
        // through real authentication; the negative case only exists when there is no
        // mechanism to authenticate with.
        if !crate::auth::mechanism_available() {
            assert!(after_restart.unlock().is_err());
            assert!(after_restart.manual_lock());
        }
        h.erase();
    }

    #[test]
    fn a_failed_unlock_keeps_the_manual_lock_and_the_flag_file() {
        let h = Harness::new();
        let flag = h.dir.join("manual-lock");
        let vault = h.unlocked();
        let saved = vault
            .save(&mut secret("Registry token", "synthetic-value-1"))
            .expect("save");
        vault.lock().expect("manual lock");
        assert!(flag.exists());

        // Authentication succeeds, but the vault cannot be opened: the wrong credential makes
        // decryption fail. The manual lock must survive that exactly as it was.
        let (_, foreign_key) = generate_identity();
        let wrong_key = VaultService::new(
            h.dir.join("secrets.vault"),
            flag.clone(),
            Credential::Provided { key: foreign_key },
        );
        assert!(
            wrong_key.manual_lock(),
            "the flag file alone locks a new service"
        );
        let failure = wrong_key.unlock_with(|| Ok(()));
        assert!(
            failure.is_err(),
            "a vault that will not decrypt must not report success"
        );
        assert!(
            flag.exists(),
            "the manual lock flag must survive a failed unlock"
        );
        assert!(
            wrong_key.manual_lock(),
            "in-memory lock must survive a failed unlock"
        );
        let state = wrong_key.state();
        assert!(
            !state.unlocked && state.manual_lock && state.authentication_required,
            "a failed unlock must still look like a manual lock to the UI"
        );
        assert!(wrong_key.list_secrets().is_err());

        // A dismissed authentication dialog must behave the same way.
        let refused = h.vault();
        assert!(refused.manual_lock());
        assert!(refused
            .unlock_with(|| Err(AppError::Other("authentication dismissed".into())))
            .is_err());
        assert!(refused.manual_lock());
        assert!(flag.exists());

        // And a restart in that state must still be a manual lock, never an auto-open.
        let after_restart = h.vault();
        after_restart.bootstrap();
        assert!(after_restart.manual_lock());
        assert!(after_restart.get(&saved.id).is_err());
        h.erase();
    }

    #[test]
    fn a_successful_unlock_opens_the_vault_before_clearing_the_flag() {
        let h = Harness::new();
        let flag = h.dir.join("manual-lock");
        let vault = h.unlocked();
        let saved = vault
            .save(&mut secret("Registry token", "synthetic-value-1"))
            .expect("save");
        vault.lock().expect("manual lock");

        let reopened = h.vault();
        assert!(reopened.manual_lock());
        reopened
            .unlock_with(|| Ok(()))
            .expect("authenticated unlock");
        assert!(!flag.exists(), "only a successful open may clear the flag");
        assert!(reopened.is_unlocked());
        assert_eq!(
            reopened.get(&saved.id).expect("read after unlock").value,
            "synthetic-value-1"
        );
        h.erase();
    }

    #[test]
    fn stale_revisions_are_rejected_and_reorder_survives() {
        let h = Harness::new();
        let vault = h.unlocked();
        let saved = vault.save(&mut secret("Token", "v1")).expect("save");
        let mut stale = secret("Token", "v2");
        stale.id = Some(saved.id.clone());
        stale.expected_revision = 99;
        assert!(matches!(vault.save(&mut stale), Err(AppError::Conflict)));

        let mut fresh = secret("Token", "v2");
        fresh.id = Some(saved.id.clone());
        fresh.expected_revision = 1;
        fresh.fields = vec![
            ExtraField {
                id: "a".into(),
                label: "Project".into(),
                value: "api".into(),
                display: crate::models::FieldDisplay::Text,
            },
            ExtraField {
                id: "b".into(),
                label: "URL".into(),
                value: "https://example.test".into(),
                display: crate::models::FieldDisplay::Url,
            },
        ];
        let updated = vault.save(&mut fresh).expect("update");
        assert_eq!(updated.revision, 2);
        let record = vault.get(&saved.id).expect("record");
        assert_eq!(
            record
                .fields
                .iter()
                .map(|f| f.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Project", "URL"]
        );
        h.erase();
    }

    #[test]
    fn a_failed_write_keeps_the_previous_vault_and_memory() {
        let h = Harness::new();
        let vault = h.unlocked();
        let saved = vault.save(&mut secret("Token", "v1")).expect("save");
        // Make the atomic rename fail by putting a directory where the vault file lives.
        std::fs::remove_file(vault.path()).expect("remove");
        std::fs::create_dir(vault.path()).expect("block the path");

        let mut second = secret("Token 2", "v2");
        second.id = Some(saved.id.clone());
        second.expected_revision = 1;
        assert!(vault.save(&mut second).is_err());
        assert_eq!(
            vault.get(&saved.id).expect("unchanged in memory").value,
            "v1"
        );
        // A failed write must not leave an empty or partial replacement vault behind.
        assert!(
            vault.path().is_dir(),
            "no replacement vault file was created"
        );
        assert!(vault
            .list_secrets()
            .expect("still readable")
            .iter()
            .any(|entry| entry.name == "Token"));
        h.erase();
    }

    #[test]
    fn delete_only_removes_the_intended_record() {
        let h = Harness::new();
        let vault = h.unlocked();
        let first = vault.save(&mut secret("One", "v1")).expect("first");
        let _second = vault.save(&mut secret("Two", "v2")).expect("second");
        assert!(matches!(
            vault.delete(&first.id, 99),
            Err(AppError::Conflict)
        ));
        vault.delete(&first.id, 1).expect("delete");
        let remaining = vault.list_secrets().expect("list");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "Two");
        h.erase();
    }
}
