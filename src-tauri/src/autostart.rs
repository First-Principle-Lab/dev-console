use std::path::PathBuf;

use crate::error::{AppError, AppResult};

const AUTOSTART_HEADER: &str = "[Desktop Entry]";

fn autostart_file() -> AppResult<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));
    Ok(PathBuf::from(base)
        .join("autostart")
        .join("dev-console.desktop"))
}

fn desktop_entry(exec: &str) -> String {
    format!(
        "{header}\nType=Application\nName=Dev Console\nComment=Commands and secrets console\nExec={exec} --background\nTerminal=false\nX-GNOME-Autostart-enabled=true\nHidden=false\n",
        header = AUTOSTART_HEADER
    )
}

/// Opt-in "start at login". Writes only this app's own autostart file; anything else in the
/// user's autostart directory is never touched.
pub fn apply(enabled: bool) -> AppResult<()> {
    let path = autostart_file()?;
    if !enabled {
        return remove_at(&path);
    }
    let exec = std::env::current_exe()
        .map_err(|e| AppError::Other(e.to_string()))?
        .to_string_lossy()
        .to_string();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::Other(e.to_string()))?;
    }
    std::fs::write(&path, desktop_entry(&exec)).map_err(|e| AppError::Other(e.to_string()))?;
    crate::paths::set_file_private(&path).ok();
    Ok(())
}

pub fn remove_at(path: &std::path::Path) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    // Refuse to delete a file this app did not create.
    if text.contains("dev-console") || text.contains("Dev Console") {
        std::fs::remove_file(path).map_err(|e| AppError::Other(e.to_string()))?;
    }
    Ok(())
}

pub fn is_enabled() -> bool {
    autostart_file().map(|path| path.exists()).unwrap_or(false)
}
