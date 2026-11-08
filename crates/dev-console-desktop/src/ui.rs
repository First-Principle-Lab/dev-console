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
//! [`Library::entries`], the vault state behind the header, the project scanners behind the
//! palette's context chip, and the ports of the draft validation, argument parsing, formatting and
//! Finder selection logic now living in `src/lib/*.ts`.
//!
//! ## Where the Finder is
//!
//! The palette's own state machine - [`Finder`], its [`Candidate`]s, `finder_view`, its keyboard
//! routing and the copy seam - is in [`crate::finder`], and the reasons this host can decide those
//! things are stated there. This module is the Library window: its tab strip, its list placeholder,
//! the editor pane, and the two panels this window can put on top of them.
//!
//! ## Which of these this container can decide
//!
//! Both windows' keyboard routing is decidable here and is therefore built here: [`Library::route_key`]
//! runs on the chords read off the same `AppSpec` the real window is built with, and
//! `dev-console-ui-tests/tests/library_keys.rs` asserts what the chords do to the state. What that
//! harness still cannot decide is *delivery*: whether the compositor hands the global shortcut to the
//! window at all, and whether the focus ring or the caret *looks* right - both stay UNVERIFIED (owner
//! desktop) in the native-feasibility ledger.
//!
//! Still unbuilt, and not faked: `Ctrl+N` (nothing opens an editor without the inspector's Edit
//! button), `Ctrl+S` (an editor draft exists now, but the `validateDraft` gate that would let the
//! chord answer for a save does not), `Space` copy and `Ctrl+O` details and `Ctrl+P` ignore-context
//! (need the details panel), the command-output panel and the SVG icon set. Three things have come
//! off that list since it was written: destructive confirmation, because [`Library::request_delete`]
//! opens [`crate::dialog`]'s shared shell (which owns the focus trap, the initial focus and the
//! restore), the Library's `Ctrl+1`/`Ctrl+2` tab chords, routed by [`Library::route_key`] off the same
//! `AppSpec` the real window is built from, and the editor pane with the draft rules behind it
//! ([`crate::editor], [`Library::start_edit`]). `Space` is deliberately left unbound rather than bound imperfectly:
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

use std::sync::LazyLock;

use crate::dialog::{self, Dialog};
use crate::editor::{Editor, EntryDraft};
pub use crate::finder::{finder_view, Candidate, Finder};
use rune::tok::{radius, space, type_};
use rune::{col, el, row, text, Chord, Cx, El, KeyEvent, Tx};

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
    /// The rows, standing in for the ranked, grouped list `src/lib` builds from the repository; empty
    /// means the empty state is shown. A [`EntryDraft`] rather than a bare name because a *saved*
    /// record is what an editor draft is compared against and written back to - see that type's doc.
    /// Every value in one is synthetic test material in every harness that builds it, and a secret's
    /// *value* has no field here at all.
    pub entries: Vec<EntryDraft>,
    /// True when the vault is unlocked. Only the *status* crosses into the UI; the key and any
    /// secret value never reach this struct.
    pub vault_unlocked: bool,
    /// The record a delete confirmation is open on, `None` while no dialog is showing. The
    /// shipped app holds the same thing as `pendingDelete`, and it is state rather than a view
    /// detail for the same reason: what the confirmation *names* has to be decided when the press
    /// lands, not when the panel is drawn.
    pub pending_delete: Option<PendingDelete>,
    /// The entry being edited and the draft typed over it, `None` while the window is showing the
    /// list. The shipped app holds the same fact as `mode === 'edit'`, and the *draft* half is state
    /// rather than a widget detail for the reason `pendingDelete` is: the unsaved-changes prompt, the
    /// dirty chip and the write-back all read the same draft, and a draft only held by a text field
    /// would leave "is there anything to lose?" unanswerable from the state the view is built from.
    ///
    /// `Default` is `None`, so every existing `Library { .. }` literal still compiles unchanged.
    pub editing: Option<Editor>,
    /// The unsaved-changes prompt is on screen: the shipped `pendingClose`, and a flag of its own for
    /// the shipped reason - it is raised by a *close request*, not by the draft being dirty, and
    /// `Keep editing` has to put it away without touching the draft. Deriving the prompt from the draft
    /// alone would leave it with no dismissive route at all.
    pub pending_close: bool,
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

