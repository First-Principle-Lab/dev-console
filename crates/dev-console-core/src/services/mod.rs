//! Typed application services: the only way a UI reaches the domain (plan section 3).
//!
//! The Tauri build reaches this code through one handler per IPC command, and each handler starts by
//! deciding which window called it and then reaching into [`AppState`] itself. That works, but it
//! means the security decision ("may this window do that?") is repeated four dozen times, and the
//! difference between the two windows exists only as an `if` inside each handler.
//!
//! Here the difference is a type instead. [`LibraryActions`] owns every privileged operation;
//! [`FinderActions`] is a separate value type with its own declared method list, and a palette IPC
//! handler can only ever hold one of those. Nothing converts between them, and nothing on the
//! palette's surface decrypts, edits, executes, or touches an archive or the recovery credential -
//! the compile-time check beside [`FINDER_FORBIDDEN_CAPABILITIES`] turns such a method into a build
//! failure in the same build that produces the binary, rather than a review finding.
//!
//! A call returns a value plus any [`Effects`] it wants the runtime to carry out: typed
//! [`AppEvent`]s and, for the handful of commands whose entire job is windows, a [`WindowCommand`].
//! The core never sees a window, a webview or a tray item; it says what should happen and the desktop
//! runtime - today the Tauri handlers, later the scripted UI harness - performs it.

mod events;
mod finder;
mod library;

pub use events::{AppEvent, Delivery, UpdatePhase, UpdateProgress};
pub use finder::{FinderActions, FINDER_OPERATIONS};
pub use library::{LibraryActions, LIBRARY_OPERATIONS};

use std::sync::Arc;

use crate::error::{AppError, AppResult};
use crate::models::{
    AppStateInfo, CopyRequest, CopyResult, EntriesChanged, EntryDetails, EntryMeta, EntryRecord,
    Kind, ListRequest, SavedEntry, SearchRequest, SearchResponse, VaultState,
};
use crate::state::AppState;
use crate::{auth, clipboard, search};

/// Which window a call came from: a type rather than a string comparison in each handler.
///
/// A label this app does not create is [`Surface::Other`] and keeps its own text in the "this window
/// is not allowed to call ..." message, exactly as the label comparison it replaces did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Surface {
    Library,
    Finder,
    Other(String),
}

impl Surface {
    pub fn from_label(label: &str) -> Surface {
        match label {
            "library" => Surface::Library,
            "finder" => Surface::Finder,
            other => Surface::Other(other.to_string()),
        }
    }

    /// The window label, which is also how the frontend names its windows.
    pub fn label(&self) -> &str {
        match self {
            Surface::Library => "library",
            Surface::Finder => "finder",
            Surface::Other(label) => label,
        }
    }

    pub fn is_library(&self) -> bool {
        matches!(self, Surface::Library)
    }

    /// The privileged guard, written once instead of in every handler.
    pub fn require_library(&self) -> AppResult<&Surface> {
        if self.is_library() {
            Ok(self)
        } else {
            Err(AppError::Forbidden(self.label().to_string()))
        }
    }

    /// The palette's own guard. Opening the library from the library window is what
    /// `ipc::open_library` refused by label, and it is refused the same way here.
    pub fn require_finder(&self) -> AppResult<&Surface> {
        if matches!(self, Surface::Finder) {
            Ok(self)
        } else {
            Err(AppError::Forbidden(self.label().to_string()))
        }
    }
}

/// What a command whose entire job is windows wants the runtime to do. The core decides *which* of
/// these applies (an `allow_close` while quitting is not the same as one while resident); the desktop
/// layer performs it against its own window type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowCommand {
    /// Present the library, then tell it which entry to show or that a draft is starting.
    ShowLibrary {
        kind: Kind,
        entry_id: Option<String>,
    },
    /// Hide the window that asked, leaving the resident app running.
    Hide { surface: Surface },
    /// The close was answered for: clear decrypted state and leave.
    FinishQuit,
    /// The user chose to stay, so the quit's countdown is cancelled.
    CancelQuit,
    /// Begin the resident quit: windows hide, unsaved edits get answered for.
    Quit,
    /// Relaunch into a visible library, which is how an installed update is entered.
    Restart,
}

