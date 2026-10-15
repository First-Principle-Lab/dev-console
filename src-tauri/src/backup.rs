//! Encrypted backups, export and import.
//!
//! An **archive** is one JSON document holding the command records and the whole decrypted
//! vault. It is sealed to **two** age recipients - the vault's own credential and the profile's
//! recovery credential ([`RecoveryKey`]) - before it ever reaches the filesystem, so a backup
//! directory, an exported file, or a USB stick never contains readable commands or secrets,
//! and losing the original keyring never loses the archives. Nothing here writes an unsealed archive, and there is no unsealed
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

/// A second, exportable credential for archives. Generated once per profile, stored 0600 in
/// the config directory, and shown in Settings so the owner can keep it somewhere completely
/// independent of the keyring. Every archive is sealed to **both** the vault identity and this
/// identity, so losing the original keyring never loses the backups: a fresh profile can open
/// them with the recovery credential alone.
pub struct RecoveryKey {
    path: PathBuf,
    secret: String,
}

impl RecoveryKey {
    pub fn load_or_create(path: PathBuf) -> AppResult<RecoveryKey> {
        if path.exists() {
            let secret = std::fs::read_to_string(&path)
                .map_err(|_| {
                    AppError::Other(
                        "the recovery credential file exists but could not be read".into(),
                    )
                })?
                .trim()
                .to_string();
            // Refuse to hand out a corrupt credential.
            crate::vault::parse_identity(&secret)?;
            crate::paths::set_file_private(&path)?;
            return Ok(RecoveryKey { path, secret });
        }
        let (_, secret) = crate::vault::generate_identity();
        // 0600 from the very first byte - never world-readable, even briefly.
        write_private(&path, secret.as_bytes())?;
        Ok(RecoveryKey { path, secret })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn secret(&self) -> &str {
        &self.secret
    }

    pub fn identity(&self) -> AppResult<Identity> {
        crate::vault::parse_identity(&self.secret)
    }
}

/// The seal every archive goes through: sealed to the vault identity *and* the recovery
/// identity, openable with either one.
pub struct ArchiveSeal {
    primary: Identity,
    recovery: Identity,
}

impl ArchiveSeal {
    pub fn new(vault: &VaultService, recovery: &RecoveryKey) -> AppResult<ArchiveSeal> {
        Ok(ArchiveSeal {
            primary: vault.identity()?,
            recovery: recovery.identity()?,
        })
    }

    /// Offline construction (tests, future recovery tooling): any two age secrets.
    pub fn from_secrets(primary: &str, recovery: &str) -> AppResult<ArchiveSeal> {
        Ok(ArchiveSeal {
            primary: crate::vault::parse_identity(primary)?,
            recovery: crate::vault::parse_identity(recovery)?,
        })
    }
}

impl Seal for ArchiveSeal {
    fn seal(&self, plaintext: &[u8]) -> AppResult<Vec<u8>> {
        crate::vault::encrypt_to(&[&self.primary, &self.recovery], plaintext)
    }

    fn open(&self, ciphertext: &[u8]) -> AppResult<Vec<u8>> {
        match crate::vault::decrypt_with(&self.primary, ciphertext) {
            Ok(plaintext) => Ok(plaintext),
            Err(primary_error) => {
                crate::vault::decrypt_with(&self.recovery, ciphertext).map_err(|_| primary_error)
            }
        }
    }
}

/// What a backup or export file contains - read without applying anything, for the review
/// step in Settings.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSummary {
    pub source: String,
    pub created_at: String,
    pub commands: usize,
    pub secrets: usize,
    pub size_bytes: u64,
}

