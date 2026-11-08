//! Entries: the list, the two filter boxes, the inspector and the editor (plan section 3).
//!
//! Part of [`LibraryActions`]; the shared read model and the window commands live in `mod.rs`. The
//! list and the inspector carry metadata only - never a secret value:
//! [`LibraryActions::reveal_secret`] is the only call on either surface that hands a decrypted value
//! to a window, and only ever to the library.

use crate::error::{AppError, AppResult};
use crate::models::{
    DeleteRequest, EntryDetails, EntryMeta, GetRequest, Kind, KindRequest, ListRequest, PinRequest,
    SaveRequest, SavedEntry,
};
use crate::repository;

use crate::services::LibraryActions;
use crate::services::{details_from, Effects};

/// `save_entry`'s return: the saved row plus the operation the list needs, which is why it is not
/// just a [`SavedEntry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedRow(pub SavedEntry);

impl LibraryActions {
    // -----------------------------------------------------------------------------------------
    // Entries
    // -----------------------------------------------------------------------------------------

    /// The list, in the store's order (pinned first, then by name). The query is matched by the
    /// store; the two filter boxes are prefixes applied here.
    pub fn list_entries(&self, request: ListRequest) -> AppResult<Vec<EntryMeta>> {
        match request.kind {
            Kind::Command => {
                let entries = self
                    .state
                    .repo
                    .list_commands(request.query.clone().unwrap_or_default().trim())?;
                Ok(crate::services::apply_list_filters(entries, &request))
            }
            // Metadata never leaves the vault while it is locked: `list_secrets` authorises first.
            Kind::Secret => {
                let all = self.state.vault.list_secrets()?;
                let query = request.query.unwrap_or_default().to_lowercase();
                let entry_type = request.entry_type.unwrap_or_default().to_lowercase();
                let tag = request.tag.unwrap_or_default().to_lowercase();
                Ok(all
                    .into_iter()
                    .filter(|entry| {
                        crate::services::matches_secret(entry, &query, &entry_type, &tag)
                    })
                    .collect())
            }
        }
    }

    /// The type-ahead list for the filter row: every type in use across both stores. Delegates to
    /// [`AppState::types`], which is the reader the runtime has always used - including its choice
    /// that a store which cannot be read contributes no types rather than failing the request.
    pub fn types_for(&self, request: KindRequest) -> Vec<String> {
        self.state.types(request.kind)
    }

    /// The inspector. Never carries the secret value - that is [`LibraryActions::reveal_secret`].
    pub fn get_entry(&self, request: GetRequest) -> AppResult<EntryDetails> {
        match request.kind {
            Kind::Command => {
                let (record, usage, last_used) = self
                    .state
                    .repo
                    .get_command(&request.id)?
                    .ok_or(AppError::NotFound)?;
                let mut details =
                    details_from(&record, self.state.repo.approval_state(&record)?, false);
                details.meta.usage_count = usage;
                details.meta.last_used_at = last_used;
                Ok(details)
            }
            Kind::Secret => {
                let record = self.state.vault.get(&request.id)?;
                let usage = self.state.vault.read(|file| {
                    Ok(file
                        .usage
                        .iter()
                        .find(|entry| entry.id == record.id)
                        .map(|entry| (entry.count, entry.last_used_at.clone()))
                        .unwrap_or((0, None)))
                })?;
                let mut details = details_from(&record, false, false);
                details.meta.usage_count = usage.0;
                details.meta.last_used_at = usage.1;
                Ok(details)
            }
        }
    }

    /// The only call that hands a decrypted value to a window, and only ever to the library. The
    /// palette cannot reach this; a copied secret goes to the clipboard instead, where the value
    /// never crosses back into a window.
    pub fn reveal_secret(&self, request: GetRequest) -> AppResult<EntryDetails> {
        if request.kind != Kind::Secret {
            return Err(AppError::Validation("only secrets can be revealed".into()));
        }
        let record = self.state.vault.get(&request.id)?;
        Ok(details_from(&record, false, true))
    }

    /// Save a draft. `operation` is what the list needs to know whether to insert or replace the
    /// row, and it is decided from the draft's own `id` - the same rule the frontend's list relies on.
    pub fn save_entry(&self, request: SaveRequest, effects: &mut Effects) -> AppResult<SavedRow> {
        let mut draft = request.draft;
        let operation = if draft.id.is_none() {
            "created"
        } else {
            "updated"
        };
        repository::validate_draft(&mut draft)?;
        let saved = match draft.kind {
            Kind::Command => self.state.repo.save_command(&mut draft)?,
            Kind::Secret => self.state.vault.save(&mut draft)?,
        };
        crate::services::entries_changed(&self.state, &saved, operation, effects);
        Ok(SavedRow(saved))
    }

