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

/// Sealed backups live here (0700). The files inside are already encrypted, but the
/// directory stays private so that neither backups nor their filenames leak to other users.
pub fn backup_dir() -> AppResult<PathBuf> {
    Ok(ensure_dir(&data_dir()?)?.join("backups"))
}

pub fn settings_path() -> AppResult<PathBuf> {
    Ok(ensure_dir(&config_dir()?)?.join("settings.json"))
}

/// Where the recovery credential lives. Unlike the vault key (OS keyring, never exportable),
/// this file is meant to be copied somewhere safe: it is the only way to open an archive
/// after the original keyring is gone. 0600, config directory, shown in Settings.
pub fn recovery_key_path() -> AppResult<PathBuf> {
    Ok(ensure_dir(&config_dir()?)?.join("recovery.agekey"))
}

/// Points both state directories at fresh 0700 temporary directories and returns their common
/// parent.
///
/// Integration tests and the packaged smoke harness call this before bootstrapping, so a test
/// run can never open (or create) a real user's database, vault or settings no matter what
/// `XDG_DATA_HOME`/`HOME` the machine has. The harnesses may still pin the profile from outside
/// with `DEV_CONSOLE_DATA_DIR`/`DEV_CONSOLE_CONFIG_DIR`; this only guarantees that a bare
/// `cargo test` stays disposable.
pub fn set_disposable_profile(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = format!(
        "dev-console-test-{tag}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    );
    let base = std::env::temp_dir().join(unique);
    let data = base.join("data");
    let config = base.join("config");
    for dir in [&data, &config] {
        std::fs::create_dir_all(dir).ok();
        let _ = set_dir_private(dir);
    }
    // Safe here: the caller owns the process and calls this before any state is constructed.
    std::env::set_var("DEV_CONSOLE_DATA_DIR", &data);
    std::env::set_var("DEV_CONSOLE_CONFIG_DIR", &config);
    base
}

/// Best-effort cleanup for `set_disposable_profile`.
pub fn remove_disposable_profile(base: &std::path::Path) {
    std::fs::remove_dir_all(base).ok();
}
