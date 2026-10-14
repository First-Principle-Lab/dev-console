use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::models::{EntryDraft, EntryMeta, EntryRecord, Kind, SavedEntry, VaultState};

/// Vault access state as presented to the UI and the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultStatus {
    Unlocked,
    Locked { manual: bool, reason: String },
    Unavailable(String),
}

/// Owns the decrypted secret set and the lock lifecycle.
///
/// Stage note: encrypted storage, the keyring credential and the authenticated unlock are
/// added in the vault checkpoint. Until then this deliberately reports the vault as
/// unavailable instead of falling back to plaintext, and it already enforces the persistent
/// manual-lock flag so every caller can be built against the final interface.
pub struct VaultService {
    records: Mutex<Vec<EntryRecord>>,
    usage: Mutex<Vec<(String, i64, Option<String>)>>,
    unlocked: AtomicBool,
    manual_lock: AtomicBool,
    generation: AtomicU64,
    reason: Mutex<Option<String>>,
    lock_flag: PathBuf,
}

impl VaultService {
    pub fn new(lock_flag: PathBuf) -> VaultService {
        let manual = std::fs::metadata(&lock_flag).is_ok();

        VaultService {
            records: Mutex::new(Vec::new()),
            usage: Mutex::new(Vec::new()),
            unlocked: AtomicBool::new(false),
            manual_lock: AtomicBool::new(manual),
            generation: AtomicU64::new(1),
            reason: Mutex::new(Some("encrypted vault is not configured yet".into())),
            lock_flag,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn is_unlocked(&self) -> bool {
        self.unlocked.load(Ordering::SeqCst)
    }

    pub fn manual_lock(&self) -> bool {
        self.manual_lock.load(Ordering::SeqCst)
    }

    pub fn status(&self) -> VaultStatus {
        if self.manual_lock() {
            return VaultStatus::Locked {
                manual: true,
                reason: "manually locked".into(),
            };
        }
        match self
            .reason
            .lock()
            .expect("vault reason lock poisoned")
            .clone()
        {
            Some(reason) => VaultStatus::Unavailable(reason),
            None => {
                if self.is_unlocked() {
                    VaultStatus::Unlocked
                } else {
                    VaultStatus::Locked {
                        manual: false,
                        reason: "locked".into(),
                    }
                }
            }
        }
    }

    pub fn state(&self) -> VaultState {
        let keyring_available = crate::keyring::provider_available();

        match self.status() {
            VaultStatus::Unlocked => VaultState {
                unlocked: true,
                manual_lock: self.manual_lock(),
                keyring_available,
                keyring_unlocked: true,
                vault_available: true,
                generation: self.generation(),
                authentication_required: false,
                reason: None,
            },
            VaultStatus::Locked { manual, reason } => VaultState {
                unlocked: false,
                manual_lock: manual,
                keyring_available,
                keyring_unlocked: !manual,
                vault_available: true,
                generation: self.generation(),
                authentication_required: manual,
                reason: Some(reason),
            },
            VaultStatus::Unavailable(reason) => VaultState {
                unlocked: false,
                manual_lock: self.manual_lock(),
                keyring_available,
                keyring_unlocked: false,
                vault_available: false,
                generation: self.generation(),
                authentication_required: false,
                reason: Some(reason),
            },
        }
    }

    fn bump(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Takes the vault lock for the duration of a secret operation so a lock request can
    /// never interleave with a reveal/copy/save.
    pub fn read<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&[EntryRecord], &[(String, i64, Option<String>)]) -> AppResult<T>,
    {
        self.authorise()?;
        let records = self.records.lock().expect("vault records lock poisoned");
        let usage = self.usage.lock().expect("vault usage lock poisoned");
        f(&records, &usage)
    }

    fn write<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&mut Vec<EntryRecord>, &mut Vec<(String, i64, Option<String>)>) -> AppResult<T>,
    {
        self.authorise()?;
        let mut records = self.records.lock().expect("vault records lock poisoned");
        let mut usage = self.usage.lock().expect("vault usage lock poisoned");
        let result = f(&mut records, &mut usage);
        if result.is_ok() {
            self.persist(&records)?;
        }
        result
    }

    /// Re-checked for every secret operation, including requests that were queued before a
    /// lock was acknowledged.
    pub fn authorise(&self) -> AppResult<()> {
        if self.manual_lock() {
            return Err(AppError::Locked);
        }
        match self.status() {
            VaultStatus::Unlocked => Ok(()),
            VaultStatus::Locked { .. } => Err(AppError::Locked),
            VaultStatus::Unavailable(reason) => Err(AppError::VaultUnavailable(reason)),
        }
    }

    fn persist(&self, records: &[EntryRecord]) -> AppResult<()> {
        let _ = records;
        // Implemented with the encrypted vault backend.
        Ok(())
    }

    pub fn lock(&self) -> AppResult<()> {
        self.set_manual_lock(true)
    }

    pub fn set_manual_lock(&self, locked: bool) -> AppResult<()> {
        if locked {
            if !self.manual_lock() {
                if let Some(parent) = self.lock_flag.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::write(&self.lock_flag, b"locked\n")
                    .map_err(|e| AppError::Other(e.to_string()))?;
                crate::paths::set_file_private(&self.lock_flag)?;
            }
            self.manual_lock.store(true, Ordering::SeqCst);
            self.clear_decrypted();
            self.bump();
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

    /// Drops every decrypted record and advances the generation so in-flight responses that
    /// carry secret metadata are discarded by both windows.
    pub fn clear_decrypted(&self) {
        self.records
            .lock()
            .expect("vault records lock poisoned")
            .clear();
        self.usage
            .lock()
            .expect("vault usage lock poisoned")
            .clear();
        self.unlocked.store(false, Ordering::SeqCst);
        self.bump();
    }

    pub fn unlock(&self) -> AppResult<()> {
        Err(AppError::VaultUnavailable(
            "encrypted vault is not configured yet".into(),
        ))
    }

    pub fn list_secrets(&self) -> AppResult<Vec<EntryMeta>> {
        self.read(|records, usage| {
            Ok(records
                .iter()
                .filter(|r| r.kind == Kind::Secret)
                .map(|r| {
                    let (count, last) = usage_for(usage, &r.id);
                    r.meta(count, last)
                })
                .collect())
        })
    }

    pub fn types(&self) -> AppResult<Vec<String>> {
        self.read(|records, _| {
            let mut types: Vec<String> = records
                .iter()
                .filter(|r| r.kind == Kind::Secret)
                .map(|r| r.entry_type.clone())
                .collect();
            types.sort_by_key(|a| a.to_lowercase());
            types.dedup();
            Ok(types)
        })
    }

    pub fn get(&self, id: &str) -> AppResult<EntryRecord> {
        self.read(|records, _| {
            records
                .iter()
                .find(|r| r.id == id && r.kind == Kind::Secret)
                .cloned()
                .ok_or(AppError::NotFound)
        })
    }

    pub fn save(&self, draft: &mut EntryDraft) -> AppResult<SavedEntry> {
        let id = draft.id.clone();
        let expected = draft.expected_revision;
        self.write(|records, _| {
            let now = crate::repository::unix_ms();
            let _ = now;
            match id {
                None => {
                    let record = new_record(draft);
                    let saved = SavedEntry {
                        id: record.id.clone(),
                        revision: 1,
                        kind: Kind::Secret,
                    };
                    records.push(record);
                    Ok(saved)
                }
                Some(id) => {
                    let record = records
                        .iter_mut()
                        .find(|r| r.id == id && r.kind == Kind::Secret)
                        .ok_or(AppError::NotFound)?;
                    if record.revision != expected {
                        return Err(AppError::Conflict);
                    }
                    let created = record.created_at.clone();
                    *record = new_record(draft);
                    record.created_at = created;
                    record.revision = expected + 1;
                    Ok(SavedEntry {
                        id,
                        revision: expected + 1,
                        kind: Kind::Secret,
                    })
                }
            }
        })
    }

    pub fn delete(&self, id: &str, expected_revision: i64) -> AppResult<()> {
        self.write(|records, usage| {
            let index = records
                .iter()
                .position(|r| r.id == id && r.kind == Kind::Secret)
                .ok_or(AppError::NotFound)?;
            if expected_revision != 0 && records[index].revision != expected_revision {
                return Err(AppError::Conflict);
            }
            records.remove(index);
            usage.retain(|(usage_id, _, _)| usage_id != id);
            Ok(())
        })
    }

    pub fn record_usage(&self, id: &str) -> AppResult<()> {
        self.write(|_, usage| {
            let now = chrono_now();
            match usage.iter_mut().find(|(usage_id, _, _)| usage_id == id) {
                Some(entry) => {
                    entry.1 += 1;
                    entry.2 = Some(now);
                }
                None => usage.push((id.to_string(), 1, Some(now))),
            }
            Ok(())
        })
    }
}

fn usage_for(usage: &[(String, i64, Option<String>)], id: &str) -> (i64, Option<String>) {
    usage
        .iter()
        .find(|(usage_id, _, _)| usage_id == id)
        .map(|(_, count, last)| (*count, last.clone()))
        .unwrap_or((0, None))
}

fn new_record(draft: &EntryDraft) -> EntryRecord {
    EntryRecord {
        id: draft
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        revision: 1,
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
        created_at: chrono_now(),
        updated_at: chrono_now(),
    }
}

fn chrono_now() -> String {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}
