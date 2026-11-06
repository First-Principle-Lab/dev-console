//! Native backend integration test: no mocks, no test doubles.
//!
//! Runs the same public operations that the IPC layer calls, against a real SQLite file, a
//! real age-encrypted vault, a real Secret Service provider, and (under Xvfb) a real X11
//! selection. A "restart" is a brand new `AppState` reading the same files, which is what the
//! application does when it is launched again.
//!
//! What this proves: persistence, ranking, lock enforcement, clipboard behaviour and backups
//! work for real, on this machine, with the production credential path
//! (`Credential::FromKeyring`).
//!
//! What it does not prove: that a WebKit webview's `invoke()` reaches these handlers - that
//! plumbing is exercised by `scripts/smoke-native.sh` against the compiled release binary and
//! by the browser suite for UI behaviour. Making the handlers themselves generic over the
//! Tauri runtime (so `tauri::test::mock_app()` can call them directly) is the remaining step
//! and is recorded in PROGRESS.md.
//!
//! Run through `scripts/test-native-ipc.sh`, which starts the provider and the display and
//! passes `--ignored --test-threads=1` (the state paths are process-global environment
//! variables). Both tests are `#[ignore]`d by default for the same reason the X11 clipboard
//! suite is: `cargo test` on a machine without a Secret Service provider must stay green rather
//! than report a failure that is really a missing environment, and the harness reports a
//! missing provider as exit 77 (UNVERIFIED) instead.

use dev_console_lib::models::{EntryDraft, ExtraField, FieldDisplay, Kind};
use dev_console_lib::search;
use dev_console_lib::state::AppState;

fn draft(kind: Kind, name: &str, command: &str, value: &str) -> EntryDraft {
    EntryDraft {
        kind,
        id: None,
        expected_revision: 0,
        name: name.to_string(),
        entry_type: if kind == Kind::Command {
            "Shell"
        } else {
            "API key"
        }
        .to_string(),
        tags: vec!["work".to_string()],
        description: String::new(),
        notes: "native-crud-notes".to_string(),
        fields: vec![ExtraField {
            id: "f1".to_string(),
            label: "Project".to_string(),
            value: "native-api".to_string(),
            display: FieldDisplay::Text,
        }],
        pinned: false,
        command: command.to_string(),
        working_directory: String::new(),
        value: value.to_string(),
    }
}

/// A disposable profile per test, so a bare `cargo test` can never open (or create) the
/// developer's own database, vault or settings.
struct Profile(std::path::PathBuf);

impl Profile {
    fn new(tag: &str) -> Self {
        let _guard = PROFILE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        Profile(dev_console_lib::paths::set_disposable_profile(tag))
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        dev_console_lib::paths::remove_disposable_profile(&self.0);
    }
}

/// The profile is process-global environment, so these tests take turns.
static PROFILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn bootstrap() -> std::sync::Arc<AppState> {
    AppState::bootstrap().expect("backend bootstrap")
}

