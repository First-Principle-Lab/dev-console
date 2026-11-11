//! Signed self-update for the AppImage, reading what the Tauri updater read.
//!
//! The endpoint, the `latest.json` shape, the signing key and the base64-wrapped Minisign signature
//! are the ones every published release already uses, so an installed Tauri build can update to
//! this shell and this shell can update itself (`docs/plans/rune-migration.md` section 4).
//!
//! A download is bounded, staged beside the installed image, and verified before anything is
//! replaced: the signature, the version the signature's trusted comment names, and the ELF magic.
//! The replacement is one rename, the previous image is kept as `Dev-Console.AppImage.previous`,
//! and every failure leaves the installed image as it was. Only an AppImage updates itself; a
//! package-managed binary has no `$APPIMAGE` and is left to its package manager.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::fd::AsRawFd,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const ENDPOINT: &str =
    "https://github.com/First-Principle-Lab/dev-console/releases/latest/download/latest.json";
/// The release signing key, as a Minisign public key file. It is the key `install.sh` pins and the
/// one `src-tauri/tauri.conf.json` carries base64-wrapped.
pub const PUBLIC_KEY: &str = include_str!("../update-key.pub");
const BASE: &str = "https://github.com/First-Principle-Lab/dev-console/releases";
const MAX_IMAGE: u64 = 512 * 1024 * 1024;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub signature: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub version: String,
    pub platforms: BTreeMap<String, Artifact>,
}
#[derive(Clone, Debug, Default)]
pub struct State {
    pub checking: bool,
    pub installing: bool,
    pub release: Option<Release>,
    pub error: Option<String>,
    pub checked: bool,
    pub last_check: Option<Instant>,
}

/// The installed AppImage this process runs from, when it is one this updater may replace.
pub fn target() -> Option<PathBuf> {
    if std::env::consts::ARCH != "x86_64" {
        return None;
    }
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_file())
}
fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .user_agent(concat!("Dev-Console/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "Could not initialize secure update transport".into())
}
/// The release a manifest offers, or `None` when it is not newer than `current`.
pub fn parse_manifest(bytes: &[u8], current: &str) -> Result<Option<Release>> {
    let release: Release = serde_json::from_slice(bytes).map_err(|_| "Invalid update manifest")?;
    let offered =
        semver::Version::parse(&release.version).map_err(|_| "Invalid release version")?;
    let current = semver::Version::parse(current).map_err(|_| "Invalid current version")?;
    if offered <= current {
        return Ok(None);
    }
    let artifact = release
        .platforms
        .get("linux-x86_64")
        .ok_or("This release has no Linux x86_64 image")?;
    let pinned = format!("{BASE}/download/v{}/Dev-Console.AppImage", release.version);
    let latest = format!("{BASE}/latest/download/Dev-Console.AppImage");
    if artifact.url != pinned && artifact.url != latest {
        return Err("Unrecognized update artifact URL".into());
    }
    Ok(Some(release))
}
pub fn check(current: &str) -> Result<Option<Release>> {
    let response = client()?
        .get(ENDPOINT)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|_| "Could not check for updates")?;
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read update manifest")?;
    if bytes.len() > 1024 * 1024 {
        return Err("Update manifest is too large".into());
    }
    parse_manifest(&bytes, current)
}
pub fn install(release: &Release) -> Result<()> {
    let target = target().ok_or("This installation is managed by its package manager")?;
    let artifact = release
        .platforms
        .get("linux-x86_64")
        .ok_or("Missing platform artifact")?;
    // The manifest's URL may be the moving `/latest/` one. Download from the tag the manifest
    // named instead, so the image and the signature cannot come from two releases.
    let url = format!("{BASE}/download/v{}/Dev-Console.AppImage", release.version);
    let response = client()?
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|_| "Could not download the update")?;
    replace_verified(
        &target,
        response,
        &artifact.signature,
        &release.version,
        PUBLIC_KEY,
    )
}

