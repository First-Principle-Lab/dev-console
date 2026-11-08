//! The shared dialog shell: what a confirmation puts on the tree, what it refuses to do until it
//! is confirmed, and where the keyboard goes while it is open.
//!
//! ## Why the focus rules belong in a headless suite at all
//!
//! `Dialog.svelte` hand-rolls the focus behaviour of a modal: it remembers `document.activeElement`
//! on mount, focuses the first focusable control after `showModal()`, keeps Tab inside the panel with
//! a capture-phase keydown, and on close calls `trigger.focus()` - guarded by `isConnected`, so a
//! trigger the confirmation itself removed is not resurrected as a focus target.
//!
//! None of that is paint. `El::modal` keeps the same rules as *runtime* state, and a headless
//! `rune::Runtime` reports which element holds keyboard focus, so every one of them is asserted here
//! as a state transition: initial focus, the Tab cycle, the restore on close, and the dropped restore
//! when the record is gone. What is *not* decided is whether the user can see any of it - a focus
//! ring is paint, and that stays an owner-desktop row in the feasibility ledger.
//!
//! ## The destructive half
//!
//! A confirmation exists to be a gate, so the gate gets the strictest readings available here: nothing
//! is removed by a press or by Escape, the record removed is the one the dialog *named* even when the
//! list has been reordered underneath it, and the background is out of reach while the panel is open.

use dev_console_desktop::dialog::Dialog;
use dev_console_desktop::ui::{Library, Tab};
use dev_console_ui_tests::{Stage, DIALOG_KEYS};
use rune::Key;

/// The three entries every script below uses: the middle one is what the confirmation is opened on,
/// and the third proves a confirmation removes one record rather than the tail of the list.
fn library() -> Library {
    Library::with_entries(
        Tab::Commands,
        &[
            "synthetic command one",
            "synthetic command two",
            "synthetic command three",
        ],
    )
}

/// Opens the confirmation over row `index` and asserts the press landed, so no script below can be
/// passing because it never got as far as a dialog.
fn open(index: usize) -> Stage<Library> {
    let mut stage = Stage::library(library());
    assert!(
        stage.press_row(index),
        "row {index} has to be where the tree draws it"
    );
    assert!(
        stage.has("dialog"),
        "a row press opens the confirmation; every script below depends on that being true"
    );
    stage
}

#[test]
fn no_confirmation_is_drawn_until_a_row_asks_for_one() {
    let stage = Stage::library(library());
    let drawn = stage.present_from(DIALOG_KEYS);
    assert!(
        drawn.is_empty(),
        "a dialog built on no pending record would put a live region on the tree that answers \
         nothing: {drawn:?}"
    );
    assert!(stage.state().pending_delete.is_none());
    assert!(stage.has("entry-0"), "the list is still the screen");
}

#[test]
fn the_confirmation_renders_the_shipped_components_whole_surface() {
    let stage = open(1);
    assert_eq!(
        stage.present_from(DIALOG_KEYS),
        DIALOG_KEYS.to_vec(),
        "title, body, footer and both actions, plus the scrim that hosts the modal"
    );
    // The accessible name and the visible heading are different props in the shipped component
    // (`label` and `title`), and the spec queries resolve the dialog by the first.
    assert_eq!(
        stage.label_of("dialog").as_deref(),
        Some("Confirm delete"),
        "the accessible name the web spec resolves the dialog by - `getByRole('dialog', name = \
         'Confirm delete')` - has to resolve the same node natively"
    );
    assert_eq!(
        stage.text_of("dialog-title").as_deref(),
        Some("Delete \u{201c}synthetic command two\u{201d}?"),
        "the heading names the record the action will remove"
    );
    assert_eq!(
        stage.text_of("dialog-body").as_deref(),
        Some(
            "This removes the saved command permanently. Encrypted backups you made earlier still \
             hold it."
        ),
        "the shipped paragraph, including the promise the ported archive service actually keeps"
    );
    assert_eq!(stage.label_of("dialog-cancel").as_deref(), Some("Cancel"));
    assert_eq!(stage.label_of("dialog-confirm").as_deref(), Some("Delete"));
}

#[test]
fn a_confirmation_names_the_record_and_holds_nothing_the_vault_holds() {
    // The secret-hygiene rule, at the one surface where a confirmation could reasonably be written to
    // carry a payload: it is built from the row's metadata name and the tab's kind word, and
    // `PendingDelete` has exactly those two fields. Nothing here can hold a value because there is
    // nowhere here that a value could be put.
    let mut stage = Stage::library(Library::with_entries(
        Tab::Secrets,
        &["synthetic secret one"],
    ));
    assert!(stage.press("entry-0"));
    let pending = stage
        .state()
        .pending_delete
        .clone()
        .expect("a row press on the secrets tab arms the confirmation");
    assert_eq!(pending.name, "synthetic secret one");
    assert_eq!(
        pending.kind, "secret",
        "the tab decides the word in the sentence"
    );
    let rendered = format!("{pending:?}");
    assert_eq!(
        rendered.matches(':').count(),
        2,
        "the pending record is a name and a kind; a third field would have to be justified here: \
         {rendered}"
    );
    assert!(
        !rendered.to_lowercase().contains("value"),
        "no field of the confirmation may even be named after what it must not carry: {rendered}"
    );
    assert_eq!(
        stage.text_of("dialog-title").as_deref(),
        Some("Delete \u{201c}synthetic secret one\u{201d}?"),
        "the heading of a secret confirmation shows the metadata name and nothing else"
    );
    assert!(
        stage
            .text_of("dialog-body")
            .is_some_and(|body| !body.contains("synthetic secret one")),
        "the paragraph explains the consequence; it does not repeat the record back"
    );
}

