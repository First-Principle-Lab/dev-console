//! Explicit UI state and the first view functions for the two windows.
//!
//! Plan §2 asks for each screen to be "a Rust view function that produces Rune elements from
//! explicit UI state". These are the first cut of that shape for the Library and the Finder, and
//! they are deliberately thin: they establish the state/element split, the element keys the
//! scripted tests in `dev-console-ui-tests` address, the token-only colour rule, and where real
//! data will come from.
//!
//! ## What is not here yet, and where it is specified
//!
//! Plan milestone 4 ("Port all feature screens") fills these in, with data from the services that
//! milestone 2 moves into `dev-console-core`: the repository and search ranking behind
//! [`Library::entries`], the vault state behind the header, the project scanners behind
//! [`Finder::context`], and the ports of the draft validation, argument parsing, formatting and
//! Finder selection logic now living in `src/lib/*.ts`.
//!
//! ## Which of these this container can decide
//!
//! The Finder's keyboard routing is decidable here and is therefore built here: [`Finder::route_key`]
//! runs on keys the runtime injects with `Runtime::key`, the query field takes focus from a scripted
//! press, and `dev-console-ui-tests/tests/finder_keys.rs` asserts the resulting state transitions.
//! What that harness still cannot decide is delivery: whether the compositor hands the global
//! shortcut to the window at all, and whether the focus ring or the caret *looks* right - both stay
//! UNVERIFIED (owner desktop) in the native-feasibility ledger.
//!
//! Copying is decidable here too, which is what [`Finder::activate`] and [`crate::copy`] exist for:
//! the value is resolved by id behind a [`copy::CopySource`] trait and written through Rune's own
//! secret-copy path, so no field of any state in this module can hold one, and the scripted tests in
//! `tests/finder_copy.rs` decide the copy, the failure message and the hygiene rules that go with it.
//! What is *not* built is the other side of that seam: the only [`CopySource`] in the shell today is
//! [`copy::Unwired`], so a real binary resolves no entry and shows the shipped failure line until
//! milestone 2 hands the seam a `dev-console-core` state.
//!
//! Still unbuilt, and not faked: `Space` copy (see below), `Ctrl+O` details and `Ctrl+P`
//! ignore-context (need the details panel), the Library's `Ctrl+N`/`Ctrl+S` (need an editor), the
//! command-output panel and the SVG icon set. Two things have come off that list since it was
//! written: destructive confirmation, because [`Library::request_delete`] opens [`crate::dialog`]'s
//! shared shell (which owns the focus trap, the initial focus and the restore), and the Library's
//! `Ctrl+1`/`Ctrl+2` tab chords, routed by [`Library::route_key`] off the same `AppSpec` the real
//! window is built from. `Space` is deliberately left unbound rather than bound imperfectly:
//! the shipped handler binds it beside `Enter` behind `shouldCommit` (`src/lib/finder-state.ts`), and
//! Rune's IME composition flag is `pub(super)`, so a handler cannot honour that guard - `route_key`
//! holds `Enter`/`Escape` back mid-composition but not `Space`, which a composed space would have to
//! survive.
//!
//! ## Colour rule
//!
//! Every colour comes from a token set - `rune::tok::theme` here, and spacing/radius/type from
//! `space`/`radius`/`type_` - never from a literal written in this file. The gate is a test, not a
//! promise: `crate::tests::no_view_source_hard_codes_a_colour` greps every source in this crate and
//! in `dev-console-ui-tests` for a hex triplet or an `rgb`/`hsl` call.
//!
//! Dev Console's real palette - the 52 shared tokens and the 24 palettes in `src/styles/tokens.css`
//! and `src/styles/omarchy.css`, with the same 76 contrast pairings the shipped audit measures - is
//! ported into `dev_console_core::theme` (checkpoint C5). What is **not** done yet is the last hop:
//! handing that palette to Rune, whose `tok::theme` still returns its own built-in default. So these
//! views use `rune::tok::theme` to stay token-driven while the palette mapping is a milestone-4
//! change, and the ported data is what that change will map from - not a second colour source.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use crate::copy::{self, CopyOutcome, CopySource, Unwired};
use crate::dialog::{self, Dialog};
use dev_console_core::models::{CopyRequest, Kind};
use rune::tok::{radius, space, type_};
use rune::{col, el, row, text, text_input, Chord, Cx, El, Key, KeyEvent, TextEdit, Tx};

/// Which list the Library is showing. Mirrors the Ctrl+1 / Ctrl+2 tabs of the current app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// The Commands tab, shown by default.
    #[default]
    Commands,
    /// The Secrets tab. Its rows are metadata only: values are revealed through a separate action.
    Secrets,
}

