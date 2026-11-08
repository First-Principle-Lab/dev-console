//! The editor's draft: what makes it "unsaved", what the shell refuses to do while it is, and the
//! prompt that stands between an unsaved draft and losing it.
//!
//! ## The gap these tests exist because of
//!
//! Until a saved record was a thing the state could hold, none of this was expressible natively: an
//! editor whose draft *was* the entry had no "differs from the saved record" to ask about, so the
//! shipped `draftDirty` (`JSON.stringify(draft) !== baseline` in `src/windows/Library.svelte`), the
//! `mode === 'edit' && draftDirty` guard on switching tabs, the close-request prompt and its
//! `dirty-indicator` chip had no native subject at all. [`Editor`] and [`EntryDraft`] supply the
//! missing term - the record as saved, beside the record as typed - and this file is what that buys:
//! fifteen rules ported from the shipped editor, decided on real element trees and real state
//! transitions.
//!
//! ## What is *not* here, and why it is not faked
//!
//! There is no text field bound to the draft. `text_input` at this revision of Rune owns its text in a
//! handle the window state must hold, so wiring one to a draft inside an `Option` would put a widget
//! detail into the state layout; the pane draws the draft's value as text instead, and the scripts
//! below change the draft through the state - the same hop a field will write through. What they then
//! assert is the *shell's* behaviour (the chip, the guard, the prompt, the write-back), which is the
//! part the shipped tests are about and does not depend on which widget last changed the draft. The
//! validation gate (`validateDraft`) and the save-failure path that keeps a draft alive are likewise
//! absent rather than simulated, and say so in the capability matrix.
//!
//! ## The prompt's own half
//!
//! `Keep editing` / `Discard` / `Save` is the shared shell's second user, and the assertions are the
//! ones that make "shared" mean something rather than "a second copy that looks familiar": a third
//! footer action, a dismissive route that changes *nothing else* (`keep_editing` writes no draft and
//! no record), a trap over three stops instead of two, and both panels on screen at once with each
//! one's presses going to its own routes.

use dev_console_desktop::editor::EntryDraft;
use dev_console_desktop::ui::{Library, Tab};
use dev_console_ui_tests::{entry_names, Stage, EDITOR_KEYS, PROMPT_KEYS};
use rune::Key;

/// The three records every script below starts from. Synthetic (AGENTS.md): a draft is metadata - a
/// name and command text - and never a value, but the rule still holds with made-up words.
fn library() -> Library {
    Library {
        entries: vec![
            EntryDraft {
                name: "synthetic command one".into(),
                command: "echo one".into(),
            },
            EntryDraft {
                name: "synthetic command two".into(),
                command: "echo two".into(),
            },
            EntryDraft {
                name: "synthetic command three".into(),
                command: "echo three".into(),
            },
        ],
        vault_unlocked: true,
        ..Library::default()
    }
}

/// A stage with row 1 open in the editor and `name` typed into its draft, which is the state every
/// dirty-draft rule in the shipped app is written against.
///
/// The draft is changed through the *state*, because no widget is bound to it yet - see the module
/// doc. What the scripts then assert is the shell's behaviour, and none of it is the setter's: the
/// chip, the guard, the prompt and the write-back are all decided from `draft != baseline`.
fn stage_with_dirty_draft() -> Stage<Library> {
    let mut stage = Stage::library(library());
    assert!(
        stage.state_mut().start_edit(1),
        "row 1 is where the list draws it"
    );
    stage
        .state_mut()
        .set_draft_name("synthetic command two edited");
    stage
}

#[test]
fn the_editor_is_clean_until_the_draft_differs_from_the_saved_record() {
    let mut stage = Stage::library(library());
    assert!(stage.state_mut().start_edit(1));
    // The shipped `toHaveCount(0)` before anything is typed, and the editor *is* open: what the chip
    // answers is "does this differ from the record?", not "is an editor on screen?".
    assert!(
        stage.has("entry-editor"),
        "the editor pane replaces the list, so the draft is on screen"
    );
    assert!(
        !stage.has("dirty-indicator"),
        "a draft that matches the saved record is not unsaved"
    );
    assert_eq!(
        stage.present_from(EDITOR_KEYS),
        vec!["entry-editor", "editor-title", "field-name"],
        "the pane's whole surface, with the chip absent rather than dimmed"
    );
    // The field shows the *draft*, which at this point is a copy of the record.
    assert_eq!(
        stage.text_of("field-name").as_deref(),
        Some("synthetic command two"),
        "the editor opens prefilled with the saved text"
    );
    assert!(!stage.state().draft_dirty());
}