/// The side effects a call asked for, in the order it asked for them.
///
/// Order is part of the contract: a restore emits the reload before the vault state, so the library
/// refetches its list against the generation the vault event then reports.
#[derive(Debug, Default)]
pub struct Effects {
    pub events: Vec<AppEvent>,
    pub windows: Vec<WindowCommand>,
    /// The tray's lock item follows the vault, and the desktop runtime refreshes it after any vault or
    /// restore change. A flag rather than an event because it is not something a window hears.
    pub tray_refresh: bool,
}

impl Effects {
    pub fn emit(&mut self, event: AppEvent) -> &mut Self {
        self.events.push(event);
        self
    }

    pub fn window(&mut self, command: WindowCommand) -> &mut Self {
        self.windows.push(command);
        self
    }

    pub fn refresh_tray(&mut self) -> &mut Self {
        self.tray_refresh = true;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.windows.is_empty() && !self.tray_refresh
    }
}

/// The capability families that belong to the library alone. A palette method whose name contains one
/// of these is a compile error, which is the strongest form of the plan's rule that the finder
/// "receives no secret reveal, editing, execution, archive, or recovery operations".
const FINDER_FORBIDDEN_CAPABILITIES: &[&str] = &[
    "approve",
    "archive",
    "backup",
    "delete",
    "edit",
    "environment",
    "execute",
    "export",
    "history",
    "import",
    "pin",
    "project",
    "recovery",
    "restore",
    "reveal",
    "run",
    "save",
    "secret",
    "settings",
    "terminal",
];

const fn contains(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return true;
    }
    let mut outer = 0;
    while outer + needle.len() <= haystack.len() {
        let mut inner = 0;
        while inner < needle.len() && haystack[outer + inner] == needle[inner] {
            inner += 1;
        }
        if inner == needle.len() {
            return true;
        }
        outer += 1;
    }
    false
}

/// Compile-time proof that the palette surface stays free of privileged operations.
///
/// Stable Rust cannot enumerate a type's methods, so the guarantee is made against the inventory
/// [`FinderActions`] declares beside it, and `the_finder_surface_declares_every_method_it_has` proves
/// that inventory is the `impl` block by reading this crate's own source. Adding a `reveal_secret`,
/// `save_entry`, `run_command`, `create_backup` or `recovery_key` to the palette is therefore a build
/// failure, in the same build that produces the binary, not a review finding.
const _: () = {
    let mut method = 0;
    while method < FINDER_OPERATIONS.len() {
        let name = FINDER_OPERATIONS[method];
        let mut forbidden = 0;
        while forbidden < FINDER_FORBIDDEN_CAPABILITIES.len() {
            if contains(name, FINDER_FORBIDDEN_CAPABILITIES[forbidden]) {
                panic!("the finder surface may not expose this capability (plan section 3)");
            }
            forbidden += 1;
        }
        method += 1;
    }
};

// ---------------------------------------------------------------------------------------------
// Behaviour shared by both surfaces. Free functions over `AppState`, so a method on one surface
// cannot quietly diverge from the same method on the other.
// ---------------------------------------------------------------------------------------------

pub(crate) fn app_state_info(state: &AppState, version: &str) -> AppStateInfo {
    AppStateInfo {
        vault: state.vault.state(),
        version: version.to_string(),
        authenticated_unlock_available: auth::mechanism_available(),
        clipboard_exclusion: clipboard::exclusion_status(),
        backup_dir: state.backups.dir().to_string_lossy().to_string(),
        // Also sent to the library, which ignores it: it is the answer to "what is this palette
        // ranking by?", and a window created after the detection thread answered still needs it.
        finder_context: state.active_finder_context(),
    }
}

