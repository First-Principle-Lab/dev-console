//! The Library window: the native port of `src/windows/Library.svelte`.
//!
//! The state here is the shipped component's `$state` block; the methods are its functions. Every
//! repository, vault and filesystem call goes through `LibraryActions` on a worker thread
//! ([`Exec`]), and an answer is applied only if it still answers the newest request of its kind,
//! so a slow list load cannot paint over a newer one and a reveal cannot land after a lock.

#[cfg(test)]
mod async_tests;
mod dialogs;
mod editor;
mod environment;
mod history;
mod import;
pub mod settings;
pub mod usage;
mod view;

use std::collections::BTreeSet;

use dev_console_core::execution::RunResult;
use dev_console_core::history::Suggestion;
use dev_console_core::models::{AppStateInfo, VaultState};
use dev_console_core::models::{
    CopyRequest, DeleteRequest, EntryDetails, EntryMeta, GetRequest, Kind, ListRequest, PinRequest,
    RunRequest, SaveRequest,
};
use dev_console_core::services::{AppEvent, Effects};
use dev_console_core::validation::{empty_draft, validate_editor_draft};
use dev_console_core::Surface;
use rune::{Chord, KeyEvent, TextEdit, Tx};
use std::sync::LazyLock;

pub use editor::Editor;
pub use environment::Environment;
pub use import::ProjectImport;
pub use settings::SettingsPanel;
pub use view::library_view;

use crate::hub::{message, Exec, Hub};

/// The width at and under which the Library stacks its panes (`max-width: 900px`).
pub const NARROW: f32 = 900.0;
/// How a status that reports success begins. These are drawn as confirmations and leave sooner.
pub const STATUS_CONFIRMATIONS: [&str; 5] = ["Copied", "Saved", "Deleted", "Launched", "Imported"];

pub const UNSAVED_ESCAPE: &str = "This entry has unsaved edits. Save, or Cancel to discard them.";
pub const BUSY_EDIT: &str = "Save or discard the open edit first.";

static NEW_ENTRY: LazyLock<Chord> = LazyLock::new(|| Chord::parse("ctrl+n").expect("valid chord"));
static SAVE: LazyLock<Chord> = LazyLock::new(|| Chord::parse("ctrl+s").expect("valid chord"));
static COMMANDS: LazyLock<Chord> = LazyLock::new(|| Chord::parse("ctrl+1").expect("valid chord"));
static SECRETS: LazyLock<Chord> = LazyLock::new(|| Chord::parse("ctrl+2").expect("valid chord"));
static FILTER: LazyLock<Chord> = LazyLock::new(|| Chord::parse("ctrl+f").expect("valid chord"));
static EDIT: LazyLock<Chord> = LazyLock::new(|| Chord::parse("ctrl+e").expect("valid chord"));

/// The command-output dialog's state.
#[derive(Debug, Clone)]
pub struct RunOutput {
    pub name: String,
    pub phase: RunPhase,
}

#[derive(Debug, Clone)]
pub enum RunPhase {
    Running,
    Done(RunResult),
    Error(String),
}

/// Which stream of a finished run was copied, or is being copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

pub struct Library {
    hub: Hub,
    exec: Exec<Library>,
    pub theme: String,
    pub updates: crate::updater::State,
    pub app: Option<AppStateInfo>,
    pub vault: Option<VaultState>,
    pub tab: Kind,
    pub usage: usage::Usage,
    pub query: TextEdit,
    pub entries: Vec<EntryMeta>,
    pub selected_id: Option<String>,
    pub selected_suggestion: Option<String>,
    pub history_expanded: bool,
    pub entry_options_open: bool,
    pub details: Option<EntryDetails>,
    /// A secret's value is on screen. Hiding it drops the plaintext, not just the mask.
    pub revealed: bool,
    pub loading: bool,
    pub list_error: Option<String>,
    pub counts: [usize; 2],
    pub editing: Option<Editor>,
    pub saving: bool,
    pub save_error: Option<String>,
    pub status: Option<String>,
    pub pending_delete: Option<EntryMeta>,
    pub pending_run: Option<EntryDetails>,
    pub run_output: Option<RunOutput>,
    run_serial: u64,
    pub copying_stream: Option<Stream>,
    pub copied_stream: Option<Stream>,
    pub output_copy_error: Option<String>,
    /// The unsaved-changes prompt raised by a close request.
    pub pending_close: bool,
    /// Project groups the reader opened.
    /// Project groups the person folded shut. Groups start open: a list you have to unfold
    /// before it shows anything is a list that hides the selection.
    pub collapsed: BTreeSet<String>,
    pub narrow: bool,
    pub sidebar_width: f32,
    window_width: f32,
    pub authenticating: bool,
    pub suggestions: Vec<Suggestion>,
    pub suggestions_busy: bool,
    pub history_enabled: bool,
    pub history_open: bool,
    pub settings: Option<SettingsPanel>,
    pub import: Option<ProjectImport>,
    pub environment: Option<Environment>,
    /// Bound to a field whose owner is gone, so a late keystroke has somewhere harmless to land.
    scratch: TextEdit,
    list_serial: u64,
    details_serial: u64,
    sensitive_serial: u64,
    settings_serial: u64,
    /// The status the toast is showing and the clock time it leaves at.
    status_shown: Option<(String, f64)>,
}

