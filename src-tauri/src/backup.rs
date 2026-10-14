//! Encrypted backups, export and import.
//!
//! An **archive** is one JSON document holding the command records and the whole decrypted
//! vault. It is sealed with the vault's own age credential before it ever reaches the
//! filesystem, so a backup directory, an exported file, or a USB stick never contains readable
//! commands or secrets. Nothing here writes an unsealed archive, and there is no unsealed
//! staging file anywhere in the flow: sealing happens in memory and the ciphertext goes through
//! an atomic rename into place.
//!
//! Import is deliberately two-phase. [`BackupManager::import_from`] and
//! [`BackupManager::read_backup`] authenticate, decrypt, parse and validate, and hand the
//! archive back; only the caller - which must already hold an unlocked vault - applies it. A
//! rejected file therefore cannot modify anything, and there is no window in which the live
//! data has been replaced by a partially validated import.

use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use age::x25519::Identity;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::models::{EntryRecord, Kind};
use crate::repository::now_rfc3339;
use crate::vault::{VaultFile, VaultService, VAULT_VERSION};

/// Bumped when the archive layout changes. Newer files are always refused.
pub const ARCHIVE_VERSION: u32 = 1;
/// How many generated backups are kept by default.
pub const DEFAULT_KEEP: usize = 8;
const PREFIX: &str = "dev-console-";
const SUFFIX: &str = ".age";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Archive {
    pub version: u32,
    pub application: String,
    pub created_at: String,
    pub commands: Vec<EntryRecord>,
    pub secrets: VaultFile,
}

impl Archive {
    pub fn new(commands: Vec<EntryRecord>, secrets: VaultFile) -> Archive {
        Archive {
            version: ARCHIVE_VERSION,
            application: format!("dev-console {}", env!("CARGO_PKG_VERSION")),
            created_at: now_rfc3339(),
            commands,
            secrets,
        }
    }

    pub fn to_json(&self) -> AppResult<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(self)?)
    }
}

/// Something that can seal and open archive bytes. Implemented by the live [`VaultService`]
/// (keyring credential) and by [`KeySeal`] (an explicit key, used by tests).
pub trait Seal {
    fn seal(&self, plaintext: &[u8]) -> AppResult<Vec<u8>>;
    fn open(&self, ciphertext: &[u8]) -> AppResult<Vec<u8>>;
}

impl Seal for VaultService {
    fn seal(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        VaultService::seal(self, plaintext)
    }

    fn open(&self, ciphertext: &[u8]) -> AppResult<Vec<u8>> {
        VaultService::open_sealed(self, ciphertext)
    }
}

/// A key holder that is not a running vault service - used by the tests and by offline tooling.
pub struct KeySeal {
    identity: Identity,
}

impl KeySeal {
    pub fn from_secret(secret: &str) -> AppResult<KeySeal> {
        Ok(KeySeal {
            identity: crate::vault::parse_identity(secret)?,
        })
    }
}

impl Seal for KeySeal {
    fn seal(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        crate::vault::encrypt_to(&[&self.identity], plaintext)
    }

