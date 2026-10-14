use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};
use crate::models::{ClipboardExclusion, CopyResult, Kind};

const PASSWORD_HINT_TARGET: &str = "x-kde-passwordManagerHint";

#[derive(Debug, Clone)]
struct PendingCopy {
    value: String,
    expires_at: Instant,
}

/// Which clipboard history exclusion is actually available on this session.
///
/// Wayland exposes no way to mark an offering as sensitive, and on X11 only KDE-aware
/// managers honour `x-kde-passwordManagerHint`. Neither can retract a value a running
/// history manager has already archived, so the app never claims universal exclusion.
pub fn exclusion_status() -> ClipboardExclusion {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        ClipboardExclusion {
            supported: false,
            mechanism: "none".into(),
            detail: "this Wayland session exposes no clipboard-history exclusion protocol; a \
                     running clipboard manager may keep a copy, so lock the vault and use the \
                     clipboard expiry"
                .into(),
        }
    } else if std::env::var_os("DISPLAY").is_some() {
        ClipboardExclusion {
            supported: false,
            mechanism: PASSWORD_HINT_TARGET.into(),
            detail: "the value is written with the x-kde-passwordManagerHint target, which only \
                     KDE-aware managers skip; other clipboard managers may keep a copy"
                .into(),
        }
    } else {
        ClipboardExclusion {
            supported: false,
            mechanism: "none".into(),
            detail: "no clipboard provider was detected on this session".into(),
        }
    }
}

fn write_with(program: &str, args: &[&str], value: &str) -> AppResult<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| AppError::Clipboard(format!("{} is unavailable: {}", program, e.kind())))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| AppError::Clipboard("clipboard stdin unavailable".into()))?
        .write_all(value.as_bytes())
        .map_err(|e| AppError::Clipboard(e.to_string()))?;
    let status = child
        .wait()
        .map_err(|e| AppError::Clipboard(e.to_string()))?;
    if !status.success() {
        return Err(AppError::Clipboard("clipboard provider failed".into()));
    }
    Ok(())
}

fn read_with(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn write_clipboard(value: &str) -> AppResult<()> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        write_with("wl-copy", &[], value)
    } else if std::env::var_os("DISPLAY").is_some() {
        write_with(
            "xclip",
            &["-selection", "clipboard", "-target", PASSWORD_HINT_TARGET],
            value,
        )
        .or_else(|_| write_with("xclip", &["-selection", "clipboard"], value))
    } else {
        Err(AppError::Clipboard(
            "no clipboard provider on this session".into(),
        ))
    }
}

pub fn read_clipboard() -> AppResult<String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        read_with("wl-paste", &["-n"]).ok_or_else(|| AppError::Clipboard("wl-paste failed".into()))
    } else if std::env::var_os("DISPLAY").is_some() {
        read_with("xclip", &["-selection", "clipboard", "-o"])
            .ok_or_else(|| AppError::Clipboard("xclip failed".into()))
    } else {
        Err(AppError::Clipboard(
            "no clipboard provider on this session".into(),
        ))
    }
}

#[derive(Default)]
pub struct Clipboard {
    pending: Mutex<Option<PendingCopy>>,
}

impl Clipboard {
    pub fn new() -> Clipboard {
        Clipboard::default()
    }

    pub fn copy(
        &self,
        kind: Kind,
        id: &str,
        value: &str,
        expiry_seconds: u64,
    ) -> AppResult<CopyResult> {
        write_clipboard(value)?;
        let seconds = expiry_seconds.clamp(5, 3600);
        let expires_at = Instant::now() + Duration::from_secs(seconds);
        *self.pending.lock().expect("clipboard lock poisoned") = Some(PendingCopy {
            value: value.to_string(),
            expires_at,
        });
        let exclusion = exclusion_status();
        Ok(CopyResult {
            kind,
            id: id.to_string(),
            expires_at_ms: crate::repository::unix_ms() + (seconds as i64) * 1000,
            excluded: exclusion.supported,
            exclusion,
        })
    }

    /// Clears the clipboard only when it still holds the value this app copied, so anything
    /// the user copied afterwards survives. Called by the expiry ticker and by vault lock.
    pub fn expire_due(&self) -> usize {
        let mut pending = self.pending.lock().expect("clipboard lock poisoned");
        let Some(copy) = pending.as_ref() else {
            return 0;
        };
        if copy.expires_at > Instant::now() {
            return 0;
        }
        let value = copy.value.clone();
        *pending = None;
        drop(pending);
        match read_clipboard() {
            Ok(current) if current == value => {
                if write_clipboard("").is_ok() {
                    return 1;
                }
                0
            }
            _ => 0,
        }
    }

    /// Used when the vault locks: an app-owned secret copy is cleared immediately.
    pub fn clear_owned(&self) -> usize {
        let mut pending = self.pending.lock().expect("clipboard lock poisoned");
        let Some(copy) = pending.as_ref() else {
            return 0;
        };
        let value = copy.value.clone();
        *pending = None;
        drop(pending);
        if matches!(read_clipboard(), Ok(current) if current == value)
            && write_clipboard("").is_ok()
        {
            return 1;
        }
        0
    }

    pub fn pending_value_for_test(&self) -> Option<String> {
        self.pending
            .lock()
            .expect("clipboard lock poisoned")
            .as_ref()
            .map(|p| p.value.clone())
    }
}

/// Runs the expiry check on a slow timer; no secret data is logged or retained here.
pub fn spawn_expiry_worker(
    state: std::sync::Arc<crate::state::AppState>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("clipboard-expiry".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            let _ = state.clipboard.expire_due();
        })
        .expect("clipboard expiry worker")
}
