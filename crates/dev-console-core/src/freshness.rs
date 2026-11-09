//! Whether an answer that arrived late may still be applied (plan §3: "Retain request IDs,
//! revision checks, and vault generations so obsolete results cannot restore stale or sensitive
//! state").
//!
//! Blocking work - a query, a ranking, a scan, a reveal - has to leave the thread that paints the
//! window. Once it does, a second question appears, and it is not a threading question at all:
//! *is the answer still about the screen the user is looking at?* Two keystrokes, a manual lock, a
//! library replacement or somebody else's Save can each turn a perfectly correct answer into a
//! wrong one. This module is that decision, in one place, for every stream of background work in
//! the app, so a view cannot forget to ask.
//!
//! ## The rules are the shipped app's rules
//!
//! The palette has done this in TypeScript since before this crate existed
//! (`src/windows/Finder.svelte`, `runSearch`):
//!
//! ```text
//! const id = ++requestId;
//! ...
//! if (id !== requestId) return;                                     // a newer query won
//! const generation = vault?.generation ?? response.vaultGeneration;
//! if (response.vaultGeneration < generation && vault?.unlocked === false) return;
//! ```
//!
//! [`Ledger`] is `requestId` (including the bare `requestId += 1` in `reset()` and `hide()`, which
//! abandons whatever is in flight without asking for anything new), and [`Stale::VaultGeneration`]
//! is the generation test - with both of its asymmetries kept rather than quietly improved on:
//! only an answer *behind* the live generation is dropped, and only while the vault is **locked**.
//! See [`post_lock`] for why that difference is the right one, and
//! [`a_late_answer_that_arrived_after_an_unlock_is_still_applied`] for the test that keeps it.
//!
//! Reading the live values here rather than from a cached event (as the webview does) is the one
//! improvement: [`AppState`] is the thing that actually changed, so it is what gets asked.
//!
//! ## Why the value is behind a type
//!
//! [`Landing::accept`] consumes the answer and *moves* the value out only when the guards agree. A
//! refused answer's payload is dropped inside the call, and [`Stale`] carries no part of it. That
//! is the whole point of the type: "the view checked first" is not a convention a screen can skip,
//! and a refused value cannot be printed, copied into a row, or resurrected after a lock by
//! anything downstream - the caller simply no longer has it.
//!
//! Dropping is not zeroising. This module guarantees a refused value never reaches a window; the
//! vault's own handling of decrypted material stays where it always was, in `vault.rs`.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::models::Kind;
use crate::state::AppState;

/// The window's record of which request is the newest one - `requestId` in the palette.
///
/// One counter per stream (the search box has one, the inspector has another), shared between
/// whoever asks and whoever applies the answer, which is why it is behind an `Arc` rather than a
/// field of a view.
#[derive(Debug, Default)]
pub struct Ledger {
    newest: AtomicU64,
}

impl Ledger {
    pub fn new() -> Ledger {
        Ledger::default()
    }

    /// `++requestId`: the number for the next request, and the reason every earlier answer -
    /// landed, running or queued - becomes history.
    pub fn issue(&self) -> u64 {
        self.newest.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// The bare `requestId += 1` the palette makes when the user clears the query or dismisses it:
    /// nothing new is being asked for, only the screen that would have shown the answer is gone.
    /// Returns the number now considered current, which no in-flight answer can equal.
    pub fn abandon(&self) -> u64 {
        self.issue()
    }

    /// The newest request id. An answer carrying anything else is history.
    pub fn newest(&self) -> u64 {
        self.newest.load(Ordering::SeqCst)
    }

    pub fn is_current(&self, request_id: u64) -> bool {
        self.newest() == request_id
    }
}

/// What the work read, so an answer about one record can be compared with that record now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// A list or a ranking: there is no single record to compare against.
    Everything,
    /// One record, at the revision the work found it at. `None` means the work could not read it
    /// at all, so the answer is about nothing.
    Record {
        kind: Kind,
        id: String,
        revision: Option<i64>,
    },
}