pub(crate) fn search_entries(state: &AppState, request: SearchRequest) -> SearchResponse {
    let entries = state.searchable_metadata(None);
    let context = state.active_finder_context();
    let results = search::rank_in_context(
        &entries,
        &request.query,
        request.limit.unwrap_or(search::DEFAULT_LIMIT),
        context.as_ref(),
    );
    SearchResponse {
        request_id: request.request_id,
        vault_generation: state.vault.generation(),
        results,
    }
}

/// Metadata only for the list, full detail for the inspector, and the decrypted value only for the
/// explicit reveal. Keeping this in one place is what makes "the palette can never receive a value"
/// checkable: no call path passes `include_value` but [`LibraryActions::reveal_secret`].
pub(crate) fn details_from(
    record: &EntryRecord,
    approved: bool,
    include_value: bool,
) -> EntryDetails {
    EntryDetails {
        meta: record.meta(0, None),
        command: if record.kind == Kind::Command {
            Some(record.command.clone())
        } else {
            None
        },
        working_directory: record.working_directory.clone(),
        notes: record.notes.clone(),
        fields: record.fields.clone(),
        value: include_value.then(|| record.value.clone()),
        execution_approved: approved,
    }
}

/// The two filter boxes in the list's header, over whatever the store returned for the search box.
/// The two rules are deliberately different, because that is how the command list has always
/// filtered: the type box names a whole type (the box is completed from the list of types in use,
/// and the command rows are compared case-insensitively but exactly), while a tag box matches the
/// start of a tag. The secret list, which is read out of the vault rather than out of SQL, matches
/// its type box as a prefix - see [`matches_secret`]. The free-text query is matched by the store
/// itself, so it is not repeated here.
pub(crate) fn apply_list_filters(entries: Vec<EntryMeta>, request: &ListRequest) -> Vec<EntryMeta> {
    let mut entries = entries;
    if let Some(entry_type) = request.entry_type.as_deref() {
        entries.retain(|entry| entry.entry_type.eq_ignore_ascii_case(entry_type));
    }
    if let Some(tag) = request.tag.as_deref() {
        let needle = tag.to_lowercase();
        entries.retain(|entry| {
            entry
                .tags
                .iter()
                .any(|tag| tag.to_lowercase().starts_with(&needle))
        });
    }
    entries
}

/// The same three rules over decrypted metadata, which only exists while the vault is open, so an
/// absent box means "no filter" here rather than "match nothing". The vault list has always matched
/// its type box as a prefix where the command list matches it exactly - see [`apply_list_filters`] -
/// and both are kept, because a user who has one list filtered by `sh` and switches to secrets is
/// used to what each of them does.
pub(crate) fn matches_secret(entry: &EntryMeta, query: &str, entry_type: &str, tag: &str) -> bool {
    let query_matches = query.is_empty()
        || entry.name.to_lowercase().contains(query)
        || entry.entry_type.to_lowercase().contains(query)
        || entry.description.to_lowercase().contains(query)
        || entry
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(query));
    let type_matches =
        entry_type.is_empty() || entry.entry_type.to_lowercase().starts_with(entry_type);
    let tag_matches = tag.is_empty()
        || entry
            .tags
            .iter()
            .any(|held| held.to_lowercase().starts_with(tag));
    query_matches && type_matches && tag_matches
}