/// Replaces `target` with the image read from `source`, if `encoded` is a signature of it by `key`
/// that names `version`. `key` is a Minisign public key file; `encoded` is the base64-wrapped
/// signature file a release publishes.
pub fn replace_verified(
    target: &Path,
    mut source: impl Read,
    encoded: &str,
    version: &str,
    key: &str,
) -> Result<()> {
    let parent = target.parent().ok_or("No installation directory")?;
    let before = fs::symlink_metadata(target).map_err(|_| "Installed AppImage is unavailable")?;
    if !before.is_file() || before.uid() != unsafe { libc::geteuid() } {
        return Err("AppImage must be a regular file owned by this user".into());
    }
    // The same lock file `install.sh` takes, so the installer and the updater cannot interleave.
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(parent.join(".install.lock"))
        .map_err(|_| "Cannot lock the installation directory")?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("Another installation is running".into());
    }
    let decoded = STANDARD
        .decode(encoded.trim())
        .map_err(|_| "Invalid update signature encoding")?;
    let text = std::str::from_utf8(&decoded).map_err(|_| "Invalid update signature")?;
    let signature =
        minisign_verify::Signature::decode(text).map_err(|_| "Invalid update signature")?;
    let public =
        minisign_verify::PublicKey::decode(key).map_err(|_| "Invalid update verification key")?;
    let mut verifier = public
        .verify_stream(&signature)
        .map_err(|_| "Unsupported update signature")?;
    let mut stage = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "Cannot stage an update beside the AppImage")?;
    let mut total = 0u64;
    let mut buf = [0u8; 64 * 1024];
    let mut magic = Vec::new();
    loop {
        let n = source
            .read(&mut buf)
            .map_err(|_| "Update download was interrupted")?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > MAX_IMAGE {
            return Err("Update exceeds the size limit".into());
        }
        if magic.len() < 4 {
            magic.extend_from_slice(&buf[..n.min(4 - magic.len())]);
        }
        verifier.update(&buf[..n]);
        stage
            .write_all(&buf[..n])
            .map_err(|_| "Could not write update; check free space")?;
    }
    verifier
        .finalize()
        .map_err(|_| "Update signature verification failed")?;
    if magic != b"\x7fELF" {
        return Err("Update is not a Linux executable".into());
    }
    // The manifest is not signed, the trusted comment is: without this an old signed image could
    // be offered under a newer version number.
    if !signature
        .trusted_comment()
        .split('\t')
        .any(|field| field == format!("version:{version}"))
    {
        return Err("Signed version does not match the manifest".into());
    }
    stage
        .as_file()
        .set_permissions(fs::Permissions::from_mode(before.mode() | 0o100))
        .map_err(|_| "Cannot preserve executable permissions")?;
    stage
        .as_file()
        .sync_all()
        .map_err(|_| "Cannot flush update to disk")?;
    let now = fs::symlink_metadata(target).map_err(|_| "Installation changed while downloading")?;
    if now.ino() != before.ino() || now.dev() != before.dev() || !now.is_file() {
        return Err("Installation changed while downloading".into());
    }
    let mut backup =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "Cannot stage rollback image")?;
    std::io::copy(
        &mut File::open(target).map_err(|_| "Cannot read installed AppImage")?,
        &mut backup,
    )
    .map_err(|_| "Cannot save rollback image; check free space")?;
    backup
        .as_file()
        .set_permissions(fs::Permissions::from_mode(before.mode()))
        .map_err(|_| "Cannot preserve rollback permissions")?;
    backup
        .as_file()
        .sync_all()
        .map_err(|_| "Cannot flush rollback image")?;
    backup
        .persist(rollback_path(target))
        .map_err(|_| "Cannot publish rollback image")?;
    stage
        .persist(target)
        .map_err(|_| "Cannot atomically replace installed AppImage")?;
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| "Cannot flush installation directory")?;
    Ok(())
}

