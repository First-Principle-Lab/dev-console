//! One-shot import of a plain (unencrypted) Bitwarden JSON export into the secret vault.
//!
//! Runs inside the resident app so the vault is written through the normal save path; only the
//! file path crosses the control socket, never a secret value.

use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{EntryDraft, EntryRecord, ExtraField, FieldDisplay, Kind};
use crate::repository::is_http_url;
use crate::state::AppState;

const LOGIN_TYPE: &str = "Login";

pub struct Outcome {
    pub imported: usize,
    pub renamed: usize,
    pub duplicates: usize,
    pub skipped: Vec<String>,
}

fn text(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or("").to_string()
}

fn field(label: &str, value: String, display: FieldDisplay) -> ExtraField {
    ExtraField {
        id: String::new(),
        label: label.into(),
        value,
        display,
    }
}

fn record_stub() -> EntryRecord {
    EntryRecord {
        id: String::new(),
        revision: 0,
        kind: Kind::Secret,
        name: String::new(),
        entry_type: String::new(),
        tags: Vec::new(),
        description: String::new(),
        notes: String::new(),
        fields: Vec::new(),
        pinned: false,
        command: String::new(),
        working_directory: String::new(),
        value: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

fn username_of(fields: &[ExtraField]) -> String {
    fields
        .iter()
        .find(|f| f.label == "Username")
        .map(|f| f.value.to_lowercase())
        .unwrap_or_default()
}

/// Returns None for entries that carry no password to store.
fn draft_from(item: &Value) -> Option<EntryDraft> {
    let login = item.get("login")?;
    let password = text(login.get("password"));
    if password.is_empty() {
        return None;
    }
    let mut fields = Vec::new();
    let username = text(login.get("username"));
    if !username.is_empty() {
        fields.push(field("Username", username, FieldDisplay::Text));
    }
    for uri in login
        .get("uris")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let uri = text(uri.get("uri"));
        if uri.is_empty() {
            continue;
        }
        let display = if is_http_url(&uri) {
            FieldDisplay::Url
        } else {
            FieldDisplay::Text
        };
        fields.push(field("URL", uri, display));
    }
    let totp = text(login.get("totp"));
    if !totp.is_empty() {
        fields.push(field("TOTP", totp, FieldDisplay::Text));
    }
    for extra in item
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let label = text(extra.get("name"));
        let value = text(extra.get("value"));
        if !label.trim().is_empty() && !value.is_empty() {
            fields.push(field(&label, value, FieldDisplay::Text));
        }
    }
    Some(EntryDraft {
        kind: Kind::Secret,
        name: text(item.get("name")),
        entry_type: LOGIN_TYPE.into(),
        notes: text(item.get("notes")),
        pinned: item
            .get("favorite")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        fields,
        value: password,
        ..EntryDraft::default()
    })
}

pub fn bitwarden(state: &AppState, path: &str) -> AppResult<Outcome> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| AppError::Validation(format!("cannot read {path}: {e}")))?;
    let export: Value = serde_json::from_str(&raw)
        .map_err(|_| AppError::Validation("file is not valid JSON".into()))?;
    if export.get("encrypted").and_then(Value::as_bool) == Some(true) {
        return Err(AppError::Validation(
            "encrypted Bitwarden exports are not supported; export as plain .json".into(),
        ));
    }
    let items = export
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Validation("no items found in the export".into()))?;

    // Several accounts can share one site, so each entry is named "site (username)" and
    // identity is that display name. Entries imported earlier under the bare site name are
    // renamed in place instead of duplicated.
    let mut existing: Vec<(String, EntryRecord)> = Vec::new();
    for meta in state.vault.list_secrets()? {
        let record = state.vault.get(&meta.id)?;
        existing.push((record.name.to_lowercase(), record));
    }
    let mut outcome = Outcome {
        imported: 0,
        renamed: 0,
        duplicates: 0,
        skipped: Vec::new(),
    };
    for item in items {
        let name = text(item.get("name"));
        let Some(mut draft) = draft_from(item) else {
            outcome.skipped.push(format!("{name} (no password)"));
            continue;
        };
        let bare = draft.name.trim().to_lowercase();
        let username = username_of(&draft.fields);
        if !username.is_empty() {
            draft.name = format!("{} ({})", draft.name.trim(), username);
        }
        let key = draft.name.to_lowercase();
        if existing.iter().any(|(n, _)| *n == key) {
            outcome.duplicates += 1;
            continue;
        }
        let legacy = existing.iter().position(|(n, r)| {
            *n == bare && r.entry_type == LOGIN_TYPE && username_of(&r.fields) == username
        });
        if let Some(index) = legacy {
            let record = &existing[index].1;
            let mut renamed = EntryDraft {
                kind: Kind::Secret,
                id: Some(record.id.clone()),
                expected_revision: record.revision,
                name: draft.name.clone(),
                entry_type: record.entry_type.clone(),
                tags: record.tags.clone(),
                description: record.description.clone(),
                notes: record.notes.clone(),
                fields: record.fields.clone(),
                pinned: record.pinned,
                value: record.value.clone(),
                ..EntryDraft::default()
            };
            match state.vault.save(&mut renamed) {
                Ok(_) => {
                    existing[index].0 = key;
                    outcome.renamed += 1;
                }
                Err(error) => outcome.skipped.push(format!("{name} ({error})")),
            }
            continue;
        }
        match state.vault.save(&mut draft) {
            Ok(_) => {
                existing.push((key, record_stub()));
                outcome.imported += 1;
            }
            Err(error) => outcome.skipped.push(format!("{name} ({error})")),
        }
    }
    Ok(outcome)
}