/// The identity an answer is computed under.
///
/// The generation and revision are read *where the work runs*, not when it is queued: an answer can
/// only be true of the state it actually saw, and between asking and running a lock can happen (see
/// the desktop shell's `off_thread` module for the path that gets a request running late).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestTag {
    pub request_id: u64,
    /// `VaultService::generation()` at the moment the work read the store. The same number
    /// [`crate::models::SearchResponse`] has carried over the wire since the palette existed.
    pub vault_generation: u64,
    pub scope: Scope,
}

impl RequestTag {
    /// Tag a request over whatever the work reads - a ranking, a list, a scan.
    pub fn capture(state: &AppState, request_id: u64) -> RequestTag {
        RequestTag {
            request_id,
            vault_generation: state.vault.generation(),
            scope: Scope::Everything,
        }
    }

    /// Tag a request about one record, recording the revision the work found it at. A record that
    /// cannot be read is not an error here: the answer is about nothing, and [`Stale::Gone`] says
    /// so when it is applied.
    pub fn capture_record(state: &AppState, request_id: u64, kind: Kind, id: &str) -> RequestTag {
        RequestTag {
            request_id,
            vault_generation: state.vault.generation(),
            scope: Scope::Record {
                kind,
                id: id.to_string(),
                revision: current_revision(state, kind, id),
            },
        }
    }

    /// The decision. Order matters, and it is the order of the shipped palette: a newer request
    /// wins first, then the vault, then the record.
    ///
    /// The vault comes before the record because after a lock a secret cannot be read at all, and
    /// reporting that as [`Stale::Gone`] would be a lie - the record is where the user left it, the
    /// *vault* closed.
    fn judge(&self, ledger: &Ledger, state: &AppState) -> Result<(), Stale> {
        if !ledger.is_current(self.request_id) {
            return Err(Stale::Obsolete {
                issued: self.request_id,
                newest: ledger.newest(),
            });
        }
        // Two cheap atomic reads, deliberately not `VaultService::state()`: that one asks the
        // keyring whether it is available, which this decision does not need and a background
        // answer must not pay for.
        let generation = state.vault.generation();
        if post_lock(
            self.vault_generation,
            generation,
            !state.vault.is_unlocked(),
        ) {
            return Err(Stale::VaultGeneration {
                then: self.vault_generation,
                now: generation,
            });
        }
        if let Scope::Record { kind, id, revision } = &self.scope {
            let Some(then) = revision else {
                return Err(Stale::Gone { id: id.clone() });
            };
            match current_revision(state, *kind, id) {
                None => return Err(Stale::Gone { id: id.clone() }),
                Some(now) if now != *then => {
                    return Err(Stale::RevisionMoved {
                        id: id.clone(),
                        then: *then,
                        now,
                    })
                }
                Some(_) => {}
            }
        }
        Ok(())
    }
}

/// Why an answer may no longer be applied. Never carries the answer: a refused payload is dropped,
/// and a log line about staleness must not be able to print what the window was denied (AGENTS.md:
/// no secret values in logs). `id` is an entry id - the same identifier every `entries-changed`
/// event already carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stale {
    /// A newer request, or a clear, made this answer history. `id !== requestId` in the palette.
    Obsolete { issued: u64, newest: u64 },
    /// The vault closed (or was replaced) while the work ran.
    VaultGeneration { then: u64, now: u64 },
    /// Somebody saved the record this answer was about.
    RevisionMoved { id: String, then: i64, now: i64 },
    /// The record is gone, or the work never read it.
    Gone { id: String },
}

impl fmt::Display for Stale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stale::Obsolete { issued, newest } => write!(
                f,
                "request {issued} is history; request {newest} is the current one"
            ),
            Stale::VaultGeneration { then, now } => write!(
                f,
                "the vault changed while this was computed (generation {then} then, {now} now)"
            ),
            Stale::RevisionMoved { id, then, now } => {
                write!(
                    f,
                    "{id} was saved while this was computed (r{then} then, r{now} now)"
                )
            }
            Stale::Gone { id } => write!(f, "{id} no longer exists to be applied to"),
        }
    }
}

