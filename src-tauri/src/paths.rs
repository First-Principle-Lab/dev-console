use std::path::PathBuf;

use crate::error::{AppError, AppResult};

/// XDG-aware locations with a test/CI override.
///
/// `DEV_CONSOLE_DATA_DIR` and `DEV_CONSOLE_CONFIG_DIR` exist so tests and packaged smoke
/// runs can use disposable profiles and never touch a real user database or vault.
pub fn data_dir() -> AppResult<PathBuf> {
    if let Ok(dir) = std::env::var("DEV_CONSOLE_DATA_DIR") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let base = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        format!(
            "{}/.local/share",
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
        )
    });
    Ok(PathBuf::from(base).join("dev-console"))
}

pub fn config_dir() -> AppResult<PathBuf> {
    if let Ok(dir) = std::env::var("DEV_CONSOLE_CONFIG_DIR") {
        if !dir.trim().is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));
    Ok(PathBuf::from(base).join("dev-console"))
}

/// Creates the directory with user-only permissions (0700) when it does not exist.
pub fn ensure_dir(path: &std::path::Path) -> AppResult<PathBuf> {
    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|e| AppError::Other(e.to_string()))?;
    }
    set_dir_private(path)?;
    Ok(path.to_path_buf())
}

pub fn set_dir_private(path: &std::path::Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

pub fn set_file_private(path: &std::path::Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}

pub fn database_path() -> AppResult<PathBuf> {
    Ok(ensure_dir(&data_dir()?)?.join("commands.sqlite3"))
}

pub fn vault_path() -> AppResult<PathBuf> {
    Ok(ensure_dir(&data_dir()?)?.join("secrets.vault"))
}

pub fn lock_flag_path() -> AppResult<PathBuf> {
    Ok(ensure_dir(&config_dir()?)?.join("manual-lock"))
}

pub fn settings_path() -> AppResult<PathBuf> {
    Ok(ensure_dir(&config_dir()?)?.join("settings.json"))
}