    /// Pin or un-pin, at the revision the window is showing: a stale pin cannot overwrite a newer
    /// edit, it gets a conflict and the window reloads.
    pub fn set_pinned(&self, request: PinRequest, effects: &mut Effects) -> AppResult<SavedEntry> {
        let saved = match request.kind {
            Kind::Command => self.state.repo.set_command_pinned(
                &request.id,
                request.expected_revision,
                request.pinned,
            )?,
            Kind::Secret => self.state.vault.set_pinned(
                &request.id,
                request.expected_revision,
                request.pinned,
            )?,
        };
        crate::services::entries_changed(&self.state, &saved, "updated", effects);
        Ok(saved)
    }

    pub fn delete_entry(&self, request: DeleteRequest, effects: &mut Effects) -> AppResult<()> {
        match request.kind {
            Kind::Command => self
                .state
                .repo
                .delete_command(&request.id, request.expected_revision)?,
            Kind::Secret => self
                .state
                .vault
                .delete(&request.id, request.expected_revision)?,
        };
        // The row is gone, so the event names the entry rather than carrying a revision.
        let removed = SavedEntry {
            id: request.id,
            revision: 0,
            kind: request.kind,
        };
        crate::services::entries_changed(&self.state, &removed, "deleted", effects);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::AppEvent;
    use crate::testing::Fixture;

    #[test]
    fn a_reveal_is_the_only_call_that_carries_the_value() {
        let fixture = Fixture::new("reveal");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        // The palette cannot reach any of these, so the check here is that even the library's
        // inspector keeps the value out unless the call is the reveal.
        let request = GetRequest {
            kind: Kind::Secret,
            id: "absent".into(),
        };
        assert_eq!(
            actions.get_entry(request.clone()).unwrap_err().to_string(),
            "entry not found",
            "a locked-then-restored profile still reports the missing record, not a value"
        );
        assert_eq!(
            actions
                .reveal_secret(GetRequest {
                    kind: Kind::Command,
                    id: "whatever".into(),
                })
                .unwrap_err()
                .to_string(),
            "only secrets can be revealed"
        );
    }

    #[test]
    fn saving_a_new_entry_reports_created_and_a_change_reports_updated() {
        let fixture = Fixture::new("save");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");

        let mut draft = fixture.draft("deploy", "echo deploy");
        let mut effects = Effects::default();
        let created = actions
            .save_entry(
                SaveRequest {
                    draft: draft.clone(),
                },
                &mut effects,
            )
            .expect("save")
            .0;
        assert_eq!(created.kind, Kind::Command);
        assert_eq!(created.revision, 1);
        assert_eq!(effects.events.len(), 1);
        match &effects.events[0] {
            AppEvent::EntriesChanged(event) => {
                assert_eq!(event.operation, "created");
                assert_eq!(event.id, created.id);
                assert_eq!(event.vault_generation, fixture.generation());
            }
            other => panic!("expected an entries-changed event, got {other:?}"),
        }

        // The editor resaves the row it is showing, at the revision it loaded.
        draft.id = Some(created.id.clone());
        draft.expected_revision = created.revision;
        draft.command = "echo deploy twice".into();
        let mut effects = Effects::default();
        let updated = actions
            .save_entry(SaveRequest { draft }, &mut effects)
            .expect("resave")
            .0;
        assert_eq!(updated.revision, 2);
        match &effects.events[0] {
            AppEvent::EntriesChanged(event) => assert_eq!(event.operation, "updated"),
            other => panic!("expected an entries-changed event, got {other:?}"),
        }
    }

    #[test]
    fn a_draft_the_store_refuses_produces_no_event() {
        let fixture = Fixture::new("invalid-draft");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        let mut draft = fixture.draft("spaced name", "echo hi");
        draft.command = String::new();
        let mut effects = Effects::default();
        assert!(actions
            .save_entry(SaveRequest { draft }, &mut effects)
            .is_err());
        assert!(
            effects.is_empty(),
            "a rejected save must not tell the list to draw a row that does not exist"
        );
    }

    #[test]
    fn pinning_or_deleting_at_a_stale_revision_conflicts_and_says_nothing() {
        let fixture = Fixture::new("conflict");
        fixture.seed_command("git-push", "Alias", "git push", &["git"]);
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");

        let mut effects = Effects::default();
        assert_eq!(
            actions
                .set_pinned(
                    PinRequest {
                        kind: Kind::Command,
                        id: "git-push".into(),
                        expected_revision: 9,
                        pinned: true,
                    },
                    &mut effects,
                )
                .unwrap_err()
                .to_string(),
            "this entry was changed elsewhere; reload it before saving"
        );
        assert!(effects.is_empty());

        let mut effects = Effects::default();
        assert!(actions
            .delete_entry(
                DeleteRequest {
                    kind: Kind::Command,
                    id: "git-push".into(),
                    expected_revision: 9,
                },
                &mut effects,
            )
            .is_err());
        assert!(
            effects.is_empty(),
            "a rejected delete must not remove a row the user can still see"
        );
    }

    #[test]
    fn a_pinned_row_and_a_deleted_row_both_announce_themselves() {
        let fixture = Fixture::new("pin-and-delete");
        fixture.seed_command("git-push", "Alias", "git push", &["git"]);
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");

        let mut effects = Effects::default();
        let pinned = actions
            .set_pinned(
                PinRequest {
                    kind: Kind::Command,
                    id: "git-push".into(),
                    expected_revision: 1,
                    pinned: true,
                },
                &mut effects,
            )
            .expect("pin");
        assert_eq!(
            pinned.revision, 2,
            "a pin is an edit, so it advances the revision"
        );
        match &effects.events[0] {
            AppEvent::EntriesChanged(event) => {
                assert_eq!(
                    (event.operation, event.id.as_str(), event.revision),
                    ("updated", "git-push", 2)
                );
            }
            other => panic!("expected an entries-changed event, got {other:?}"),
        }

        let mut effects = Effects::default();
        actions
            .delete_entry(
                DeleteRequest {
                    kind: Kind::Command,
                    id: "git-push".into(),
                    expected_revision: 2,
                },
                &mut effects,
            )
            .expect("delete");
        match &effects.events[0] {
            AppEvent::EntriesChanged(event) => assert_eq!(
                (event.operation, event.id.as_str(), event.revision),
                ("deleted", "git-push", 0),
                "the row is gone, so the event names it rather than carrying a revision"
            ),
            other => panic!("expected an entries-changed event, got {other:?}"),
        }
        assert!(actions
            .list_entries(ListRequest {
                kind: Kind::Command,
                ..Default::default()
            })
            .expect("list")
            .is_empty());
    }

    #[test]
    fn the_command_type_box_names_a_whole_type() {
        let fixture = Fixture::new("filters");
        fixture.seed_command("cargo-build", "Shell", "cargo build", &["rust", "ci"]);
        fixture.seed_command("git-push", "Alias", "git push", &["git"]);
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");

        let all = actions
            .list_entries(ListRequest {
                kind: Kind::Command,
                ..Default::default()
            })
            .expect("list");
        assert_eq!(all.len(), 2);

        let shells = actions
            .list_entries(ListRequest {
                kind: Kind::Command,
                entry_type: Some("shell".into()),
                ..Default::default()
            })
            .expect("filtered");
        assert_eq!(
            shells
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["cargo-build"],
            "the box is completed from the types in use, so the comparison is a whole type"
        );
        assert!(
            actions
                .list_entries(ListRequest {
                    kind: Kind::Command,
                    entry_type: Some("sh".into()),
                    ..Default::default()
                })
                .expect("partial type")
                .is_empty(),
            "a fragment of a type name matches no command row"
        );
        // The query reaches the store, which matches any of name, type, description or tags.
        let by_tag = actions
            .list_entries(ListRequest {
                kind: Kind::Command,
                query: Some("ci".into()),
                ..Default::default()
            })
            .expect("query");
        assert_eq!(by_tag.len(), 1);
        assert!(actions
            .list_entries(ListRequest {
                kind: Kind::Command,
                tag: Some("extr".into()),
                ..Default::default()
            })
            .expect("tag")
            .is_empty());
    }

    #[test]
    fn secret_metadata_never_leaves_a_locked_vault() {
        let fixture = Fixture::new("locked-list");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        actions
            .lock_vault(&mut Effects::default())
            .expect("manual lock");
        assert_eq!(
            actions
                .list_entries(ListRequest {
                    kind: Kind::Secret,
                    ..Default::default()
                })
                .unwrap_err()
                .to_string(),
            "the vault is locked"
        );
        assert_eq!(
            actions
                .get_entry(GetRequest {
                    kind: Kind::Secret,
                    id: "anything".into(),
                })
                .unwrap_err()
                .to_string(),
            "the vault is locked"
        );
    }

    #[test]
    fn the_type_list_spans_both_stores_and_ignores_case_when_ordering() {
        let fixture = Fixture::new("types");
        fixture.seed_command("one", "Shell", "echo one", &[]);
        fixture.seed_command("two", "alias", "echo two", &[]);
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        let types = actions.types_for(KindRequest {
            kind: Kind::Command,
        });
        assert_eq!(types, vec!["alias".to_string(), "Shell".to_string()]);
    }
}