/// The rule the palette has applied since before this crate existed, kept asymmetry and all:
///
/// ```text
/// if (response.vaultGeneration < generation && vault?.unlocked === false) return;
/// ```
///
/// * only a response *behind* the live generation counts. Work cannot read a future generation, so
///   the other direction can only mean a bug somewhere else, and refusing it here would hide that
///   bug behind a plausible-looking staleness message;
/// * and only while the vault is **locked**. A list computed just before a library replacement is
///   merely old, and the rows are metadata only - [`AppState::searchable_metadata`](crate::state::AppState::searchable_metadata)
///   contributes no secret rows while the vault is closed. After a *lock*, the same list would
///   resurrect rows for secrets that can no longer be read at all. That is the one case the shipped
///   app treats as unsafe, so this is the one case the port refuses; the difference is not
///   accidental strictness and must not be quietly widened (or narrowed) here.
fn post_lock(then: u64, now: u64, locked: bool) -> bool {
    now > then && locked
}

/// The revision the stores hold for one entry right now, through the same two readers the
/// privileged calls use - [`Repository::get_command`](crate::repository::Repository::get_command)
/// and [`VaultService::get`](crate::vault::VaultService::get) - so this guard can never disagree
/// with the `Conflict` the write path raises. A locked vault cannot answer for a secret, which
/// reads as "no revision"; the generation check has already run by then.
pub fn current_revision(state: &AppState, kind: Kind, id: &str) -> Option<i64> {
    match kind {
        Kind::Command => state
            .repo
            .get_command(id)
            .ok()
            .flatten()
            .map(|(record, _, _)| record.revision),
        Kind::Secret => state.vault.get(id).ok().map(|record| record.revision),
    }
}

/// An answer from the background, plus the identity it was computed under.
///
/// The value is only reachable through [`Landing::accept`], which moves it out when the guards
/// agree and drops it when they do not. So "the view checked whether this was still current" is a
/// property of the type rather than a habit: there is no way to read the value that skips the
/// check, and after a refusal the caller has nothing left to paint.
pub struct Landing<T> {
    tag: RequestTag,
    value: T,
}

impl<T> Landing<T> {
    pub fn new(tag: RequestTag, value: T) -> Landing<T> {
        Landing { tag, value }
    }

    pub fn tag(&self) -> &RequestTag {
        &self.tag
    }