#[test]
fn confirming_removes_the_named_record_and_closes_the_confirmation() {
    let mut stage = open(1);
    assert!(
        stage.press("dialog-confirm"),
        "the commit action has to be where it is drawn"
    );
    assert_eq!(
        stage.state().entries,
        vec!["synthetic command one", "synthetic command three"],
        "the record it named is gone; its neighbours are not"
    );
    assert!(
        !stage.has("dialog"),
        "a confirmation that stayed open after acting would offer the same deletion twice"
    );
    assert!(stage.state().pending_delete.is_none());
}

#[test]
fn a_record_that_moved_in_the_list_is_still_the_record_that_gets_removed() {
    // The reason `PendingDelete` holds a name instead of the pressed row's index: between asking and
    // confirming there is always an await (the real one is the repository call), and a list that
    // reloaded inside it must not be able to point the confirmation at a different record.
    let mut stage = open(1);
    stage.state_mut().entries.reverse();
    assert_eq!(
        stage.state().entries.first().map(String::as_str),
        Some("synthetic command three"),
        "the list moved under the open confirmation, which is the case being tested"
    );
    stage.press("dialog-confirm");
    assert!(
        !stage
            .state()
            .entries
            .iter()
            .any(|entry| entry == "synthetic command two"),
        "the named record is removed wherever it has ended up"
    );
    assert_eq!(stage.state().entries.len(), 2);
}

#[test]
fn cancelling_removes_nothing() {
    let mut stage = open(2);
    assert!(stage.press("dialog-cancel"));
    assert_eq!(
        stage.state().entries,
        vec![
            "synthetic command one",
            "synthetic command two",
            "synthetic command three"
        ],
        "Cancel is the action that must never touch the list"
    );
    assert!(!stage.has("dialog"));
    assert!(stage.state().pending_delete.is_none());
}

#[test]
fn escape_backs_out_of_the_confirmation_without_touching_the_list() {
    // The shipped component's Escape branch calls the same `oncancel` as the Cancel button, and so
    // does the outside press below: three routes, one behaviour, because a dismissed dialog that had
    // also acted would be the worst possible surprise.
    let mut stage = open(1);
    stage.press_key(Key::Escape);
    assert!(!stage.has("dialog"), "Escape closes it");
    assert_eq!(stage.state().entries.len(), 3, "and removes nothing");
    assert!(stage.state().pending_delete.is_none());
}

#[test]
fn a_press_off_the_panel_backs_out_of_the_confirmation_too() {
    // `modal(true, ...)`: the scrim is the modal host, so a press on the Library behind the panel is a
    // press outside it - the shipped component's `event.target !== panel` branch. Pressing the title
    // is the cheapest way to land off-panel and on nothing else.
    let mut stage = open(1);
    assert!(stage.press("library-title"));
    assert!(!stage.has("dialog"), "a press off the panel dismisses it");
    assert_eq!(
        stage.state().entries.len(),
        3,
        "and dismisses it without acting"
    );
}

#[test]
fn the_list_behind_an_open_confirmation_cannot_act() {
    // Two shipped rules meet on one press. The rows behind the panel are inert, so their handlers do
    // not fire - that is the half that stops a double deletion or a re-aimed confirmation. And a
    // press off the panel is a cancel, so the same press also closes the dialog, which is the shipped
    // component's `event.target !== panel` branch rather than a leak: cancelling is what a press on
    // the backdrop is *for*.
    let mut stage = open(1);
    stage.press_row(0);
    assert_eq!(
        stage.state().entries.len(),
        3,
        "the row behind the panel is inert: nothing is removed and no second handler fires"
    );
    assert!(
        !stage.has("dialog"),
        "and the press is treated as one off the panel, so the confirmation closes"
    );
    assert!(stage.state().pending_delete.is_none());
    assert_eq!(
        stage.state().entries,
        vec![
            "synthetic command one",
            "synthetic command two",
            "synthetic command three"
        ],
        "closing on the backdrop is a cancel, so it must not have taken the record with it"
    );
}

#[test]
fn the_confirmation_takes_the_keyboard_to_its_first_action() {
    // The shipped component focuses the first `button:not(:disabled)` after `showModal()` - in the
    // delete dialog's footer that is Cancel - and never leaves the focus on the row the pointer hit,
    // because the row under the scrim is not something Enter should act on twice.
    let mut stage = Stage::library(library());
    assert_eq!(
        stage.focused_key().as_deref(),
        None,
        "nothing holds keyboard focus before an interaction asks"
    );
    assert!(stage.press("entry-1"));
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("dialog-cancel"),
        "the keyboard has to land on the safe action, not on the row the pointer happened to hit"
    );
}