impl Tab {
    /// The tab's label as the footer and the tabs row show it.
    pub const fn label(self) -> &'static str {
        match self {
            Tab::Commands => "Commands",
            Tab::Secrets => "Secrets",
        }
    }

    /// The same choice in the word a confirmation sentence uses - `the saved command` /
    /// `the saved secret`, matching `{pendingDelete.kind}` in the shipped delete dialog.
    pub const fn kind(self) -> &'static str {
        match self {
            Tab::Commands => "command",
            Tab::Secrets => "secret",
        }
    }
}

/// The record a delete confirmation is waiting on.
///
/// It holds the *name* and the kind, not a row index, and that is the whole point: the shipped
/// app keeps `pendingDelete` as the entry it was asked about, so a confirmation that named one
/// record removes that record even if the list has been reordered or shortened underneath it.
/// An index would be a deletion bug waiting for the first asynchronous reload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDelete {
    /// The row's display name - metadata only, never a secret value.
    pub name: String,
    /// `command` or `secret`, from the tab the row was on.
    pub kind: &'static str,
}

/// The Library window's state.
#[derive(Debug, Clone, Default)]
pub struct Library {
    /// Commands or Secrets.
    pub tab: Tab,
    /// Placeholder for the ranked, grouped rows `src/lib` builds from the repository; empty means
    /// the empty state is shown. Synthetic strings only - never entry content from a real profile.
    pub entries: Vec<String>,
    /// True when the vault is unlocked. Only the *status* crosses into the UI; the key and any
    /// secret value never reach this struct.
    pub vault_unlocked: bool,
    /// The record a delete confirmation is open on, `None` while no dialog is showing. The
    /// shipped app holds the same thing as `pendingDelete`, and it is state rather than a view
    /// detail for the same reason: what the confirmation *names* has to be decided when the press
    /// lands, not when the panel is drawn.
    pub pending_delete: Option<PendingDelete>,
}

/// The two tab chords the Library window owns, in the order the shipped handler tests them:
/// `Ctrl+1` for Commands, `Ctrl+2` for Secrets.
///
/// Parsed once and matched with [`Chord`] rather than compared by hand, because the toolkit's own
/// matcher knows two things a hand-written `mods.ctrl && key == Command('1')` comparison quietly
/// drops: off macOS a window's Ctrl chord arrives as `Key::Command` with `cmd` *and* `ctrl` set
/// (`window/keys.rs` maps the control key onto the primary modifier), and a chord on a non-Latin
/// layout's key falls back to the US key in that place. The digit itself is the same either way, so
/// this is the difference between a binding that works on the owner's keyboard and one that only
/// works in a test.
static TAB_CHORDS: [(LazyLock<Chord>, Tab); 2] = [
    (
        LazyLock::new(|| Chord::parse("ctrl+1").expect("`ctrl+1` is a valid chord")),
        Tab::Commands,
    ),
    (
        LazyLock::new(|| Chord::parse("ctrl+2").expect("`ctrl+2` is a valid chord")),
        Tab::Secrets,
    ),
];

/// The footer's promise about those two chords - stated once, next to them, and checked against
/// them by `every_library_footer_chip_names_a_binding_the_window_routes`, which parses the digits
/// out of this string and drives each one. A footer that advertises a binding the shell does not
/// route is worse than no footer at all, and the Finder already has a test holding that line.
pub const TAB_HINT: &str = "Ctrl+1 / Ctrl+2 to switch tabs";

impl Library {
    /// A Library showing `entries` on `tab`, for tests and for the headless scenes.
    pub fn with_entries(tab: Tab, entries: &[&str]) -> Library {
        Library {
            tab,
            entries: entries.iter().map(|entry| (*entry).to_owned()).collect(),
            vault_unlocked: true,
            pending_delete: None,
        }
    }

    /// Asks whether to remove row `index` - the shipped row's `onDelete` hop, and the only way a
    /// deletion starts. Names the record it found at `index` rather than remembering `index`, so
    /// [`Library::confirm_delete`] cannot remove a different row if the list has moved. False when
    /// there is no such row, which leaves no dialog open to cancel.
    pub fn request_delete(&mut self, index: usize) -> bool {
        let Some(entry) = self.entries.get(index) else {
            return false;
        };
        self.pending_delete = Some(PendingDelete {
            name: entry.clone(),
            kind: self.tab.kind(),
        });
        true
    }