/// Copying the *saved* record, never a draft, so a stale window cannot put text on the clipboard that
/// the user never saved. An `expected_revision` of 0 means "whatever is current", which is what the
/// palette sends because it never holds a revision.
///
/// The secret branch reads the value inside the vault guard, so a copy queued before a lock cannot
/// complete after one was acknowledged.
pub(crate) fn copy_entry(state: &Arc<AppState>, request: CopyRequest) -> AppResult<CopyResult> {
    let expiry = state.settings.get().clipboard_expiry_seconds;
    match request.kind {
        Kind::Command => {
            let (record, _, _) = state
                .repo
                .get_command(&request.id)?
                .ok_or(AppError::NotFound)?;
            if request.expected_revision != 0 && request.expected_revision != record.revision {
                return Err(AppError::Conflict);
            }
            let result =
                state
                    .clipboard
                    .copy(Kind::Command, &record.id, &record.command, expiry)?;
            state.repo.record_usage(&record.id)?;
            Ok(result)
        }
        Kind::Secret => {
            let result = state.vault.read(|file| {
                let record = file
                    .records
                    .iter()
                    .find(|record| record.id == request.id && record.kind == Kind::Secret)
                    .ok_or(AppError::NotFound)?;
                if request.expected_revision != 0 && request.expected_revision != record.revision {
                    return Err(AppError::Conflict);
                }
                // The native copy happens inside the vault guard.
                state
                    .clipboard
                    .copy(Kind::Secret, &record.id, &record.value, expiry)
            })?;
            let _ = state.vault.record_usage(&request.id);
            Ok(result)
        }
    }
}

/// The lock both windows may call. Order matters, and is the order the Tauri handlers used: secrets
/// stop being readable first, then any app-owned clipboard copy goes away while it is still the app's
/// value.
pub(crate) fn lock_vault(state: &Arc<AppState>, effects: &mut Effects) -> AppResult<VaultState> {
    state.vault.lock()?;
    state.clipboard.clear_owned();
    let vault = state.vault.state();
    effects.emit(AppEvent::VaultStateChanged(vault.clone()));
    effects.refresh_tray();
    Ok(vault)
}

/// Unlock, then settle a library replacement that startup could not decide about (there was no
/// readable keyring then). Until it succeeds the secret store stays shut: the two stores might
/// otherwise disagree about which library is live, and a half-applied replacement would be presented
/// as a working vault.
pub(crate) fn unlock_vault(state: &Arc<AppState>, effects: &mut Effects) -> AppResult<VaultState> {
    state.vault.unlock()?;
    match state.reconcile_interrupted_replacement() {
        Ok(Some(report)) => {
            // The same line the runtime printed before this layer existed: the owner has to be able
            // to tell, from the log, that a replacement was settled rather than silently lost.
            eprintln!("dev-console: {}", report.message());
            // The library has to refetch: what it listed may have just been replaced.
            reload_all(state, effects);
        }
        Ok(None) => {}
        Err(error) => {
            let reason = error.to_string();
            state.vault.quarantine(&reason);
            let vault = state.vault.state();
            effects.emit(AppEvent::VaultStateChanged(vault));
            effects.refresh_tray();
            return Err(AppError::VaultUnavailable(reason));
        }
    }
    let vault = state.vault.state();
    effects.emit(AppEvent::VaultStateChanged(vault.clone()));
    effects.refresh_tray();
    Ok(vault)
}

/// Closing a window, from either window. The rule is shared because the shipped handler never
/// looked at which window asked: a close answered for *while the app is quitting* finishes that
/// quit whichever window it came from, and only otherwise is it a plain hide. Keeping one
/// implementation is what stops the palette's `Escape` from leaving a quit hanging in the air.
pub(crate) fn allow_close(state: &AppState, surface: Surface) -> WindowCommand {
    if state.quitting.load(std::sync::atomic::Ordering::SeqCst) {
        WindowCommand::FinishQuit
    } else {
        WindowCommand::Hide { surface }
    }
}

/// One refresh signal after a bulk operation (a backup restore or an archive import): the reload goes
/// first so the refetch is read at the generation the vault event then reports.
pub(crate) fn reload_all(state: &Arc<AppState>, effects: &mut Effects) {
    entries_changed(state, &reloaded_marker(), "reloaded", effects);
    effects.emit(AppEvent::VaultStateChanged(state.vault.state()));
    effects.refresh_tray();
}

/// The list-reload signal: no entry, revision 0, and the generation the reload must be read at.
fn reloaded_marker() -> SavedEntry {
    SavedEntry {
        id: String::new(),
        revision: 0,
        kind: Kind::Command,
    }
}

