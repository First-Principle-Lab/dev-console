use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Kind {
    #[default]
    Command,
    Secret,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Command => "command",
            Kind::Secret => "secret",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum FieldDisplay {
    #[default]
    Text,
    Multiline,
    Url,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraField {
    pub id: String,
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub display: FieldDisplay,
}

/// Search/browse metadata. Deliberately has no value, notes, or custom-field contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryMeta {
    pub id: String,
    pub kind: Kind,
    pub revision: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub tags: Vec<String>,
    pub description: String,
    pub pinned: bool,
    pub updated_at: String,
    #[serde(default)]
    pub usage_count: i64,
    #[serde(default)]
    pub last_used_at: Option<String>,
    /// Commands only: the stored working directory, used to decide project membership when
    /// ranking. Never carried by secrets. Deliberately still no value, notes or field data.
    #[serde(default)]
    pub working_directory: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    /// The finder renders tags on every row, so a result without them breaks the list.
    pub tags: Vec<String>,
    pub description: String,
    pub completion: Option<String>,
    pub pinned: bool,
    pub usage_count: i64,
    pub last_used_at: Option<String>,
    /// True when this entry belongs to the project context the search was ranked against.
    pub in_context: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub request_id: u64,
    pub vault_generation: u64,
    pub results: Vec<SearchResult>,
    /// The context `results` were ranked against, so the Finder chip can never disagree
    /// with the ordering it is showing. Null when disabled, cleared or undetected.
    pub context: Option<crate::context::FinderContext>,
}

/// Full record as returned to the library. `value` is only ever populated by an explicit
/// reveal of a secret while the vault is unlocked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDetails {
    pub meta: EntryMeta,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub working_directory: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub fields: Vec<ExtraField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub execution_approved: bool,
}

/// A complete record as stored by the backend (commands in SQLite, secrets in the vault).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryRecord {
    pub id: String,
    pub revision: i64,
    pub kind: Kind,
    pub name: String,
    pub entry_type: String,
    pub tags: Vec<String>,
    pub description: String,
    pub notes: String,
    pub fields: Vec<ExtraField>,
    pub pinned: bool,
    pub command: String,
    pub working_directory: String,
    #[serde(default)]
    pub value: String,
    pub created_at: String,
    pub updated_at: String,
}

impl EntryRecord {
    pub fn meta(&self, usage_count: i64, last_used_at: Option<String>) -> EntryMeta {
        EntryMeta {
            id: self.id.clone(),
            kind: self.kind,
            revision: self.revision,
            name: self.name.clone(),
            entry_type: self.entry_type.clone(),
            tags: self.tags.clone(),
            description: self.description.clone(),
            pinned: self.pinned,
            updated_at: self.updated_at.clone(),
            usage_count,
            last_used_at,
            // Secrets never carry a working directory, even if a draft somehow set one.
            working_directory: if self.kind == Kind::Command {
                self.working_directory.clone()
            } else {
                String::new()
            },
        }
    }

    pub fn executable_hash(&self) -> (String, String) {
        (self.command.clone(), self.working_directory.clone())
    }
}

/// Draft submitted by the editor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDraft {
    pub kind: Kind,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub expected_revision: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub fields: Vec<ExtraField>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub working_directory: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedEntry {
    pub id: String,
    pub revision: i64,
    pub kind: Kind,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchRequest {
    pub query: String,
    pub request_id: u64,
    pub limit: Option<usize>,
    /// Set by the Finder after the user cleared the chip: rank as if there were no context
    /// for this query without discarding the detection itself.
    #[serde(default)]
    pub ignore_context: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListRequest {
    pub kind: Kind,
    pub query: Option<String>,
    #[serde(rename = "type")]
    pub entry_type: Option<String>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KindRequest {
    pub kind: Kind,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GetRequest {
    pub kind: Kind,
    pub id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeleteRequest {
    pub kind: Kind,
    pub id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PinRequest {
    pub kind: Kind,
    pub id: String,
    pub expected_revision: i64,
    pub pinned: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CopyRequest {
    pub kind: Kind,
    pub id: String,
    /// 0 means "whatever revision the backend currently holds" (finder path).
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunRequest {
    pub id: String,
    pub expected_revision: i64,
    pub approved: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SaveRequest {
    pub draft: EntryDraft,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OpenLibraryRequest {
    pub kind: Kind,
    pub entry_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardExclusion {
    pub supported: bool,
    pub mechanism: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyResult {
    pub kind: Kind,
    pub id: String,
    pub expires_at_ms: i64,
    pub excluded: bool,
    pub exclusion: ClipboardExclusion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultState {
    pub unlocked: bool,
    pub manual_lock: bool,
    pub keyring_available: bool,
    pub keyring_unlocked: bool,
    pub vault_available: bool,
    pub generation: u64,
    pub authentication_required: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStateInfo {
    pub vault: VaultState,
    pub version: String,
    pub authenticated_unlock_available: bool,
    pub clipboard_exclusion: ClipboardExclusion,
    pub backup_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: String,
    pub autostart: bool,
    pub clipboard_expiry_seconds: u64,
    pub terminal: Vec<String>,
    /// Detect the focused terminal's project when the Finder opens and promote its commands.
    pub project_context_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            autostart: false,
            clipboard_expiry_seconds: 30,
            terminal: vec!["x-terminal-emulator".into(), "-e".into()],
            project_context_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntriesChanged {
    pub kind: Kind,
    pub id: String,
    pub revision: i64,
    pub operation: &'static str,
    pub vault_generation: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowIntent {
    pub action: &'static str,
    pub kind: Kind,
    pub entry_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupOutcome {
    pub source: String,
    pub commands: usize,
    pub secrets: usize,
}