    /// Goes ahead with the confirmation: removes the named record and closes the dialog. A record
    /// that is no longer in the list is simply not removed - the dialog says which one it meant,
    /// and it is that one or nothing.
    pub fn confirm_delete(&mut self) {
        let Some(pending) = self.pending_delete.take() else {
            return;
        };
        if let Some(position) = self.entries.iter().position(|entry| *entry == pending.name) {
            self.entries.remove(position);
        }
    }

    /// Backs out of the confirmation - the Cancel button, Escape, and a press off the panel all
    /// arrive here, exactly as the shipped `oncancel` serves all three. Nothing is removed.
    pub fn cancel_delete(&mut self) {
        self.pending_delete = None;
    }

    /// The confirmation to draw, built from the pending record. `None` means no dialog, and the
    /// view function turns on it.
    pub fn delete_dialog(&self) -> Option<Dialog> {
        self.pending_delete
            .as_ref()
            .map(|pending| Dialog::delete(&pending.name, pending.kind))
    }

    /// Shows `tab`: the port of `selectTab` in `src/windows/Library.svelte`. False when that tab is
    /// already showing, because the shipped handler's first line is `if (tab === next) return` and
    /// the rest of it (clearing `selectedId`, dropping `details`, reloading the list) has no native
    /// twin until the shell has a selection, a details pane and a repository behind the list.
    pub fn select_tab(&mut self, tab: Tab) -> bool {
        if self.tab == tab {
            return false;
        }
        self.tab = tab;
        true
    }

    /// The Library window's app-level key router: the native mirror of
    /// `<svelte:window onkeydown={onGlobalKeydown}>`, reached through [`rune::AppSpec::on_key`] which
    /// [`crate::app::library_spec`] sets, so the window and the scripted tests route the same table
    /// rather than two lookalikes.
    ///
    /// Rune consults this for every key the focused control leaves alone, and first of all when
    /// nothing has focus (`runtime/input/keys.rs`), which is the shape of the shipped window handler:
    /// the chords work from the list, from a focused tab or row, and from an empty window alike.
    ///
    /// Of the shipped handler's five branches only the two tab chords are here. `Ctrl+N` (new entry)
    /// and `Ctrl+S` (save the open edit) wait on an editor the shell has not built; the two `Escape`
    /// branches need no twin, because [`crate::dialog`]'s modal dismisses itself on Escape - that is
    /// the whole of `Escape && pendingDelete`, and there is no `pendingRun` yet.
    ///
    /// A matched chord is consumed whether or not it moved the tab, as the shipped handler calls
    /// `preventDefault()` before `selectTab`'s "already showing" return: the Library owns that key
    /// either way, and reporting it unhandled would only invite something else to act on it.
    pub fn route_key(&mut self, ev: &KeyEvent, _tx: &mut Tx) -> bool {
        TAB_CHORDS
            .iter()
            .find(|(chord, _)| chord.matches(ev))
            .is_some_and(|(_, tab)| {
                self.select_tab(*tab);
                true
            })
    }
}

/// One row of the palette's result list.
///
/// A candidate is *metadata*: the entry's id, the name the row prints, and which kind it is. That is
/// the whole shape, and it is the point - the shipped palette receives exactly this from
/// `api.search()` (`SearchResult`), and the value behind a `Secret` candidate reaches the clipboard
/// only through [`Finder::activate`]'s call to the [`CopySource`], never through a field of a state
/// struct a dump could print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The entry id the copy request is made for (`api.copy(kind, id, 0)`). Metadata, not a secret.
    pub id: String,
    /// The name the row prints, and the text Tab writes into the field (`tabCompletion`'s input).
    pub name: String,
    /// Command or secret: it decides the copy's wording and which backend path resolves the value.
    pub kind: Kind,
}

impl Candidate {
    /// A command candidate whose id is `id`.
    pub fn command(id: impl Into<String>, name: impl Into<String>) -> Self {
        Candidate {
            id: id.into(),
            name: name.into(),
            kind: Kind::Command,
        }
    }

    /// A secret candidate: `name` is all the window ever sees of it.
    pub fn secret(id: impl Into<String>, name: impl Into<String>) -> Self {
        Candidate {
            id: id.into(),
            name: name.into(),
            kind: Kind::Secret,
        }
    }
}

/// A bare name is a command candidate whose id is that name - the shorthand the placeholder lists and
/// the scripted fixtures use, so `vec!["git status".into()]` still reads as one row.
impl From<&str> for Candidate {
    fn from(name: &str) -> Self {
        Candidate::command(name, name)
    }
}