pub(crate) fn entries_changed(
    state: &Arc<AppState>,
    saved: &SavedEntry,
    operation: &'static str,
    effects: &mut Effects,
) {
    effects.emit(AppEvent::EntriesChanged(EntriesChanged {
        kind: saved.kind,
        id: saved.id.clone(),
        revision: saved.revision,
        operation,
        vault_generation: state.vault.generation(),
    }));
}

/// An http(s) link from a URL field, opened in the user's default browser. Anything else is rejected
/// here so a stored value can never launch an arbitrary scheme or program, and the leading-dash rule
/// is part of that: `-file=...` would be an argument to the opener.
pub(crate) fn validated_openable_url(url: &str) -> AppResult<&str> {
    let url = url.trim();
    if !crate::repository::is_http_url(url) || url.starts_with('-') {
        return Err(AppError::Validation(
            "only http(s) links can be opened".into(),
        ));
    }
    Ok(url)
}

/// The `pub fn` names in a service source file, skipping the constructor.
///
/// Stable Rust cannot enumerate a type's methods, so each surface declares its operation list as a
/// const beside the `impl`, and the tests in this module read the same source back to prove the
/// declaration is the surface. A method added without a name in the inventory fails the build, which
/// is what makes the compile-time capability check on the palette's list worth anything.
#[cfg(test)]
pub(crate) fn public_methods(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix("pub fn ")?;
            let name = rest.split('(').next()?;
            if name == "new" || name.is_empty() {
                return None;
            }
            Some(name.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::FieldDisplay;

    /// Every IPC command the Tauri runtime registers, from its `invoke_handler` list. The two service
    /// surfaces together must cover exactly this set: nothing unreachable, nothing missing.
    const IPC_COMMANDS: &[&str] = &[
        "allow_close",
        "app_state",
        "cancel_quit",
        "clear_dismissed_suggestions",
        "clear_finder_context",
        "copy_command_output",
        "copy_entry",
        "copy_environment",
        "copy_recovery_key",
        "create_backup",
        "delete_entry",
        "dismiss_suggestion",
        "dismissed_suggestion_count",
        "environment_projects",
        "export_archive",
        "get_entry",
        "get_settings",
        "get_theme",
        "hide_self",
        "history_suggestions",
        "import_archive",
        "import_project",
        "inspect_archive",
        "inspect_backup",
        "install_history_tracking",
        "list_backups",
        "list_entries",
        "list_terminals",
        "lock_vault",
        "open_library",
        "open_settings",
        "open_url",
        "quit_app",
        "recovery_key",
        "restart_app",
        "restore_backup",
        "reveal_secret",
        "run_command",
        "save_entry",
        "save_settings",
        "scan_project",
        "search_entries",
        "set_environment_sync",
        "set_pinned",
        "sync_environment",
        "types_for",
        "unlock_vault",
    ];

    #[test]
    fn the_two_surfaces_together_cover_every_ipc_command_exactly_once() {
        let mut declared: Vec<&str> = LIBRARY_OPERATIONS.to_vec();
        declared.extend(
            FINDER_OPERATIONS
                .iter()
                .filter(|operation| !LIBRARY_OPERATIONS.contains(operation))
                .copied(),
        );
        declared.sort_unstable();
        declared.dedup();

        let mut expected = IPC_COMMANDS.to_vec();
        expected.sort_unstable();
        assert_eq!(
            declared, expected,
            "a command with no service method, or a service method with no command"
        );
        assert_eq!(
            LIBRARY_OPERATIONS.len() + FINDER_OPERATIONS.len() - shared_operations().len(),
            IPC_COMMANDS.len(),
            "an operation named in one inventory but implemented by only one surface"
        );
    }

    fn shared_operations() -> Vec<&'static str> {
        FINDER_OPERATIONS
            .iter()
            .copied()
            .filter(|operation| LIBRARY_OPERATIONS.contains(operation))
            .collect()
    }

    #[test]
    fn the_palette_exposes_nothing_the_library_does_not_besides_its_own_two_moves() {
        // Dismissing the chip and summoning the library are the palette's own; every other palette
        // operation is also a library operation, so there is no capability whose only home is the
        // window that cannot decrypt anything.
        let palette_only: Vec<&str> = FINDER_OPERATIONS
            .iter()
            .copied()
            .filter(|operation| !LIBRARY_OPERATIONS.contains(operation))
            .collect();
        assert_eq!(palette_only, ["clear_finder_context", "open_library"]);
        assert_eq!(
            shared_operations(),
            [
                "allow_close",
                "app_state",
                "copy_entry",
                "get_theme",
                "hide_self",
                "lock_vault",
                "quit_app",
                "search_entries",
                "unlock_vault"
            ]
        );
    }

    #[test]
    fn the_guard_names_the_window_that_was_refused() {
        assert!(Surface::Library.require_library().is_ok());
        assert_eq!(
            Surface::Finder.require_library().unwrap_err().to_string(),
            "this window is not allowed to call finder",
            "the message is the label comparison's own, so the shipped behaviour is unchanged"
        );
        assert_eq!(
            Surface::Other("webview-9".into())
                .require_library()
                .unwrap_err()
                .to_string(),
            "this window is not allowed to call webview-9"
        );
        assert!(Surface::Finder.require_finder().is_ok());
        assert_eq!(
            Surface::Library.require_finder().unwrap_err().to_string(),
            "this window is not allowed to call library"
        );
        for label in ["library", "finder", "something-else"] {
            assert_eq!(Surface::from_label(label).label(), label);
        }
        assert!(Surface::from_label("library").is_library());
        assert!(!Surface::from_label("finder").is_library());
    }

    fn entry(name: &str, entry_type: &str, tags: &[&str]) -> EntryMeta {
        EntryMeta {
            id: name.into(),
            kind: Kind::Command,
            revision: 1,
            name: name.into(),
            entry_type: entry_type.into(),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            description: String::new(),
            pinned: false,
            updated_at: String::new(),
            usage_count: 0,
            last_used_at: None,
            working_directory: String::new(),
            project: None,
        }
    }

    fn filters(query: Option<&str>, entry_type: Option<&str>, tag: Option<&str>) -> ListRequest {
        ListRequest {
            kind: Kind::Command,
            query: query.map(str::to_string),
            entry_type: entry_type.map(str::to_string),
            tag: tag.map(str::to_string),
        }
    }

    #[test]
    fn the_command_type_box_names_a_whole_type_while_a_tag_box_names_its_start() {
        // Shipped asymmetry, kept deliberately: the command list's type box is completed from the
        // types already in use and compared exactly (case-insensitively), while a tag box matches
        // the start of a tag. `the_secret_list_matches_its_type_box_as_a_prefix` covers the other
        // half of the pair, which is the vault list and does prefix-match its type box.
        let entries = vec![
            entry("cargo-build", "Shell", &["rust", "ci"]),
            entry("git-push", "Alias", &["git"]),
        ];
        let names = |filter: crate::models::ListRequest| -> Vec<String> {
            apply_list_filters(entries.clone(), &filter)
                .into_iter()
                .map(|entry| entry.name)
                .collect()
        };
        // No boxes: the store's order and rows pass straight through.
        assert_eq!(
            names(filters(None, None, None)),
            ["cargo-build", "git-push"]
        );
        // The query box is matched by the store, so it does not reach this filter at all.
        assert_eq!(
            names(filters(Some("anything-not-there"), None, None)).len(),
            2,
            "the free-text box is the store's job, not this filter's"
        );
        assert_eq!(names(filters(None, Some("Shell"), None)), ["cargo-build"]);
        assert_eq!(names(filters(None, Some("ALIAS"), None)), ["git-push"]);
        assert!(
            names(filters(None, Some("sh"), None)).is_empty(),
            "the command type box names a whole type, not the start of one"
        );
        assert_eq!(names(filters(None, None, Some("ci"))), ["cargo-build"]);
        assert_eq!(names(filters(None, None, Some("ru"))), ["cargo-build"]);
        assert!(
            names(filters(None, None, Some("ust"))).is_empty(),
            "a tag box matches the start of a tag, never the middle of one"
        );
    }

    #[test]
    fn the_secret_list_matches_its_type_box_as_a_prefix() {
        // The other half of the pair: `matches_secret` is the vault list, and it has always
        // prefix-matched the type box. Asserted next to the command rule so the asymmetry is a
        // decision on the record rather than two files disagreeing by accident.
        let entry = entry("aws", "API key", &["cloud"]);
        // The needles arrive lowercased, as `list_entries` lowers them once per request.
        assert!(
            matches_secret(&entry, "", "api", ""),
            "a prefix is enough here"
        );
        assert!(!matches_secret(&entry, "", "", "nope"));
    }

    #[test]
    fn secret_metadata_matches_the_same_three_rules() {
        // The record's own fields are mixed case ("API key", "cloud"); the needles arrive lowercased,
        // because `LibraryActions::list_entries` lowers them once for the whole request - the same
        // thing the Tauri handler did before this code existed.
        let entry = entry("aws", "API key", &["cloud"]);
        assert!(matches_secret(&entry, "", "", ""));
        assert!(matches_secret(&entry, "aws", "", ""), "by name");
        assert!(
            matches_secret(&entry, "api", "", ""),
            "by type, lowered by the caller"
        );
        assert!(matches_secret(&entry, "clou", "", ""), "and by tag");
        assert!(
            matches_secret(&entry, "", "api", ""),
            "the type box matches from the start"
        );
        assert!(
            !matches_secret(&entry, "", "key", ""),
            "a prefix, not a substring"
        );
        assert!(matches_secret(&entry, "", "", "clo"));
        assert!(
            !matches_secret(&entry, "", "", "lou"),
            "a tag box matches the start"
        );
        assert!(!matches_secret(&entry, "nothing", "", ""));
    }

    #[test]
    fn a_reveal_is_the_only_call_that_hands_over_the_value() {
        let record = crate::models::EntryRecord {
            id: "secret-1".into(),
            revision: 4,
            kind: Kind::Secret,
            name: "aws".into(),
            entry_type: "API key".into(),
            tags: vec!["cloud".into()],
            description: String::new(),
            notes: "prod".into(),
            fields: vec![crate::models::ExtraField {
                id: "f".into(),
                label: "Dashboard".into(),
                value: "https://example.com".into(),
                display: FieldDisplay::Url,
            }],
            pinned: true,
            command: String::new(),
            working_directory: String::new(),
            value: "hunter2".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };

        let listed = details_from(&record, false, false);
        assert_eq!(listed.value, None, "the inspector never carries a secret");
        assert_eq!(listed.command, None, "a secret has no command line to show");
        assert_eq!(listed.notes, "prod", "metadata is not the sensitive part");
        assert!(listed.meta.pinned);
        assert_eq!(listed.fields.len(), 1);

        let revealed = details_from(&record, false, true);
        assert_eq!(revealed.value.as_deref(), Some("hunter2"));

        let command = crate::models::EntryRecord {
            kind: Kind::Command,
            command: "git push".into(),
            ..record
        };
        let details = details_from(&command, true, false);
        assert_eq!(details.command.as_deref(), Some("git push"));
        assert!(details.execution_approved);
        assert_eq!(details.value, None);
    }

    #[test]
    fn only_an_http_or_https_link_may_be_opened() {
        assert_eq!(
            validated_openable_url(" https://example.com/a").ok(),
            Some("https://example.com/a")
        );
        assert_eq!(
            validated_openable_url("http://localhost:8080").ok(),
            Some("http://localhost:8080")
        );
        for rejected in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "mailto:someone@example.com",
            // A leading dash would be an argument to the opener, not a URL for it.
            "-file=/tmp/evil",
            "",
        ] {
            assert!(
                validated_openable_url(rejected).is_err(),
                "{rejected} must not be openable"
            );
        }
        assert_eq!(
            validated_openable_url("ssh://example.com")
                .unwrap_err()
                .to_string(),
            "only http(s) links can be opened"
        );
        // The two URL rules in this crate are deliberately different, and the split predates this
        // module: the *editor* refuses whitespace so a URL field cannot silently carry a second
        // word, while the *opener* only asks whether the scheme is http(s), because that is all the
        // shipped `open_url` ever checked. A value the editor accepts is therefore always openable,
        // and the extra strictness stays where it belongs - on the way in.
        assert!(validated_openable_url("https://example.com/a b").is_ok());
        assert!(!crate::validation::is_openable_url(
            "https://example.com/a b"
        ));
    }

    #[test]
    fn effects_keep_the_order_the_frontend_reads_them_in() {
        let mut effects = Effects::default();
        assert!(effects.is_empty());
        effects
            .emit(AppEvent::VaultStateChanged(vault()))
            .window(WindowCommand::Quit)
            .refresh_tray();
        assert_eq!(effects.events.len(), 1);
        assert_eq!(effects.windows, vec![WindowCommand::Quit]);
        assert!(effects.tray_refresh);
        assert!(!effects.is_empty());
        assert_eq!(effects.events[0].delivery(), Delivery::All);
        assert_eq!(AppEvent::OpenSettings.delivery(), Delivery::Library);
        assert_eq!(AppEvent::FinderShown.delivery(), Delivery::Finder);
    }

    /// Whole-line comments and doc comments dropped, so the scan is about code. The prose in these
    /// files deliberately names the runtime it is decoupled from - that is how a reader learns where
    /// each `AppEvent` ends up.
    fn strip_comments(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn no_desktop_runtime_type_crosses_into_the_service_layer() {
        // The point of this module is that the same code drives a webview window, a Rune view and a
        // scripted test, so nothing in it may name a runtime type. Asserted on the source, because a
        // `grep` in a commit message stops being true the moment someone edits the file.
        //
        // The names are assembled at run time: this test's own source is one of the files it scans,
        // and writing a literal here would fail the assertion it is part of.
        let runtime = ["tau", "ri"].concat();
        let forbidden = [
            format!("{runtime}::"),
            format!("use {runtime}"),
            format!("{runtime}_lib::"),
            ["Webview", "Window"].concat(),
            ["App", "Handle"].concat(),
            ["Rw", "Hook"].concat(),
        ];
        for (name, source) in [
            ("mod.rs", include_str!("mod.rs")),
            ("finder.rs", include_str!("finder.rs")),
            ("library/mod.rs", include_str!("library/mod.rs")),
            ("library/archive.rs", include_str!("library/archive.rs")),
            ("library/entries.rs", include_str!("library/entries.rs")),
            ("library/execution.rs", include_str!("library/execution.rs")),
            ("library/projects.rs", include_str!("library/projects.rs")),
            ("library/settings.rs", include_str!("library/settings.rs")),
            ("events.rs", include_str!("events.rs")),
            ("../testing.rs", include_str!("../testing.rs")),
        ] {
            let code = strip_comments(source);
            for needle in &forbidden {
                assert!(
                    !code.contains(needle.as_str()),
                    "{name} mentions {needle}, which belongs to a desktop runtime"
                );
            }
        }
        // And no runtime dependency, in this crate or in anything the services compile against.
        let manifest = include_str!("../../Cargo.toml");
        let dependencies = manifest
            .split_once("[dependencies]")
            .expect("dependencies section")
            .1
            .split_once("[dev-dependencies]")
            .expect("dev-dependencies section")
            .0;
        assert!(
            !dependencies.contains(&runtime) && !dependencies.contains("webview"),
            "the domain crate must not depend on a desktop runtime"
        );
    }

    fn vault() -> crate::models::VaultState {
        crate::models::VaultState {
            unlocked: false,
            manual_lock: true,
            keyring_available: false,
            keyring_unlocked: false,
            vault_available: true,
            generation: 1,
            authentication_required: false,
            reason: None,
        }
    }
}