/// Where the image an update replaced is kept: `Dev-Console.AppImage.previous`.
pub fn rollback_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".previous");
    target.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &[u8] = b"\x7fELF the installed image";
    const NEW: &[u8] = b"\x7fELF the next image";

    struct Fixture {
        dir: tempfile::TempDir,
        keys: minisign::KeyPair,
    }

    impl Fixture {
        fn new() -> Fixture {
            let dir = tempfile::tempdir().expect("tempdir");
            fs::write(dir.path().join("Dev-Console.AppImage"), OLD).expect("installed image");
            fs::set_permissions(
                dir.path().join("Dev-Console.AppImage"),
                fs::Permissions::from_mode(0o755),
            )
            .expect("chmod");
            Fixture {
                dir,
                keys: minisign::KeyPair::generate_unencrypted_keypair().expect("keypair"),
            }
        }

        fn target(&self) -> PathBuf {
            self.dir.path().join("Dev-Console.AppImage")
        }

        fn key(&self) -> String {
            self.keys.pk.to_box().expect("key box").to_string()
        }

        /// What a release publishes as `Dev-Console.AppImage.sig`.
        fn sign(&self, image: &[u8], version: &str) -> String {
            let comment = format!("timestamp:1\tfile:Dev-Console.AppImage\tversion:{version}");
            let signature = minisign::sign(None, &self.keys.sk, image, Some(&comment), None)
                .expect("sign")
                .into_string();
            STANDARD.encode(signature)
        }

        fn installed(&self) -> Vec<u8> {
            fs::read(self.target()).expect("installed image")
        }

        /// Nothing but the image and the lock may be left beside it after a refusal.
        fn leftovers(&self) -> Vec<String> {
            let mut names: Vec<String> = fs::read_dir(self.dir.path())
                .expect("read dir")
                .map(|entry| {
                    entry
                        .expect("entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .filter(|name| name != "Dev-Console.AppImage" && name != ".install.lock")
                .collect();
            names.sort();
            names
        }
    }

    fn manifest(version: &str, url: &str) -> Vec<u8> {
        serde_json::json!({
            "version": version,
            "platforms": {"linux-x86_64": {"url": url, "signature": "c2ln"}}
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn the_shipped_key_is_the_one_every_release_is_signed_with() {
        minisign_verify::PublicKey::decode(PUBLIC_KEY).expect("the key file parses");
        // `install.sh` pins the same key as a bare string, and the Tauri config wraps this file.
        let bare = PUBLIC_KEY.lines().nth(1).expect("key line");
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let installer = fs::read_to_string(format!("{root}/install.sh")).expect("install.sh");
        assert!(installer.contains(&format!("PUBLIC_KEY = '{bare}'")));
        let tauri = fs::read_to_string(format!("{root}/src-tauri/tauri.conf.json")).expect("conf");
        assert!(tauri.contains(&STANDARD.encode(PUBLIC_KEY)));
    }

    /// The compatibility check against real artifacts, for a release rehearsal:
    ///
    /// ```sh
    /// gh release download v0.0.12 -D /tmp/release
    /// DEV_CONSOLE_RELEASE_DIR=/tmp/release cargo test -p dev-console-desktop --lib \
    ///     a_downloaded_release_installs -- --ignored
    /// ```
    ///
    /// The directory holds the three release assets. A `key.pub` beside them (a Minisign public key
    /// file) replaces the shipped key, for artifacts signed with a throwaway one.
    #[test]
    #[ignore = "needs DEV_CONSOLE_RELEASE_DIR pointing at downloaded release assets"]
    fn a_downloaded_release_installs() {
        let dir = PathBuf::from(std::env::var("DEV_CONSOLE_RELEASE_DIR").expect("release dir"));
        let manifest = fs::read(dir.join("latest.json")).expect("latest.json");
        let release = parse_manifest(&manifest, "0.0.0")
            .expect("manifest")
            .expect("newer than 0.0.0");
        let signature = fs::read_to_string(dir.join("Dev-Console.AppImage.sig")).expect("sig");
        assert_eq!(
            signature.trim(),
            release.platforms["linux-x86_64"].signature.trim(),
            "the manifest carries the published signature"
        );
        let key = fs::read_to_string(dir.join("key.pub")).unwrap_or_else(|_| PUBLIC_KEY.into());
        let fixture = Fixture::new();
        let image = File::open(dir.join("Dev-Console.AppImage")).expect("image");
        replace_verified(&fixture.target(), image, &signature, &release.version, &key)
            .expect("the release verifies and installs");
        assert_eq!(
            fixture.installed().len() as u64,
            fs::metadata(dir.join("Dev-Console.AppImage"))
                .expect("meta")
                .len()
        );
    }

    #[test]
    fn a_manifest_offers_only_a_newer_release_from_this_repository() {
        let latest = format!("{BASE}/latest/download/Dev-Console.AppImage");
        let offered = parse_manifest(&manifest("0.0.13", &latest), "0.0.12")
            .expect("valid")
            .expect("newer");
        assert_eq!(offered.version, "0.0.13");
        let pinned = format!("{BASE}/download/v0.0.13/Dev-Console.AppImage");
        assert!(parse_manifest(&manifest("0.0.13", &pinned), "0.0.12")
            .expect("valid")
            .is_some());
        assert!(parse_manifest(&manifest("0.0.12", &latest), "0.0.12")
            .expect("valid")
            .is_none());
        assert!(parse_manifest(&manifest("0.0.9", &latest), "0.0.12")
            .expect("valid")
            .is_none());
        // 0.0.10 is newer than 0.0.9: versions compare as numbers, not as text.
        assert!(parse_manifest(&manifest("0.0.10", &latest), "0.0.9")
            .expect("valid")
            .is_some());
        assert!(parse_manifest(
            &manifest("0.0.13", "https://example.com/Dev-Console.AppImage"),
            "0.0.12"
        )
        .is_err());
        assert!(parse_manifest(&manifest("latest", &latest), "0.0.12").is_err());
        assert!(parse_manifest(b"<html>", "0.0.12").is_err());
        assert!(parse_manifest(br#"{"version":"0.0.13","platforms":{}}"#, "0.0.12").is_err());
    }

    #[test]
    fn a_verified_image_replaces_the_installed_one_and_keeps_a_rollback() {
        let fixture = Fixture::new();
        replace_verified(
            &fixture.target(),
            NEW,
            &fixture.sign(NEW, "0.0.13"),
            "0.0.13",
            &fixture.key(),
        )
        .expect("update");
        assert_eq!(fixture.installed(), NEW);
        let mode = fs::metadata(fixture.target()).expect("meta").mode();
        assert_eq!(mode & 0o777, 0o755, "the image stays executable");
        let rollback = rollback_path(&fixture.target());
        assert!(rollback.ends_with("Dev-Console.AppImage.previous"));
        assert_eq!(fs::read(&rollback).expect("rollback"), OLD);
        assert_eq!(fixture.leftovers(), ["Dev-Console.AppImage.previous"]);
    }

    #[test]
    fn a_refused_update_leaves_the_installed_image_alone() {
        let fixture = Fixture::new();
        let other = Fixture::new();
        let good = fixture.sign(NEW, "0.0.13");
        let not_elf: &[u8] = b"#!/bin/sh\n";
        let cases: [(&str, &[u8], String, &str, String); 6] = [
            (
                "tampered image",
                b"\x7fELF something else",
                good.clone(),
                "0.0.13",
                fixture.key(),
            ),
            (
                "another key",
                NEW,
                other.sign(NEW, "0.0.13"),
                "0.0.13",
                fixture.key(),
            ),
            (
                "older signed image",
                NEW,
                good.clone(),
                "0.0.14",
                fixture.key(),
            ),
            (
                "not an executable",
                not_elf,
                fixture.sign(not_elf, "0.0.13"),
                "0.0.13",
                fixture.key(),
            ),
            (
                "not base64",
                NEW,
                "not a signature".into(),
                "0.0.13",
                fixture.key(),
            ),
            ("no key", NEW, good.clone(), "0.0.13", String::new()),
        ];
        for (name, image, signature, version, key) in cases {
            let outcome = replace_verified(&fixture.target(), image, &signature, version, &key);
            assert!(outcome.is_err(), "{name} was installed");
            assert_eq!(
                fixture.installed(),
                OLD,
                "{name} changed the installed image"
            );
            assert!(
                fixture.leftovers().is_empty(),
                "{name} left {:?}",
                fixture.leftovers()
            );
        }
    }

    #[test]
    fn an_interrupted_download_leaves_the_installed_image_alone() {
        struct Broken(usize);
        impl Read for Broken {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.0 == 0 {
                    return Err(std::io::Error::other("connection reset"));
                }
                self.0 -= 1;
                buf[..4].copy_from_slice(b"\x7fELF");
                Ok(4)
            }
        }
        let fixture = Fixture::new();
        let outcome = replace_verified(
            &fixture.target(),
            Broken(3),
            &fixture.sign(NEW, "0.0.13"),
            "0.0.13",
            &fixture.key(),
        );
        assert_eq!(outcome, Err("Update download was interrupted".into()));
        assert_eq!(fixture.installed(), OLD);
        assert!(fixture.leftovers().is_empty());
    }

    #[test]
    fn a_running_installer_blocks_the_update() {
        let fixture = Fixture::new();
        let lock = File::create(fixture.dir.path().join(".install.lock")).expect("lock");
        assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);
        let outcome = replace_verified(
            &fixture.target(),
            NEW,
            &fixture.sign(NEW, "0.0.13"),
            "0.0.13",
            &fixture.key(),
        );
        assert_eq!(outcome, Err("Another installation is running".into()));
        assert_eq!(fixture.installed(), OLD);
    }

    #[test]
    fn a_symlinked_image_is_not_replaced() {
        let fixture = Fixture::new();
        let link = fixture.dir.path().join("link.AppImage");
        std::os::unix::fs::symlink(fixture.target(), &link).expect("symlink");
        let outcome = replace_verified(
            &link,
            NEW,
            &fixture.sign(NEW, "0.0.13"),
            "0.0.13",
            &fixture.key(),
        );
        assert!(outcome.is_err());
        assert_eq!(fixture.installed(), OLD);
    }
}
