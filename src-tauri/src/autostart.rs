use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

const AUTOSTART_HEADER: &str = "[Desktop Entry]";
pub const AUTOSTART_NAME: &str = "dev-console.desktop";

fn autostart_file() -> AppResult<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));
    Ok(PathBuf::from(base).join("autostart").join(AUTOSTART_NAME))
}

fn desktop_entry(exec: &str) -> String {
    format!(
        "{header}\nType=Application\nName=Dev Console\nComment=Commands and secrets console\nExec={exec} --background\nIcon=dev-console\nTerminal=false\nX-GNOME-Autostart-enabled=true\nHidden=false\nNoDisplay=true\n",
        header = AUTOSTART_HEADER
    )
}

/// Opt-in "start at login". Writes only this app's own autostart file; anything else in the
/// user's autostart directory is never touched.
pub fn apply(enabled: bool) -> AppResult<()> {
    if !enabled {
        return remove_at(&autostart_file()?);
    }
    let exec = std::env::current_exe()
        .map_err(|e| AppError::Other(e.to_string()))?
        .to_string_lossy()
        .to_string();
    apply_to(&autostart_file()?, &exec)
}

/// The part that touches the filesystem, split out so the behaviour can be tested against a
/// disposable directory instead of a real `$XDG_CONFIG_HOME`.
pub fn apply_to(path: &Path, exec: &str) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::Other(e.to_string()))?;
    }
    std::fs::write(path, desktop_entry(exec)).map_err(|e| AppError::Other(e.to_string()))?;
    crate::paths::set_file_private(path)
}

pub fn remove_at(path: &Path) -> AppResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabling_autostart_writes_only_this_apps_own_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let other = dir.path().join("autostart").join("other-desktop.desktop");
        std::fs::create_dir_all(other.parent().expect("parent")).expect("mkdir");
        std::fs::write(
            &other,
            "[Desktop Entry]\nType=Application\nName=Something Else\n",
        )
        .expect("write");

        let path = dir.path().join("autostart").join(AUTOSTART_NAME);
        apply_to(&path, "/usr/bin/dev-console").expect("apply");

        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.starts_with(AUTOSTART_HEADER));
        assert!(text.contains("Exec=/usr/bin/dev-console --background"));
        assert!(text.contains("X-GNOME-Autostart-enabled=true"));
        assert!(text.contains("NoDisplay=true"));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777,
            0o600
        );

        apply_to(&path, "/usr/bin/dev-console").expect("re-apply");
        assert!(
            other.exists(),
            "another application's autostart entry survived"
        );
        remove_at(&path).expect("remove");
        assert!(!path.exists());
        assert!(
            other.exists(),
            "removal must not touch foreign entries either"
        );

        remove_at(&other).expect("foreign removal is refused, not an error");
        assert!(other.exists());
    }
}
