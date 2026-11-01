//! System authentication for an explicit manual unlock.
//!
//! Reading a credential out of an already-unlocked keyring is *not* authentication, so a
//! manual lock can only be cleared through a polkit-checked helper: `pkexec` asks polkit,
//! polkit asks the desktop's registered authentication agent, and only a successful
//! interaction lets us clear the flag. Cancelled or failed authentication leaves the vault
//! locked.
//!
//! Threat model, stated honestly: this is application access control for the person at the
//! keyboard. A keyring that the logged-in user can read is not protection against malicious
//! software running as that same user. Lock the desktop when you leave the machine.
//!
//! Nothing here is privileged inside the app: the helper performs no action of its own, we
//! never touch PAM or login configuration, we never lock the shared login keyring, and no
//! password is ever read, stored, or passed by this process.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{AppError, AppResult};

pub const ACTION_ID: &str = "ai.codemate.devconsole.unlock";
pub const HELPER_PATH: &str = "/usr/libexec/dev-console-auth-helper";
pub const POLICY_PATH: &str = "/usr/share/polkit-1/actions/ai.codemate.devconsole.policy";
pub const PKEXEC_PATH: &str = "/usr/bin/pkexec";

/// True only when the real mechanism is installed. Until the documented privileged install
/// step is run this returns false and the app reports that authenticated unlock is
/// unavailable, instead of presenting an immediately reversible hide as a secure lock.
pub fn mechanism_available() -> bool {
    Path::new(PKEXEC_PATH).is_file()
        && Path::new(HELPER_PATH).is_file()
        && Path::new(POLICY_PATH).is_file()
}

pub fn unavailable_reason() -> &'static str {
    "authenticated unlock is not installed on this system: run the documented privileged step \
     that installs the polkit policy and dev-console-auth-helper. Commands stay available; \
     secrets stay locked"
}

/// Runs the polkit-gated helper. Success means the user authenticated for
/// [`ACTION_ID`]; anything else (including a dismissed dialog) is an error that keeps the
/// vault locked.
pub fn authenticate(_reason: &str) -> AppResult<()> {
    if !mechanism_available() {
        return Err(AppError::VaultUnavailable(unavailable_reason().into()));
    }
    let output = Command::new(PKEXEC_PATH)
        .arg(HELPER_PATH)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|child| child.wait_with_output())
        .map_err(|error| {
            AppError::Execution(format!(
                "could not start the authentication helper: {}",
                error.kind()
            ))
        })?;
    let code = output.status.code();
    match code {
        Some(0) => Ok(()),
        // 126: authentication was dismissed. 127: not authorized.
        Some(126) => Err(AppError::Locked),
        Some(127) => Err(AppError::Locked),
        Some(other) => {
            let detail = String::from_utf8_lossy(&output.stderr)
                .trim()
                .chars()
                .take(160)
                .collect::<String>();
            Err(AppError::Other(format!(
                "the authentication helper failed with status {other}: {detail}"
            )))
        }
        None => Err(AppError::Locked),
    }
}
