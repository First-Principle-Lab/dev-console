//! One profile in a temp directory, for the service tests.
//!
//! This is the same shape `state.rs`'s tests build: every path a real process owns, over a scratch
//! directory, with the vault's credential supplied directly instead of read from a keyring. That is
//! what lets the service tests run in a container with no Secret Service, no polkit agent and no X11
//! clipboard - and it is also why nothing here asserts that a *copy* reached the clipboard, which
//! needs a display server and is checked on the desktop instead.

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use crate::backup::{BackupManager, RecoveryKey};
use crate::clipboard::Clipboard;
use crate::context::FinderContext;
use crate::models::{EntryDraft, EntryRecord, Settings};
use crate::recovery::ReplacementJournal;
use crate::repository::Repository;
use crate::settings::SettingsStore;
use crate::state::AppState;
use crate::vault::{generate_identity, Credential, VaultService};

pub(crate) struct Fixture {
    pub(crate) state: Arc<AppState>,
    base: std::path::PathBuf,
}

impl Fixture {
    pub(crate) fn new(tag: &str) -> Fixture {
        let base = std::env::temp_dir().join(format!(
            "dev-console-services-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&base).expect("temp base");
        let (_identity, key) = generate_identity();
        let state = Arc::new(build_state(&base, &key));
        // No manual-lock flag exists yet, so this opens the vault: the tests start where a normal
        // first run of the app would be.
        state.vault.bootstrap();
        Fixture { state, base }
    }

    /// A command row with an id a test can name. It goes in through the same table a restore
    /// replaces, so seeded rows are ordinary rows rather than a special case.
    pub(crate) fn seed_command(&self, id: &str, entry_type: &str, command: &str, tags: &[&str]) {
        let mut records = self
            .state
            .repo
            .all_command_records()
            .expect("existing commands")
            .into_iter()
            .map(|(record, _, _)| record)
            .collect::<Vec<_>>();
        records.push(command_record(id, entry_type, command, tags));
        self.state
            .repo
            .replace_all_commands(&records)
            .expect("seed");
    }

    /// A draft as the library's editor would hand one over: no id, so it is a new entry.
    pub(crate) fn draft(&self, name: &str, command: &str) -> EntryDraft {
        EntryDraft {
            kind: crate::models::Kind::Command,
            id: None,
            expected_revision: 0,
            name: name.into(),
            entry_type: "Shell".into(),
            tags: vec![],
            description: String::new(),
            notes: String::new(),
            fields: vec![],
            pinned: false,
            command: command.into(),
            working_directory: String::new(),
            value: String::new(),
        }
    }

    pub(crate) fn set_theme(&self, theme: &str) {
        let settings = self.state.settings.get();
        self.state
            .settings
            .save(Settings {
                theme: theme.into(),
                ..settings
            })
            .expect("save settings");
    }

    pub(crate) fn set_finder_context(&self, root: &str, name: &str) {
        self.state.set_finder_context(Some(FinderContext {
            root: root.into(),
            name: name.into(),
        }));
    }

    pub(crate) fn generation(&self) -> u64 {
        self.state.vault.generation()
    }

    pub(crate) fn backup_dir(&self) -> String {
        self.state.backups.dir().to_string_lossy().to_string()
    }

    /// The scratch directory as a path string. It exists and holds no project, which is what the
    /// "nothing to import" tests need.
    #[allow(dead_code)]
    pub(crate) fn scratch_dir(&self) -> String {
        self.base.to_string_lossy().to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn build_state(base: &std::path::Path, key: &str) -> AppState {
    AppState {
        repo: Repository::open(&base.join("commands.sqlite3")).expect("repo"),
        vault: VaultService::new(
            base.join("secrets.vault"),
            base.join("manual.lock"),
            Credential::Provided { key: key.into() },
        ),
        settings: SettingsStore::load(base.join("settings.json")).expect("settings"),
        clipboard: Clipboard::new(),
        backups: BackupManager::new(base.join("backups"), 4).expect("backups"),
        recovery: RecoveryKey::load_or_create(base.join("recovery.agekey")).expect("recovery key"),
        journal: ReplacementJournal::new(base.join("pending-replace")).expect("journal"),
        quitting: AtomicBool::new(false),
        finder_context: Mutex::new(None),
        finder_detection: AtomicU64::new(0),
    }
}

fn command_record(id: &str, entry_type: &str, command: &str, tags: &[&str]) -> EntryRecord {
    EntryRecord {
        id: id.into(),
        revision: 1,
        kind: crate::models::Kind::Command,
        name: id.into(),
        entry_type: entry_type.into(),
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        description: String::new(),
        notes: String::new(),
        fields: vec![],
        pinned: false,
        command: command.into(),
        working_directory: String::new(),
        value: String::new(),
        created_at: crate::repository::now_rfc3339(),
        updated_at: crate::repository::now_rfc3339(),
    }
}