    fn open(&self, ciphertext: &[u8]) -> AppResult<Vec<u8>> {
        crate::vault::decrypt_with(&self.identity, ciphertext)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub name: String,
    pub size: u64,
    pub modified_at_ms: i64,
}

/// Sealed backups in one private directory, rotated to a fixed count.
pub struct BackupManager {
    dir: PathBuf,
    keep: usize,
}

impl BackupManager {
    pub fn new(dir: PathBuf, keep: usize) -> AppResult<BackupManager> {
        Ok(BackupManager {
            dir: crate::paths::ensure_dir(&dir)?,
            keep: keep.max(1),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes a sealed backup and rotates the directory. Returns the new file's name.
    pub fn create(&self, archive: &Archive, seal: &dyn Seal) -> AppResult<String> {
        let bytes = seal.seal(&archive.to_json()?)?;
        let name = format!(
            "{}{}-{}{}",
            PREFIX,
            chrono_like_stamp(),
            &uuid::Uuid::new_v4().simple().to_string()[..8],
            SUFFIX
        );
        write_private(&self.dir.join(&name), &bytes)?;
        self.rotate()?;
        Ok(name)
    }

    pub fn list(&self) -> AppResult<Vec<BackupInfo>> {
        let mut found = Vec::new();
        if !self.dir.exists() {
            return Ok(found);
        }
        for entry in std::fs::read_dir(&self.dir).map_err(|e| AppError::Other(e.to_string()))? {
            let entry = entry.map_err(|e| AppError::Other(e.to_string()))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_ours(&name) {
                continue;
            }
            let meta = entry
                .metadata()
                .map_err(|e| AppError::Other(e.to_string()))?;
            let modified_at_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or_default();
            found.push(BackupInfo {
                name,
                size: meta.len(),
                modified_at_ms,
            });
        }
        found.sort_by(|a, b| {
            b.modified_at_ms
                .cmp(&a.modified_at_ms)
                .then(b.name.cmp(&a.name))
        });
        Ok(found)
    }

    /// Reads and validates one of this directory's own backups. The name is validated first:
    /// it may never address anything outside the backup directory.
    pub fn read_backup(&self, name: &str, seal: &dyn Seal) -> AppResult<Archive> {
        if !is_ours(name) {
            return Err(AppError::Validation("not a Dev Console backup name".into()));
        }
        let path = self.dir.join(name);
        let bytes = std::fs::read(&path)
            .map_err(|_| AppError::Other("that backup file could not be read".into()))?;
        open_archive(&bytes, seal)
    }

    pub fn export_to(
        &self,
        destination: &Path,
        archive: &Archive,
        seal: &dyn Seal,
    ) -> AppResult<()> {
        let bytes = seal.seal(&archive.to_json()?)?;
        write_private(destination, &bytes)
    }

    /// Decrypts and validates an exported file without touching live data. Wrong keys, corrupt
    /// or truncated files and newer formats all fail here, leaving everything as it was.
    pub fn import_from(&self, source: &Path, seal: &dyn Seal) -> AppResult<Archive> {
        let bytes = std::fs::read(source)
            .map_err(|_| AppError::Other("the selected file could not be read".into()))?;
        open_archive(&bytes, seal)
    }

    /// Keeps the newest `keep` backups. Files this manager did not create are never removed.
    pub fn rotate(&self) -> AppResult<usize> {
        let mut ours = self.list()?;
        if ours.len() <= self.keep {
            return Ok(0);
        }
        let mut removed = 0;
        while ours.len() > self.keep {
            let old = ours.pop();
            if let Some(info) = old {
                std::fs::remove_file(self.dir.join(&info.name))
                    .map_err(|e| AppError::Other(e.to_string()))?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

fn is_ours(name: &str) -> bool {
    !name.contains('/')
        && !name.contains("..")
        && name.starts_with(PREFIX)
        && name.ends_with(SUFFIX)
}

fn open_archive(bytes: &[u8], seal: &dyn Seal) -> AppResult<Archive> {
    let plaintext = seal.open(bytes).map_err(|_| {
        AppError::VaultUnavailable(
            "the file could not be decrypted with this machine's key (wrong machine, wrong key, \
             or the file is damaged)"
                .into(),
        )
    })?;
    let archive: Archive = serde_json::from_slice(&plaintext).map_err(|_| {
        AppError::Validation("the archive is not a valid Dev Console backup".into())
    })?;
    validate(&archive)?;
    Ok(archive)
}

fn validate(archive: &Archive) -> AppResult<()> {
    if archive.version > ARCHIVE_VERSION {
        return Err(AppError::Other(
            "that backup was written by a newer version of Dev Console".into(),
        ));
    }
    if archive.secrets.version > VAULT_VERSION {
        return Err(AppError::Other(
            "that backup uses a newer vault format".into(),
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for record in &archive.commands {
        if record.name.trim().is_empty() {
            return Err(AppError::Validation(
                "the archive contains an entry without a name".into(),
            ));
        }
        if record.kind != Kind::Command {
            return Err(AppError::Validation(
                "the command section of the archive contains a non-command entry".into(),
            ));
        }
        if !ids.insert(record.id.clone()) {
            return Err(AppError::Validation(
                "the archive contains the same entry twice".into(),
            ));
        }
    }
    for record in &archive.secrets.records {
        if record.name.trim().is_empty() {
            return Err(AppError::Validation(
                "the archive contains a secret without a name".into(),
            ));
        }
        if !ids.insert(record.id.clone()) {
            return Err(AppError::Validation(
                "the archive contains the same entry twice".into(),
            ));
        }
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::Other(e.to_string()))?;
    }
    let tmp = path.with_extension("part");
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| AppError::Other(e.to_string()))?;
        file.write_all(bytes)
            .map_err(|e| AppError::Other(e.to_string()))?;
        file.sync_all()
            .map_err(|e| AppError::Other(e.to_string()))?;
    }
    crate::paths::set_file_private(&tmp)?;
    std::fs::rename(&tmp, path).map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

fn chrono_like_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    // Seconds since epoch, base36-ish and sortable; no external date library needed.
    format!("{:0>13}", now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    use crate::vault::UsageRecord;

    fn record(id: &str, name: &str, value: &str) -> EntryRecord {
        EntryRecord {
            id: id.into(),
            revision: 1,
            kind: Kind::Secret,
            name: name.into(),
            entry_type: "API key".into(),
            tags: vec!["work".into()],
            description: String::new(),
            notes: "unique-notes-marker".into(),
            fields: Vec::new(),
            pinned: false,
            command: String::new(),
            working_directory: String::new(),
            value: value.into(),
            created_at: now_rfc3339(),
            updated_at: now_rfc3339(),
        }
    }

    fn command(id: &str) -> EntryRecord {
        let mut record = record(id, "Deploy staging", "");
        record.kind = Kind::Command;
        record.command = "make deploy".into();
        record.value = String::new();
        record
    }

    fn archive() -> Archive {
        Archive {
            version: ARCHIVE_VERSION,
            application: "dev-console test".into(),
            created_at: now_rfc3339(),
            commands: vec![command("c1")],
            secrets: VaultFile {
                version: VAULT_VERSION,
                records: vec![record("s1", "Registry token", "UNIQUE-SECRET-marker")],
                usage: vec![UsageRecord {
                    id: "s1".into(),
                    count: 3,
                    last_used_at: Some("2026-10-14T12:00:00+00:00".into()),
                }],
            },
        }
    }

    struct Dir(std::path::PathBuf);
    impl Dir {
        fn new() -> Dir {
            let path =
                std::env::temp_dir().join(format!("dc-backup-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).expect("temp dir");
            Dir(path)
        }
        fn manager(&self, keep: usize) -> BackupManager {
            BackupManager::new(self.0.join("backups"), keep).expect("manager")
        }
        fn keys() -> (KeySeal, KeySeal) {
            let (identity, secret) = crate::vault::generate_identity();
            let (_, foreign) = crate::vault::generate_identity();
            drop(identity);
            (
                KeySeal::from_secret(&secret).expect("key"),
                KeySeal::from_secret(&foreign).expect("foreign key"),
            )
        }
        fn erase(&self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    #[test]
    fn a_backup_round_trips_commands_secrets_and_usage() {
        let dir = Dir::new();
        let (key, _) = Dir::keys();
        let manager = dir.manager(4);
        let name = manager.create(&archive(), &key).expect("create");

        let restored = manager.read_backup(&name, &key).expect("read backup");
        assert_eq!(restored.commands.len(), 1);
        assert_eq!(restored.secrets.records.len(), 1);
        assert_eq!(restored.secrets.records[0].value, "UNIQUE-SECRET-marker");
        assert_eq!(restored.secrets.usage[0].count, 3);
        dir.erase();
    }

    #[test]
    fn backup_files_never_contain_plaintext() {
        let dir = Dir::new();
        let (key, _) = Dir::keys();
        let manager = dir.manager(4);
        let name = manager.create(&archive(), &key).expect("create");
        let bytes = std::fs::read(manager.dir().join(&name)).expect("read file");
        for marker in [
            "UNIQUE-SECRET-marker",
            "unique-notes-marker",
            "Deploy staging",
            "make deploy",
        ] {
            assert!(
                !bytes.windows(marker.len()).any(|w| w == marker.as_bytes()),
                "plaintext marker found in the backup file"
            );
        }
        dir.erase();
    }

    #[test]
    fn rotation_keeps_the_newest_and_touches_foreign_files() {
        let dir = Dir::new();
        let (key, _) = Dir::keys();
        let manager = dir.manager(3);
        let foreign = manager.dir().join("someone-else.txt");
        std::fs::write(&foreign, b"not ours").expect("foreign file");

        let mut created = Vec::new();
        for _ in 0..7 {
            created.push(manager.create(&archive(), &key).expect("create"));
            // mtime is what rotation orders by, so make the order deterministic.
            let path = manager.dir().join(created.last().expect("last"));
            let stamp = std::time::UNIX_EPOCH
                + std::time::Duration::from_secs(1_700_000_000 + created.len() as u64);
            let file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("open");
            file.set_modified(stamp).expect("set mtime");
        }

        let kept = manager.list().expect("list");
        assert_eq!(kept.len(), 3, "rotation must keep exactly `keep` files");
        let newest = created.last().expect("newest");
        assert!(kept.iter().any(|b| &b.name == newest), "newest survived");
        assert!(
            foreign.exists(),
            "a file this manager did not create survives rotation"
        );
        assert_eq!(
            std::fs::metadata(manager.dir())
                .expect("meta")
                .permissions()
                .mode()
                & 0o777,
            0o700,
            "the backup directory must be private"
        );
        dir.erase();
    }

    #[test]
    fn a_foreign_key_cannot_open_a_backup_or_an_export() {
        let dir = Dir::new();
        let (key, other) = Dir::keys();
        let manager = dir.manager(4);
        let name = manager.create(&archive(), &key).expect("create");
        let failure = manager
            .read_backup(&name, &other)
            .expect_err("wrong key must fail");
        assert!(failure.to_string().contains("could not be decrypted"));

        let export = dir.0.join("portable.age");
        manager
            .export_to(&export, &archive(), &key)
            .expect("export");
        assert!(manager.import_from(&export, &other).is_err());
        assert!(manager.import_from(&export, &key).is_ok());
        dir.erase();
    }

    #[test]
    fn corrupt_truncated_and_newer_archives_are_refused_without_changing_anything() {
        let dir = Dir::new();
        let (key, _) = Dir::keys();
        let manager = dir.manager(4);
        let existing = manager
            .create(&archive(), &key)
            .expect("a real backup exists first");
        let export = dir.0.join("portable.age");
        manager
            .export_to(&export, &archive(), &key)
            .expect("export");
        let sealed = std::fs::read(&export).expect("read export");

        // Truncation must not be mistaken for an empty-but-valid archive.
        let truncated = dir.0.join("truncated.age");
        std::fs::write(&truncated, &sealed[..sealed.len() / 2]).expect("truncate");
        assert!(manager.import_from(&truncated, &key).is_err());

        // Byte damage inside the payload likewise fails the authentication tag.
        let mut damaged = sealed.clone();
        let tail = damaged.len() - 12;
        damaged[tail] ^= 0xff;
        let damaged_path = dir.0.join("damaged.age");
        std::fs::write(&damaged_path, &damaged).expect("write damaged");
        assert!(manager.import_from(&damaged_path, &key).is_err());

        // A future format is refused instead of being half-applied.
        let newer = Archive {
            version: ARCHIVE_VERSION + 1,
            ..archive()
        };
        let newer_path = dir.0.join("newer.age");
        manager
            .export_to(&newer_path, &newer, &key)
            .expect("export newer");
        let error = manager
            .import_from(&newer_path, &key)
            .expect_err("newer must fail");
        assert!(error.to_string().contains("newer version"));

        // None of the rejected files touched the live backups.
        let kept = manager.list().expect("list");
        assert_eq!(
            kept.len(),
            1,
            "a rejected import must not add or remove backups"
        );
        assert_eq!(kept[0].name, existing);
        assert!(
            manager.read_backup(&existing, &key).is_ok(),
            "the surviving backup still opens"
        );
        dir.erase();
    }

    #[test]
    fn an_archive_with_inconsistent_entries_is_rejected() {
        let dir = Dir::new();
        let (key, _) = Dir::keys();
        let manager = dir.manager(4);

        let duplicate = Archive {
            commands: vec![command("same"), command("same")],
            ..archive()
        };
        let path = dir.0.join("duplicate.age");
        manager.export_to(&path, &duplicate, &key).expect("export");
        assert!(manager.import_from(&path, &key).is_err());

        let nameless = Archive {
            secrets: VaultFile {
                version: VAULT_VERSION,
                records: vec![record("s9", "   ", "x")],
                usage: Vec::new(),
            },
            ..archive()
        };
        let path = dir.0.join("nameless.age");
        manager.export_to(&path, &nameless, &key).expect("export");
        assert!(manager.import_from(&path, &key).is_err());

        // A backup name from outside the manager's own directory is refused outright.
        assert!(manager.read_backup("../escape.age", &key).is_err());
        assert!(manager.read_backup("/etc/passwd", &key).is_err());
        dir.erase();
    }
}
