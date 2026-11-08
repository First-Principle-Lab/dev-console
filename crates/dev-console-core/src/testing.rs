//! One real profile in a scratch directory, for tests in this workspace.
//!
//! This is the same shape `state.rs`'s tests build: every path a real process owns, over a scratch
//! directory, with the vault's credential supplied directly instead of read from a keyring. That is
//! what lets the service tests - and now the desktop shell's delivery tests - run against a real
//! `rusqlite` repository and a real age vault in a container with no Secret Service, no polkit
//! agent, no X11 clipboard and no display, and it is also why nothing here asserts that a *copy*
//! reached the clipboard, which needs a display server and is checked on the desktop instead.
//!
//! ## Why this is public
//!
//! `dev-console-desktop` has to drive the same guards over a real store (see its `delivery`
//! module), and a second copy of this scaffolding in that crate would mean two fixtures that can
//! drift into disagreeing about what a profile is. Nothing in a shipped binary may call this: the
//! production entry point is [`AppState::bootstrap`], which uses the real XDG paths.
//!
//! Every string a caller puts in here is synthetic test material (AGENTS.md: never capture a real
//! secret value), and the directory is removed on drop.

use std::sync::atomic::AtomicBool;
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

pub struct Fixture {
    pub state: Arc<AppState>,
    base: std::path::PathBuf,
}

impl Fixture {
    pub fn new(tag: &str) -> Fixture {
        let base = std::env::temp_dir().join(format!(
            "dev-console-profile-{tag}-{}-{}",
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
    pub fn seed_command(&self, id: &str, entry_type: &str, command: &str, tags: &[&str]) {
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

    /// The profile's service container, as a plain reference: a caller that only needs an
    /// `&AppState` should not have to name the `Arc` or reach through the field.
    pub fn state(&self) -> &Arc<AppState> {
        &self.state
    }

    /// A secret, put in through the vault's own save path so it carries a real revision and moves
    /// the generation exactly as a user's edit would. Returns its id.
    ///
    /// The value is the caller's to make up, and every caller in this workspace passes a synthetic
    /// one (AGENTS.md: never capture a real secret value). The vault here is a real age vault, so
    /// whatever goes in really is encrypted on disk.
    pub fn seed_secret(&self, name: &str, value: &str) -> String {
        let mut draft = self.secret_draft(name, value);
        let saved = self.state.vault.save(&mut draft).expect("seed secret");
        saved.id
    }

    /// A secret draft - the sibling of [`Fixture::draft`], which builds a command.
    pub fn secret_draft(&self, name: &str, value: &str) -> EntryDraft {
        EntryDraft {
            kind: crate::models::Kind::Secret,
            name: name.into(),
            entry_type: "Token".into(),
            value: value.into(),
            ..Default::default()
        }
    }

    /// A draft as the library's editor would hand one over: no id, so it is a new entry.
    pub fn draft(&self, name: &str, command: &str) -> EntryDraft {
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

    pub fn set_theme(&self, theme: &str) {
        let settings = self.state.settings.get();
        self.state
            .settings
            .save(Settings {
                theme: theme.into(),
                ..settings
            })
            .expect("save settings");
    }

    pub fn set_finder_context(&self, root: &str, name: &str) {
        self.state.set_finder_context(Some(FinderContext {
            root: root.into(),
            name: name.into(),
        }));
    }

    pub fn generation(&self) -> u64 {
        self.state.vault.generation()
    }

    pub fn backup_dir(&self) -> String {
        self.state.backups.dir().to_string_lossy().to_string()
    }

    /// The scratch directory as a path string. It exists and holds no project, which is what the
    /// "nothing to import" tests need.
    pub fn scratch_dir(&self) -> String {
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
        finder_detection: crate::freshness::Ledger::new(),
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
