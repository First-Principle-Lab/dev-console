use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::models::Settings;
use crate::paths;

/// Settings live outside the vault: they contain no secrets. Written atomically so an
/// interrupted save cannot leave a truncated file behind.
pub struct SettingsStore {
    path: PathBuf,
    cached: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> AppResult<Self> {
        let settings = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            serde_json::from_str::<Settings>(&text).unwrap_or_default()
        } else {
            Settings::default()
        };
        Ok(SettingsStore {
            path,
            cached: Mutex::new(settings),
        })
    }

    pub fn get(&self) -> Settings {
        self.cached.lock().expect("settings lock poisoned").clone()
    }

    pub fn save(&self, settings: Settings) -> AppResult<Settings> {
        let mut settings = settings;
        settings.clipboard_expiry_seconds = settings.clipboard_expiry_seconds.clamp(5, 3600);
        if settings.terminal.is_empty() {
            settings.terminal = Settings::default().terminal;
        }
        if !matches!(settings.theme.as_str(), "system" | "light" | "dark") {
            return Err(AppError::Validation(
                "theme must be system, light or dark".into(),
            ));
        }
        let json = serde_json::to_string_pretty(&settings)?;
        atomic_write(&self.path, json.as_bytes())?;
        *self.cached.lock().expect("settings lock poisoned") = settings.clone();
        Ok(settings)
    }
}

/// Write to a sibling temp file, fsync, then rename. `sync_all` before rename is what makes
/// the write survive a crash; a failed write leaves the previous file untouched.
pub fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> AppResult<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Validation("missing parent directory".into()))?;
    std::fs::create_dir_all(parent).map_err(|e| AppError::Other(e.to_string()))?;
    paths::set_dir_private(parent)?;
    let tmp = parent.join(format!("{}.tmp-{}", file_name(path), std::process::id()));
    {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| AppError::Other(e.to_string()))?;
        file.write_all(bytes)
            .map_err(|e| AppError::Other(e.to_string()))?;
        file.flush().map_err(|e| AppError::Other(e.to_string()))?;
        file.sync_all()
            .map_err(|e| AppError::Other(e.to_string()))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::Other(e.to_string())
    })?;
    paths::set_file_private(path)?;
    Ok(())
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into())
}
