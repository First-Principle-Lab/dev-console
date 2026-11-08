//! The Finder palette's keyboard routing, driven with injected keys against the real element tree.
//!
//! Plan §2 asks for the palette's keys to be behaviour, not prose, and this container can decide
//! that: Rune routes a `Runtime::key` event through the same path a compositor's key event takes,
//! and a scripted press gives the query field keyboard focus. What these scripts therefore pin is
//! the three things that make the port faithful to `src/windows/Finder.svelte` rather than merely
//! similar:
//!
//! - the bindings live on the **query field**, because a focused Rune text field is the only thing
//!   that sees its own keys before its editor does (`route_key` in `runtime/input/keys.rs` asks the
//!   focused input first, then the editor, then the window's `on_key` - which is why a root
//!   `El::on_key` handler would be dead code here, and why the shipped handler is on the `<input>`);
//! - consuming a key has to look like `preventDefault`: the caret does not move, the field does not
//!   blur on Escape, and the text survives;
//! - what the palette does *not* bind has to stay with the editor. `Space` is the deliberate
//!   divergence recorded in `ui.rs`: the shipped handler binds it beside `Enter`, but Rune's IME
//!   composition flag is private and its composition guard holds only `Enter`/`Escape` back, so
//!   binding Space would eat a space typed mid-composition.
//!
//! Nothing here observes a key that came from a device, so none of it says the compositor delivers
//! the palette's shortcut; that row stays UNVERIFIED (owner desktop) in the feasibility ledger.

use dev_console_desktop::ui::Finder;
use dev_console_ui_tests::{finder_with_rows, Stage};
use rune::Key;

/// The palette as it is whenever a user is typing: rows up, project chip shown, and the query field
/// holding keyboard focus - which the press below establishes rather than assumes, since every key
/// in this file is only meaningful while the field holds focus.
fn palette() -> Stage<Finder> {
    let mut stage = Stage::finder(finder_with_rows());
    assert!(
        stage.press_until_focused("finder-query"),
        "the query field has to hold focus for these keys to be the palette's rather than the \
         window's; a failure here means Rune stopped giving a pressed field focus, not that a \
         binding broke"
    );
    stage
}

#[test]
fn arrows_move_the_highlight_and_never_reach_the_caret() {
    let mut stage = palette();
    stage.type_text("git");
    let caret = stage.state().query.anchor;
    assert_eq!(caret, 3, "the typed text put the caret where the text ends");

    for expected in [1usize, 2] {
        stage.press_key(Key::Down);
        assert_eq!(stage.state().selected, expected, "Down steps the highlight");
    }
    stage.press_key(Key::Up);
    assert_eq!(stage.state().selected, 1, "Up steps it back");

    assert_eq!(
        (
            stage.state().query.anchor,
            stage.state().query.text.as_str()
        ),
        (caret, "git"),
        "consuming an arrow means the caret never moved: if the palette stopped routing arrows to \
         the field's editor, a Down would move the caret to the text's end (or its start) and this \
         assertion is what notices"
    );
    assert_eq!(
        stage.state().query.anchor,
        3,
        "the caret is still where the typed text left it"
    );
}

#[test]
fn the_highlight_clamps_at_both_ends_instead_of_wrapping() {
    let mut stage = palette();
    for _ in 0..6 {
        stage.press_key(Key::Down);
    }
    assert_eq!(
        stage.state().selected,
        2,
        "`nextSelection` clamps to the last row rather than wrapping to the first"
    );
    for _ in 0..6 {
        stage.press_key(Key::Up);
    }
    assert_eq!(stage.state().selected, 0, "and clamps at the top too");
    assert_eq!(
        stage.state().query.text,
        "",
        "arrows with nothing typed stay consumed and edit nothing"
    );
}

#[test]
fn arrows_do_nothing_when_there_is_nothing_to_highlight() {
    let mut stage = Stage::finder(Finder::default());
    assert!(stage.press_until_focused("finder-query"));
    stage.type_text("no matches yet");
    stage.press_key(Key::Down);
    stage.press_key(Key::Up);
    assert_eq!(stage.state().selected, 0);
    assert_eq!(
        stage.state().query.text,
        "no matches yet",
        "an empty palette still consumes the arrows - the shipped handler preventDefaults them \
         before it ever looks at the row list - but it cannot move a highlight that is not there"
    );
    assert!(!stage.state().dismissed, "arrows never dismiss the palette");
}

#[test]
fn a_highlight_that_fell_out_of_a_shortened_list_restarts_where_it_was_heading() {
    let mut stage = palette();
    stage.press_key(Key::Down);
    stage.press_key(Key::Down);
    assert_eq!(stage.state().selected, 2);
    // The search port shrinks the result list under the highlight; no key here can, so the fixture
    // says so directly (`Stage::state_mut` exists for exactly this and for nothing else).
    stage.state_mut().rows = vec!["git status".into()];
    stage.press_key(Key::Down);
    assert_eq!(
        stage.state().selected,
        0,
        "`nextSelection` restarts at the first row when the movement was downward"
    );
    stage.state_mut().rows = vec!["git status".into(), "cargo test".into()];
    stage.state_mut().selected = 7;
    stage.press_key(Key::Up);
    assert_eq!(
        stage.state().selected,
        1,
        "…and at the last row when it was upward, which is what keeps an out-of-range highlight \
         from stranding the keyboard at index 0"
    );
}