impl Library {
    pub fn new(hub: Hub, exec: Exec<Library>) -> Library {
        let app = hub.library.app_state();
        let mut library = Library {
            theme: hub.library.get_theme(),
            updates: crate::updater::State::default(),
            vault: Some(app.vault.clone()),
            app: Some(app),
            hub,
            exec,
            tab: Kind::Command,
            usage: usage::Usage::default(),
            query: TextEdit::new(String::new()),
            entries: Vec::new(),
            selected_id: None,
            selected_suggestion: None,
            history_expanded: false,
            entry_options_open: false,
            details: None,
            revealed: false,
            loading: true,
            list_error: None,
            counts: [0, 0],
            editing: None,
            saving: false,
            save_error: None,
            status: None,
            pending_delete: None,
            pending_run: None,
            run_output: None,
            run_serial: 0,
            copying_stream: None,
            copied_stream: None,
            output_copy_error: None,
            pending_close: false,
            collapsed: BTreeSet::new(),
            narrow: false,
            sidebar_width: crate::theme::size::PANE_LIST,
            window_width: 1040.0,
            authenticating: false,
            suggestions: Vec::new(),
            suggestions_busy: false,
            history_enabled: false,
            history_open: true,
            settings: None,
            import: None,
            environment: None,
            scratch: TextEdit::new(String::new()),
            list_serial: 0,
            details_serial: 0,
            sensitive_serial: 0,
            settings_serial: 0,
            status_shown: None,
        };
        library.check_updates_if_due();
        library.load_suggestions();
        library.load_list();
        library
    }

    pub fn hub(&self) -> &Hub {
        &self.hub
    }

    pub(crate) fn exec(&self) -> &Exec<Library> {
        &self.exec
    }

    /// The open draft differs from the record it was opened on: `mode === 'edit' && draftDirty`.
    pub fn draft_dirty(&self) -> bool {
        self.editing.as_ref().is_some_and(Editor::dirty)
    }

    /// A locked vault hides secret metadata: the designed state, never a failure.
    pub fn secret_metadata_hidden(&self) -> bool {
        self.tab == Kind::Secret && self.vault.as_ref().is_some_and(|vault| !vault.unlocked)
    }

    pub fn meta(&self) -> Option<&EntryMeta> {
        let id = self.selected_id.as_deref()?;
        self.entries.iter().find(|entry| entry.id == id)
    }

    fn count_index(kind: Kind) -> usize {
        match kind {
            Kind::Command => 0,
            Kind::Secret => 1,
        }
    }

    /// Reloads the list for the current tab and filter, then the selected entry's details.
    pub fn load_list(&mut self) {
        self.list_serial += 1;
        let serial = self.list_serial;
        self.loading = true;
        self.list_error = None;
        let actions = self.hub.library.clone();
        let kind = self.tab;
        let query = Some(self.query.text.trim().to_owned()).filter(|query| !query.is_empty());
        let unlocked = self.vault.as_ref().is_some_and(|vault| vault.unlocked);
        self.exec.clone().run(
            self,
            move || {
                let listed = actions.list_entries(ListRequest {
                    kind,
                    query,
                    ..Default::default()
                });
                let commands = actions
                    .list_entries(ListRequest {
                        kind: Kind::Command,
                        ..Default::default()
                    })
                    .map_or(0, |all| all.len());
                let secrets = if unlocked {
                    actions
                        .list_entries(ListRequest {
                            kind: Kind::Secret,
                            ..Default::default()
                        })
                        .map_or(0, |all| all.len())
                } else {
                    0
                };
                (listed, [commands, secrets])
            },
            move |state, (listed, counts)| {
                if serial != state.list_serial {
                    return;
                }
                state.loading = false;
                state.counts = counts;
                match listed {
                    Ok(entries) => {
                        state.counts[Self::count_index(state.tab)] = entries.len();
                        state.entries = entries;
                        let still_there = state
                            .selected_id
                            .as_ref()
                            .is_some_and(|id| state.entries.iter().any(|entry| &entry.id == id));
                        if state.selected_id.is_some() && !still_there {
                            state.selected_id = state.entries.first().map(|entry| entry.id.clone());
                        } else if state.selected_id.is_none()
                            && state.selected_suggestion.is_none()
                            && !state.narrow
                        {
                            // A narrow window opens on the list: selecting silently would drop the
                            // reader into a detail pane they did not ask for.
                            state.selected_id = state.entries.first().map(|entry| entry.id.clone());
                        }
                        match state.selected_id.clone() {
                            Some(id) => state.load_details(id),
                            None => state.details = None,
                        }
                    }
                    Err(error) => {
                        let text = message(&error);
                        state.entries.clear();
                        state.details = None;
                        if !(state.secret_metadata_hidden()
                            && text.to_lowercase().contains("locked"))
                        {
                            state.list_error = Some(text);
                        }
                    }
                }
            },
        );
    }

