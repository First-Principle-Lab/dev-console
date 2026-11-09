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
/// Where the helper and policy can live. The documented privileged install and the
/// manifest installer use different prefixes (`/usr` vs `/usr/local`), and polkitd searches
/// both policy directories, so the app must accept either location instead of hardcoding
/// one - otherwise a correctly installed system reports "not installed" and every unlock
/// fails closed.
pub const HELPER_CANDIDATES: &[&str] = &[
    "/usr/libexec/dev-console-auth-helper",
    "/usr/local/libexec/dev-console-auth-helper",
];
pub const POLICY_CANDIDATES: &[&str] = &[
    "/usr/share/polkit-1/actions/ai.codemate.devconsole.policy",
    "/usr/local/share/polkit-1/actions/ai.codemate.devconsole.policy",
];
pub const PKEXEC_PATH: &str = "/usr/bin/pkexec";

pub fn helper_path() -> Option<&'static Path> {
    HELPER_CANDIDATES
        .iter()
        .map(Path::new)
        .find(|path| path.is_file())
}

pub fn policy_installed() -> bool {
    POLICY_CANDIDATES
        .iter()
        .any(|path| Path::new(path).is_file())
}

/// True only when the real mechanism is installed. Until the documented privileged install
/// step is run this returns false and the app reports that authenticated unlock is
/// unavailable, instead of presenting an immediately reversible hide as a secure lock.
pub fn mechanism_available() -> bool {
    Path::new(PKEXEC_PATH).is_file() && helper_path().is_some() && policy_installed()
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
    let output = match helper_path() {
        None => {
            return Err(AppError::VaultUnavailable(unavailable_reason().into()));
        }
        Some(helper) => Command::new(PKEXEC_PATH)
            .arg(helper)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|child| child.wait_with_output()),
    }
    .map_err(|error| {
        AppError::Execution(format!(
            "could not start the authentication helper: {}",
            error.kind()
        ))
    })?;
    let code = output.status.code();
    match code {
        Some(0) => Ok(()),
        // 126: authentication was dismissed by the user.
        Some(126) => Err(AppError::Locked),
        // 127: the request was refused without ever being answered - most often because no
        // authentication agent is running to show the prompt. That is not the user saying
        // no, so report the real cause instead of masquerading as a dismissed lock.
        Some(127) => Err(AppError::Other(
            "the system could not ask for authentication (no polkit authentication agent is \
             answering, or this session is not allowed to authenticate); the vault stays \
             locked"
                .into(),
        )),
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