/// The Finder window's state.
pub struct Finder {
    /// The query field's editable text.
    pub query: TextEdit,
    /// Completion candidates, best first. Placeholder for the ported Finder selection logic; the
    /// ranking rules live in `src/lib/search-ranking.ts` until that moves.
    pub rows: Vec<Candidate>,
    /// Index of the highlighted row.
    pub selected: usize,
    /// The project folder the Finder detected from the focused window, shown as a chip. Empty
    /// means no context was detected. A path only - never file contents.
    pub context: String,
    /// Set when the palette asked to be dismissed. Explicit state rather than a window call: the
    /// shell that hides the window reads it (the shipped palette's `api.hideSelf()` hop), so the
    /// decision "Escape clears the palette and takes it out of the way" is testable here while the
    /// unmapping itself stays the owner-desktop check it is.
    pub dismissed: bool,
    /// What the last copy attempt left to say, the mirror of the shipped `status`/`error` pair.
    /// [`CopyOutcome`] is an enum of fixed strings, so there is no field here a value could be
    /// written into - which is the whole of the shipped app's "the value itself never appears here"
    /// comment turned into a type. Cleared by the next copy; the shipped palette clears it when a new
    /// search starts, which is the seam that does not exist yet.
    pub copy_status: Option<CopyOutcome>,
    /// How long a copied value may stay on the clipboard before the shell clears it, if it still owns
    /// it: the native form of `clipboardExpirySeconds` ([`copy::default_expiry`] reads the same
    /// default the shipped app reads).
    pub copy_expiry: Duration,
    /// Where a highlighted row's value comes from. A `dyn` pointer, deliberately: the resolver is the
    /// only thing in this struct that can reach a value, and it reaches it *outwards* - straight to
    /// the clipboard - so no state field, `Debug` line or element dump can carry one.
    pub source: Arc<dyn CopySource>,
}

impl Default for Finder {
    fn default() -> Self {
        Finder {
            query: TextEdit::new(String::new()),
            rows: Vec::new(),
            selected: 0,
            context: String::new(),
            dismissed: false,
            copy_status: None,
            copy_expiry: copy::default_expiry(),
            source: Arc::new(Unwired),
        }
    }
}

/// Hand-written rather than derived, and the difference matters: the derived form would print
/// `source`, whose `Debug` is this crate's own and prints the resolver's *identity* only - so the one
/// path a value could take out of this struct in a log line or a `assert_eq!` diff is closed by what
/// the impl says rather than by what it forgot to skip.
impl std::fmt::Debug for Finder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Finder")
            .field("query", &self.query.text)
            .field("rows", &self.rows)
            .field("selected", &self.selected)
            .field("context", &self.context)
            .field("dismissed", &self.dismissed)
            .field("copy_status", &self.copy_status)
            .field("copy_expiry", &self.copy_expiry)
            .field("source", &"dyn CopySource")
            .finish()
    }
}

impl Finder {
    /// A Finder showing `rows` as command candidates, for tests and the headless snapshot scenes.
    pub fn with_rows(rows: &[&str]) -> Self {
        Finder {
            rows: rows.iter().map(|row| Candidate::from(*row)).collect(),
            ..Finder::default()
        }
    }

    /// A Finder showing exact candidates, so a fixture can name the id and kind a copy will ask for.
    pub fn with_candidates(rows: &[Candidate]) -> Self {
        Finder {
            rows: rows.to_vec(),
            ..Finder::default()
        }
    }

    /// The Finder backed by `source` instead of [`Unwired`]: the seam the scripted tests put a
    /// synthetic vault behind, and the one milestone 2 will hand an `Arc<AppState>` to.
    pub fn with_copy_source(mut self, source: Arc<dyn CopySource>) -> Self {
        self.source = source;
        self
    }

    /// Highlights `index` when it names a row, and says whether anything moved. The scripted tests
    /// drive selection through this rather than through a private field, so the rule "a press on a
    /// row that is not in the list cannot select it" is one tested place.
    pub fn select(&mut self, index: usize) -> bool {
        if index < self.rows.len() && self.selected != index {
            self.selected = index;
            return true;
        }
        false
    }

    /// Moves the highlight by `delta`, the port of `nextSelection` in `src/lib/finder-state.ts`:
    /// it clamps instead of wrapping, keeps a still-valid selection, and when the selected row has
    /// fallen out of a shortened list it restarts at the end the movement was heading for. Returns
    /// whether the highlight moved.
    pub fn move_selection(&mut self, delta: isize) -> bool {
        if self.rows.is_empty() {
            return false;
        }
        let last = self.rows.len() - 1;
        if self.selected > last {
            self.selected = if delta >= 0 { 0 } else { last };
            return true;
        }
        let wanted = self.selected as isize + delta;
        let next = wanted.clamp(0, last as isize) as usize;
        if next == self.selected {
            return false;
        }
        self.selected = next;
        true
    }