    pub fn load_details(&mut self, id: String) {
        self.details_serial += 1;
        let serial = self.details_serial;
        self.revealed = false;
        let actions = self.hub.library.clone();
        let kind = self.tab;
        self.exec.clone().run(
            self,
            move || actions.get_entry(GetRequest { kind, id }),
            move |state, outcome| {
                if serial != state.details_serial {
                    return;
                }
                match outcome {
                    Ok(details) => {
                        state.details = Some(details);
                        if state.editing.is_some() && !state.draft_dirty() {
                            state.editing = None;
                        }
                    }
                    Err(error) => {
                        let text = message(&error);
                        if !(state.secret_metadata_hidden()
                            && text.to_lowercase().contains("locked"))
                        {
                            state.list_error = Some(text);
                        }
                        state.details = None;
                    }
                }
            },
        );
    }

    /// Ctrl+1 / Ctrl+2 and the tab strip. Refused while an unsaved edit is open.
    pub fn select_tab(&mut self, next: Kind) -> bool {
        if self.tab == next && !self.usage.open {
            return false;
        }
        if self.draft_dirty() {
            self.status = Some(BUSY_EDIT.into());
            return false;
        }
        self.editing = None;
        self.selected_suggestion = None;
        self.entry_options_open = false;
        self.usage.open = false;
        self.tab = next;
        self.selected_id = None;
        self.details = None;
        self.load_list();
        true
    }

    pub fn select(&mut self, id: &str) {
        if self.draft_dirty() {
            self.status = Some(BUSY_EDIT.into());
            return;
        }
        self.editing = None;
        self.selected_suggestion = None;
        self.entry_options_open = false;
        self.selected_id = Some(id.to_owned());
        self.load_details(id.to_owned());
    }

    /// The narrow layout's "List" button: back from the detail pane.
    pub fn back_to_list(&mut self) {
        self.selected_id = None;
        self.selected_suggestion = None;
        self.entry_options_open = false;
        self.details = None;
    }