#[test]
#[ignore = "needs a real Secret Service provider, started by scripts/test-native-ipc.sh"]
fn commands_and_secrets_survive_a_real_restart_through_the_real_backend() {
    let _profile = Profile::new("native-crud-restart");
    let state = bootstrap();

    // ---- commands: create, pin, usage ----
    let mut plain = draft(Kind::Command, "Ship it", "make ship", "");
    let created = state.repo.save_command(&mut plain).expect("create command");
    assert_eq!(created.revision, 1);

    let mut pinned = draft(Kind::Command, "Always first", "make pin", "");
    pinned.pinned = true;
    let pinned = state
        .repo
        .save_command(&mut pinned)
        .expect("create pinned command");

    let mut edit = draft(Kind::Command, "Ship it", "make ship --force", "");
    edit.id = Some(created.id.clone());
    edit.expected_revision = 1;
    state.repo.save_command(&mut edit).expect("edit command");

    state.repo.record_usage(&created.id).expect("usage");

    // ---- secrets: create through the keyring-backed vault ----
    let mut secret = draft(Kind::Secret, "Registry token", "", "synthetic-native-value");
    // Command rows are plaintext by design, so the sweep markers must be secret-only.
    secret.notes = "native-crud-secret-notes".to_string();
    let saved_secret = state.vault.save(&mut secret).expect("create secret");
    assert!(
        state.vault.is_unlocked(),
        "a real Secret Service provider should open the vault automatically"
    );

    // ---- the finder sees both kinds, pinned first, exact match on top ----
    let ranked = search::rank(&state.searchable_metadata(None), "Ship", 10, None);
    assert!(
        ranked.iter().any(|r| r.id == created.id),
        "command found by prefix"
    );
    let commands = state.repo.list_commands("").expect("list commands");
    assert_eq!(
        commands.first().expect("rows").id,
        pinned.id,
        "pinned sorts first"
    );
    assert_eq!(commands.len(), 2);

    // ---- a copy reaches a real X11 selection when a display exists ----
    if std::env::var_os("DISPLAY").is_some() {
        let record = state.vault.get(&saved_secret.id).expect("secret record");
        let copied = state
            .clipboard
            .copy(Kind::Secret, &saved_secret.id, &record.value, 5)
            .expect("copy to clipboard");
        assert!(!copied.excluded, "the app must not claim history exclusion");
        assert_eq!(
            dev_console_lib::clipboard::read_clipboard().expect("paste"),
            "synthetic-native-value"
        );
    }

    // ---- a sealed backup is written through the same credential ----
    let archive = state.build_archive().expect("build archive");
    let backup = state
        .backups
        .create(&archive, &state.vault)
        .expect("backup created");

    // ---- lock: secrets shut, commands stay usable ----
    state.vault.lock().expect("manual lock");
    assert!(state.vault.list_secrets().is_err());
    assert!(state.vault.get(&saved_secret.id).is_err());
    assert!(!state
        .searchable_metadata(None)
        .iter()
        .any(|e| e.kind == Kind::Secret));
    assert_eq!(
        state
            .repo
            .list_commands("")
            .expect("commands while locked")
            .len(),
        2
    );

    drop(state);

    // ---- restart: the manual lock persists and nothing else changed ----
    let after = bootstrap();
    assert!(
        after.vault.manual_lock(),
        "the manual lock survived the restart"
    );
    assert!(after.vault.list_secrets().is_err());
    let commands = after
        .repo
        .list_commands("")
        .expect("commands after restart");
    assert_eq!(commands.len(), 2);
    let ship = commands
        .iter()
        .find(|c| c.name == "Ship it")
        .expect("command survived");
    assert_eq!(ship.revision, 2, "the edit persisted");
    let (record, usage, last_used) = after
        .repo
        .get_command(&ship.id)
        .expect("query")
        .expect("row after restart");
    assert_eq!(record.command, "make ship --force");
    assert_eq!(usage, 1);
    assert!(last_used.is_some());
    assert_eq!(record.fields[0].value, "native-api");
    assert!(
        after.backups.dir().join(&backup).exists(),
        "the sealed backup file is still on disk"
    );

    // Whole-profile marker sweep: every file this run wrote - database, vault, sealed
    // backup, flag files, autostart entry - is read back and scanned, not just the vault.
    // The command text itself is plaintext by design; these markers are secret-only.
    sweep_for_leaks(
        &_profile.0,
        &[
            "synthetic-native-value",
            "native-crud-secret-notes",
            "Registry token",
        ],
    );
}

/// Read every regular file under `root` and fail if any marker appears in any of them.
fn sweep_for_leaks(root: &std::path::Path, markers: &[&str]) {
    let mut stack = vec![root.to_path_buf()];
    let mut files = 0usize;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read sweep dir") {
            let path = entry.expect("sweep entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                files += 1;
                let bytes = std::fs::read(&path).expect("read swept file");
                for marker in markers {
                    assert!(
                        !bytes.windows(marker.len()).any(|w| w == marker.as_bytes()),
                        "plaintext marker {marker:?} leaked into {}",
                        path.display()
                    );
                }
            }
        }
    }
    assert!(
        files >= 3,
        "the sweep must cover a populated profile, saw only {files} files under {}",
        root.display()
    );
}

#[test]
fn the_command_store_refuses_a_stale_writer_and_keeps_the_winner() {
    let _profile = Profile::new("native-crud-conflict");
    let state = bootstrap();
    let mut base = draft(Kind::Command, "Contended", "echo base", "");
    let created = state.repo.save_command(&mut base).expect("create");

    let mut winner = draft(Kind::Command, "Contended", "echo winner", "");
    winner.id = Some(created.id.clone());
    winner.expected_revision = created.revision;
    state
        .repo
        .save_command(&mut winner)
        .expect("first writer wins");

    let mut loser = draft(Kind::Command, "Contended", "echo loser", "");
    loser.id = Some(created.id.clone());
    loser.expected_revision = created.revision;
    assert!(matches!(
        state.repo.save_command(&mut loser),
        Err(dev_console_lib::error::AppError::Conflict)
    ));

    let (record, _, _) = state
        .repo
        .get_command(&created.id)
        .expect("q")
        .expect("row");
    assert_eq!(record.command, "echo winner");

    // Deletion honours the same contract, and the row really disappears on restart.
    state
        .repo
        .delete_command(&created.id, record.revision)
        .expect("delete");
    drop(state);
    let after = bootstrap();
    assert!(after.repo.get_command(&created.id).expect("q").is_none());
    use std::os::unix::fs::PermissionsExt;
    let db = dev_console_lib::paths::database_path().expect("db path");
    let mode = std::fs::metadata(&db)
        .expect("db metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "the command database stays private");
}