    /// Accepts the selected row into the field: the port of `tabCompletion` in
    /// `src/lib/finder-state.ts`, which returns the row's name for both a prefix match and a fuzzy
    /// one, and nothing at all once the field already holds that name. Writes through
    /// [`TextEdit::replace_text`], which zeroes the old draft - the query field can hold a secret's
    /// name, so a plain `String` assignment would leave the old bytes in the allocation.
    ///
    /// The shipped handler also re-searches and keeps the same row selected (`acceptCompletion`);
    /// there is no search backend behind this state yet, so only the field changes.
    pub fn complete(&mut self) -> bool {
        let Some(row) = self.rows.get(self.selected) else {
            return false;
        };
        if row.name == self.query.text {
            return false;
        }
        let name = row.name.clone();
        self.query.replace_text(name);
        true
    }

    /// Copies the highlighted row: the port of `activate()` in `src/windows/Finder.svelte`, all three
    /// of its hops.
    ///
    /// The value is asked for by *id* through [`CopySource`] and handed straight to
    /// [`Tx::copy_secret`], which zeroes the buffer and re-clears the board when the deadline passes
    /// **only if the clipboard still holds what this copy wrote** - Rune's version of the shipped
    /// `expire_due` guard ("clear the clipboard only when it still holds the value this app copied"),
    /// and the reason the write goes through the toolkit rather than a second owner beside it. A
    /// secret and a command both take `copy_secret`, because in the shipped app both go through
    /// [`dev_console_core::clipboard::Clipboard::copy`], which schedules the same expiry whichever
    /// kind it was handed.
    ///
    /// `None` means there was nothing highlighted: the shipped `activate` returns before touching
    /// `status` in that case, so no line appears, which is why the return type distinguishes it from
    /// [`CopyOutcome::Failed`]. On success this sets [`Finder::dismissed`] *without* calling
    /// [`Finder::dismiss`], because the shipped success path is `api.hideSelf()` - the palette closes
    /// as it stands - while Escape's `hide()` is the one that clears the query and the rows.
    pub fn activate(&mut self, tx: &mut Tx) -> Option<CopyOutcome> {
        let row = self.rows.get(self.selected)?;
        let request = CopyRequest {
            kind: row.kind,
            id: row.id.clone(),
            // The shipped palette passes a literal 0 here: "whatever revision the backend currently
            // holds" (see `CopyRequest`). The revision-checked variant is the Library's copy action,
            // not the palette's, and the staleness rules behind it are ported and tested in core.
            expected_revision: 0,
        };
        let outcome = match self.source.value_for(&request) {
            Some(value) => {
                tx.copy_secret(value, self.copy_expiry);
                CopyOutcome::for_kind(request.kind)
            }
            None => CopyOutcome::Failed,
        };
        self.copy_status = Some(outcome);
        if outcome.succeeded() {
            self.dismissed = true;
        }
        Some(outcome)
    }

    /// Takes the palette out of the way: the port of `hide` in `src/windows/Finder.svelte`, which
    /// clears the query, the rows and the selection and then closes the window. `clear` rather than
    /// an empty assignment because it zeroes the old draft and keeps keyboard focus on the field,
    /// which is where the next summon wants it. The request-counter half of the shipped `hide`
    /// belongs to the search port and is not here yet.
    pub fn dismiss(&mut self) {
        self.query.clear();
        self.rows.clear();
        self.selected = 0;
        self.dismissed = true;
        // The shipped `hide()` clears `error` and leaves `status` alone, and the split is deliberate:
        // "Copy failed." describes an attempt the user is entitled to be told about on the way out,
        // while a "Copied …" confirmation describes a palette state that is about to be gone. The one
        // place the shipped app clears a confirmation is at the start of the next search
        // (`runSearch`), which is the seam that does not exist here yet - hence a confirmation
        // surviving this call, and the matrix row that says so staying uncovered rather than being
        // tested against a search that isn't there.
        if matches!(self.copy_status, Some(CopyOutcome::Failed)) {
            self.copy_status = None;
        }
    }