    /// Ctrl+N and New: an empty draft on the current tab, in the selected command's project.
    pub fn start_new(&mut self) {
        self.usage.open = false;
        if self.draft_dirty() {
            self.status = Some(BUSY_EDIT.into());
            return;
        }
        let project_directory = if self.tab == Kind::Command {
            self.meta()
                .and_then(|meta| {
                    meta.project
                        .as_ref()
                        .map(|project| project.root.to_string_lossy().into_owned())
                        .or_else(|| {
                            Some(meta.working_directory.clone()).filter(|dir| !dir.is_empty())
                        })
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        let mut draft = empty_draft(self.tab);
        self.selected_suggestion = None;
        self.entry_options_open = false;
        draft.working_directory = project_directory;
        self.selected_id = None;
        self.details = None;
        self.save_error = None;
        self.editing = Some(Editor::new(draft));
    }

    /// Edit: copies the saved record into a draft. A secret's draft needs its value, which is a
    /// deliberate reveal.
    pub fn start_edit(&mut self) {
        let (Some(details), Some(id)) = (self.details.clone(), self.selected_id.clone()) else {
            return;
        };
        if self.tab == Kind::Command {
            self.editing = Some(Editor::from_details(&details, self.tab));
            self.save_error = None;
            self.status = None;
            return;
        }
        let actions = self.hub.library.clone();
        let kind = self.tab;
        let generation = self.vault.as_ref().map_or(0, |vault| vault.generation);
        let sensitive_serial = self.sensitive_serial;
        let details_serial = self.details_serial;
        self.exec.clone().run(
            self,
            move || actions.reveal_secret(GetRequest { kind, id }),
            move |state, outcome| match outcome {
                Ok(revealed) => {
                    // A lock between asking and answering voids the answer.
                    if sensitive_serial != state.sensitive_serial
                        || details_serial != state.details_serial
                        || state.tab != kind
                        || state.selected_id.as_deref() != Some(revealed.meta.id.as_str())
                        || state
                            .vault
                            .as_ref()
                            .is_some_and(|vault| !vault.unlocked || vault.generation != generation)
                    {
                        return;
                    }
                    state.editing = Some(Editor::from_details(&revealed, kind));
                    state.save_error = None;
                    state.status = None;
                    state.revealed = true;
                }
                Err(error) => state.save_error = Some(message(&error)),
            },
        );
    }

    /// Cancel: drop the draft, and with it any plaintext it held.
    pub fn cancel_edit(&mut self) {
        self.sensitive_serial += 1;
        if let Some(mut editor) = self.editing.take() {
            editor.wipe();
        }
        self.revealed = false;
        self.save_error = None;
    }

    /// Save, and Ctrl+S: validate, write, reload. A failed write keeps the draft.
    pub fn save_draft(&mut self) {
        let Some(editor) = self.editing.as_mut() else {
            return;
        };
        editor.attempted = true;
        let draft = editor.draft();
        if let Some(problem) = validate_editor_draft(&draft) {
            self.save_error = Some(problem);
            return;
        }
        if self.saving {
            return;
        }
        self.saving = true;
        self.save_error = None;
        let actions = self.hub.library.clone();
        let hub = self.hub.clone();
        let close_after = self.pending_close;
        self.exec.clone().run(
            self,
            move || {
                let mut effects = Effects::default();
                let saved = actions.save_entry(SaveRequest { draft }, &mut effects);
                (saved, effects)
            },
            move |state, (saved, effects)| {
                state.saving = false;
                match saved {
                    Ok(row) => {
                        if let Some(mut editor) = state.editing.take() {
                            editor.wipe();
                        }
                        state.revealed = false;
                        state.selected_suggestion = None;
                        state.selected_id = Some(row.0.id.clone());
                        state.status = Some("Saved.".into());
                        state.pending_close = false;
                        state.load_list();
                        // A command that is now in the library must stop being offered.
                        state.load_suggestions();
                        if close_after {
                            state.allow_close();
                        }
                    }
                    Err(error) => state.save_error = Some(message(&error)),
                }
                hub.dispatch(effects);
            },
        );
    }

    pub fn toggle_pin(&mut self, target: Option<EntryMeta>) {
        let Some(target) = target.or_else(|| self.meta().cloned()) else {
            return;
        };
        if self.draft_dirty() {
            return;
        }
        let actions = self.hub.library.clone();
        let hub = self.hub.clone();
        self.exec.clone().run(
            self,
            move || {
                let mut effects = Effects::default();
                let outcome = actions.set_pinned(
                    PinRequest {
                        kind: target.kind,
                        id: target.id,
                        expected_revision: target.revision,
                        pinned: !target.pinned,
                    },
                    &mut effects,
                );
                (outcome, effects)
            },
            move |state, (outcome, effects)| {
                match outcome {
                    Ok(_) => state.load_list(),
                    Err(error) => state.status = Some(message(&error)),
                }
                hub.dispatch(effects);
            },
        );
    }

    /// Copy through the core clipboard service, which owns the expiry and the clearing on lock.
    pub fn copy_entry(&mut self, target: Option<EntryMeta>) {
        let Some(target) = target.or_else(|| self.meta().cloned()) else {
            return;
        };
        let actions = self.hub.library.clone();
        self.exec.clone().run(
            self,
            move || {
                actions.copy_entry(CopyRequest {
                    kind: target.kind,
                    id: target.id,
                    expected_revision: target.revision,
                })
            },
            move |state, outcome| match outcome {
                Ok(result) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |since| since.as_millis() as i64);
                    let seconds = ((result.expires_at_ms - now) as f64 / 1000.0)
                        .round()
                        .max(1.0) as i64;
                    state.status = Some(if result.excluded {
                        format!("Copied for about {seconds}s (excluded from clipboard history).")
                    } else {
                        format!(
                            "Copied for about {seconds}s. {}",
                            settings::sentence(&result.exclusion.detail)
                        )
                    });
                }
                Err(error) => state.status = Some(message(&error)),
            },
        );
    }

    /// Reveal or hide a secret's value. Hiding drops the plaintext.
    pub fn reveal(&mut self) {
        let (Some(id), Some(details)) = (self.selected_id.clone(), self.details.as_mut()) else {
            return;
        };
        if self.revealed {
            details.value = None;
            self.revealed = false;
            return;
        }
        let actions = self.hub.library.clone();
        let kind = self.tab;
        let generation = self.vault.as_ref().map_or(0, |vault| vault.generation);
        let sensitive_serial = self.sensitive_serial;
        let details_serial = self.details_serial;
        self.exec.clone().run(
            self,
            move || actions.reveal_secret(GetRequest { kind, id }),
            move |state, outcome| match outcome {
                Ok(revealed) => {
                    let same = state
                        .details
                        .as_ref()
                        .is_some_and(|details| details.meta.id == revealed.meta.id);
                    let still_open = state
                        .vault
                        .as_ref()
                        .is_some_and(|vault| vault.unlocked && vault.generation == generation);
                    if same
                        && still_open
                        && sensitive_serial == state.sensitive_serial
                        && details_serial == state.details_serial
                    {
                        state.details = Some(revealed);
                        state.revealed = true;
                    }
                }
                Err(error) => state.status = Some(message(&error)),
            },
        );
    }

    pub fn request_delete(&mut self, target: Option<EntryMeta>) {
        if self.draft_dirty() {
            return;
        }
        self.pending_delete = target.or_else(|| self.meta().cloned());
    }

    pub fn cancel_delete(&mut self) {
        self.pending_delete = None;
    }

    /// Removes the record the dialog named, at the revision it was shown at.
    pub fn confirm_delete(&mut self) {
        let Some(target) = self.pending_delete.take() else {
            return;
        };
        let actions = self.hub.library.clone();
        let hub = self.hub.clone();
        self.exec.clone().run(
            self,
            move || {
                let mut effects = Effects::default();
                let outcome = actions.delete_entry(
                    DeleteRequest {
                        kind: target.kind,
                        id: target.id.clone(),
                        expected_revision: target.revision,
                    },
                    &mut effects,
                );
                (outcome, effects, target)
            },
            move |state, (outcome, effects, target)| {
                match outcome {
                    Ok(()) => {
                        if state.selected_id.as_deref() == Some(target.id.as_str()) {
                            state.selected_id = None;
                            state.details = None;
                        }
                        state.status = Some(format!("Deleted “{}”.", target.name));
                        state.load_list();
                    }
                    Err(error) => state.status = Some(message(&error)),
                }
                hub.dispatch(effects);
            },
        );
    }

    /// Run: ask first. The dialog shows the exact command and where it runs.
    pub fn request_run(&mut self) {
        self.pending_run = self.details.clone();
    }

    pub fn cancel_run(&mut self) {
        self.pending_run = None;
    }

    pub fn confirm_run(&mut self) {
        let Some(target) = self.pending_run.take() else {
            return;
        };
        self.status = None;
        self.run_serial += 1;
        let serial = self.run_serial;
        self.copied_stream = None;
        self.output_copy_error = None;
        let name = target.meta.name.clone();
        self.run_output = Some(RunOutput {
            name: name.clone(),
            phase: RunPhase::Running,
        });
        let actions = self.hub.library.clone();
        self.exec.clone().run(
            self,
            move || {
                actions.run_command(RunRequest {
                    id: target.meta.id,
                    expected_revision: target.meta.revision,
                    approved: true,
                })
            },
            move |state, outcome| {
                if serial != state.run_serial {
                    return;
                }
                match outcome {
                    Ok(result) if result.mode == "terminal" => {
                        state.run_output = None;
                        state.status = Some("Launched in a terminal for sudo.".into());
                    }
                    Ok(result) => {
                        state.run_output = Some(RunOutput {
                            name,
                            phase: RunPhase::Done(result),
                        })
                    }
                    Err(error) => {
                        state.run_output = Some(RunOutput {
                            name,
                            phase: RunPhase::Error(message(&error)),
                        })
                    }
                }
            },
        );
    }

    /// Closing the output view does not stop the command; its answer is simply dropped.
    pub fn close_run_output(&mut self) {
        self.run_serial += 1;
        self.run_output = None;
        self.copied_stream = None;
        self.output_copy_error = None;
    }

    pub fn copy_run_stream(&mut self, stream: Stream) {
        let Some(RunOutput {
            phase: RunPhase::Done(result),
            ..
        }) = &self.run_output
        else {
            return;
        };
        if self.copying_stream.is_some() {
            return;
        }
        let value = match stream {
            Stream::Stdout => result.stdout.clone(),
            Stream::Stderr => result.stderr.clone(),
        };
        if value.is_empty() {
            return;
        }
        let serial = self.run_serial;
        self.copying_stream = Some(stream);
        self.output_copy_error = None;
        let actions = self.hub.library.clone();
        self.exec.clone().run(
            self,
            move || actions.copy_command_output(value),
            move |state, outcome| {
                state.copying_stream = None;
                if serial != state.run_serial {
                    return;
                }
                match outcome {
                    Ok(_) => state.copied_stream = Some(stream),
                    Err(error) => state.output_copy_error = Some(message(&error)),
                }
            },
        );
    }

    /// Unlock through the system's authentication prompt.
    pub fn unlock(&mut self) {
        self.authenticating = true;
        self.status = None;
        let actions = self.hub.library.clone();
        let hub = self.hub.clone();
        self.exec.clone().run(
            self,
            move || {
                let mut effects = Effects::default();
                let outcome = actions.unlock_vault(&mut effects);
                let after = actions.app_state().vault;
                (outcome, after, effects)
            },
            move |state, (outcome, after, effects)| {
                state.authenticating = false;
                match outcome {
                    Ok(vault) => {
                        state.vault = Some(vault);
                        state.load_list();
                    }
                    Err(error) => {
                        state.vault = Some(after);
                        state.status = Some(message(&error));
                    }
                }
                hub.dispatch(effects);
            },
        );
    }

    pub fn lock(&mut self) {
        let mut effects = Effects::default();
        match self.hub.library.lock_vault(&mut effects) {
            Ok(vault) => self.apply_vault(vault),
            Err(error) => self.status = Some(message(&error)),
        }
        self.hub.dispatch(effects);
    }

    fn apply_vault(&mut self, vault: VaultState) {
        let locked = !vault.unlocked;
        let was_locked = self.vault.as_ref().is_some_and(|vault| !vault.unlocked);
        self.vault = Some(vault);
        if locked {
            self.entry_options_open = false;
            self.list_serial += 1;
            self.details_serial += 1;
            self.sensitive_serial += 1;
            if let Some(details) = self.details.as_mut() {
                details.value = None;
            }
            self.revealed = false;
            if self.tab == Kind::Secret {
                self.entries.clear();
                self.details = None;
                self.selected_id = None;
                if self
                    .editing
                    .as_ref()
                    .is_some_and(|editor| editor.kind == Kind::Secret)
                {
                    self.cancel_edit();
                }
            }
            if let Some(settings) = self.settings.as_mut() {
                settings.on_lock();
            }
            // The environment view shows decrypted metadata; a lock closes it.
            self.environment = None;
        }
        self.load_counts();
        if !locked && was_locked && self.tab == Kind::Secret {
            self.load_list();
        }
    }

    fn load_counts(&mut self) {
        let actions = self.hub.library.clone();
        let unlocked = self.vault.as_ref().is_some_and(|vault| vault.unlocked);
        let generation = self.vault.as_ref().map(|vault| vault.generation);
        self.exec.clone().run(
            self,
            move || {
                let commands = actions
                    .list_entries(ListRequest {
                        kind: Kind::Command,
                        ..Default::default()
                    })
                    .map_or(0, |all| all.len());
                let secrets = if unlocked {
                    actions
                        .list_entries(ListRequest {
                            kind: Kind::Secret,
                            ..Default::default()
                        })
                        .map_or(0, |all| all.len())
                } else {
                    0
                };
                [commands, secrets]
            },
            move |state, counts| {
                if generation == state.vault.as_ref().map(|vault| vault.generation) {
                    state.counts = counts;
                }
            },
        );
    }

    /// A window close request: with an unsaved draft, ask first; otherwise hide.
    pub fn request_close(&mut self) -> bool {
        if self.draft_dirty() {
            self.pending_close = true;
            return false;
        }
        self.allow_close();
        true
    }

    pub fn keep_editing(&mut self) {
        self.hub
            .state
            .quitting
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.pending_close = false;
    }

    pub fn discard_and_close(&mut self) {
        self.pending_close = false;
        self.cancel_edit();
        self.allow_close();
    }

    pub fn save_and_close(&mut self) {
        self.save_draft();
    }

    /// Hides the window, dropping any revealed plaintext first.
    pub fn allow_close(&mut self) {
        self.pending_close = false;
        self.clear_sensitive();
        self.hub
            .perform(self.hub.library.allow_close(Surface::Library));
    }

    /// Drops every decrypted value this window holds.
    pub fn clear_sensitive(&mut self) {
        self.entry_options_open = false;
        self.sensitive_serial += 1;
        self.details_serial += 1;
        self.revealed = false;
        if let Some(details) = self.details.as_mut() {
            details.value = None;
        }
        if let Some(editor) = self.editing.as_mut() {
            editor.wipe_value();
        }
        if let Some(settings) = self.settings.as_mut() {
            settings.on_lock();
        }
    }

    /// A `ShowLibrary` command from the Finder or the control socket.
    pub fn show_intent(&mut self, kind: Kind, entry_id: Option<String>, _tx: &mut Tx) {
        if self.draft_dirty() {
            self.status = Some(BUSY_EDIT.into());
            return;
        }
        self.editing = None;
        self.tab = kind;
        match entry_id {
            Some(id) => {
                self.selected_id = Some(id);
                self.load_list();
            }
            None => {
                self.load_list();
                self.start_new();
            }
        }
    }

    pub fn load_suggestions(&mut self) {
        self.suggestions_busy = true;
        let actions = self.hub.library.clone();
        self.exec.clone().run(
            self,
            move || {
                let enabled = actions.get_settings().history_suggestions_enabled;
                let suggestions = if enabled {
                    actions.history_suggestions(None).unwrap_or_default()
                } else {
                    Vec::new()
                };
                (enabled, suggestions)
            },
            |state, (enabled, suggestions)| {
                state.suggestions_busy = false;
                state.history_enabled = enabled;
                state.suggestions = suggestions;
                if state
                    .selected_suggestion
                    .as_ref()
                    .is_some_and(|key| !state.suggestions.iter().any(|s| &s.key == key))
                {
                    state.selected_suggestion = None;
                }
            },
        );
    }

    /// Opens the editor with the suggestion prefilled; nothing is saved until it is saved.
    pub fn save_suggestion(&mut self, key: &str) {
        if self.draft_dirty() {
            self.status = Some(BUSY_EDIT.into());
            return;
        }
        let Some(suggestion) = self
            .suggestions
            .iter()
            .find(|suggestion| suggestion.key == key)
            .cloned()
        else {
            return;
        };
        self.tab = Kind::Command;
        self.entry_options_open = false;
        self.details_serial += 1;
        self.selected_id = None;
        self.details = None;
        let mut draft = empty_draft(Kind::Command);
        draft.name = if suggestion.suggested_name.is_empty() {
            suggestion.command.chars().take(60).collect()
        } else {
            suggestion.suggested_name.clone()
        };
        draft.command = suggestion.command.clone();
        draft.working_directory = suggestion.working_directory.clone().unwrap_or_default();
        if let Some(project) = &suggestion.project {
            let slug: String = project
                .to_lowercase()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("-")
                .chars()
                .take(32)
                .collect();
            draft.tags = vec![format!("project:{slug}")];
        }
        // The prefilled draft is the baseline: an untouched suggestion is not an unsaved edit.
        self.editing = Some(Editor::new(draft));
        self.save_error = None;
        self.status = None;
    }

    pub fn dismiss_suggestion(&mut self, key: &str) {
        if self.selected_suggestion.as_deref() == Some(key) {
            self.selected_suggestion = None;
        }
        // Gone immediately; the backend only ever learns the digest.
        self.suggestions.retain(|suggestion| suggestion.key != key);
        let actions = self.hub.library.clone();
        let key = key.to_owned();
        self.exec.clone().run(
            self,
            move || actions.dismiss_suggestion(key),
            |state, outcome| match outcome {
                Ok(()) => state.status = Some("Dismissed - only its hash is remembered.".into()),
                Err(error) => {
                    state.status = Some(message(&error));
                    state.load_suggestions();
                }
            },
        );
    }

    /// The import dialog only ever creates commands, so it opens on the tab that shows them.
    pub fn start_project_import(&mut self) {
        if self.draft_dirty() {
            self.status = Some(BUSY_EDIT.into());
            return;
        }
        if self.tab != Kind::Command {
            self.select_tab(Kind::Command);
        }
        self.import = Some(ProjectImport::new());
    }

    pub fn finish_project_import(&mut self, imported: usize) {
        self.import = None;
        if imported > 0 {
            self.status = Some(format!(
                "Imported {imported} {}.",
                if imported == 1 { "task" } else { "tasks" }
            ));
            self.load_list();
        }
    }

    /// Opening a project group syncs its environment snapshot, as the web list does.
    pub fn toggle_group(&mut self, key: &str, root: &str) {
        if !self.collapsed.remove(key) {
            self.collapsed.insert(key.to_owned());
        } else {
            if !root.is_empty() && self.vault.as_ref().is_some_and(|vault| vault.unlocked) {
                let actions = self.hub.library.clone();
                let root = root.to_owned();
                self.exec.clone().run(
                    self,
                    move || actions.sync_environment(root),
                    |state, outcome| {
                        if outcome.is_err() {
                            state.status = Some(
                                "Environment sync failed. Open the project’s Environment section to retry and see the error.".into(),
                            );
                        }
                    },
                );
            }
        }
    }

    /// The frame hook: a status is a toast, so it leaves on its own. A confirmation ("Copied",
    /// "Saved") stays long enough to be read once; anything else stays twice as long.
    pub fn poll(&mut self, clock: f64, tx: &mut Tx) -> bool {
        if crate::updater::target().is_some() {
            if self
                .updates
                .last_check
                .is_none_or(|time| time.elapsed() >= std::time::Duration::from_secs(900))
            {
                self.check_for_updates();
            }
            tx.wake_after(std::time::Duration::from_secs(900));
        }
        let Some(status) = &self.status else {
            self.status_shown = None;
            return false;
        };
        match &self.status_shown {
            Some((shown, until)) if shown == status => {
                if clock < *until {
                    return false;
                }
                self.status = None;
                self.status_shown = None;
                true
            }
            _ => {
                let confirmation = STATUS_CONFIRMATIONS
                    .iter()
                    .any(|prefix| status.starts_with(prefix));
                // Long enough to read: a base, then time for every word past the first eight.
                let extra = status.split_whitespace().count().saturating_sub(8) as f64 * 0.4;
                let stay = (if confirmation { 4.0 } else { 8.0 } + extra).min(20.0);
                self.status_shown = Some((status.clone(), clock + stay));
                tx.wake_after(std::time::Duration::from_secs_f64(stay));
                false
            }
        }
    }

    pub fn filter_changed(&mut self) {
        self.load_list();
    }

    pub fn set_width(&mut self, width: f32) {
        self.window_width = width;
        let narrow = width <= NARROW;
        if narrow != self.narrow {
            self.narrow = narrow;
            if !narrow
                && self.selected_id.is_none()
                && self.selected_suggestion.is_none()
                && self.editing.is_none()
            {
                if let Some(id) = self.entries.first().map(|entry| entry.id.clone()) {
                    self.select(&id);
                }
            }
        }
    }

    pub fn resize_sidebar(&mut self, width: f32) {
        let max = crate::theme::size::PANE_LIST_MAX.min(self.window_width - 406.0);
        self.sidebar_width = width.clamp(
            crate::theme::size::PANE_LIST_MIN,
            max.max(crate::theme::size::PANE_LIST_MIN),
        );
    }

    pub fn sidebar_width(&self) -> f32 {
        self.sidebar_width
            .min((self.window_width - 406.0).max(crate::theme::size::PANE_LIST_MIN))
    }

    /// Whether a confirmation, a panel or the Settings sheet is over the window.
    pub fn dialog_open(&self) -> bool {
        self.pending_delete.is_some()
            || self.pending_run.is_some()
            || self.run_output.is_some()
            || self.pending_close
            || self.import.is_some()
            || self.environment.is_some()
            || self.settings.is_some()
    }

    /// Escape in the editor, from the window or from one of its fields. It leaves an editor
    /// nothing was typed in. With edits it does nothing: a key this easy to hit must not be able
    /// to lose them.
    pub fn editor_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        if ev.key != rune::Key::Escape
            || ev.mods != rune::Mods::default()
            || self.editing.is_none()
            || self.dialog_open()
        {
            return false;
        }
        if self.draft_dirty() {
            // Say why nothing happened, rather than leave the key looking broken.
            self.status = Some(UNSAVED_ESCAPE.into());
            return true;
        }
        tx.instant();
        self.cancel_edit();
        true
    }