/// The element-key prefix of the delete confirmation, which is also the prefix the shipped spec's
/// `data-testid`s resolve (`dialog`, `dialog-cancel`, `dialog-confirm`). The one carried as a value
/// rather than a literal at the call site because the shell's two panels are distinguished by it:
/// Rune addresses elements by key, so two live panels in one tree cannot share the seven keys their
/// actions are drawn under.
const DIALOG_ID: &str = "dialog";

/// The unsaved-changes prompt's prefix, for the same reason - and the reason it is not `"dialog"` is
/// the reason [`DIALOG_ID`] is not inlined: the two panels can be on screen at once, and a scripted
/// test has to be able to press *this* panel's `Keep editing` rather than whichever one the runtime
/// resolves a shared key to.
const PROMPT_ID: &str = "prompt";

impl Library {
    /// A Library showing `entries` on `tab`, for tests and for the headless scenes. Each row is saved
    /// under its own name with no command text, which is all the placeholder list and the placeholder
    /// editor need; note that [`Library::save_edit`] overwrites the whole saved record, so a fixture
    /// that wants command text in the record it edits has to build `entries` itself.
    pub fn with_entries(tab: Tab, entries: &[&str]) -> Library {
        Library {
            tab,
            entries: entries
                .iter()
                .map(|name| EntryDraft {
                    name: (*name).to_owned(),
                    command: String::new(),
                })
                .collect(),
            vault_unlocked: true,
            editing: None,
            pending_close: false,
            pending_delete: None,
        }
    }

    /// The name row `index` draws, for the view. A view that reached into `entries` itself could
    /// assume the placeholder list is a `Vec<String>`, which is the assumption the saved-record type
    /// exists to break; this is the one place the list's shape is allowed to show. Out of range reads
    /// as empty, which no row of the drawn list can be.
    pub fn name_field(&self, index: usize) -> String {
        self.entries
            .get(index)
            .map(|entry| entry.name.clone())
            .unwrap_or_default()
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
            name: entry.name.clone(),
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
        if let Some(position) = self
            .entries
            .iter()
            .position(|entry| entry.name == pending.name)
        {
            self.entries.remove(position);
        }
    }

    /// Backs out of the confirmation - the Cancel button, Escape, and a press off the panel all
    /// arrive here, exactly as the shipped `oncancel` serves all three. Nothing is removed.
    pub fn cancel_delete(&mut self) {
        self.pending_delete = None;
    }

    /// Opens the editor on row `index`, porting the shipped `startEdit`: the draft is a *copy* of the
    /// saved record, and nothing written into it touches that record until [`Library::save_edit`] says
    /// so. False when there is no such row - the shipped `if (!details || !selectedId) return;`.
    ///
    /// The one hop that is not here is the secret's `api.reveal`, which the shell cannot perform and
    /// which is not faked: a secret editor, and the deliberate reveal that opens it, are recorded in
    /// the capability matrix as absent rather than simulated with an empty draft.
    pub fn start_edit(&mut self, index: usize) -> bool {
        let Some(record) = self.entries.get(index) else {
            return false;
        };
        self.editing = Some(Editor::new(index, record));
        true
    }

    /// True when a draft differs from the record it was copied from - the shipped `draftDirty`, which
    /// is `JSON.stringify(draft) !== JSON.stringify(baseline)`. False with no editor at all, because
    /// `mode === 'edit' && draftDirty` is a conjunction and the list has nothing to lose.
    ///
    /// Field-wise rather than a JSON string compare: the comparison is *about the values*, and going
    /// through a string encoding would mean the shell's idea of "changed" depended on an encoding
    /// order. The shipped `baseline` is the same fact held the other way round - a snapshot to compare
    /// against, not the live draft.
    pub fn draft_dirty(&self) -> bool {
        self.editing
            .as_ref()
            .is_some_and(|editor| editor.draft != editor.baseline)
    }