#[test]
fn tab_writes_the_highlighted_row_into_the_field() {
    let mut stage = palette();
    stage.type_text("git");
    stage.press_key(Key::Tab);
    assert_eq!(
        stage.state().query.text,
        "git status",
        "`tabCompletion` appends the remainder of a prefix match"
    );
    assert_eq!(
        stage.state().query.anchor,
        10,
        "the caret follows the completed text, which is what makes a second typed character land \
         at the end rather than in the middle"
    );
    assert_eq!(
        focused_after(&mut stage).as_deref(),
        Some("finder-query"),
        "Tab is consumed rather than falling to focus traversal: the shipped palette binds it so \
         that the key completes, and the caret's home is the same field it was in"
    );

    stage.press_key(Key::Tab);
    assert_eq!(
        stage.state().query.text,
        "git status",
        "a field that already holds the highlighted name has nothing left to complete"
    );
}

#[test]
fn tab_replaces_the_whole_query_when_the_match_is_fuzzy() {
    let mut stage = palette();
    stage.press_key(Key::Down);
    assert_eq!(stage.state().selected, 1, "cargo test is highlighted");
    stage.type_text("arg");
    stage.press_key(Key::Tab);
    assert_eq!(
        stage.state().query.text,
        "cargo test",
        "`tabCompletion` returns the full name for a fuzzy match too, so the query is replaced \
         rather than extended"
    );
}

#[test]
fn escape_clears_the_palette_without_taking_focus_out_of_the_field() {
    let mut stage = palette();
    stage.type_text("git");
    stage.press_key(Key::Down);
    stage.press_key(Key::Escape);
    let state = stage.state();
    assert_eq!(state.query.text, "", "`hide` clears the query");
    assert!(state.rows.is_empty(), "`hide` clears the results");
    assert_eq!(state.selected, 0, "and the selection with them");
    assert!(
        state.dismissed,
        "and asks the shell to take the palette out of the way"
    );
    assert_eq!(
        focused_after(&mut stage).as_deref(),
        Some("finder-query"),
        "Escape is consumed before the editor can act on it: Rune's field blurs on an unhandled \
         Escape, so focus still being here is the difference between the palette dismissing itself \
         and the field quietly losing focus"
    );
    assert_eq!(
        stage.state().query.anchor,
        0,
        "the caret sits in the emptied field, ready for the next summon"
    );

    stage.press_key(Key::Escape);
    assert!(
        stage.state().dismissed,
        "a second Escape dismisses an already-empty palette rather than tripping over it"
    );
}

#[test]
fn keys_the_palette_does_not_bind_stay_with_the_editor() {
    let mut stage = palette();
    stage.type_text("a b");
    assert_eq!(
        stage.state().query.text, "a b",
        "Space inserts: the shipped handler binds Space beside Enter, and this port deliberately \
         does not, because Rune's IME composition guard holds Enter and Escape back mid-composition \
         but not Space, so a bound Space would swallow one typed while composing an IME draft"
    );
    stage.press_key(Key::Backspace);
    assert_eq!(stage.state().query.text, "a ");
    assert!(
        !stage.state().dismissed,
        "nothing above asked to hide the palette"
    );
    assert_eq!(
        stage.state().selected,
        0,
        "typing never moves the highlight"
    );
}

#[test]
fn the_footer_advertises_only_bindings_that_exist() {
    let stage = Stage::finder(finder_with_rows());
    let mut hints = stage.labels_starting_with("footer-");
    hints.sort();
    assert_eq!(
        hints,
        vec![
            "Esc dismiss".to_owned(),
            "Tab complete".to_owned(),
            "Up+Down select".to_owned()
        ],
        "every hint in the footer has to be a binding the palette honours, in the shipped palette's \
         words"
    );
    let joined = hints.join(" ");
    for advertised in [
        "Enter", "Space", "Ctrl+O", "Ctrl+P", "copy", "details", "open",
    ] {
        assert!(
            !joined.contains(advertised),
            "the footer offers {advertised:?}, which this shell does not bind: `Enter`/`Space` copy \
             waits on the clipboard seam and `Ctrl+O` details on the details panel, and a hint is a \
             promise the window has to keep"
        );
    }
}

/// Focus, re-read after a frame, so a check cannot pass on a focus change the next commit undid.
fn focused_after<S: 'static>(stage: &mut Stage<S>) -> Option<String> {
    stage.step(0.0);
    stage.focused_key()
}
