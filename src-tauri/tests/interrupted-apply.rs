//! What happens to a whole-library replacement when the process is killed between its two
//! writes - the case no in-process rollback can cover.
//!
//! A child process runs the real replacement path against a real keyring-backed profile and is
//! aborted at a chosen write boundary (`abort()`, so no destructor or cleanup handler runs).
//! The parent then bootstraps a brand new `AppState` over the same files, exactly like the next
//! launch of the app, and checks that the command table and the secret vault describe one
//! complete version of the library, that the journal is retired, and that nothing sensitive
//! leaked into the recovery files.
//!
//! Run through `scripts/test-native-ipc.sh`, which supplies the Secret Service provider and a
//! display and passes `--ignored --test-threads=1`.

use std::os::unix::process::ExitStatusExt;
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

use dev_console_lib::models::{EntryRecord, Kind};
use dev_console_lib::paths;
use dev_console_lib::recovery::ApplyCrash;
use dev_console_lib::state::AppState;
use dev_console_lib::vault::{VaultFile, VAULT_VERSION};

/// SIGABRT, which is what `abort()` delivers: no destructors, no cleanup handlers.
fn libc_abort_signal() -> i32 {
    6
}

const TEST_NAME: &str = "a_replacement_killed_mid_write_leaves_one_complete_version_behind";

static PROFILE_LOCK: Mutex<()> = Mutex::new(());

fn profile_lock() -> MutexGuard<'static, ()> {
    match PROFILE_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn command_record(id: &str, name: &str) -> EntryRecord {
    EntryRecord {
        id: id.into(),
        revision: 1,
        kind: Kind::Command,
        name: name.into(),
        entry_type: "Shell".into(),
        tags: vec!["work".into()],
        description: String::new(),
        notes: "interrupted-apply-command-note".into(),
        fields: vec![],
        pinned: false,
        command: format!("echo {name}"),
        working_directory: String::new(),
        value: String::new(),
        created_at: dev_console_lib::repository::now_rfc3339(),
        updated_at: dev_console_lib::repository::now_rfc3339(),
    }
}

fn secret_record(id: &str, name: &str, value: &str) -> EntryRecord {
    EntryRecord {
        id: id.into(),
        revision: 1,
        kind: Kind::Secret,
        name: name.into(),
        entry_type: "API key".into(),
        tags: vec!["work".into()],
        description: String::new(),
        notes: "interrupted-apply-secret-note".into(),
        fields: vec![],
        pinned: false,
        command: String::new(),
        working_directory: String::new(),
        value: value.into(),
        created_at: dev_console_lib::repository::now_rfc3339(),
        updated_at: dev_console_lib::repository::now_rfc3339(),
    }
}

fn vault(previous: bool) -> VaultFile {
    let (tag, value) = if previous {
        ("previous", "synthetic-prev-value")
    } else {
        ("incoming", "synthetic-incoming-value")
    };
    VaultFile {
        version: VAULT_VERSION,
        records: vec![secret_record(
            &format!("secret-{tag}"),
            &format!("Token {tag}"),
            value,
        )],
        usage: vec![],
        environments: vec![],
    }
}

/// The archive being imported when the process dies: one command and one secret, both clearly
/// distinguishable from the seeded library.
fn incoming() -> dev_console_lib::backup::Archive {
    dev_console_lib::backup::Archive::new(
        vec![command_record("cmd-incoming", "Deploy incoming")],
        vault(false),
    )
}

fn seed(state: &AppState) {
    state
        .repo
        .replace_all_commands(&[command_record("cmd-previous", "Deploy previous")])
        .expect("seed commands");
    state
        .vault
        .restore(vault(true))
        .expect("seed the keyring-backed vault");
}

fn command_name(state: &AppState) -> String {
    state
        .repo
        .list_commands("")
        .expect("commands")
        .first()
        .expect("one command")
        .name
        .clone()
}

fn secret_name(state: &AppState) -> String {
    state
        .vault
        .list_secrets()
        .expect("secrets")
        .first()
        .expect("one secret")
        .name
        .clone()
}

/// The child half: runs the real replacement and never returns, because the replacement is
/// interrupted by a genuine crash at the requested boundary.
fn child_run(point: ApplyCrash) -> ! {
    let state = AppState::bootstrap().expect("child bootstrap");
    match state.apply_archive_with_crash(&incoming(), point) {
        Ok(counts) => panic!("the child was supposed to die mid-replacement, applied {counts:?}"),
        Err(error) => panic!("the child reported an error instead of crashing: {error}"),
    }
}

fn spawn_crashed_child(stage: &str) -> std::process::ExitStatus {
    Command::new(std::env::current_exe().expect("test binary"))
        .args([
            TEST_NAME,
            "--exact",
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("DEV_CONSOLE_CRASH_AT", stage)
        .status()
        .expect("spawn the crash child")
}

/// Every file in the profile, including the recovery directory, read back and scanned for the
/// secret-only markers. Command rows are plaintext by design, so only secret markers are used.
fn assert_no_secret_leak(profile: &std::path::Path) {
    let markers = [
        "synthetic-prev-value",
        "synthetic-incoming-value",
        "Token previous",
        "Token incoming",
    ];
    let mut stack = vec![profile.to_path_buf()];
    let mut files = 0usize;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read sweep dir") {
            let path = entry.expect("entry").path();
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
    assert!(files >= 3, "the sweep covered only {files} files");
}

#[test]
#[ignore = "needs a real Secret Service provider, started by scripts/test-native-ipc.sh"]
fn a_replacement_killed_mid_write_leaves_one_complete_version_behind() {
    if let Ok(stage) = std::env::var("DEV_CONSOLE_CRASH_AT") {
        child_run(match stage.as_str() {
            "after-vault-write" => ApplyCrash::AfterVaultWrite,
            "after-command-commit" => ApplyCrash::AfterCommandCommit,
            other => panic!("unknown crash stage {other:?}"),
        });
    }

    let _guard = profile_lock();
    // (stage, expected command, expected secret): what a correct reconciliation must produce.
    // Killed before the command commit, the whole replacement is undone; killed after it, the
    // commit and the vault write are already one complete version, so it is completed.
    for (stage, expected) in [
        ("after-vault-write", ("Deploy previous", "Token previous")),
        (
            "after-command-commit",
            ("Deploy incoming", "Token incoming"),
        ),
    ] {
        let profile = paths::set_disposable_profile(&format!("interrupted-{stage}"));
        {
            let state = AppState::bootstrap().expect("first bootstrap");
            if !state.vault.is_unlocked() {
                paths::remove_disposable_profile(&profile);
                panic!("the vault did not open through the real provider");
            }
            seed(&state);
            // The child owns the database from here, so the parent must not hold it open.
            drop(state);
        }

        let status = spawn_crashed_child(stage);
        assert_eq!(
            status.signal(),
            Some(libc_abort_signal()),
            "the child must die from a real abort inside the replacement; it exited as {status:?}"
        );

        let after = AppState::bootstrap().expect("restart after the interrupted replacement");
        let pair = (command_name(&after), secret_name(&after));
        assert_eq!(
            (pair.0.as_str(), pair.1.as_str()),
            expected,
            "after {stage} the command table and the vault must describe one complete library"
        );
        assert!(
            !after.journal.has_pending(),
            "recovery must retire the journal after {stage}"
        );
        assert_eq!(
            after.repo.apply_operation_marker().expect("marker"),
            None,
            "the shared operation id must not outlive the replacement it describes"
        );
        assert_no_secret_leak(&profile);
        paths::remove_disposable_profile(&profile);
    }
}
