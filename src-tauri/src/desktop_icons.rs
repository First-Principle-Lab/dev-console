use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// The launcher icon as shipped in this build, one PNG per hicolor size.
const ICONS: [(&str, &[u8]); 4] = [
    ("32x32", include_bytes!("../icons/32x32.png")),
    ("128x128", include_bytes!("../icons/128x128.png")),
    ("256x256", include_bytes!("../icons/icon.png")),
    ("512x512", include_bytes!("../icons/icon@2x.png")),
];

/// `Icon=dev-console` in the desktop entry resolves through the icon theme, not the binary.
/// The in-app updater replaces only the AppImage, so the theme kept serving whatever PNGs an
/// earlier install left behind. The user data dir is searched before /usr/local and /usr, so
/// writing this build's icons there makes the launcher match the running version.
///
/// Only AppImage runs do this: a distro package owns its icons and updates them itself.
pub fn refresh_for_appimage() {
    if std::env::var_os("APPIMAGE").is_none() {
        return;
    }
    let Some(base) = user_icon_theme_dir() else {
        return;
    };
    match refresh_at(&base) {
        Ok(0) => {}
        Ok(written) => eprintln!("dev-console: refreshed {written} launcher icon(s)"),
        Err(error) => eprintln!("dev-console: launcher icon refresh skipped ({error})"),
    }
}

fn user_icon_theme_dir() -> Option<PathBuf> {
    let base = match std::env::var("XDG_DATA_HOME") {
        Ok(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".local/share"),
    };
    Some(base.join("icons/hicolor"))
}

/// Writes every icon whose bytes differ from what is on disk and returns how many it wrote.
/// Unchanged icons are left alone so a normal start does not touch the icon theme.
pub fn refresh_at(theme_dir: &Path) -> AppResult<usize> {
    let mut written = 0;
    for (size, bytes) in ICONS {
        let path = theme_dir.join(size).join("apps").join("dev-console.png");
        if std::fs::read(&path).is_ok_and(|current| current == bytes) {
            continue;
        }
        let dir = path.parent().expect("icon path has a parent");
        std::fs::create_dir_all(dir).map_err(|e| AppError::Other(e.to_string()))?;
        // Write beside the target and rename, so a launcher never reads half an icon.
        let partial = dir.join(".dev-console.png.partial");
        std::fs::write(&partial, bytes).map_err(|e| AppError::Other(e.to_string()))?;
        std::fs::rename(&partial, &path).map_err(|e| AppError::Other(e.to_string()))?;
        written += 1;
    }
    if written > 0 {
        // Icon caches compare the theme directory's mtime; a new file in a size subdirectory
        // does not change it, so bump it explicitly.
        let now = std::fs::FileTimes::new().set_modified(std::time::SystemTime::now());
        if let Ok(dir) = std::fs::File::open(theme_dir) {
            let _ = dir.set_times(now);
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_theme(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dev-console-icons-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn writes_every_size_then_leaves_matching_icons_alone() {
        let theme = temp_theme("fresh");
        assert_eq!(refresh_at(&theme).expect("refresh"), 4);
        for (size, bytes) in ICONS {
            let path = theme.join(size).join("apps/dev-console.png");
            assert_eq!(std::fs::read(path).expect("icon"), bytes);
        }
        assert_eq!(refresh_at(&theme).expect("second refresh"), 0);
        std::fs::remove_dir_all(theme).ok();
    }

    #[test]
    fn replaces_an_icon_left_by_an_older_install() {
        let theme = temp_theme("stale");
        let stale = theme.join("128x128/apps/dev-console.png");
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(&stale, b"old logo").unwrap();
        let other = theme.join("128x128/apps/other-app.png");
        std::fs::write(&other, b"not ours").unwrap();

        assert_eq!(refresh_at(&theme).expect("refresh"), 4);
        assert_eq!(std::fs::read(&stale).unwrap(), ICONS[1].1);
        assert_eq!(std::fs::read(&other).unwrap(), b"not ours");
        std::fs::remove_dir_all(theme).ok();
    }
}
