//! Editor-side draft validation, and the empty draft the editor starts from.
//!
//! Ported from `src/lib/validation.ts` and `EMPTY_DRAFT` in `src/lib/models.ts`, the two
//! TypeScript-only pieces plan §3 asks to move into Rust. This is the *editor's* check: the webview
//! recomputes it on every draft change (`problem` in `EntryEditor.svelte`) and refuses to send the
//! draft at all when it answers `Some` (`saveDraft` in `Library.svelte`), so it is what the user
//! experiences as "can I save?". It is deliberately not the same function as
//! [`crate::repository::validate_draft`], which is the backend's authority and also normalises
//! (trims the name and type, lowercases tags, assigns field ids, and enforces the stored length
//! caps). Both rules are kept because they do different jobs:
//!
//! * [`validate_editor_draft`] answers "can the user press Save?" and speaks the sentences the
//!   editor shows, in the order the editor showed them.
//! * [`crate::repository::validate_draft`] runs on the way into storage and is the one that
//!   cannot be argued with.
//!
//! The one rule that exists only here is the spaces-in-name rule: names stay typable in the
//! Finder, where Space copies the selected row. It is a keyboard-ergonomics rule, so the storage
//! layer never had it, and stored entries (including imported ones) may already contain spaces.
//! That is why this lives here and not in `repository::validate_draft`: moving it there would
//! make previously-saved entries unsaveable, which no version of this app has ever done.
//!
//! The tests are the ported `tests/unit/validation.test.ts` cases, named after them.

use crate::models::{EntryDraft, ExtraField, FieldDisplay, Kind};
use crate::repository::is_http_url;

/// Shown when a name contains a space. The sentence is the current UI's, kept verbatim so the
/// ported text does not silently change while the app is still shipping the webview copy.
pub const NAME_SPACES_MESSAGE: &str = "Names can't contain spaces: Space copies the selected result in the finder. Use a dash instead, e.g. wireguard-off.";

/// True when the trimmed name holds whitespace anywhere inside it. Leading and trailing space is
/// not the user typing a space into the name - the backend trims it on save - so `wireguard-off  `
/// is fine and `wireguard off` is not.
pub fn name_has_spaces(name: &str) -> bool {
    name.trim().chars().any(char::is_whitespace)
}

/// True when the app will let this value be opened as a link.
///
/// The one predicate for that question in the crate is [`is_http_url`], which the backend
/// ([`crate::repository::validate_draft`]), the importer ([`crate::import`]) and the `open_url`
/// action already share; this only adds the rule the webview's regex carried, that a URL may not
/// contain whitespace anywhere (the shared predicate tolerates it inside the path). Reusing the
/// shared predicate is deliberate: two answers to "is this http(s)" is how a `javascript:` link
/// gets through one gate and not the other. Where the two rules are not identical, this keeps the
/// split the current app already has - the webview's editor refused any whitespace while the
/// backend's `open_url` gate used `is_http_url`, which additionally has a rule the regex never had
/// (the host must exist and may not start or end with a dot) - instead of inventing a third answer.
pub fn is_openable_url(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.chars().any(char::is_whitespace) && is_http_url(trimmed)
}