    /// The value, if it is still about the screen that is asking. Consumes the landing either way:
    /// a refused payload is dropped in here, and the reason carries no part of it.
    ///
    /// A failure is an answer too - `Landing<AppResult<T>>` gates the error the same way, which is
    /// how the palette's "only the newest failure may clear the list" branch works
    /// (`if (id === requestId) { error = ...; results = [] }`).
    pub fn accept(self, ledger: &Ledger, state: &AppState) -> Result<T, Stale> {
        match self.tag.judge(ledger, state) {
            // `self.value` is dropped on this path and cannot be recovered: `Stale` is built from
            // the tag and the live counters only.
            Err(stale) => Err(stale),
            Ok(()) => Ok(self.value),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Landing<T> {
    /// Debug prints the value, so this is the one place a `Landing` can leak what it carries. The
    /// type is deliberately not `Clone` and never leaves the thread that received it, so the only
    /// way to print it is to have the value in hand already - which is what a window would have
    /// done with it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Landing")
            .field("tag", &self.tag)
            .field("value", &self.value)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::freshness::{Landing, Ledger, RequestTag, Scope, Stale};
    use crate::models::{EntryDraft, Kind};
    use crate::testing::Fixture;

    /// A draft the editor would hand over. `id: None` means "new entry".
    fn secret_draft(name: &str, value: &str) -> EntryDraft {
        EntryDraft {
            kind: Kind::Secret,
            name: name.into(),
            entry_type: "Token".into(),
            value: value.into(),
            ..Default::default()
        }
    }

    #[test]
    fn an_answer_for_a_query_the_user_replaced_is_not_applied() {
        // The palette's `if (id !== requestId) return;`, on a search stream.
        let fixture = Fixture::new("obsolete");
        let state = fixture.state();
        let ledger = Ledger::new();
        fixture.seed_command("cmd-old", "Shell", "git status", &[]);

        let first = ledger.issue();
        let second = ledger.issue();

        // The slow answer for the first keystroke arrives after the second one has been applied.
        let late = RequestTag::capture(state, first);
        let rows = state.searchable_metadata(None);
        let landing = Landing::new(late, rows);
        let refused = landing
            .accept(&ledger, state)
            .expect_err("an answer for a replaced request must not be applied");
        assert_eq!(
            Some(Stale::Obsolete {
                issued: first,
                newest: second
            }),
            Some(refused.clone()),
            "{refused:?}"
        );

        // The current request's own answer still goes through, so the guard is not a blanket "no".
        let fresh = Landing::new(
            RequestTag::capture(state, second),
            state.searchable_metadata(None),
        );
        assert!(fresh.accept(&ledger, state).is_ok());
    }

    #[test]
    fn clearing_the_query_invalidates_an_answer_that_never_had_a_successor() {
        // `reset()` and `hide()` do `requestId += 1` with no request after it. The answer that
        // arrives for the abandoned one is for a window that is gone.
        let fixture = Fixture::new("abandon");
        let state = fixture.state();
        let ledger = Ledger::new();
        fixture.seed_command("cmd-1", "Shell", "echo hi", &[]);

        let issued = ledger.issue();
        let now = ledger.abandon();
        assert_eq!(issued + 1, now, "abandoning bumps the same counter");

        let landing = Landing::new(
            RequestTag::capture(state, issued),
            state.searchable_metadata(None),
        );
        assert_eq!(
            Some(Stale::Obsolete {
                issued,
                newest: now
            }),
            landing.accept(&ledger, state).err()
        );
    }

    #[test]
    fn a_response_from_before_a_manual_lock_cannot_restore_the_secret_rows() {
        // C4's third guard, end to end against a real vault: rows are read while the secret store
        // is open, the user locks it, and the late answer must not put those rows back on screen.
        let fixture = Fixture::new("post-lock");
        let state = fixture.state();
        let ledger = Ledger::new();
        let saved = fixture
            .state()
            .vault
            .save(&mut secret_draft(
                "deploy-token",
                "synthetic-secret-value-not-a-real-one",
            ))
            .expect("seed secret");

        let issued = ledger.issue();
        // The work runs now, unlocked, and its answer really does carry the secret's row.
        let tag = RequestTag::capture(state, issued);
        let rows = state.searchable_metadata(None);
        assert!(
            rows.iter().any(|row| row.id == saved.id),
            "the fixture should be readable while unlocked: {:?}",
            rows.iter().map(|r| &r.name).collect::<Vec<_>>()
        );

        // The user locks the vault. Everything decrypted is gone, and the generation moved.
        let before = state.vault.generation();
        state.vault.lock().expect("lock");
        assert!(state.vault.generation() > before);
        assert!(
            state
                .searchable_metadata(None)
                .iter()
                .all(|row| row.kind != Kind::Secret),
            "the vault must not be contributing secret rows any more"
        );

        let landing = Landing::new(tag, rows);
        let refused = landing
            .accept(&ledger, state)
            .expect_err("a pre-lock answer must not restore secret rows");
        match refused {
            Stale::VaultGeneration { then, now } => {
                assert_eq!(then, before);
                assert_eq!(now, state.vault.generation());
            }
            other => panic!("expected the generation guard, got {other:?}"),
        }
    }

    #[test]
    fn a_refused_answer_loses_its_payload_without_naming_it() {
        // The reason is what a log line and any status text get, so it must not be able to carry
        // the value the window was refused (AGENTS.md: never log secret values). Synthetic value.
        let fixture = Fixture::new("redact");
        let state = fixture.state();
        let ledger = Ledger::new();
        let secret = "synthetic-token-do-not-print-8f2b";
        let saved = fixture
            .state()
            .vault
            .save(&mut secret_draft("api-token", secret))
            .expect("seed secret");

        let issued = ledger.issue();
        let tag = RequestTag::capture_record(state, issued, Kind::Secret, &saved.id);
        assert_eq!(
            tag.request_id, issued,
            "the record tag keeps the request it was issued for"
        );
        let value = state.vault.get(&saved.id).expect("readable").value;
        assert_eq!(value, secret);

        state.vault.lock().expect("lock");
        let refused = Landing::new(tag, value)
            .accept(&ledger, state)
            .expect_err("the post-lock answer is refused");

        let rendered = format!("{refused:?} {refused}");
        assert!(
            !rendered.contains(secret),
            "the staleness reason leaked what it refused: {rendered}"
        );
        assert!(!rendered.contains("api-token"), "not even the name");
    }

    #[test]
    fn a_late_answer_that_arrived_after_an_unlock_is_still_applied() {
        // The shipped rule's asymmetry, pinned so nobody "tightens" it by accident: a response
        // behind the live generation is only discarded while the vault is *locked*. Between the
        // work and the answer the vault can go lock -> unlock (two bumps) and stay readable, and
        // then the rows are merely old, not secret-bearing. Refusing them here would be a
        // behaviour change over the app that is shipping.
        let fixture = Fixture::new("asymmetric");
        let state = fixture.state();
        let ledger = Ledger::new();
        fixture.seed_command("cmd-a", "Shell", "cargo test", &[]);

        let issued = ledger.issue();
        let tag = RequestTag::capture(state, issued);
        let rows = state.searchable_metadata(None);
        assert!(!rows.is_empty());

        // Generation moves twice (lock, then unlock) and the vault ends up open. The
        // authenticator is injected because the polkit helper this app installs is not present in
        // a container - the same seam `vault.rs`'s own tests use.
        state.vault.lock().expect("lock");
        state
            .vault
            .unlock_with(|| Ok(()))
            .expect("unlock with an injected authenticator");
        assert!(
            state.vault.generation() > tag.vault_generation,
            "the generation should have moved on"
        );
        assert!(state.vault.is_unlocked());

        let landing = Landing::new(tag, rows);
        assert!(
            landing.accept(&ledger, state).is_ok(),
            "an answer the shipped app would still show must not be refused here"
        );
    }

    #[test]
    fn a_draft_loaded_before_someone_else_saved_is_not_painted_over_their_record() {
        // C4's second guard. A command edit leaves the vault generation alone, so the revision is
        // the only thing that can catch this one - which is why the guard reads the record and not
        // just the vault.
        let fixture = Fixture::new("revision");
        let state = fixture.state();
        let ledger = Ledger::new();
        fixture.seed_command("cmd-edit", "Shell", "git push", &[]);

        let issued = ledger.issue();
        // The background load reads the record as it is: revision 1, `git push`.
        let tag = RequestTag::capture_record(state, issued, Kind::Command, "cmd-edit");
        assert!(
            matches!(
                &tag.scope,
                Scope::Record {
                    revision: Some(1),
                    ..
                }
            ),
            "the fixture should start at revision 1, got {:?}",
            tag.scope
        );
        let loaded = state
            .repo
            .get_command("cmd-edit")
            .expect("read")
            .expect("present")
            .0
            .command;
        assert_eq!(loaded, "git push");

        // Somebody else saves through the ordinary path, which is the thing that bumps a revision.
        let mut draft = fixture.draft("cmd-edit", "git push --force-with-lease");
        draft.id = Some("cmd-edit".into());
        draft.expected_revision = 1;
        let saved = state.repo.save_command(&mut draft).expect("other save");
        assert_eq!(saved.revision, 2);

        let refused = Landing::new(tag, loaded)
            .accept(&ledger, state)
            .expect_err("a record that moved on cannot be painted back");
        assert_eq!(
            Some(Stale::RevisionMoved {
                id: "cmd-edit".into(),
                then: 1,
                now: 2
            }),
            Some(refused.clone()),
            "{refused:?}"
        );
        // And the store still holds the other person's save: refusing did not write anything.
        assert_eq!(
            state
                .repo
                .get_command("cmd-edit")
                .expect("read")
                .expect("present")
                .0
                .command,
            "git push --force-with-lease"
        );

        // Both halves of the app answer the same way: the write path's own check refuses the stale
        // revision with `Conflict`, exactly as the read path just refused it with `RevisionMoved`.
        assert!(
            matches!(
                state.repo.delete_command("cmd-edit", 1),
                Err(crate::AppError::Conflict)
            ),
            "the write path must refuse the same stale revision"
        );
    }

    #[test]
    fn a_record_that_vanished_while_the_work_ran_is_reported_as_gone() {
        let fixture = Fixture::new("gone");
        let state = fixture.state();
        let ledger = Ledger::new();
        fixture.seed_command("cmd-doomed", "Shell", "echo bye", &[]);

        let issued = ledger.issue();
        let tag = RequestTag::capture_record(state, issued, Kind::Command, "cmd-doomed");
        state.repo.delete_command("cmd-doomed", 1).expect("delete");

        let refused = Landing::new(tag, "echo bye".to_string())
            .accept(&ledger, state)
            .expect_err("there is no record to apply it to");
        assert_eq!(
            Some(Stale::Gone {
                id: "cmd-doomed".into()
            }),
            Some(refused.clone()),
            "{refused:?}"
        );
    }

    #[test]
    fn the_vault_is_judged_before_the_record_so_a_post_lock_answer_says_the_truth() {
        // A locked vault cannot answer for a secret, so `current_revision` is `None` - and reading
        // that as "the record is gone" would blame the wrong thing. Order is: newer request, then
        // vault, then record.
        let fixture = Fixture::new("order");
        let state = fixture.state();
        let ledger = Ledger::new();
        let saved = fixture
            .state()
            .vault
            .save(&mut secret_draft("ssh-key", "synthetic-private-material"))
            .expect("seed secret");

        let issued = ledger.issue();
        let tag = RequestTag::capture_record(state, issued, Kind::Secret, &saved.id);
        state.vault.lock().expect("lock");

        let refused = Landing::new(tag, "value".to_string())
            .accept(&ledger, state)
            .expect_err("post-lock");
        assert!(
            matches!(refused, Stale::VaultGeneration { .. }),
            "the honest reason is the vault, not the record: {refused:?}"
        );

        // And when the request is *also* obsolete, the newer request still comes first.
        let tag = RequestTag::capture_record(state, ledger.issue(), Kind::Secret, &saved.id);
        ledger.abandon();
        let refused = Landing::new(tag, "value".to_string())
            .accept(&ledger, state)
            .expect_err("obsolete");
        assert!(matches!(refused, Stale::Obsolete { .. }), "{refused:?}");
    }

    #[test]
    fn a_scope_that_never_read_the_record_is_refused_rather_than_trusted() {
        // `capture_record` on an id that does not exist records `revision: None`. Such an answer is
        // about nothing, so it must not be applied just because nothing has moved on since.
        let fixture = Fixture::new("unreadable");
        let state = fixture.state();
        let ledger = Ledger::new();
        let issued = ledger.issue();
        let tag = RequestTag::capture_record(state, issued, Kind::Command, "never-existed");
        assert!(
            matches!(&tag.scope, Scope::Record { revision: None, .. }),
            "{:?}",
            tag.scope
        );
        // A tag that cannot name a revision is refused even though the vault never moved.
        let refused = Landing::new(tag, "anything".to_string())
            .accept(&ledger, state)
            .expect_err("nothing was read, so nothing can be applied");
        assert_eq!(
            Some(Stale::Gone {
                id: "never-existed".into()
            }),
            Some(refused),
        );
    }

    #[test]
    fn an_error_is_an_answer_and_the_newest_one_only_may_clear_the_list() {
        // `Landing<AppResult<..>>`: the palette gates its failure branch on the same id check
        // (`if (id === requestId) { error = ...; results = [] }`), so a superseded failure cannot
        // wipe a list a newer request already painted.
        let fixture = Fixture::new("error-path");
        let state = fixture.state();
        let ledger = Ledger::new();
        fixture.seed_command("cmd-keep", "Shell", "git status", &[]);

        let old = ledger.issue();
        let current = ledger.issue();
        let rows = state.searchable_metadata(None);

        // The current request succeeds first, then the old one comes back failing.
        let failure: crate::AppResult<Vec<crate::models::EntryMeta>> =
            Err(crate::AppError::Other("scan failed".into()));
        let landing = Landing::new(RequestTag::capture(state, old), failure);
        assert!(
            landing.accept(&ledger, state).is_err(),
            "the old request is obsolete"
        );
        let kept: Landing<crate::AppResult<Vec<crate::models::EntryMeta>>> =
            Landing::new(RequestTag::capture(state, current), Ok(rows));
        let applied = kept
            .accept(&ledger, state)
            .expect("the current request applies")
            .expect("and its rows are there");
        assert!(!applied.is_empty());
    }
}
