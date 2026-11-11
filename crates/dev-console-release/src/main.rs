//! `sign-update <file> <version>`: writes `<file>.sig`, the updater signature a release publishes.
//!
//! The Tauri bundler used to produce this file. The native build has no bundler, and installed
//! clients, `install.sh` and the native updater all verify the same thing, so the format is kept
//! exactly: a prehashed Minisign signature whose trusted comment carries `timestamp:`, `file:` and
//! `version:` fields separated by tabs, the whole signature file base64-encoded.
//!
//! The key comes from `TAURI_SIGNING_PRIVATE_KEY` (the base64-wrapped secret key file the
//! repository secret already holds) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (empty when unset).
//! Neither is ever printed.

use std::fs::File;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine};
use minisign::{SecretKey, SecretKeyBox};

const KEY_ENV: &str = "TAURI_SIGNING_PRIVATE_KEY";
const PASSWORD_ENV: &str = "TAURI_SIGNING_PRIVATE_KEY_PASSWORD";

fn secret_key(encoded: &str, password: &str) -> Result<SecretKey, String> {
    let decoded = STANDARD
        .decode(encoded.trim())
        .map_err(|_| format!("{KEY_ENV} is not base64"))?;
    let text = String::from_utf8(decoded).map_err(|_| format!("{KEY_ENV} is not a key file"))?;
    // A key generated without a password opens with the empty one. Passing it explicitly keeps
    // the library from prompting on a terminal.
    SecretKeyBox::from_string(&text)
        .and_then(|key| key.into_secret_key(Some(password.to_string())))
        .map_err(|_| format!("{KEY_ENV} could not be opened; check {PASSWORD_ENV}"))
}

/// The contents of `<file>.sig` for the file at `path`.
fn sign(path: &Path, version: &str, key: &SecretKey, timestamp: u64) -> Result<String, String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the file needs a UTF-8 name")?;
    if version.is_empty() || version.contains(|c: char| c.is_whitespace()) {
        return Err("the version must be one word".into());
    }
    let comment = format!("timestamp:{timestamp}\tfile:{name}\tversion:{version}");
    let data =
        File::open(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let signature = minisign::sign(
        None,
        key,
        data,
        Some(&comment),
        Some("signature from tauri secret key"),
    )
    .map_err(|error| format!("signing failed: {error}"))?;
    Ok(STANDARD.encode(signature.into_string()))
}

fn run(args: &[String]) -> Result<(), String> {
    let [file, version] = args else {
        return Err("usage: sign-update <file> <version>".into());
    };
    let encoded = std::env::var(KEY_ENV).map_err(|_| format!("{KEY_ENV} is not set"))?;
    let password = std::env::var(PASSWORD_ENV).unwrap_or_default();
    let key = secret_key(&encoded, &password)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    let path = Path::new(file);
    let signature = sign(path, version, &key, timestamp)?;
    let target = format!("{file}.sig");
    std::fs::write(&target, signature)
        .map_err(|error| format!("cannot write {target}: {error}"))?;
    println!("{target}");
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = run(&args) {
        eprintln!("sign-update: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key as `tauri signer generate` stores it: the secret key file, base64-wrapped.
    fn wrapped(pair: &minisign::KeyPair) -> String {
        STANDARD.encode(pair.sk.to_box(None).expect("box").to_string())
    }

    fn verify(signature: &str, pair: &minisign::KeyPair, data: &[u8]) -> String {
        let text = String::from_utf8(STANDARD.decode(signature).expect("base64")).expect("text");
        let signature = minisign_verify::Signature::decode(&text).expect("signature file");
        let key = minisign_verify::PublicKey::decode(&pair.pk.to_box().expect("box").to_string())
            .expect("public key");
        // The streaming verifier accepts prehashed signatures only, which is what the native
        // updater relies on for an image it never holds in memory.
        let mut verifier = key.verify_stream(&signature).expect("prehashed");
        verifier.update(data);
        verifier.finalize().expect("verifies");
        signature.trusted_comment().to_string()
    }

    #[test]
    fn a_signature_verifies_and_names_the_file_and_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let image = dir.path().join("Dev-Console.AppImage");
        std::fs::write(&image, b"\x7fELF image").expect("write");
        for (pair, password) in [
            (
                minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).expect("pair"),
                "",
            ),
            (
                minisign::KeyPair::generate_encrypted_keypair(Some("synthetic".into()))
                    .expect("pair"),
                "synthetic",
            ),
        ] {
            let key = secret_key(&wrapped(&pair), password).expect("key opens");
            let signature = sign(&image, "0.0.13", &key, 1794102944).expect("sign");
            assert_eq!(
                verify(&signature, &pair, b"\x7fELF image"),
                "timestamp:1794102944\tfile:Dev-Console.AppImage\tversion:0.0.13"
            );
        }
    }

    #[test]
    fn a_wrong_password_or_a_malformed_key_is_refused() {
        let pair =
            minisign::KeyPair::generate_encrypted_keypair(Some("synthetic".into())).expect("pair");
        assert!(secret_key(&wrapped(&pair), "").is_err());
        assert!(secret_key("not base64!", "").is_err());
        assert!(secret_key(&STANDARD.encode("not a key"), "").is_err());
    }

    #[test]
    fn a_version_with_a_tab_cannot_forge_a_comment_field() {
        let dir = tempfile::tempdir().expect("tempdir");
        let image = dir.path().join("Dev-Console.AppImage");
        std::fs::write(&image, b"x").expect("write");
        let pair =
            minisign::KeyPair::generate_encrypted_keypair(Some(String::new())).expect("pair");
        let key = secret_key(&wrapped(&pair), "").expect("key");
        assert!(sign(&image, "0.0.13\tversion:9.9.9", &key, 1).is_err());
        assert!(sign(&image, "", &key, 1).is_err());
    }
}