    /// Asks the state, not a widget, whether there is anything to lose: `mode === 'edit' &&
    /// draftDirty`, the guard the shipped app repeats at the top of `select`, `togglePin`,
    /// `startEdit`, `saveSuggestion` and the close request. Every native guard is written against
    /// this one method, so the phrase cannot mean two things in this crate.
    pub fn draft_is_open(&self) -> bool {
        self.draft_dirty()
    }

    /// Writes the draft's name into the saved record - `api.save(draft)` in the shipped app, minus the
    /// repository, the revision check and the change event, none of which the shell has.
    ///
    /// The draft is *copied over* the saved record rather than swapped for it, which is what keeps the
    /// name and the command text of the original record intact: an editor that could reach only the
    /// name must not be able to blank a field it cannot show. A record that has gone away is not
    /// created.
    ///
    /// Returns whether anything was written. A record that went away while the editor was open is not
    /// recreated - and the caller is told, because silently "saving" into nothing is the one outcome
    /// the shipped `api.save(draft)` makes noise about.
    pub fn save_edit(&mut self) -> bool {
        let Some(editor) = self.editing.take() else {
            return false;
        };
        // The slot is not the record. `index` says where the editor was opened, but between opening
        // and saving the list can lose that row - the same await-shaped hole the delete confirmation
        // refuses to act on by index - and then the slot holds somebody else's record. Writing the
        // draft into it would be the bug, so the write goes to the record that is still what the
        // editor was opened on, matched on its saved text: the same identity rule `confirm_delete`
        // states, with the same caveat (two byte-identical saved records are not tellable apart here,
        // and the real fix is the id the placeholder record does not have).
        let written = match self
            .entries
            .iter()
            .position(|record| *record == editor.baseline)
        {
            Some(position) => {
                self.entries[position].name = editor.draft.name;
                true
            }
            None => false,
        };
        self.pending_close = false;
        written
    }

    /// Drops the draft and keeps the saved record exactly as it is - the shipped `cancelEdit`, whose
    /// whole promise is that the record was never written to.
    pub fn discard_draft(&mut self) {
        self.editing = None;
        self.pending_close = false;
    }

    /// The prompt's `Keep editing`: put the question away and change *nothing else*. This is the route
    /// the shipped prompt shares with Escape and with a click off the panel (`oncancel` is
    /// `() => (pendingClose = false)` - it sets one flag and nothing else), and it is the assertion
    /// that separates a real cancel from a discard that happens to look the same on screen.
    pub fn keep_editing(&mut self) {
        self.pending_close = false;
    }

    /// The window asked to close: with something to lose, ask first - the shipped `close-requested`
    /// handler's `if (mode === 'edit' && draftDirty) pendingClose = true;`, returning whether the window
    /// may go now.
    ///
    /// Two honest gaps, both in the host rather than in this state machine, and neither worked around.
    /// There is no native *producer* for the event yet: the window's close request belongs to Rune, and
    /// `AppSpec::on_close` is not wired to this state, so the scripted tests call the handler directly,
    /// as they call the chord router's state methods. And Rune's close hook has no veto at this
    /// revision - where the shipped app keeps the window open to answer the prompt, a native close
    /// would hide the window with the prompt on it. Both are recorded in the capability matrix.
    pub fn request_close(&mut self) -> bool {
        if self.draft_is_open() {
            self.pending_close = true;
            return false;
        }
        true
    }

    /// The prompt's `Save`: write the draft back, which closes the editor and so closes the prompt -
    /// the shipped `saveDraft().then(() => allowClose())`, whose prompt cannot outlive the save because
    /// the editor is no longer open to be dirty.
    pub fn save_draft_from_prompt(&mut self) {
        self.save_edit();
    }

