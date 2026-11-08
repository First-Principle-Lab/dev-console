//! The Library window's own keyboard: the chords that switch the tab the list is built from, the
//! ones that do not exist yet, and the footer chip that promises the difference.
//!
//! ## Why every stage here is built from the shipped `AppSpec`
//!
//! `src/windows/Library.svelte` binds its chords on `<svelte:window onkeydown>`, not on a control:
//! they belong to the window and fire wherever the keyboard happens to be. Rune's equivalent is
//! `AppSpec::on_key`, which the runtime consults first when nothing has focus and after the focused
//! control has declined a key when something has (`runtime/input/keys.rs`). So each stage below
//! comes from [`dev_console_ui_tests::Stage::library`], which builds its runtime out of
//! `app::library_spec()` - view, size *and* router - rather than assembling its own. That is the
//! difference between testing the shell and testing a lookalike: the state transition itself
//! ([`dev_console_desktop::ui::Library::select_tab`]) is three lines, and everything interesting
//! about this feature is in the wiring in front of it.
//!
//! ## The branches that are not ported, said out loud
//!
//! The shipped handler has five branches. The two `Escape` ones need no native twin:
//! [`dev_console_desktop::dialog`]'s modal dismisses itself on Escape, which is the whole of
//! `Escape && pendingDelete`, and there is no `pendingRun` at all yet. `Ctrl+N` (new entry) and
//! `Ctrl+S` (save the open edit) wait on an editor the shell has not built - and
//! [`the_chords_that_wait_on_an_editor_do_nothing_yet`] asserts they are inert rather than leaving
//! them unmentioned, because a branch nobody tests is a branch someone later assumes exists.
//!
//! Whether a compositor delivers a chord to a Wayland window at all is the owner's row in the
//! feasibility ledger. What is decided here is that the shell does the right thing once it arrives.

use dev_console_desktop::ui::{Library, Tab, TAB_HINT};
use dev_console_ui_tests::{library_with_entries, Stage};
use rune::Key;

/// The digits the footer's tab chip advertises, read out of the same constant the footer draws.
fn advertised_digits() -> Vec<char> {
    TAB_HINT.chars().filter(char::is_ascii_digit).collect()
}

#[test]
fn ctrl_two_switches_to_the_secrets_tab_and_rebuilds_the_list() {
    let mut stage = Stage::library(library_with_entries());
    assert_eq!(stage.state().tab, Tab::Commands);
    assert_eq!(stage.label_of("entry-list").as_deref(), Some("Commands"));

    stage.press_ctrl('2');

    assert_eq!(stage.state().tab, Tab::Secrets);
    // Not just a flag: the list is a different list, same element and key, the new tab's rows under
    // it. A view that read the tab once at build time would keep saying `Commands` here.
    assert_eq!(stage.label_of("entry-list").as_deref(), Some("Secrets"));
    assert!(
        stage.has("entry-0"),
        "the list stays mounted across a switch"
    );
}

#[test]
fn ctrl_one_returns_to_the_commands_tab() {
    let mut stage = Stage::library(Library::with_entries(
        Tab::Secrets,
        &["synthetic deploy token"],
    ));
    assert_eq!(stage.label_of("entry-list").as_deref(), Some("Secrets"));

    stage.press_ctrl('1');

    assert_eq!(stage.state().tab, Tab::Commands);
    assert_eq!(stage.label_of("entry-list").as_deref(), Some("Commands"));
}

#[test]
fn the_empty_state_copy_follows_the_tab() {
    // The other half of "the list was rebuilt": with nothing in it, the empty state names the kind of
    // the tab now showing, and that copy is in the live tree rather than only implied by a field.
    let mut stage = Stage::library(Library::default());
    let before = stage.element_strings();
    assert!(
        before.iter().any(|text| text == "No commands yet"),
        "the default tab's empty state, so the assertion after the switch is not vacuous"
    );
    assert!(!before.iter().any(|text| text == "No secrets yet"));

    stage.press_ctrl('2');

    assert!(
        stage
            .element_strings()
            .iter()
            .any(|text| text == "No secrets yet"),
        "the secrets tab says so in the empty state, not only in `state.tab`"
    );
}

#[test]
fn a_chord_for_the_tab_already_showing_changes_nothing() {
    // `selectTab`'s first line is `if (tab === next) return`, and the chord is still consumed:
    // Ctrl+1 pressed on the Commands tab must not fall through as an unhandled key for something
    // else to pick up. The shipped handler calls `preventDefault()` before that early return.
    let mut stage = Stage::library(library_with_entries());
    stage.press_ctrl('1');

    assert_eq!(stage.state().tab, Tab::Commands);
    assert_eq!(stage.label_of("entry-list").as_deref(), Some("Commands"));
    assert!(
        stage.focused_key().is_none(),
        "a consumed chord does not move the keyboard anywhere either"
    );
}

#[test]
fn a_digit_without_control_is_typing_not_a_chord() {
    let mut stage = Stage::library(library_with_entries());
    stage.press_key(Key::Text("2".to_owned()));

    assert_eq!(
        stage.state().tab,
        Tab::Commands,
        "a bare 2 is text; only the chord is the window's"
    );
}

