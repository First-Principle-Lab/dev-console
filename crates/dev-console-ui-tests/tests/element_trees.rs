//! Scripted element-tree and state-transition checks over the two windows.
//!
//! Everything here runs `rune::Runtime` without a display. It settles *structure* and *transitions*:
//! which keys exist for a state, whether a press reaches the handler that changes state, and whether a
//! state can produce elements it should not. It does not settle appearance - hover, focus rings, the
//! focus trap, Escape, text rasterisation, shadows, or the mapped size on Hyprland - all of which plan
//! §5 reserves for a real session and which stay UNVERIFIED here.

use dev_console_desktop::identity::{FINDER_SIZE, LIBRARY_MIN_SIZE, LIBRARY_SIZE};
use dev_console_desktop::ui::{Finder, Library, Tab};
use dev_console_ui_tests::{
    FINDER_KEYS, LIBRARY_KEYS, Stage, finder_with_rows, library_with_entries,
};

#[test]
fn an_empty_finder_shows_the_query_field_and_no_rows_or_context_chip() {
    let stage = Stage::finder(Finder::default());
    let present = stage.present_from(FINDER_KEYS);
    assert!(present.contains(&"finder-window"));
    assert!(present.contains(&"finder-query"));
    assert!(present.contains(&"finder-empty"), "a Finder with no candidates still needs a body");
    assert!(!present.contains(&"finder-rows"), "no candidates, no result list");
    assert!(
        !present.contains(&"finder-context"),
        "an undetected project folder must not draw an empty chip"
    );
    assert!(!stage.has("finder-row-0"));
}

#[test]
fn a_finder_with_candidates_lists_exactly_those_rows() {
    let stage = Stage::finder(finder_with_rows());
    assert!(stage.has("finder-rows"));
    assert!(stage.has("finder-context"));
    assert!(!stage.has("finder-empty"));
    for index in 0..3 {
        assert!(stage.has(&format!("finder-row-{index}")), "row {index} is missing");
    }
    assert!(!stage.has("finder-row-3"), "the tree must not invent a fourth row");
}

#[test]
fn pressing_a_result_selects_it() {
    let mut stage = Stage::finder(finder_with_rows());
    assert_eq!(stage.state().selected, 0, "the first candidate starts highlighted");
    assert!(stage.press("finder-row-2"), "the row has to be pressable where it is drawn");
    assert_eq!(stage.state().selected, 2);

    // And the highlight follows the state on the next frame: press row 0 back.
    assert!(stage.press("finder-row-0"));
    assert_eq!(stage.state().selected, 0);
}

/// The rule `Finder::select` encodes - a press cannot select a row that is not in the list - asserted
/// through the tree rather than the method, so a view that invented a row would be caught.
#[test]
fn pressing_beyond_the_last_row_changes_nothing() {
    let mut stage = Stage::finder(finder_with_rows());
    assert!(!stage.press("finder-row-7"));
    assert_eq!(stage.state().selected, 0);
    // Selecting the already-selected row is not a change either.
    assert!(stage.press("finder-row-0"));
    assert_eq!(stage.state().selected, 0);
}

#[test]
fn the_finder_measures_the_surface_it_was_asked_to_fill() {
    // Geometry of the *layout*, at the logical size the shell passes to the window: proof that the
    // root fills the viewport, not that the compositor mapped the window at that size. The second
    // number is the scale the runtime measured the tree at.
    let stage = Stage::finder(finder_with_rows());
    let ([_, _, width, height], _) = stage.box_of("finder-window").expect("the window root is laid out");
    assert!(
        (width - FINDER_SIZE.0).abs() < 1.0 && (height - FINDER_SIZE.1).abs() < 1.0,
        "the Finder root should fill the {FINDER_SIZE:?} it was built for, got {width}x{height}"
    );
}

#[test]
fn an_empty_library_shows_the_empty_state_and_no_list() {
    let stage = Stage::finder(Finder::default());
    drop(stage);

    let empty = Stage::library(Library::default());
    let present = empty.present_from(LIBRARY_KEYS);
    assert!(present.contains(&"empty-state"));
    assert!(!present.contains(&"entry-list"));
    assert!(present.contains(&"tab-Commands"));
    assert!(present.contains(&"tab-Secrets"));
    assert!(present.contains(&"vault-status"));
    assert!(!empty.has("entry-0"));
}

#[test]
fn a_library_with_entries_lists_exactly_those_rows() {
    let stage = Stage::library(library_with_entries());
    assert!(stage.has("entry-list"));
    assert!(!stage.has("empty-state"));
    assert!(stage.has("entry-0"));
    assert!(stage.has("entry-1"));
    assert!(!stage.has("entry-2"));
}

#[test]
fn switching_tabs_changes_the_state_the_list_is_built_from() {
    let mut stage = Stage::library(library_with_entries());
    assert_eq!(stage.state().tab, Tab::Commands);
    assert!(stage.press("tab-Secrets"), "the tab has to be reachable where it is drawn");
    assert_eq!(stage.state().tab, Tab::Secrets);
    assert!(stage.press("tab-Commands"));
    assert_eq!(stage.state().tab, Tab::Commands);
}