    /// The palette's key routing, the native mirror of `<input on:keydown={onKeydown}>` in
    /// `src/windows/Finder.svelte`. It is reached through [`El::on_input_key`] on the query field -
    /// the only Rune hook that sees a key while a text field holds focus, because `El::on_key`
    /// covers controls that own their keys and `Runtime::on_key` runs only for keys the focused
    /// field left alone (`route_key` in `runtime/input/keys.rs` consults the input's own handler
    /// before the editor and the window's).
    ///
    /// Returning true is `preventDefault`: the key never reaches the editor, so an arrow does not
    /// move the caret and Escape does not blur the field. The five bindings below are the shipped ones
    /// this state can honour today; `Ctrl+O`/`Ctrl+P` fall through to the field until the details
    /// panel and the ignore-context hop exist, and the footer says the same thing this comment does
    /// rather than advertising a binding that is not there.
    ///
    /// `Space` stays unbound on purpose, unlike the shipped handler which binds it beside `Enter`
    /// behind `shouldCommit`: Rune's IME composition flag is `pub(super)` and its own guard holds
    /// `Enter`/`Escape` back mid-composition but not `Space`, so binding it would swallow a space the
    /// user is composing. `Enter` is safe for exactly that reason.
    pub fn route_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        match ev.key {
            Key::Down => {
                self.move_selection(1);
                true
            }
            Key::Up => {
                self.move_selection(-1);
                true
            }
            Key::Tab => {
                self.complete();
                true
            }
            Key::Enter => {
                // Consumed whether or not a row is highlighted, as the shipped `case 'Enter'` is: it
                // calls `preventDefault()` before `activate()` decides there is nothing to copy.
                self.activate(tx);
                true
            }
            Key::Escape => {
                self.dismiss();
                true
            }
            _ => false,
        }
    }
}