    /// The field's hop: what a text input bound to the draft would call on every change.
    ///
    /// No Rune field is wired to it yet - see `editor_pane` for why a controlled input needs a
    /// widget handle the state does not hold - so this is the seam that stands in for one, and the
    /// scripted tests change a draft through it. That is not the same as testing a field (there is
    /// none), and it is not the shell's rule either: every dirty-draft rule in here is decided from
    /// `draft != baseline`, so what the scripts assert is the *shell's* behaviour once that relation
    /// holds - the chip, the tab guard, the prompt, the write-back - none of which is this setter.
    pub fn set_draft_name(&mut self, name: impl Into<String>) {
        if let Some(editor) = self.editing.as_mut() {
            editor.draft.name = name.into();
        }
    }

    /// The prompt to draw, built from the draft rather than from a copy of it: the shipped prompt names
    /// no entry and shows no field, so there is nothing here for a value to reach.
    pub fn unsaved_dialog(&self) -> Option<Dialog> {
        (self.pending_close && self.draft_is_open()).then(Dialog::unsaved_changes)
    }

    /// The confirmation to draw, built from the pending record. `None` means no dialog, and the view
    /// function turns on it.
    pub fn delete_dialog(&self) -> Option<Dialog> {
        self.pending_delete
            .as_ref()
            .map(|pending| Dialog::delete(&pending.name, pending.kind))
    }