#[test]
fn typing_into_the_draft_makes_it_unsaved_and_draws_the_chip() {
    let mut stage = Stage::library(library());
    stage.state_mut().start_edit(1);
    stage
        .state_mut()
        .set_draft_name("synthetic command two edited");
    assert!(
        stage.state().draft_dirty(),
        "the draft now differs from the record it was copied from"
    );
    assert_eq!(
        stage.present_from(EDITOR_KEYS),
        vec![
            "entry-editor",
            "editor-title",
            "dirty-indicator",
            "field-name"
        ],
        "the chip is the web's `dirty-indicator`, in the same place, under the same condition"
    );
    assert_eq!(
        stage.text_of("dirty-indicator").as_deref(),
        Some("unsaved changes"),
        "the shipped chip's own words"
    );
}

#[test]
fn putting_the_draft_back_to_the_saved_text_loses_the_chips_reason() {
    // The condition is `draft != baseline`, not "has anything ever been typed": the shipped app's
    // baseline is the snapshot taken when the editor opened, and a draft returned to it is clean.
    let mut stage = stage_with_dirty_draft();
    assert!(stage.has("dirty-indicator"));
    stage
        .state_mut()
        .set_draft_name("synthetic command two".to_owned());
    assert!(!stage.state().draft_dirty());
    assert!(
        !stage.has("dirty-indicator"),
        "a draft equal to the saved record has nothing to lose, so nothing should say it does"
    );
}

#[test]
fn the_editor_replaces_the_list_rather_than_joining_it() {
    let mut stage = Stage::library(library());
    assert!(stage.has("entry-list"), "the list is the screen at rest");
    stage.state_mut().start_edit(1);
    assert!(
        !stage.has("entry-list"),
        "the shipped editor is a swap, so a saved list and its draft are never both on screen"
    );
    assert!(stage.has("entry-editor"));
    // The tab strip and the status line stay: the swap is `EntryList` for `EntryEditor`, not the
    // window for a sheet, and a tab chord pressed now has something to be refused by.
    assert!(stage.has("library-tabs") && stage.has("library-footer"));
}

#[test]
fn switching_tabs_with_an_unsaved_draft_is_refused_and_says_nothing_else() {
    let mut stage = stage_with_dirty_draft();
    assert!(
        !stage.state_mut().select_tab(Tab::Secrets),
        "precondition: the tab guard refuses the switch"
    );
    let before = stage.state().clone();
    stage.press_ctrl('2');
    assert_eq!(
        stage.state().tab,
        before.tab,
        "`selectTab` returns before touching `tab` when a draft is open, so Secrets never shows"
    );
    assert!(
        stage.has("entry-editor") && !stage.has("entry-list"),
        "and the screen does not half-switch: the editor is still what is drawn"
    );
    assert_eq!(
        stage.label_of("tab-Secrets").as_deref(),
        Some("Secrets"),
        "the tabs themselves stay drawn and focusable - the refusal is in the handler, exactly as \
         the shipped one is a function that returns"
    );
}

#[test]
fn switching_tabs_with_a_clean_draft_works() {
    // The refusal is about the *draft*, not about the editor being open: an untouched draft has
    // nothing to lose, and the shipped guard (`draftDirty`) lets it through.
    let mut stage = Stage::library(library());
    stage.state_mut().start_edit(1);
    stage.press_ctrl('2');
    assert_eq!(stage.state().tab, Tab::Secrets);
    assert!(
        stage.has("entry-editor"),
        "the editor stays open across the switch"
    );
}

#[test]
fn saving_writes_the_draft_into_the_record_it_came_from_and_closes_the_editor() {
    let mut stage = stage_with_dirty_draft();
    assert!(
        stage.state_mut().save_edit(),
        "and the save reports that it wrote"
    );
    assert_eq!(
        entry_names(stage.state()),
        vec![
            "synthetic command one",
            "synthetic command two edited",
            "synthetic command three"
        ],
        "the edited record and no other"
    );
    assert!(
        !stage.has("entry-editor") && stage.has("entry-list"),
        "the write closes the editor, so the list is the screen again"
    );
    assert!(
        !stage.state().draft_dirty(),
        "with no editor open there is nothing left to be unsaved"
    );
}