#[test]
fn an_unbound_chord_on_another_digit_does_not_switch_tabs() {
    // Guards the matcher rather than the transition, and in both directions. A router that claimed
    // "any Ctrl chord is mine" would send an unbound digit to whichever tab its table lists first,
    // so the check has to start on *each* tab and show the other one never arrives - starting only
    // on Commands would pass a sloppy matcher that routed everything to Commands.
    let mut commands = Stage::library(library_with_entries());
    for digit in ['0', '3', '4', '9'] {
        commands.press_ctrl(digit);
        assert_eq!(
            commands.state().tab,
            Tab::Commands,
            "Ctrl+{digit} is bound by neither app, so it must not reach Secrets"
        );
    }

    let mut secrets = Stage::library(Library::with_entries(
        Tab::Secrets,
        &["synthetic deploy token"],
    ));
    for digit in ['0', '3', '4', '9'] {
        secrets.press_ctrl(digit);
        assert_eq!(
            secrets.state().tab,
            Tab::Secrets,
            "and from the other tab the same unbound chord must not arrive there either"
        );
    }
}

#[test]
fn the_chords_that_wait_on_an_editor_do_nothing_yet() {
    // `Ctrl+N` (new entry) and `Ctrl+S` (save the open edit) are branches of the shipped window
    // handler with no native twin, because there is no editor and no draft to save. Asserted inert
    // rather than left out: this is the row that stops either chord being mistaken for ported later,
    // and the reason `ui.rs` still lists them as unbuilt.
    let mut stage = Stage::library(library_with_entries());
    for key in ['n', 's'] {
        stage.press_ctrl(key);
        assert_eq!(stage.state().tab, Tab::Commands);
        assert!(
            stage.state().pending_delete.is_none(),
            "Ctrl+{key} must not reach as far as opening or closing anything"
        );
        assert!(
            !stage.has("dialog"),
            "and in particular it must not raise the one modal the shell does have"
        );
    }
}

#[test]
fn the_chord_works_while_a_tab_button_holds_the_keyboard() {
    // The shipped binding is on the window, so it fires wherever the keyboard is. Rune's order is
    // different in the two cases: with nothing focused `AppSpec::on_key` gets first refusal, and
    // with something focused it is asked after that element declines the key
    // (`runtime/input/keys.rs`). This script lands on the second branch, which is the one a
    // window-level router is most likely to get wrong.
    let mut stage = Stage::library(library_with_entries());
    assert!(stage.press("tab-Commands"));
    assert_eq!(
        stage.focused_key().as_deref(),
        Some("tab-Commands"),
        "the script is meaningless unless the keyboard is on a control"
    );

    stage.press_ctrl('2');

    assert_eq!(
        stage.state().tab,
        Tab::Secrets,
        "a focused control must not swallow the window's chord"
    );
}

#[test]
fn a_confirmation_survives_a_tab_switch_under_it_and_still_names_its_own_record() {
    // The shipped window handler is global and `selectTab` does not clear `pendingDelete`, so a chord
    // pressed while a confirmation is open switches the tab underneath it and leaves the panel up.
    // That is the shipped reading, so it is pinned here rather than quietly improved on - and the
    // part that *would* be a defect is answered: the confirmation still names the record it was
    // opened on, and still removes exactly that one whatever tab is now showing.
    //
    // Worth re-reading when the list becomes repository-backed, because the pending record's kind and
    // the tab now showing will then name two different lists. It is asserted through the confirm
    // button rather than through the state field alone for exactly that reason.
    let mut stage = Stage::library(library_with_entries());
    assert!(stage.press("entry-1"));
    assert_eq!(
        stage
            .state()
            .pending_delete
            .as_ref()
            .map(|pending| pending.name.as_str()),
        Some("synthetic command two")
    );

    stage.press_ctrl('2');

    assert_eq!(stage.state().tab, Tab::Secrets);
    assert!(stage.has("dialog"), "the panel stays up over the new tab");
    assert_eq!(
        stage
            .state()
            .pending_delete
            .as_ref()
            .map(|pending| pending.name.as_str()),
        Some("synthetic command two"),
        "switching tabs must not re-point the confirmation at a row of the tab it switched to"
    );

    assert!(stage.press("dialog-confirm"));

    assert_eq!(
        stage.state().entries[0].name,
        "synthetic command one",
        "the record it named, not the one the newly shown tab happens to list first"
    );
    assert!(stage.state().pending_delete.is_none());
}

#[test]
fn every_library_footer_chip_names_a_binding_the_window_routes() {
    // The Library's half of the rule the Finder footer is held to: a chip is a promise, so the footer
    // may advertise no chord the shell does not route, and the chords it does advertise have to work
    // when they are pressed. That last part is what makes this a behaviour check instead of a string
    // comparison against a constant somebody could edit into a lie.
    let stage = Stage::library(library_with_entries());
    assert_eq!(
        stage.labels_starting_with("footer-"),
        vec![TAB_HINT.to_owned()],
        "every chip in the Library footer, each one a binding the window honours"
    );
    assert_eq!(
        advertised_digits(),
        vec!['1', '2'],
        "the chips' digits are the two the router is bound to, in tab order"
    );

    // And each of them really does move the tab, in both directions, from a window the tests did not
    // pre-arrange: `Commands` is where the app starts and `Secrets` is where the other chord goes.
    let mut stage = Stage::library(Library::with_entries(Tab::Secrets, &[]));
    stage.press_ctrl('1');
    assert_eq!(
        stage.state().tab,
        Tab::Commands,
        "`{TAB_HINT}` promises this"
    );
    stage.press_ctrl('2');
    assert_eq!(
        stage.state().tab,
        Tab::Secrets,
        "and the same in the other direction"
    );
}