    /// Shows `tab`: the port of `selectTab` in `src/windows/Library.svelte`. False when that tab is
    /// already showing, because the shipped handler's first line is `if (tab === next) return`, and
    /// false when an unsaved draft is open, because its next three lines are the guard this is the
    /// shell's only expression of:
    ///
    /// ```text
    /// if (mode === 'edit' && draftDirty) { status = 'Save or discard the open edit first.'; return; }
    /// ```
    ///
    /// ## Why the refusal raises the prompt
    ///
    /// The shipped app writes that sentence into its status line, which the native shell has not
    /// built (no `StatusLine`, no `status` field), and a refusal that swallowed the keystroke in
    /// silence would be a worse port than an incomplete one. So the guard asks the question that
    /// sentence is asking instead: the unsaved-changes prompt, whose `Keep editing` / `Discard` /
    /// `Save` are the three answers the refusal is waiting for, and which is built here rather than
    /// promised. What the port does *not* do is change the tab, discard the draft, or pretend the
    /// switch happened; a `status: Option<&'static str>` status line remains the literal port for
    /// later.
    pub fn select_tab(&mut self, tab: Tab) -> bool {
        if self.tab == tab {
            return false;
        }
        if self.draft_is_open() {
            self.pending_close = true;
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

/// Draws the Library window.
pub fn library_view(state: &Library, cx: &mut Cx<Library>) -> El {
    let theme = rune::tok::theme(cx.system_dark());
    // Both panels are built by the one shell, and the *order* they are drawn in is what decides which
    // one owns the keyboard: `El::modal` marks everything before the modal in tree order inert, so the
    // panel drawn last takes the trap and the one underneath stays visible and clickable without
    // owning Tab or Escape. A delete confirmation opened over an open unsaved-changes prompt is the
    // only way to get two here, and the confirmation is the more urgent thing to put on top.
    let pending = state.delete_dialog();
    let prompt = state.unsaved_dialog();
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
        // The shipped `{#if mode === 'edit'}` swap, and it is a swap rather than an overlay for the
        // shipped reason: the editor replaces the list, so there is no second copy of the entry on
        // screen and no way to read a saved record and its draft as if both were current.
        .child(if state.editing.is_some() {
            editor_pane(state, theme)
        } else {
            entry_list(state, theme)
        })
        .child(footer(state, theme))
        // The prompt, when it is the only panel up: the shell's modal, drawn from the same
        // `dialog::panel` as the confirmation with a third footer action and different routes. When a
        // confirmation is *also* open it is drawn underneath and does not take the trap - see the
        // comment above and `prompt_and_confirmation_can_both_be_on_screen_at_once`.
        .children(prompt.iter().map(|spec| {
            dialog::panel::<Library>(
                spec,
                theme,
                if pending.is_some() {
                    dialog::Modal::Under
                } else {
                    dialog::Modal::Trap
                },
                PROMPT_ID,
                dialog::Actions {
                    cancel: Library::keep_editing,
                    discard: Some(Library::discard_draft),
                    confirm: Library::save_draft_from_prompt,
                },
            )
        }))
        // Drawn from the same state the confirmation acts on, and last so it is the top of the tree.
        .children(pending.iter().map(|spec| {
            dialog::panel::<Library>(
                spec,
                theme,
                dialog::Modal::Trap,
                DIALOG_ID,
                dialog::Actions {
                    cancel: Library::cancel_delete,
                    discard: None,
                    confirm: Library::confirm_delete,
                },
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

/// The editor pane: the native half of `EntryEditor.svelte`'s header - which record is open, and
/// whether what is on screen differs from what is saved.
///
/// ## What is here, and what is a field the shell cannot draw yet
///
/// The shipped header is a kind glyph, `Edit command` / `Edit secret` / `New ...` (the `New` branch
/// waits on `Ctrl+N`, which is still inert), the **`unsaved changes` chip** the web asserts as
/// `data-testid="dirty-indicator"`, and then the fields. Two of those are here and the fields are not,
/// and the reason is worth having down: `text_input` at this revision of Rune takes its text *by
/// reference* and returns a `Text<String>` that the window state must own, so a field bound to a draft
/// held in an `Option` needs that `Option` to hold a Rune text widget - which is a widget detail
/// reaching back into the state layout. It is not built here, and it is not faked with an unbound
/// field that would look like an editor and edit nothing.
///
/// What *is* built is the part every one of the shipped dirty-draft behaviours actually turns on: the
/// draft's current value drawn as text, and the chip that says it differs from the saved record. The
/// draft itself is state ([`Editor`]) rather than widget state, which is what lets the tab guard, the
/// close request and the prompt be decided at all - and it is why a field can be added later without
/// moving those rules.
fn editor_pane(state: &Library, theme: rune::tok::Theme) -> El {
    let Some(editor) = state.editing.as_ref() else {
        return el().size(0.0, 0.0);
    };
    col()
        .key("entry-editor")
        .role("form")
        .w_full()
        .gap(space::S2)
        .child(
            text(format!("Edit {}", state.tab.kind()))
                .key("editor-title")
                .font(type_::TITLE)
                .color(theme.text),
        )
        // The shipped chip: `{#if dirty}<span data-testid="dirty-indicator">unsaved changes</span>`,
        // and absent - not dimmed - when the draft matches the record, which is what the web asserts
        // with `toHaveCount(0)`. Same word, same condition, one that the state can answer.
        .children(state.draft_dirty().then(|| {
            text("unsaved changes")
                .key("dirty-indicator")
                .role("status")
                .font(type_::LABEL)
                .color(theme.text_secondary)
        }))
        // The shipped Name field, drawn rather than bound: the box carries the label the web's
        // `<label>` gives it and the key the spec queries (`field-name`), and the text inside is the
        // draft's current value. Binding a real field to it needs the state to own a Rune text widget
        // (see this function's doc), so the value is *shown* and the rules built on it are decided from
        // the state, which is where they were always going to be.
        .child(
            col()
                .key("field-name-box")
                .role("textbox")
                .label("Name")
                .w_full()
                .h(28.0)
                .px(space::S2)
                .center_y()
                .radius(radius::MD)
                .fill(theme.card)
                .border(1.0, theme.card_border)
                .child(
                    text(editor.draft.name.clone())
                        .key("field-name")
                        .font(type_::BODY)
                        .color(theme.text),
                ),
        )
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
        .children(state.entries.iter().enumerate().map(|(index, _)| {
            row()
                .key(format!("entry-{index}"))
                .role("listitem")
                .label(state.name_field(index))
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
                .child(text(state.name_field(index)).font(type_::BODY))
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

/// A keyboard hint chip. `Keycap` in the current UI; the milestone-4 pass adds the real glyph set.
pub(crate) fn keycap(key: &'static str, label: &str, theme: rune::tok::Theme) -> El {
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
