//! The Finder window's state and view: the palette's query field, its context chip, its candidate
//! rows and the keyboard footer.
//!
//! This is the palette half of what `src/windows/Finder.svelte` is on the shipped side. It lives
//! apart from [`crate::ui`] because the two windows are separate state machines with one thing in
//! common - the toolkit they draw with - and because the keyboard rules ported here are the reason
//! this host can decide anything about the palette at all: `route_key` in
//! `crates/rune/src/runtime/input/keys.rs` asks the focused input, then its editor, then the
//! window's handler, so every binding asserted below is driven through the same seam the real window
//! uses.
//!
//! Secret hygiene is a shape property here, not a discipline: [`Candidate`] carries an id, a name and
//! a kind, and the value behind a secret candidate reaches the clipboard only inside
//! [`Finder::activate`]'s call to the [`crate::copy::CopySource`] - there is no field on
//! any of these structs that a `Debug` dump or an `assert_eq!` diff could print a value out of, which
//! is what [`Finder`]'s hand-written `Debug` and the harness's element-tree scans assert.

use std::sync::Arc;
use std::time::Duration;

use crate::copy::{self, CopyOutcome, CopySource, Unwired};
use crate::ui::keycap;
use dev_console_core::models::{CopyRequest, Kind};
use rune::tok::{radius, space, type_};
use rune::{col, el, row, text, text_input, Cx, El, Key, KeyEvent, TextEdit, Tx};

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