#[test]
fn a_cancelled_edit_leaves_the_saved_record_untouched() {
    // The clause the objective names: `cancelEdit` closes the editor without ever reaching the
    // record, which is only a checkable promise once the record and the draft are different things.
    let mut stage = stage_with_dirty_draft();
    stage.state_mut().discard_draft();
    assert_eq!(
        entry_names(stage.state()),
        vec![
            "synthetic command one",
            "synthetic command two",
            "synthetic command three"
        ],
        "the discarded words never reached the list"
    );
    assert!(!stage.has("entry-editor"));
    assert!(stage.has("entry-list"));
}

#[test]
fn saving_to_a_record_that_has_gone_away_creates_nothing_and_reports_it() {
    // The await-shaped hole the delete confirmation already had: between asking and saving, the
    // record can go. A native shell with one record in the list and one in the editor would either
    // recreate it or write the draft into the wrong row; this reports "did not write".
    let mut stage = stage_with_dirty_draft();
    stage.state_mut().entries.remove(1);
    assert!(
        !stage.state_mut().save_edit(),
        "there is no record to write, and inventing one is not the editor's call"
    );
    assert_eq!(
        entry_names(stage.state()),
        vec!["synthetic command one", "synthetic command three"],
        "and the list is exactly what the removal left"
    );
    assert!(
        stage.state().editing.is_none(),
        "the editor still closes: there is nothing left to keep"
    );
}

#[test]
fn nothing_is_asked_while_the_draft_matches_the_record() {
    let mut stage = Stage::library(library());
    stage.state_mut().start_edit(1);
    assert!(
        stage.state_mut().request_close(),
        "an untouched draft gives the window nothing to answer"
    );
    assert!(
        !stage.has("prompt"),
        "and no panel is drawn on the strength of an editor merely being open"
    );
}

#[test]
fn closing_the_window_asks_first_when_the_draft_is_unsaved() {
    let mut stage = stage_with_dirty_draft();
    assert!(
        !stage.state_mut().request_close(),
        "`close-requested` sets `pendingClose` instead of allowing the close"
    );
    assert_eq!(
        stage.present_from(PROMPT_KEYS),
        PROMPT_KEYS.to_vec(),
        "the panel is the shared shell's surface with a *third* action, which is the whole reason a \
         second call site is a test of the shell and not a copy of it"
    );
    assert_eq!(
        stage.label_of("prompt").as_deref(),
        Some("Unsaved changes"),
        "the accessible name the web spec resolves the prompt by"
    );
    assert_eq!(
        stage.text_of("prompt-title").as_deref(),
        Some("Unsaved changes")
    );
    assert_eq!(
        stage.text_of("prompt-body").as_deref(),
        Some("This entry has edits that are not saved."),
        "the shipped sentence, naming no entry and no field"
    );
    assert_eq!(
        stage.label_of("prompt-cancel").as_deref(),
        Some("Keep editing")
    );
    assert_eq!(stage.label_of("prompt-discard").as_deref(), Some("Discard"));
    assert_eq!(stage.label_of("prompt-confirm").as_deref(), Some("Save"));
}

#[test]
fn keeping_editing_puts_the_question_away_and_changes_nothing_else() {
    // The route that separates a real cancel from a discard that looks the same: `oncancel` is
    // `() => (pendingClose = false)`, one flag, and the draft and the record must both be untouched.
    let mut stage = stage_with_dirty_draft();
    stage.state_mut().request_close();
    assert!(stage.has("prompt"));
    let before = stage.state().clone();
    assert!(stage.press("prompt-cancel"));
    assert_eq!(
        before.entries,
        stage.state().entries,
        "the saved records did not move"
    );
    assert_eq!(
        before.editing,
        stage.state().editing,
        "and neither did the draft or the open editor"
    );
    assert!(
        !stage.has("prompt"),
        "the question is gone, so it can be asked again later"
    );
    assert!(
        stage.state().draft_dirty(),
        "there is still something to lose"
    );
}