/// The editor's check. `None` means the draft may be saved; `Some(message)` is what the form
/// shows, in the order the current UI shows it (name, spaces, type, kind-specific body, fields).
///
/// It reads the draft as typed and writes nothing back - trimming and normalising is
/// [`crate::repository::validate_draft`]'s job on the way into storage.
pub fn validate_editor_draft(draft: &EntryDraft) -> Option<String> {
    if draft.name.trim().is_empty() {
        return Some("Name is required.".to_string());
    }
    if name_has_spaces(&draft.name) {
        return Some(NAME_SPACES_MESSAGE.to_string());
    }
    if draft.entry_type.trim().is_empty() {
        return Some("Type is required.".to_string());
    }
    match draft.kind {
        Kind::Command => {
            if draft.command.trim().is_empty() {
                return Some("Command text is required.".to_string());
            }
        }
        // Checked for length, not for content: a secret of spaces is a real secret, and refusing
        // an empty value here is what stops an abandoned reveal/edit from wiping what is stored.
        Kind::Secret => {
            if draft.value.is_empty() {
                return Some("Secret value is required.".to_string());
            }
        }
    }
    for field in &draft.fields {
        if field.label.trim().is_empty() {
            return Some("Every extra field needs a label.".to_string());
        }
        if field.display == FieldDisplay::Url
            && !field.value.trim().is_empty()
            && !is_openable_url(&field.value)
        {
            let ExtraField { label, .. } = field;
            return Some(format!(r#"Field "{label}" must be an http(s) URL."#));
        }
    }
    None
}

/// The draft a new entry starts from (`EMPTY_DRAFT` in `src/lib/models.ts`). The default type is
/// the kind's name, which is what the editor showed before the user touched the type field.
pub fn empty_draft(kind: Kind) -> EntryDraft {
    EntryDraft {
        kind,
        id: None,
        expected_revision: 0,
        name: String::new(),
        entry_type: match kind {
            Kind::Command => "Command".to_string(),
            Kind::Secret => "Secret".to_string(),
        },
        tags: Vec::new(),
        description: String::new(),
        notes: String::new(),
        fields: Vec::new(),
        pinned: false,
        command: String::new(),
        working_directory: String::new(),
        value: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(label: &str, value: &str, display: FieldDisplay) -> ExtraField {
        ExtraField {
            id: "f1".to_string(),
            label: label.to_string(),
            value: value.to_string(),
            display,
        }
    }

    /// The fixture behind almost every case in `validation.test.ts`: a complete, saveable command.
    fn command(overrides: fn(&mut EntryDraft)) -> EntryDraft {
        let mut draft = EntryDraft {
            kind: Kind::Command,
            name: "prune-containers".to_string(),
            entry_type: "Docker".to_string(),
            command: "docker container prune".to_string(),
            ..empty_draft(Kind::Command)
        };
        overrides(&mut draft);
        draft
    }

    /// `draft validation > accepts a complete command draft`
    #[test]
    fn accepts_a_complete_command_draft() {
        assert_eq!(validate_editor_draft(&command(|_| {})), None);
    }

    /// `draft validation > requires name, type and command text`
    #[test]
    fn requires_name_type_and_command_text() {
        let name = command(|draft| draft.name = "  ".to_string());
        assert_eq!(
            validate_editor_draft(&name).as_deref(),
            Some("Name is required.")
        );
        let entry_type = command(|draft| draft.entry_type = String::new());
        assert_eq!(
            validate_editor_draft(&entry_type).as_deref(),
            Some("Type is required.")
        );
        let text = command(|draft| draft.command = "  ".to_string());
        assert_eq!(
            validate_editor_draft(&text).as_deref(),
            Some("Command text is required.")
        );
    }

    /// `draft validation > rejects spaces inside a name because Space copies in the finder`
    #[test]
    fn rejects_spaces_inside_a_name_because_space_copies_in_the_finder() {
        let space = command(|draft| draft.name = "wireguard off".to_string());
        assert_eq!(
            validate_editor_draft(&space).as_deref(),
            Some(NAME_SPACES_MESSAGE)
        );
        let tab = command(|draft| draft.name = "wireguard\toff".to_string());
        assert_eq!(
            validate_editor_draft(&tab).as_deref(),
            Some(NAME_SPACES_MESSAGE)
        );
        // Padding is trimmed by the backend before it is ever part of the name.
        let padded = command(|draft| draft.name = "  wireguard-off  ".to_string());
        assert_eq!(validate_editor_draft(&padded), None);
        assert!(name_has_spaces("wireguard off"));
        assert!(!name_has_spaces("  wireguard-off  "));
    }

    /// `draft validation > requires a secret value so an empty reveal cannot wipe a stored secret`
    #[test]
    fn requires_a_secret_value_so_an_empty_reveal_cannot_wipe_a_stored_secret() {
        let secret = || EntryDraft {
            kind: Kind::Secret,
            name: "token".to_string(),
            entry_type: "API key".to_string(),
            value: String::new(),
            ..empty_draft(Kind::Secret)
        };
        assert_eq!(
            validate_editor_draft(&secret()).as_deref(),
            Some("Secret value is required.")
        );
        let filled = EntryDraft {
            value: "hunter2".to_string(),
            ..secret()
        };
        assert_eq!(validate_editor_draft(&filled), None);
        // A secret of spaces is still a secret: the rule is about emptiness, not content.
        let spaces = EntryDraft {
            value: " ".to_string(),
            ..secret()
        };
        assert_eq!(validate_editor_draft(&spaces), None);
    }

    /// `draft validation > rejects blank labels and non-http URLs in custom fields`
    #[test]
    fn rejects_blank_labels_and_non_http_urls_in_custom_fields() {
        let blank = command(|draft| {
            draft.fields = vec![field(" ", "x", FieldDisplay::Text)];
        });
        assert_eq!(
            validate_editor_draft(&blank).as_deref(),
            Some("Every extra field needs a label.")
        );
        let bad_url = command(|draft| {
            draft.fields = vec![field("URL", "ftp://x.test", FieldDisplay::Url)];
        });
        assert_eq!(
            validate_editor_draft(&bad_url).as_deref(),
            Some(r#"Field "URL" must be an http(s) URL."#)
        );
        let good_url = command(|draft| {
            draft.fields = vec![field("URL", "https://x.test/a", FieldDisplay::Url)];
        });
        assert_eq!(validate_editor_draft(&good_url), None);
        // An empty url field is "not filled in yet", which the editor has never complained about.
        let empty_url = command(|draft| {
            draft.fields = vec![field("URL", "   ", FieldDisplay::Url)];
        });
        assert_eq!(validate_editor_draft(&empty_url), None);
    }

    /// `draft validation > only treats http(s) as openable URLs`
    #[test]
    fn only_treats_http_s_as_openable_urls() {
        assert!(is_openable_url("https://x.test"));
        assert!(is_openable_url("http://localhost:8080"));
        assert!(!is_openable_url("javascript:alert(1)"));
        assert!(!is_openable_url("file:///etc/passwd"));
        // The webview's regex refused any whitespace; so does this.
        assert!(!is_openable_url("https://x.test/a b"));
    }

    #[test]
    fn an_empty_draft_is_rejected_until_the_user_types() {
        for kind in [Kind::Command, Kind::Secret] {
            assert_eq!(
                validate_editor_draft(&empty_draft(kind)).as_deref(),
                Some("Name is required.")
            );
        }
        assert_eq!(empty_draft(Kind::Command).entry_type, "Command");
        assert_eq!(empty_draft(Kind::Secret).entry_type, "Secret");
        assert_eq!(empty_draft(Kind::Command).id, None);
        assert_eq!(empty_draft(Kind::Command).expected_revision, 0);
    }
}
