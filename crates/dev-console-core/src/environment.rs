//! Project .env history lives inside the encrypted vault, including file contents and paths.
//! Metadata-only responses and native secret clipboard copies keep values out of the webview.
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::vault::VaultService;

const MAX_FILE: usize = 1024 * 1024;
const MAX_HISTORY: usize = 32 * 1024 * 1024;
const MAX_FILES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSnapshot {
    pub id: String,
    pub file: String,
    pub version: u32,
    pub saved_at: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentProject {
    pub root: String,
    pub enabled: bool,
    pub snapshots: Vec<EnvironmentSnapshot>,
}

/// One saved version of an environment file: metadata only, never the file's contents.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub file: String,
    pub version: u32,
    pub saved_at: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncResult {
    pub root: String,
    pub projects: Vec<ProjectMeta>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    pub root: String,
    pub enabled: bool,
    pub snapshots: Vec<SnapshotMeta>,
}

pub fn list(vault: &VaultService) -> AppResult<Vec<ProjectMeta>> {
    vault.read(|file| {
        Ok(file
            .environments
            .iter()
            .map(|p| ProjectMeta {
                root: p.root.clone(),
                enabled: p.enabled,
                snapshots: p
                    .snapshots
                    .iter()
                    .rev()
                    .map(|s| SnapshotMeta {
                        id: s.id.clone(),
                        file: s.file.clone(),
                        version: s.version,
                        saved_at: s.saved_at.clone(),
                        bytes: s.content.len(),
                    })
                    .collect(),
            })
            .collect())
    })
}

fn env_name(name: &str) -> bool {
    name == ".env" || name.starts_with(".env.")
}

fn invalid(message: &str) -> AppError {
    AppError::Validation(message.into())
}

/// No recursion, symlinks, FIFOs or arbitrary filenames. Reads are bounded on the open handle,
/// so a file growing or changing type between enumeration and opening cannot bypass the limits.
fn read_files(root: &Path) -> AppResult<Vec<(String, String)>> {
    let mut files = Vec::new();
    for item in std::fs::read_dir(root)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().to_string();
        if !env_name(&name) || !item.file_type()?.is_file() {
            continue;
        }
        if files.len() >= MAX_FILES {
            return Err(invalid(
                "Too many environment files. Limit: 64 per project.",
            ));
        }
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(item.path())?;
        if !handle.metadata()?.is_file() {
            return Err(invalid(
                "An environment file changed while reading. Try again.",
            ));
        }
        let mut data = Vec::new();
        handle.take((MAX_FILE + 1) as u64).read_to_end(&mut data)?;
        if data.len() > MAX_FILE {
            return Err(invalid(
                "An environment file exceeds the 1 MiB limit. Nothing was synced.",
            ));
        }
        let content = String::from_utf8(data).map_err(|_| {
            invalid("Environment files must contain UTF-8 text. Nothing was synced.")
        })?;
        files.push((name, content));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// First explicit sync opts this project in. Automatic sync never registers new projects or
/// resumes a paused project. Identical files cause no vault rewrite and consume no version.
pub fn sync(vault: &VaultService, path: &str, explicit: bool) -> AppResult<()> {
    sync_impl(vault, path, explicit, explicit)
}

/// A directory visit discovers projects without requiring a library entry or manual addition.
/// Root files and an entered monorepo subfolder with its own env files are tracked separately.
pub fn visit(vault: &VaultService, path: &str) -> AppResult<()> {
    vault.authorise()?;
    let cwd = std::fs::canonicalize(path)?;
    let home = std::env::var_os("HOME").and_then(|p| std::fs::canonicalize(p).ok());
    if home.as_ref() == Some(&cwd) {
        return Ok(());
    }
    let project = crate::project_history::project_for_directory(path);
    let has_env = || -> AppResult<bool> {
        for item in std::fs::read_dir(&cwd)? {
            let item = item?;
            if env_name(&item.file_name().to_string_lossy()) && item.file_type()?.is_file() {
                return Ok(true);
            }
        }
        Ok(false)
    };
    if let Some(project) = project {
        sync_impl(vault, &project.root.to_string_lossy(), true, false)?;
        if cwd != project.root && has_env()? {
            sync_impl(vault, &cwd.to_string_lossy(), true, false)?;
        }
    } else if has_env()? {
        sync_impl(vault, &cwd.to_string_lossy(), true, false)?;
    }
    Ok(())
}

fn sync_impl(vault: &VaultService, path: &str, register: bool, force: bool) -> AppResult<()> {
    // Serialize filesystem reads as well as commits. Otherwise a slow terminal scan can
    // commit an old file after a newer manual/background scan has already saved it.
    let _sync = vault
        .environment_sync
        .lock()
        .expect("environment sync lock poisoned");
    vault.authorise()?;
    let root = std::fs::canonicalize(path)?;
    if !root.is_dir() {
        return Err(invalid("Choose a project folder."));
    }
    let root = root.to_string_lossy().to_string();
    let before = vault.read(|v| Ok(v.environments.iter().find(|p| p.root == root).cloned()))?;
    if !force && before.as_ref().map_or(!register, |p| !p.enabled) {
        return Ok(());
    }
    let files = read_files(Path::new(&root))?;
    let changed = before.as_ref().is_none_or(|p| {
        files.iter().any(|(name, content)| {
            p.snapshots
                .iter()
                .rev()
                .find(|s| &s.file == name)
                .is_none_or(|s| &s.content != content)
        })
    });
    if !changed {
        return Ok(());
    }
    vault.write(|v| {
        // Recheck under the write lock: a pause or concurrent sync may have happened during IO.
        let index = match v.environments.iter().position(|p| p.root == root) {
            Some(i) => i,
            None if register => {
                v.environments.push(EnvironmentProject { root: root.clone(), enabled: true, snapshots: vec![] });
                v.environments.len() - 1
            }
            None => return Ok(()),
        };
        if !force && !v.environments[index].enabled { return Ok(()); }
        let mut total: usize = v.environments.iter().flat_map(|p| &p.snapshots).map(|s| s.content.len()).sum();
        let p = &mut v.environments[index];
        for (name, content) in files {
            let last = p.snapshots.iter().rev().find(|s| s.file == name);
            if last.is_some_and(|s| s.content == content) { continue; }
            total += content.len();
            if total > MAX_HISTORY { return Err(invalid("Environment history reached 32 MiB. Existing versions were kept; new changes were not synced.")); }
            let version = last.map_or(1, |s| s.version + 1);
            p.snapshots.push(EnvironmentSnapshot { id: uuid::Uuid::new_v4().to_string(), file: name,
                version, saved_at: crate::repository::now_rfc3339(), content });
        }
        Ok(())
    })
}

pub fn set_enabled(vault: &VaultService, root: &str, enabled: bool) -> AppResult<()> {
    vault.write(|v| {
        v.environments
            .iter_mut()
            .find(|p| p.root == root)
            .ok_or(AppError::NotFound)?
            .enabled = enabled;
        Ok(())
    })
}

pub fn validate(projects: &[EnvironmentProject]) -> AppResult<()> {
    let mut roots = std::collections::HashSet::new();
    let mut ids = std::collections::HashSet::new();
    let mut total = 0;
    for p in projects {
        if !Path::new(&p.root).is_absolute() || !roots.insert(&p.root) {
            return Err(invalid("Invalid environment project in archive."));
        }
        for s in &p.snapshots {
            total += s.content.len();
            if !env_name(&s.file)
                || s.file.contains('/')
                || s.file.contains('\\')
                || s.version == 0
                || s.content.len() > MAX_FILE
                || !ids.insert(&s.id)
                || total > MAX_HISTORY
            {
                return Err(invalid("Invalid environment history in archive."));
            }
        }
    }
    Ok(())
}

pub fn spawn_sync_worker(state: Arc<crate::state::AppState>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
        if state.quitting.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let roots = state.vault.read(|v| {
            Ok(v.environments
                .iter()
                .filter(|p| p.enabled)
                .map(|p| p.root.clone())
                .collect::<Vec<_>>())
        });
        if let Ok(roots) = roots {
            for root in roots {
                let _ = sync(&state.vault, &root, false);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{Credential, VaultFile};

    fn vault(dir: &Path) -> VaultService {
        let (_, key) = crate::vault::generate_identity();
        let service = VaultService::new(
            dir.join("vault.age"),
            dir.join("locked"),
            Credential::Provided { key },
        );
        service.bootstrap();
        assert!(service.is_unlocked());
        service
    }

    #[test]
    fn versions_are_per_project_file_deduplicated_and_survive_missing_sources_and_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();
        let v = vault(dir.path());
        let sync_a = || sync(&v, a.to_str().unwrap(), true).unwrap();
        std::fs::write(a.join(".env"), "SYNTHETIC=one\n").unwrap();
        std::fs::write(a.join(".env.local"), "SYNTHETIC=local\n").unwrap();
        std::fs::write(a.join("other.txt"), "ignored").unwrap();
        sync_a();
        let generation = v.generation();
        sync_a();
        assert_eq!(
            generation,
            v.generation(),
            "unchanged sync does not rewrite the vault"
        );
        std::fs::write(a.join(".env"), "SYNTHETIC=two\n").unwrap();
        sync_a();
        std::fs::write(b.join(".env"), "SYNTHETIC=other\n").unwrap();
        sync(&v, b.to_str().unwrap(), true).unwrap();
        std::fs::remove_file(a.join(".env")).unwrap();
        sync_a();
        std::fs::remove_dir_all(&a).unwrap();
        assert!(sync(&v, a.to_str().unwrap(), true).is_err());
        let metadata = serde_json::to_string(&list(&v).unwrap()).unwrap();
        assert!(!metadata.contains("SYNTHETIC"));
        assert!(!String::from_utf8_lossy(&std::fs::read(v.path()).unwrap()).contains("SYNTHETIC"));
        let saved = v.snapshot().unwrap();
        let p = &saved.environments[0];
        assert_eq!(p.snapshots.len(), 3);
        assert_eq!(p.snapshots[2].version, 2);
        assert!(!p.snapshots[2].saved_at.is_empty());
        assert_eq!(saved.environments[1].snapshots[0].version, 1);
        v.lock().unwrap();
        assert!(list(&v).is_err());
        assert!(sync(&v, b.to_str().unwrap(), true).is_err());
        v.unlock_with(|| Ok(())).unwrap();
        assert_eq!(v.snapshot().unwrap(), saved);
        let ciphertext = v
            .seal_to_recipients(&serde_json::to_vec(&saved).unwrap(), &[])
            .unwrap();
        let decoded: VaultFile =
            serde_json::from_slice(&v.open_sealed(&ciphertext).unwrap()).unwrap();
        assert_eq!(decoded, saved);
    }

    #[test]
    fn pause_blocks_automatic_sync_and_symlinks_and_nested_files_are_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir(&root).unwrap();
        let v = vault(dir.path());
        let path = root.to_str().unwrap();
        std::fs::write(root.join(".env"), "SYNTHETIC=before").unwrap();
        std::fs::write(dir.path().join("outside"), "SYNTHETIC=outside").unwrap();
        std::os::unix::fs::symlink(dir.path().join("outside"), root.join(".env.link")).unwrap();
        std::fs::create_dir(root.join("nested")).unwrap();
        std::fs::write(root.join("nested/.env"), "SYNTHETIC=nested").unwrap();
        sync(&v, path, false).unwrap();
        assert!(list(&v).unwrap().is_empty());
        sync(&v, path, true).unwrap();
        let canonical = std::fs::canonicalize(&root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        set_enabled(&v, &canonical, false).unwrap();
        std::fs::write(root.join(".env"), "SYNTHETIC=after").unwrap();
        sync(&v, path, false).unwrap();
        assert_eq!(v.snapshot().unwrap().environments[0].snapshots.len(), 1);
        set_enabled(&v, &canonical, true).unwrap();
        sync(&v, path, false).unwrap();
        assert_eq!(v.snapshot().unwrap().environments[0].snapshots.len(), 2);
    }

    #[test]
    fn oversize_or_invalid_text_does_not_partially_save_a_project() {
        let dir = tempfile::tempdir().unwrap();
        let v = vault(dir.path());
        std::fs::write(dir.path().join(".env"), "SYNTHETIC=valid").unwrap();
        std::fs::write(dir.path().join(".env.large"), vec![b'x'; MAX_FILE + 1]).unwrap();
        assert!(sync(&v, dir.path().to_str().unwrap(), true).is_err());
        assert!(list(&v).unwrap().is_empty());
        std::fs::write(dir.path().join(".env.large"), [0xff]).unwrap();
        assert!(sync(&v, dir.path().to_str().unwrap(), true).is_err());
        assert!(list(&v).unwrap().is_empty());
    }

    #[test]
    fn entering_a_directory_discovers_environments_without_adding_a_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let nested = root.join("server");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(root.join(".env"), "SYNTHETIC=root").unwrap();
        std::fs::write(nested.join(".env.local"), "SYNTHETIC=server").unwrap();
        let v = vault(dir.path());
        visit(&v, nested.to_str().unwrap()).unwrap();
        let saved = v.snapshot().unwrap();
        assert_eq!(saved.environments.len(), 2);
        assert_eq!(saved.environments[0].snapshots[0].content, "SYNTHETIC=root");
        assert_eq!(
            saved.environments[1].snapshots[0].content,
            "SYNTHETIC=server"
        );
        let generation = v.generation();
        visit(&v, nested.to_str().unwrap()).unwrap();
        assert_eq!(generation, v.generation());
        let canonical = std::fs::canonicalize(&root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        set_enabled(&v, &canonical, false).unwrap();
        std::fs::write(root.join(".env"), "SYNTHETIC=paused").unwrap();
        visit(&v, root.to_str().unwrap()).unwrap();
        assert_eq!(v.snapshot().unwrap().environments[0].snapshots.len(), 1);
        let plain = dir.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        std::fs::write(plain.join(".env"), "SYNTHETIC=plain").unwrap();
        visit(&v, plain.to_str().unwrap()).unwrap();
        assert_eq!(v.snapshot().unwrap().environments.len(), 3);
        v.lock().unwrap();
        assert!(visit(&v, plain.to_str().unwrap()).is_err());
    }

    #[test]
    fn queued_scans_read_the_file_after_acquiring_the_sync_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join(".env"), "SYNTHETIC=old").unwrap();
        let v = Arc::new(vault(dir.path()));
        let guard = v.environment_sync.lock().unwrap();
        let (done, received) = std::sync::mpsc::channel();
        let worker = {
            let v = Arc::clone(&v);
            let root = root.clone();
            std::thread::spawn(move || {
                done.send(sync(&v, root.to_str().unwrap(), true)).unwrap();
            })
        };
        assert!(matches!(
            received.recv_timeout(std::time::Duration::from_millis(100)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        std::fs::write(root.join(".env"), "SYNTHETIC=new").unwrap();
        drop(guard);
        received
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        worker.join().unwrap();
        let saved = v.snapshot().unwrap();
        assert_eq!(saved.environments[0].snapshots.len(), 1);
        assert_eq!(saved.environments[0].snapshots[0].content, "SYNTHETIC=new");
    }

    #[test]
    fn old_vaults_load_without_environment_history() {
        let old: VaultFile =
            serde_json::from_str(r#"{"version":1,"records":[],"usage":[]}"#).unwrap();
        assert!(old.environments.is_empty());
    }
}