#[test]
fn escape_and_a_press_off_the_panel_are_the_same_answer_as_the_button() {
    // The shell's one dismissive route reached three ways. It is asserted on the *prompt* rather than
    // re-proved per panel deliberately: `El::modal` is given `Keep editing` as its dismiss handler and
    // `click_outside` is on, so a second screen wired to a *different* route would prove the shell is
    // shared in the only way that matters - the same three doors, whichever handler is behind them.
    for route in ["escape", "backdrop"] {
        let mut stage = stage_with_dirty_draft();
        stage.apply(|library| library.request_close());
        assert!(stage.has("prompt"), "and the prompt is up to be dismissed");
        match route {
            // The keyboard door. Rune gives the *focused control* the first look at Escape, and the
            // prompt took focus on appearing, so the scripted frame has to settle once before the
            // modal's own dismiss branch sees it - which is the real order of events in a running
            // window, not a harness trick: the first frame belongs to the focused control.
            "escape" => {
                stage.press_key(Key::Escape);
                stage.settle();
            }
            // The pointer door, aimed off the panel. The scrim spans the window with the panel centred
            // in it, so pressing the scrim's centre is pressing the panel; a corner is the only place
            // in the scrim that is not above the panel, and that is exactly what the shipped
            // component's `event.target !== panel` branch is about.
            _ => {
                let ([x, y, _, _], _) = stage.box_of("prompt-scrim").expect("the scrim is drawn");
                stage.press_point(x + 4.0, y + 4.0);
            }
        }
        assert!(
            !stage.has("prompt"),
            "{route} has to close the prompt like the button does"
        );
        assert!(
            stage.state().draft_dirty() && stage.state().editing.is_some(),
            "{route} must not have quietly discarded the draft on the way out"
        );
    }
}

#[test]
fn the_prompt_traps_its_keyboard_inside_three_stops() {
    // The same four rules the delete confirmation is held to, on a panel with one more action: the
    // trap is not a two-button special case.
    let mut stage = stage_with_dirty_draft();
    stage.state_mut().request_close();
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("prompt-cancel"),
        "initial focus is the first control of the panel, which here is `Keep editing`"
    );
    stage.press_tab(false);
    assert_eq!(stage.focused_key().as_deref(), Some("prompt-discard"));
    stage.press_tab(false);
    assert_eq!(stage.focused_key().as_deref(), Some("prompt-confirm"));
    stage.press_tab(false);
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("prompt-cancel"),
        "Tab wraps over three stops, not two"
    );
    stage.press_tab(true);
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("prompt-confirm"),
        "and Shift+Tab wraps the other way"
    );
    // The background is inert: the editor's field and the tab strip are both behind the panel.
    assert!(
        stage.centre_of("field-name").is_some(),
        "the editor is still drawn under the panel"
    );
}

#[test]
fn discarding_loses_the_draft_but_never_writes_it() {
    let mut stage = stage_with_dirty_draft();
    stage.state_mut().request_close();
    assert!(stage.press("prompt-discard"));
    assert_eq!(
        entry_names(stage.state()),
        vec![
            "synthetic command one",
            "synthetic command two",
            "synthetic command three"
        ],
        "`Discard` is `cancelEdit` then the close: the typed words go, and never arrive"
    );
    assert!(stage.state().editing.is_none());
    assert!(
        !stage.has("prompt"),
        "with nothing left to lose the question cannot still be on screen"
    );
    assert!(
        stage.has("entry-list"),
        "and the window is back to the list it had before the editor"
    );
}

#[test]
fn saving_from_the_prompt_writes_the_draft_and_is_what_allows_the_close() {
    // `saveDraft().then(() => allowClose())`: the answer is a write, and the prompt cannot outlive
    // the save because the editor it was about is closed.
    let mut stage = stage_with_dirty_draft();
    stage.state_mut().request_close();
    assert!(stage.press("prompt-confirm"));
    assert_eq!(
        entry_names(stage.state())[1],
        "synthetic command two edited",
        "the draft became the record"
    );
    assert!(
        !stage.has("prompt"),
        "and the question closed with the editor that raised it"
    );
    assert!(
        stage.state_mut().request_close(),
        "there is nothing left to ask about"
    );
}

#[test]
fn the_prompt_survives_being_asked_again_after_a_keep_editing() {
    // The shipped flag is per request: dismissing the prompt does not resolve the draft, so the next
    // close request has to ask again rather than remember that the user already said "keep editing".
    let mut stage = stage_with_dirty_draft();
    stage.state_mut().request_close();
    stage.press("prompt-cancel");
    assert!(!stage.has("prompt"));
    assert!(!stage.state_mut().request_close(), "it asks again");
    assert!(stage.has("prompt"));
}