impl ArchiveSummary {
    fn of(source: &str, archive: &Archive, size_bytes: u64) -> ArchiveSummary {
        ArchiveSummary {
            source: source.to_string(),
            created_at: archive.created_at.clone(),
            commands: archive.commands.len(),
            secrets: archive.secrets.records.len(),
            size_bytes,
        }
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

    /// Review-before-apply for generated backups. Same read-only guarantee as [`Self::read_backup`].
    pub fn inspect_backup(&self, name: &str, seal: &dyn Seal) -> AppResult<ArchiveSummary> {
        if !is_ours(name) {
            return Err(AppError::Validation("not a Dev Console backup name".into()));
        }
        let path = self.dir.join(name);
        let bytes = std::fs::read(&path)
            .map_err(|_| AppError::Other("that backup file could not be read".into()))?;
        let archive = open_archive(&bytes, seal)?;
        Ok(ArchiveSummary::of(name, &archive, bytes.len() as u64))
    }

    pub fn export_to(
        &self,
        destination: &Path,
        archive: &Archive,
        seal: &dyn Seal,
        allow_overwrite: bool,
    ) -> AppResult<()> {
        let bytes = seal.seal(&archive.to_json()?)?;
        write_private_mode(destination, &bytes, allow_overwrite)
    }

    /// Decrypts and validates an exported file without touching live data. Wrong keys, corrupt
    /// or truncated files and newer formats all fail here, leaving everything as it was.
    pub fn import_from(&self, source: &Path, seal: &dyn Seal) -> AppResult<Archive> {
        let bytes = std::fs::read(source)
            .map_err(|_| AppError::Other("the selected file could not be read".into()))?;
        open_archive(&bytes, seal)
    }

    /// Review-before-apply for exports: opens the archive read-only and reports its date and
    /// counts. Nothing in this path can modify live data.
    pub fn inspect_file(&self, source: &Path, seal: &dyn Seal) -> AppResult<ArchiveSummary> {
        let bytes = std::fs::read(source)
            .map_err(|_| AppError::Other("the selected file could not be read".into()))?;
        let archive = open_archive(&bytes, seal)?;
        Ok(ArchiveSummary::of(
            &source.to_string_lossy(),
            &archive,
            bytes.len() as u64,
        ))
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
            "the file could not be decrypted with this machine's key or its recovery \
             credential (wrong archive, wrong credential, or the file is damaged)"
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
    write_private_mode(path, bytes, false)
}

fn refuses_overwrite() -> AppError {
    AppError::Other(
        "that file already exists - nothing was overwritten. choose another name, or confirm \
         that the existing file may be replaced"
            .into(),
    )
}

/// Publishes a fully written, already-synced staging file at `path` **without any possibility
/// of replacing** an existing file. `hard_link` either creates the new name or fails with
/// `AlreadyExists`, so unlike `rename` it has no window in which a destination that appeared
/// after the caller's own existence check gets clobbered. The staging name is in the same
/// directory, so the link is local and the content is visible at `path` the instant the link
/// exists.
pub(crate) fn publish_no_replace(staged: &Path, path: &Path) -> AppResult<()> {
    match std::fs::hard_link(staged, path) {
        Ok(()) => {
            std::fs::remove_file(staged).ok();
            Ok(())
        }
        Err(e) => {
            std::fs::remove_file(staged).ok();
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                return Err(refuses_overwrite());
            }
            Err(AppError::Other(e.to_string()))
        }
    }
}

/// Publishes a staging file by replacing whatever is at `path`. Only for the explicit,
/// user-confirmed overwrite route; the ordinary publish is [`publish_no_replace`].
pub(crate) fn publish_replace(staged: &Path, path: &Path) -> AppResult<()> {
    match std::fs::rename(staged, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            std::fs::remove_file(staged).ok();
            Err(AppError::Other(e.to_string()))
        }
    }
}

/// Atomic 0600 write. The staging file is unique per call (O_EXCL in the destination
/// directory, so concurrent writers can never clobber each other's partial file), is removed
/// on any failure, and only ever reaches its destination through a same-directory publish.
/// Without `allow_overwrite` the publish itself is no-replace: an existing destination -
/// including one created after this call started - is never overwritten.
fn write_private_mode(path: &Path, bytes: &[u8], allow_overwrite: bool) -> AppResult<()> {
    if path.exists() && !allow_overwrite {
        return Err(refuses_overwrite());
    }
    let parent = path.parent().ok_or_else(|| {
        AppError::Validation("the destination path has no directory to write into".into())
    })?;
    std::fs::create_dir_all(parent).map_err(|e| AppError::Other(e.to_string()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::Validation("the destination has no file name".into()))?
        .to_string_lossy()
        .to_string();
    let tmp = parent.join(format!(
        ".{}.{}.{}.part",
        file_name,
        std::process::id(),
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    ));
    {
        use std::io::Write;
        let mut file = match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)
        {
            Ok(file) => file,
            Err(e) => return Err(AppError::Other(e.to_string())),
        };
        match file.write_all(bytes).and_then(|()| file.sync_all()) {
            Ok(()) => {}
            Err(e) => {
                drop(file);
                std::fs::remove_file(&tmp).ok();
                return Err(AppError::Other(e.to_string()));
            }
        }
    }
    crate::paths::set_file_private(&tmp)?;
    if allow_overwrite {
        publish_replace(&tmp, path)
    } else {
        publish_no_replace(&tmp, path)
    }
}