/// The rows list survives a tab switch because the tab is state, not a rebuilt subtree - the property
/// the real Library depends on when it swaps ranked lists.
#[test]
fn a_tab_switch_keeps_the_list_present() {
    let mut stage = Stage::library(library_with_entries());
    stage.press("tab-Secrets");
    assert!(stage.has("entry-list"));
    assert!(stage.has("entry-0"));
}

/// The placeholder row handler removes the row it is pressed on; the scripted test pins that a press
/// reaches *the* row, so a milestone-4 handler that opens the wrong entry fails here rather than on a
/// desktop.
#[test]
fn pressing_a_row_reaches_that_rows_handler() {
    let mut stage = Stage::library(library_with_entries());
    assert!(stage.press("entry-1"));
    assert_eq!(stage.state().entries, vec!["synthetic command one"]);
    assert!(!stage.has("entry-1"), "the tree must drop the removed row");
    assert!(stage.has("entry-0"));
}

#[test]
fn the_library_measures_the_surface_it_was_asked_to_fill() {
    let stage = Stage::library(library_with_entries());
    let ([_, _, width, height], _) = stage.box_of("library-window").expect("the window root is laid out");
    assert!(
        (width - LIBRARY_SIZE.0).abs() < 1.0 && (height - LIBRARY_SIZE.1).abs() < 1.0,
        "the Library root should fill {LIBRARY_SIZE:?}, got {width}x{height}"
    );
    // The minimum is a window-frame property, not a layout one; the tree cannot settle whether the
    // compositor honours it, which is why it is asserted as a specification in `identity`.
    assert_eq!(LIBRARY_MIN_SIZE, (760.0, 520.0));
}

/// Plan §3: the Finder receives search metadata, copy, context dismissal and navigation - and *no*
/// secret reveal, edit, execute, archive or recovery operation. With no domain services wired yet the
/// capability set is empty by construction, but the element keys are the surface a milestone-4 commit
/// would add a reveal button to, so the guard is written against them.
#[test]
fn no_finder_element_offers_a_secret_capability() {
    let forbidden: &[&str] = &["reveal", "secret-value", "copy-secret", "edit", "execute", "run", "archive", "recover", "vault"];
    for key in FINDER_KEYS {
        let lowered = key.to_lowercase();
        for word in forbidden {
            assert!(!lowered.contains(word), "the Finder exposes {key}, which reads like a {word} capability");
        }
    }
    // The two states a Finder can be in, checked the same way.
    for stage in [Stage::finder(Finder::default()), Stage::finder(finder_with_rows())] {
        for word in forbidden {
            assert!(
                !stage.present_from(FINDER_KEYS).iter().any(|key| key.to_lowercase().contains(word)),
                "a Finder state exposes a {word}-shaped control"
            );
        }
    }
}

/// The same guard on the shell's own vocabulary: the only thing another process can ask for is a
/// window. `instance::Intent::ALL` is the list a reviewer can extend by mistake, so it is asserted
/// closed here as well as in `dev-console-desktop`.
#[test]
fn the_control_channel_knows_only_two_intents() {
    use dev_console_desktop::instance::Intent;
    assert_eq!(Intent::ALL.len(), 2);
    assert!(matches!(Intent::ALL[0], Intent::Library));
    assert!(matches!(Intent::ALL[1], Intent::Finder));
    for flag in ["--reveal", "--execute", "--unlock", "--import", "--lock", "--copy"] {
        assert!(Intent::from_flag(flag).is_none(), "{flag} must not be a wakeup intent");
    }
}

/// The desktop crate holds no domain surface at all yet, which is what keeps "the Finder cannot reveal
/// a secret" true by construction rather than by care. Asserted against the view state types: they have
/// no place to put a secret value.
#[test]
fn no_window_state_can_hold_a_secret_value() {
    let finder = Finder {
        context: "synthetic-project".into(),
        ..Finder::with_rows(&["git status"])
    };
    // A Finder is a query field, candidate *labels*, a highlighted index and a folder name.
    assert_eq!(finder.rows.len(), 1);
    assert_eq!(finder.selected, 0);
    assert_eq!(finder.context, "synthetic-project");
    assert!(finder.query.text.is_empty());

    // A Library is a tab, entry *titles* and a lock flag. Nothing in either type can carry a value,
    // a key or command output, so `Debug`-formatting a state cannot print one either.
    let library = Library { entries: vec!["synthetic command".into()], ..Library::default() };
    let debug = format!("{library:?} {finder:?}");
    for marker in ["BEGIN PRIVATE", "password=", "token", "vault-key"] {
        assert!(!debug.to_lowercase().contains(&marker.to_lowercase()), "a state carries {marker:?}");
    }
    // And the only secret-adjacent field is the *status* of the lock, never a value.
    assert!(!library.vault_unlocked);
}