#[test]
fn a_confirmation_can_open_over_the_prompt_and_keeps_its_own_routes() {
    // Two panels in one tree, and the rule that decides what the keyboard does there. `El::modal`
    // marks everything *before* the modal in tree order inert, so a host with two panels can give the
    // keyboard to exactly one of them, and the order the view draws them in is what says which: the
    // delete confirmation, drawn last, because a destructive question the user can see must not be
    // invisible to the keyboard, while the prompt underneath keeps its own three buttons.
    //
    // ## How a script gets here, and how a user does not
    //
    // The row's delete hop is driven through the state, because the list is not drawn while an editor
    // is open - which is the point worth having down: that swap is the native shell's expression of the
    // shipped `actionsDisabled={mode === 'edit' && draftDirty}` on the entry list. A user of this shell
    // cannot reach two panels, because they cannot press a row that is not drawn; the state hop goes
    // around that on purpose, to pin the shell's behaviour in the state its own view code can produce
    // (the prompt is a flag, and a confirmation can be raised by anything that reaches the state).
    let mut stage = stage_with_dirty_draft();
    stage.apply(|library| library.request_close());
    assert!(
        stage.apply(|library| library.request_delete(2)),
        "and a row can still be asked about"
    );
    assert!(
        stage.has("prompt"),
        "the prompt stays drawn under the panel"
    );
    assert!(stage.has("dialog"), "and the confirmation is on top of it");
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("dialog-cancel"),
        "initial focus goes to the topmost panel, not to whichever appeared first"
    );
    stage.press_tab(false);
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("dialog-confirm"),
        "and its Tab cycle stays inside the confirmation, over the two actions it has"
    );
    stage.press_tab(false);
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("dialog-cancel"),
        "wrapping without ever reaching the prompt's three buttons behind it"
    );
    // Cancelling the confirmation leaves the prompt and the draft exactly where they were: two panels,
    // two routes, and neither one's cancel reaching through the other.
    assert!(stage.press("dialog-cancel"));
    assert!(!stage.has("dialog"), "the confirmation closed");
    assert!(
        stage.has("prompt"),
        "the prompt is still there to be answered"
    );
    assert_eq!(
        stage.label_of("prompt-cancel").as_deref(),
        Some("Keep editing"),
        "and it is still its own panel with its own buttons, not a relabelled confirmation"
    );
    assert!(
        stage.state().draft_dirty(),
        "with the draft still open underneath"
    );
    // Answering it now writes nothing to the record the confirmation was about: the prompt's routes
    // and the confirmation's are separate, and neither one's cancel reaches through the other.
    assert!(stage.press("prompt-discard"));
    assert_eq!(
        entry_names(stage.state()),
        vec![
            "synthetic command one",
            "synthetic command two",
            "synthetic command three"
        ],
        "the draft went, and none of it reached a saved record"
    );
    assert!(
        !stage.has("prompt") && !stage.has("dialog"),
        "both panels are gone"
    );
    assert!(
        stage.has("entry-list"),
        "and the window is back to the list, which is the only screen that can offer a delete again"
    );
    // The delete the earlier panel was about can be asked and confirmed for real now.
    assert!(stage.state_mut().request_delete(2));
    assert!(stage.press("dialog-confirm"));
    assert_eq!(
        entry_names(stage.state()),
        vec!["synthetic command one", "synthetic command two"],
        "the record it named, once - and nothing the discarded draft left behind"
    );
}

#[test]
fn the_prompt_names_nothing_a_value_could_hide_in() {
    // Secret hygiene, on the panel that exists *because* of unsaved edits: the prompt is built from
    // the draft being open, not from a copy of it, so it has no surface for a value to arrive on -
    // and with the placeholder draft holding command text rather than secret values, the check is
    // about the shape, which is the honest way to test a panel that has no value to leak.
    let mut stage = stage_with_dirty_draft();
    stage.state_mut().request_close();
    let shown = stage.element_strings();
    for surface in &shown {
        assert!(
            !surface.contains("echo two"),
            "the draft's command text reached a surface of the prompt: {surface:?}"
        );
    }
    assert!(
        shown.iter().any(|s| s == "Unsaved changes"),
        "and the panel is actually there to check"
    );
}
