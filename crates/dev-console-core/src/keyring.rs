//! Linux Secret Service adapter for the vault credential.
//!
//! The credential is a randomly generated `age` X25519 identity key (a bech32 string) stored
//! in the user's keyring under the `dev-console` service. It is never derived from a
//! passphrase, never written to disk next to the vault, and never printed. If the provider is
//! missing or locked the vault stays unavailable: there is no plaintext fallback and no
//! embedded default key anywhere in this application.

use keyring::{Entry, Error};

pub const SERVICE: &str = "dev-console";
pub const ACCOUNT: &str = "vault-key";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyringState {
    /// A Secret Service provider is reachable and its default collection appears usable.
    Available,
    /// No provider is registered, or the collection is locked / inaccessible.
    Unavailable(String),
}

fn entry() -> Result<Entry, Error> {
    Entry::new(SERVICE, ACCOUNT)
}

/// Cheap probe used for the vault status report. Initialising the store is what detects a
/// missing provider, and it is done once per process by the keyring library.
pub fn state() -> KeyringState {
    match Entry::store_status() {
        Ok(()) => KeyringState::Available,
        Err(error) => KeyringState::Unavailable(describe(&error.to_string())),
    }
}

pub fn is_available() -> bool {
    matches!(state(), KeyringState::Available)
}

/// The credential, or `None` when nothing has been stored yet. A locked or missing provider
/// is an error, never a `None`, so callers cannot accidentally treat it as "first run".
pub fn read_key() -> Result<Option<String>, String> {
    let entry = entry().map_err(|e| describe(&e.to_string()))?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(Error::NoEntry) => Ok(None),
        Err(Error::NoStorageAccess(_)) | Err(Error::NoDefaultStore) => {
            Err("the keyring is locked or unavailable".into())
        }
        Err(error) => Err(describe(&error.to_string())),
    }
}

pub fn write_key(secret: &str) -> Result<(), String> {
    let entry = entry().map_err(|e| describe(&e.to_string()))?;
    entry
        .set_password(secret)
        .map_err(|e| describe(&e.to_string()))
}

/// Removes only this application's own credential.
pub fn delete_key() -> Result<(), String> {
    let entry = entry().map_err(|e| describe(&e.to_string()))?;
    match entry.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(error) => Err(describe(&error.to_string())),
    }
}

/// Short, user-presentable cause. Never contains secret material.
fn describe(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let missing_provider = [
        "no default store",
        "unable to look up",
        "service unknown",
        "cannot connect",
        "connection refused",
        "no session bus",
        "not running",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    if missing_provider {
        "no Secret Service provider is running (install and start gnome-keyring or kwallet)".into()
    } else if lower.contains("locked") || lower.contains("auth") || lower.contains("denied") {
        "the keyring collection is locked; unlock it in the desktop keyring application".into()
    } else if lower.contains("no entry") {
        "no vault key is stored yet".into()
    } else if raw.trim().is_empty() {
        // Never surface a blank reason in the UI or the CLI.
        "the keyring could not be reached".into()
    } else {
        raw.chars().take(180).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_common_provider_failures() {
        assert!(
            describe("Error calling org.freedesktop.secrets: Cannot connect").contains("provider")
        );
        assert!(describe("collection is locked").contains("locked"));
        assert!(!describe("").is_empty());
    }

    /// Run with `cargo test -- --ignored` inside a session bus that owns
    /// `org.freedesktop.secrets` (see scripts/test-keyring.sh).
    #[test]
    #[ignore = "requires a real Secret Service provider"]
    fn real_keyring_round_trip() {
        assert_eq!(state(), KeyringState::Available);
        let marker = format!("probe-{}", std::process::id());
        write_key(&marker).expect("store credential");
        assert_eq!(read_key().expect("read"), Some(marker.clone()));
        delete_key().expect("delete");
        assert_eq!(read_key().expect("read after delete"), None);
    }
}