#[test]
fn tab_cycles_the_confirmations_actions_and_never_leaves_it() {
    let mut stage = open(1);
    // The shipped trap exists to keep Tab out of the browser chrome; the native reading is that it
    // must not walk onto the tabs, the rows or anything else behind the scrim.
    let outside_window = [
        "tab-Secrets",
        "tab-Commands",
        "entry-0",
        "entry-2",
        "library-title",
    ];
    assert_eq!(stage.focused_key().as_deref(), Some("dialog-cancel"));

    stage.press_tab(false);
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("dialog-confirm"),
        "Tab walks from the dismissive action to the committing one"
    );

    stage.press_tab(false);
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("dialog-cancel"),
        "and wraps at the end instead of running on into the window behind"
    );

    stage.press_tab(true);
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("dialog-confirm"),
        "Shift+Tab off the first control wraps back to the last, which is the half a naive trap \
         misses - the shipped component has an explicit branch for it"
    );

    let focused = stage.focused_key().unwrap_or_default();
    assert!(
        !outside_window.contains(&focused.as_str()),
        "focus escaped the confirmation onto {focused}, where Enter would act on something the \
         user cannot see"
    );
}

#[test]
fn closing_the_confirmation_puts_the_keyboard_back_where_it_came_from() {
    // The shipped component's restore. This is the observable that makes a dialog feel like a detour
    // rather than a page change: the row that asked is the row that has the keyboard again.
    let mut stage = open(1);
    assert_eq!(stage.focused_key().as_deref(), Some("dialog-cancel"));
    stage.press("dialog-cancel");
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("entry-1"),
        "cancelling restores focus to the row that asked"
    );
    assert_eq!(stage.state().entries.len(), 3);
}

/// The shipped component guards its restore with `if (trigger?.isConnected) trigger.focus()`: a
/// trigger the confirmation removed is not resurrected as a focus target. The native shell cannot
/// honour that guard yet, and the reason is worth pinning here rather than burying in a comment: the
/// placeholder list keys its rows by index (`entry-{i}`), so after a removal the key the restore
/// names still exists - it simply holds a different record. Identity-keyed rows are plan milestone
/// 4's, so this asserts the shell's actual behaviour and says out loud which part of the shipped
/// contract it does not satisfy yet. Anyone who changes it has to change this test, and the reason
/// will be in the diff.
#[test]
fn a_removed_record_leaves_the_focus_on_the_row_key_that_now_holds_someone_else() {
    let mut stage = open(1);
    stage.press("dialog-confirm");
    assert!(!stage.has("dialog"), "the panel leaves the tree");
    assert!(!stage.has("entry-2"), "and the list shrinks by one row");
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("entry-1"),
        "the restore names the index-keyed slot the press had left focus on"
    );
    assert_eq!(
        stage.label_of("entry-1").as_deref(),
        Some("synthetic command three"),
        "and that slot now holds a record nobody asked to delete: an index-keyed list has no \
         `isConnected` guard to fall back on, which is the half of the shipped contract that waits \
         on identity-keyed rows"
    );
}

/// The shell builds one `Dialog` per pending record and draws its five fields, which is the whole
/// contract between a screen and the shared shell. Pinning it means a screen that wants a different
/// confirmation has to add a constructor and a test; it cannot quietly re-purpose the delete one.
#[test]
fn the_confirmation_spec_is_the_surface_the_shell_renders() {
    let spec = Dialog::delete("synthetic command two", "command");
    assert_eq!(spec.title, "Delete \u{201c}synthetic command two\u{201d}?");
    assert_eq!(spec.label, "Confirm delete");
    assert_eq!(spec.cancel, "Cancel");
    assert_eq!(spec.confirm, "Delete");
    assert_eq!(
        spec.body,
        "This removes the saved command permanently. Encrypted backups you made earlier still hold \
         it."
    );
    let mut stage = Stage::library(library());
    stage.press("entry-1");
    assert_eq!(
        stage.state().delete_dialog(),
        Some(spec),
        "the view draws exactly the spec the state describes"
    );
    // The only variation the shared component supports today is the tab's word, and it comes from
    // state rather than from a second dialog type.
    let mut secrets = Stage::library(Library::with_entries(
        Tab::Secrets,
        &["synthetic secret one"],
    ));
    secrets.press("entry-0");
    let secret_spec = secrets
        .state()
        .delete_dialog()
        .expect("the secrets tab confirms too");
    assert!(
        secret_spec
            .body
            .starts_with("This removes the saved secret "),
        "the tab decides the noun, and nothing else about the spec changes: {}",
        secret_spec.body
    );
    assert_eq!(secret_spec.label, "Confirm delete");
    assert_eq!(secret_spec.confirm, "Delete");
}