/// Sealed, private, never-replacing write used by the recovery journal.
pub(crate) fn write_private_new(path: &Path, bytes: &[u8]) -> AppResult<()> {
    write_private_mode(path, bytes, false)
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
            .export_to(&export, &archive(), &key, false)
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
            .export_to(&export, &archive(), &key, false)
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
            .export_to(&newer_path, &newer, &key, false)
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
        manager
            .export_to(&path, &duplicate, &key, false)
            .expect("export");
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
        manager
            .export_to(&path, &nameless, &key, false)
            .expect("export");
        assert!(manager.import_from(&path, &key).is_err());

        // A backup name from outside the manager's own directory is refused outright.
        assert!(manager.read_backup("../escape.age", &key).is_err());
        assert!(manager.read_backup("/etc/passwd", &key).is_err());
        dir.erase();
    }

    fn identity_pair() -> (String, String) {
        let (identity, secret) = crate::vault::generate_identity();
        drop(identity);
        let (other, other_secret) = crate::vault::generate_identity();
        drop(other);
        (secret, other_secret)
    }

    #[test]
    fn a_recovery_credential_restores_an_archive_into_a_fresh_profile() {
        let dir = Dir::new();
        let (original_key, _) = identity_pair();
        let (recovery_secret, _) = identity_pair();

        // Old profile: seal exactly the way the app does - vault identity + recovery identity.
        let old_seal = ArchiveSeal::from_secrets(&original_key, &recovery_secret).expect("seal");
        let export = dir.0.join("portable.age");
        BackupManager::new(dir.0.join("backups"), 4)
            .expect("manager")
            .export_to(&export, &archive(), &old_seal, false)
            .expect("export");

        // Fresh profile: the original keyring is gone and this machine generated a brand-new
        // vault identity. The recovery credential alone must open the archive.
        let (fresh_key, _) = identity_pair();
        let fresh_seal =
            ArchiveSeal::from_secrets(&fresh_key, &recovery_secret).expect("fresh seal");
        let manager = dir.manager(4);
        let restored = manager
            .import_from(&export, &fresh_seal)
            .expect("recovery open");
        assert_eq!(restored.commands.len(), 1);
        assert_eq!(restored.secrets.records.len(), 1);
        assert_eq!(restored.secrets.records[0].name, "Registry token");
        assert_eq!(restored.secrets.records[0].value, "UNIQUE-SECRET-marker");
        // ...and the original credential still works too (sealed to both recipients).
        assert!(manager.import_from(&export, &old_seal).is_ok());
        dir.erase();
    }

    #[test]
    fn wrong_or_missing_recovery_credentials_are_rejected() {
        let dir = Dir::new();
        let (original_key, _) = identity_pair();
        let (recovery_secret, other_recovery) = identity_pair();
        let (fresh_key, _) = identity_pair();
        let seal = ArchiveSeal::from_secrets(&original_key, &recovery_secret).expect("seal");
        let export = dir.0.join("portable.age");
        dir.manager(4)
            .export_to(&export, &archive(), &seal, false)
            .expect("export");

        // Wrong recovery credential, brand-new vault identity: refused.
        let wrong = ArchiveSeal::from_secrets(&fresh_key, &other_recovery).expect("seal");
        let failure = dir
            .manager(4)
            .import_from(&export, &wrong)
            .expect_err("wrong recovery key must fail");
        assert!(failure.to_string().contains("recovery credential"));

        // Fresh machine without the recovery credential at all: also refused (a new profile's
        // own vault key must never open someone else's archive).
        let none = KeySeal::from_secret(&fresh_key).expect("seal");
        assert!(dir.manager(4).import_from(&export, &none).is_err());
        // A wrong recovery credential must not even leak the plaintext into the summary path.
        assert!(dir.manager(4).inspect_file(&export, &wrong).is_err());
        dir.erase();
    }

    #[test]
    fn a_recovery_key_is_created_once_kept_private_and_reusable() {
        let dir = Dir::new();
        let path = dir.0.join("recovery.agekey");
        let first = RecoveryKey::load_or_create(path.clone()).expect("create");
        let second = RecoveryKey::load_or_create(path.clone()).expect("load");
        assert_eq!(first.secret(), second.secret(), "the credential is stable");
        assert!(first.secret().starts_with("AGE-SECRET-KEY-1"));
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the recovery file must be private");

        // It really is the recipient the archives use.
        let archive_seal =
            ArchiveSeal::from_secrets(&crate::vault::generate_identity().1, first.secret())
                .expect("seal");
        let export = dir.0.join("portable.age");
        dir.manager(4)
            .export_to(&export, &archive(), &archive_seal, false)
            .expect("export");
        let opener =
            ArchiveSeal::from_secrets(&crate::vault::generate_identity().1, second.secret())
                .expect("reopen");
        assert!(dir.manager(4).import_from(&export, &opener).is_ok());
        dir.erase();
    }

    #[test]
    fn an_export_never_silently_overwrites_and_leaves_no_staging_files() {
        let dir = Dir::new();
        let (key, _) = Dir::keys();
        let manager = dir.manager(4);
        let export = dir.0.join("portable.age");
        manager
            .export_to(&export, &archive(), &key, false)
            .expect("first export");
        let first_bytes = std::fs::read(&export).expect("read");

        // Second export to the same destination: refused, original bytes untouched.
        let failure = manager
            .export_to(&export, &archive(), &key, false)
            .expect_err("must not overwrite silently");
        assert!(failure.to_string().contains("already exists"));
        assert_eq!(std::fs::read(&export).expect("reread"), first_bytes);

        // Only an explicit confirmation replaces the file.
        manager
            .export_to(&export, &archive(), &key, true)
            .expect("confirmed export");
        assert!(!std::fs::read(&export).expect("read again").is_empty());

        // No `.part` staging leftovers anywhere in the directory.
        for entry in std::fs::read_dir(&dir.0).expect("read dir") {
            let name = entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .to_string();
            assert!(
                !name.starts_with('.') && !name.ends_with(".part"),
                "staging file left behind: {name}"
            );
        }
        dir.erase();
    }

    #[test]
    fn a_destination_that_appears_mid_export_is_never_overwritten() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Dir::new();
        let (key, _) = Dir::keys();
        let manager = dir.manager(4);
        let destination = dir.0.join("portable.age");

        // The exact state of an export that is about to publish, plus a competing file created
        // after the caller's existence check - the window a `exists()` guard cannot cover.
        let staged = dir.0.join(".staged.age");
        std::fs::write(&staged, b"archive-we-would-have-written").expect("stage");
        std::fs::write(&destination, b"someone-elses-file").expect("competing creation");

        let error = publish_no_replace(&staged, &destination).expect_err("no-replace must refuse");
        assert!(error.to_string().contains("already exists"), "{error}");
        assert_eq!(
            std::fs::read(&destination).expect("reread"),
            b"someone-elses-file",
            "the competing file stays byte for byte"
        );
        assert!(
            !staged.exists(),
            "the staging file is cleaned up, not published"
        );

        // The same publish, with the destination free, writes the file and keeps it private.
        let free = dir.0.join("free.age");
        let staged_again = dir.0.join(".staged2.age");
        std::fs::write(&staged_again, b"payload").expect("stage again");
        crate::paths::set_file_private(&staged_again).expect("private stage");
        publish_no_replace(&staged_again, &free).expect("publish");
        assert_eq!(std::fs::read(&free).expect("published"), b"payload");
        assert_eq!(
            std::fs::metadata(&free).expect("meta").permissions().mode() & 0o777,
            0o600,
            "a published file is user-only"
        );

        // And the ordinary export route still refuses up front, leaving the old bytes alone,
        // while only an explicit confirmation replaces them.
        let export = dir.0.join("clean.age");
        manager
            .export_to(&export, &archive(), &key, false)
            .expect("first export");
        let first = std::fs::read(&export).expect("first bytes");
        assert!(manager.export_to(&export, &archive(), &key, false).is_err());
        assert_eq!(std::fs::read(&export).expect("reread"), first);
        manager
            .export_to(&export, &archive(), &key, true)
            .expect("confirmed export");
        dir.erase();
    }

    #[test]
    fn inspect_reports_date_and_counts_without_touching_anything() {
        let dir = Dir::new();
        let (key, _) = Dir::keys();
        let manager = dir.manager(4);
        let name = manager.create(&archive(), &key).expect("create");
        let summary = manager.inspect_backup(&name, &key).expect("inspect");
        assert_eq!(summary.commands, 1);
        assert_eq!(summary.secrets, 1);
        assert!(summary.size_bytes > 0);
        assert!(!summary.created_at.is_empty());
        assert!(manager.inspect_backup("../escape.age", &key).is_err());
        // Inspection is read-only: still exactly one backup afterwards.
        assert_eq!(manager.list().expect("list").len(), 1);
        dir.erase();
    }
}