    /// The window's key router, `<svelte:window onkeydown>`.
    pub fn route_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        // A dialog owns the keyboard. Nothing behind it changes while it is up.
        if self.dialog_open() || self.entry_options_open {
            return false;
        }
        if NEW_ENTRY.matches(ev) {
            tx.instant();
            self.start_new();
        } else if FILTER.matches(ev) {
            // The list's filter, from anywhere in the window.
            self.query.focused = true;
        } else if EDIT.matches(ev) {
            if self.editing.is_none() && self.details.is_some() {
                tx.instant();
                self.start_edit();
            }
        } else if self.editor_key(ev, tx) {
        } else if SAVE.matches(ev) {
            if self.editing.is_some() {
                self.save_draft();
            }
        } else if COMMANDS.matches(ev) {
            tx.instant();
            self.select_tab(Kind::Command);
        } else if SECRETS.matches(ev) {
            tx.instant();
            self.select_tab(Kind::Secret);
        } else {
            return false;
        }
        true
    }

    pub fn on_event(&mut self, event: &AppEvent, tx: &mut Tx) {
        match event {
            AppEvent::EntriesChanged(changed) => {
                if changed.kind == self.tab {
                    self.load_list();
                } else {
                    self.load_counts();
                }
            }
            AppEvent::VaultStateChanged(vault) => {
                if !vault.unlocked {
                    // Nothing decrypted may fade out on its way off the screen.
                    tx.instant();
                }
                self.apply_vault(vault.clone());
            }
            AppEvent::SettingsSaved(saved) => {
                self.on_saved_settings(saved);
                self.theme = saved.theme.clone();
            }
            AppEvent::ThemeChanged(theme) => self.theme = theme.clone(),
            AppEvent::OpenSettings => self.open_settings(),
            AppEvent::CloseRequested => {
                self.request_close();
            }
            AppEvent::WindowIntent(intent) => {
                self.selected_suggestion = None;
                self.entry_options_open = false;
                self.tab = intent.kind;
                if intent.action == "new-entry" {
                    self.load_list();
                    self.start_new();
                } else if let Some(id) = &intent.entry_id {
                    self.selected_id = Some(id.clone());
                    self.load_list();
                }
            }
            _ => {}
        }
    }

    /// The field accessor for a bound text input whose owner may have gone away.
    pub(crate) fn editor_field(&mut self, pick: fn(&mut Editor) -> &mut TextEdit) -> &mut TextEdit {
        match &mut self.editing {
            Some(editor) => pick(editor),
            None => &mut self.scratch,
        }
    }
}