/// Draws the Library window.
pub fn library_view(state: &Library, cx: &mut Cx<Library>) -> El {
    let theme = rune::tok::theme(cx.system_dark());
    // Drawn from the same state the confirmation acts on, and last so it is the top of the tree:
    // `El::modal` is what makes everything before it inert, not an ordering trick here.
    let pending = state.delete_dialog();
    col()
        .key("library-window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .px(space::S6)
        .py(space::S6)
        .gap(space::S3)
        .child(header(state, theme))
        .child(tabs(state))
        .child(entry_list(state, theme))
        .child(footer(state, theme))
        .children(pending.iter().map(|dialog| {
            dialog::confirm::<Library>(
                dialog,
                theme,
                Library::cancel_delete,
                Library::confirm_delete,
            )
        }))
}

fn header(state: &Library, theme: rune::tok::Theme) -> El {
    row()
        .key("library-header")
        .w_full()
        .gap(space::S3)
        .center_y()
        .child(
            text("Dev Console")
                .key("library-title")
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            row()
                .key("vault-status")
                .h(22.0)
                .px(space::S2)
                .radius(radius::SM)
                .center_y()
                .fill(theme.card)
                .border(1.0, theme.card_border)
                .child(
                    text(if state.vault_unlocked {
                        "Vault unlocked"
                    } else {
                        "Vault locked"
                    })
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
                ),
        )
}

fn tabs(state: &Library) -> El {
    row()
        .key("library-tabs")
        .gap(space::S2)
        .child(tab(Tab::Commands, state.tab))
        .child(tab(Tab::Secrets, state.tab))
}

fn tab(tab: Tab, current: Tab) -> El {
    let selected = tab == current;
    row()
        .key(format!("tab-{}", tab.label()))
        .role("tab")
        .label(tab.label())
        // `rune` at this revision exposes no `selected` state on an element (`el/a11y.rs` offers
        // `checked`, `position_in_set`, `a11y_value`, `active_descendant`). The visible difference
        // is the fill; whether that is enough for the tabs' accessible selection is a milestone-4
        // decision to take against a screen reader, not a call to make headlessly.
        .position_in_set(if selected { 1 } else { 2 }, 2)
        .focusable(true)
        .h(28.0)
        .px(space::S3)
        .center_y()
        .radius(radius::MD)
        .on_press_tx(move |state: &mut Library, _| state.tab = tab)
        .child(text(tab.label()).font(type_::LABEL))
}

fn entry_list(state: &Library, theme: rune::tok::Theme) -> El {
    if state.entries.is_empty() {
        return col()
            .key("empty-state")
            .w_full()
            .h_full()
            .center()
            .gap(space::S2)
            .child(
                text(if state.tab == Tab::Secrets {
                    "No secrets yet"
                } else {
                    "No commands yet"
                })
                .font(type_::TITLE)
                .color(theme.text),
            )
            .child(
                text("Add one with the editor, or import tasks from a project folder.")
                    .font(type_::BODY)
                    .color(theme.text_secondary),
            );
    }

    col()
        .key("entry-list")
        .role("list")
        .label(state.tab.label())
        .w_full()
        .h_full()
        .gap(space::S1)
        .children(state.entries.iter().enumerate().map(|(index, entry)| {
            row()
                .key(format!("entry-{index}"))
                .role("listitem")
                .label(entry.clone())
                .focusable(true)
                .h(28.0)
                .px(space::S2)
                .center_y()
                .radius(radius::MD)
                .on_press_tx(move |state: &mut Library, _| {
                    // Asking to delete is the shipped row action's destructive half (`onDelete` →
                    // `pendingDelete`), and it goes through the confirmation rather than removing
                    // anything here: the real deletion is a repository call at a known revision, so
                    // this shell's honest half is the gate in front of a placeholder.
                    state.request_delete(index);
                })
                .child(text(entry.clone()).font(type_::BODY))
        }))
}

fn footer(state: &Library, theme: rune::tok::Theme) -> El {
    row()
        .key("library-footer")
        .w_full()
        .gap(space::S3)
        .center_y()
        .child(
            text(format!(
                "{} {}",
                state.entries.len(),
                if state.tab == Tab::Secrets {
                    "secrets"
                } else {
                    "commands"
                }
            ))
            .font(type_::LABEL)
            .color(theme.text_secondary),
        )
        .child(keycap("footer-hint", TAB_HINT, theme))
}

/// Draws the Finder window: query field, context chip, candidates and the keyboard footer.
pub fn finder_view(state: &Finder, cx: &mut Cx<Finder>) -> El {
    let theme = rune::tok::theme(cx.system_dark());
    col()
        .key("finder-window")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .px(space::S3)
        .py(space::S3)
        .gap(space::S2)
        .child(query_field(state, theme))
        .child(context_chip(state, theme))
        .child(result_rows(state, theme))
        .child(copy_line(state, theme))
        .child(finder_footer(state, theme))
}

/// The one line the palette shows after a copy attempt, and the only surface a copy ever writes to.
///
/// Mirrors the shipped pair exactly: `class="copied" role="status"` for a copy that happened and
/// `class="error-banner" role="alert"` for one that did not, one line at a time (`{:else if}`), with
/// the same text. The element *key* follows that split too - `finder-status` / `finder-error` are the
/// `data-testid`s `tests/e2e/finder.spec.ts` reaches for - and what both can ever carry is one of
/// [`CopyOutcome`]'s three fixed strings, which is the leak-proof half of the shipped design done in
/// the type rather than in the template.
///
/// With nothing to report this emits an unkeyed zero-sized spacer, exactly as [`context_chip`] does:
/// a `finder-status` element that says nothing would be a live region a screen reader announces as
/// empty, and a scripted check reading its presence as "a copy just happened" would be wrong.
fn copy_line(state: &Finder, theme: rune::tok::Theme) -> El {
    let Some(outcome) = state.copy_status else {
        return el().size(0.0, 0.0);
    };
    // The shipped markup distinguishes the two: a confirmation is a `role="status"` live region, a
    // failure is a `role="alert"` one. Which one a headless tree announces is the a11y half this
    // container cannot settle, so the split is carried by the role and the key, not by a colour -
    // `rune::tok::Theme` has no danger token, and inventing a hex for a failure would break both the
    // token rule and the palette port. The panel's two states are told apart by contrast within the
    // tokens that exist.
    let failed = outcome == CopyOutcome::Failed;
    row()
        .key(if failed {
            "finder-error"
        } else {
            "finder-status"
        })
        .role(if failed { "alert" } else { "status" })
        .label(outcome.message())
        .w_full()
        .h(20.0)
        .px(space::S2)
        .center_y()
        .radius(radius::SM)
        .fill(if failed {
            theme.control_off
        } else {
            theme.card
        })
        .border(1.0, theme.card_border)
        .child(text(outcome.message()).font(type_::LABEL).color(if failed {
            theme.text
        } else {
            theme.text_secondary
        }))
}

fn query_field(state: &Finder, theme: rune::tok::Theme) -> El {
    row()
        .key("finder-query-row")
        .w_full()
        .h(34.0)
        .px(space::S2)
        .center_y()
        .radius(radius::MD)
        .fill(theme.card)
        .border(1.0, theme.card_border)
        .child(
            text_input(&state.query, type_::BODY)
                .key("finder-query")
                .role("textbox")
                .label("Search commands and secrets")
                .color(theme.text)
                .on_edit(|state: &mut Finder| &mut state.query)
                // The palette's bindings belong on the field, not on the window: the shipped
                // handler is `<input on:keydown>`, and a focused Rune field is also the only thing
                // that sees its own keys before the editor does. See [`Finder::route_key`].
                .on_input_key(|state: &mut Finder, ev: &KeyEvent, tx: &mut Tx| {
                    state.route_key(ev, tx)
                }),
        )
}

fn context_chip(state: &Finder, theme: rune::tok::Theme) -> El {
    // No detected project folder: emit an unkeyed, zero-sized spacer, not a `finder-context`
    // element. A keyed-but-empty chip would put an accessible node that announces nothing into the
    // tree, and the scripted checks read the presence of `finder-context` as "a folder was detected"
    // (`an_empty_finder_shows_the_query_field_and_no_rows_or_context_chip`). The unkeyed spacer keeps
    // the column's gap and layout identical; only the misleading identity is gone.
    if state.context.is_empty() {
        return el().size(0.0, 0.0);
    }
    row()
        .key("finder-context")
        .h(22.0)
        .px(space::S2)
        .center_y()
        .radius(radius::SM)
        .fill(theme.card)
        .border(1.0, theme.card_border)
        .label(format!("Project {}", state.context))
        .child(
            text(state.context.clone())
                .font(type_::MONO_CAPTION)
                .color(theme.text_secondary),
        )
}

fn result_rows(state: &Finder, theme: rune::tok::Theme) -> El {
    if state.rows.is_empty() {
        return col().key("finder-empty").w_full().h_full().center().child(
            text("Type to search")
                .font(type_::BODY)
                .color(theme.text_secondary),
        );
    }
    col()
        .key("finder-rows")
        .role("list")
        .label("Results")
        .w_full()
        .h_full()
        .gap(space::S1)
        .children(state.rows.iter().enumerate().map(|(index, candidate)| {
            let selected = index == state.selected;
            row()
                .key(format!("finder-row-{index}"))
                .role("listitem")
                // The name, never the value: `Candidate::name` is the row's metadata and the same
                // string `row.name` prints in the shipped list. The kind glyph (`Icon
                // name="secret"`) has no native equivalent yet - the SVG icon set is still on the
                // unbuilt list - so a secret row and a command row are told apart by what they copy,
                // not by what they print, which is the safer direction for a leak.
                .label(candidate.name.clone())
                // No `selected` state exists on `El` at this revision (see `tab`); the row's place
                // in the list is announced and the highlight is drawn.
                .position_in_set(index + 1, state.rows.len())
                .focusable(true)
                .h(28.0)
                .px(space::S2)
                .center_y()
                .radius(radius::MD)
                .fill(if selected { theme.hover } else { theme.bg })
                .on_press_tx(move |state: &mut Finder, _| {
                    state.select(index);
                })
                .child(
                    text(candidate.name.clone())
                        .font(type_::BODY)
                        .color(if selected {
                            theme.text
                        } else {
                            theme.text_secondary
                        }),
                )
        }))
}

/// The footer's hints. Each string is the binding [`Finder::route_key`] actually honours, in the order
/// `src/windows/Finder.svelte` lists them. `Ctrl+O details` is the one shipped chip left out, because
/// there is no details panel to open yet and a hint is a promise the shell has to keep.
const FINDER_HINTS: [(&str, &str); 4] = [
    ("footer-move", "Up+Down select"),
    ("footer-tab", "Tab complete"),
    ("footer-copy", "Enter copy"),
    ("footer-dismiss", "Esc dismiss"),
];

fn finder_footer(state: &Finder, theme: rune::tok::Theme) -> El {
    let mut footer = row()
        .key("finder-footer")
        .w_full()
        .gap(space::S2)
        .center_y();
    for (key, hint) in FINDER_HINTS {
        footer = footer.child(keycap(key, hint, theme));
    }
    // The shipped footer pushes its row count to the far edge with a flex spacer; `grow` is Rune's.
    footer.child(el().grow(1.0)).child(
        text(format!("{}", state.rows.len()))
            .key("footer-count")
            .font(type_::LABEL)
            .color(theme.text_secondary),
    )
}

/// A keyboard hint chip. `Keycap` in the current UI; the milestone-4 pass adds the real glyph set.
fn keycap(key: &'static str, label: &str, theme: rune::tok::Theme) -> El {
    row()
        .key(key)
        .h(18.0)
        .px(space::S1)
        .center_y()
        .radius(radius::SM)
        .fill(theme.control_off)
        .label(label.to_owned())
        .child(
            text(label.to_owned())
                .font(type_::MONO_CAPTION)
                .color(theme.text_secondary),
        )
}
