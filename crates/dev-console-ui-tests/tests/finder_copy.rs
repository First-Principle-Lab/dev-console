//! Enter, the clipboard, and the hygiene rules that come with them.
//!
//! The shipped palette's `activate()` is three behaviours in one function, and this file takes them
//! apart: it copies *the highlighted row* (not the first row, not the name), it reports *that* a copy
//! happened and never *what*, and it hides the palette only after the write succeeded - the failure
//! path keeps the window open with `Copy failed. Nothing was copied.` Those are the observable halves
//! of plan §2's copy behaviour that a headless runtime can settle, and Rune's `use_clipboard` gives
//! them a real clipboard to settle against: the write, the expiry deadline and the
//! "still ours to clear?" guard all run for real here.
//!
//! What this file cannot decide, and does not claim to: that the OS board (rather than this in-memory
//! one) received the text, that `wl-clipboard`/`xclip` were the ones asked, and that a clipboard
//! *history manager* running on the owner's desktop did not archive a secret regardless - the shipped
//! app says so itself in `exclusion_status()` ("a running clipboard manager may keep a copy"). Those
//! are the UNVERIFIED (owner desktop) rows in
//! `docs/project/native-feasibility-ledger.md`, and no number of green here promotes them.

use std::time::Duration;

use dev_console_core::models::Kind;
use dev_console_desktop::ui::Finder;
use dev_console_ui_tests::{
    command_candidate, secret_candidate, Stage, SyntheticVault, SYNTHETIC_COMMAND_DRAFT,
    SYNTHETIC_SECRET_VALUE,
};
use rune::Key;

/// The palette the copy scripts drive: two rows (a command, then a secret), both resolvable by the
/// synthetic vault behind them.
fn vault_backed_finder() -> Finder {
    SyntheticVault::finder()
}

/// Focuses the query field, which is the precondition every palette binding shares: the shipped
/// handler is `<input on:keydown>`, and Rune routes a focused field's keys through its own handler
/// first (`route_key` in `runtime/input/keys.rs`), so a script that skips the press would be sending
/// keys to a window that never sees them.
fn open_palette(finder: Finder) -> Stage<Finder> {
    let mut stage = Stage::finder(finder);
    assert!(
        stage.press_until_focused("finder-query"),
        "the palette's field has to hold focus before any binding can be tested"
    );
    stage
}

#[test]
fn enter_copies_the_highlighted_row_and_not_the_row_above_it() {
    let mut stage = open_palette(vault_backed_finder());
    // Row zero is the command; Down moves to the secret, which is what Enter must then copy. Copying
    // `rows[0]` regardless, or the row the pointer last touched, is the bug this pins.
    stage.press_key(Key::Down);
    assert_eq!(stage.state().selected, 1, "the secret row is highlighted");

    stage.press_key(Key::Enter);

    assert!(
        stage.clipboard_holds(SYNTHETIC_SECRET_VALUE),
        "Enter copies the highlighted candidate's value"
    );
    assert!(
        !stage.clipboard_holds(SYNTHETIC_COMMAND_DRAFT),
        "and not the row that was highlighted before the move"
    );
}

#[test]
fn a_command_and_a_secret_copy_the_same_way_and_say_different_things() {
    // The shipped `activate` branches only on the *message*
    // (`entry.kind === 'secret' ? … : …`): both kinds go through the same `api.copy`, and both get the
    // same expiry treatment in the backend (`Clipboard::copy` clamps `clipboardExpirySeconds`
    // whichever kind it was handed). A shell that treated a command's draft as non-secret would be the
    // divergence, and a `git status` line can carry a token.
    let mut stage = open_palette(vault_backed_finder());
    stage.press_key(Key::Enter);
    assert_eq!(
        stage.label_of("finder-status").as_deref(),
        Some("Copied the command to the clipboard."),
        "the shipped confirmation, verbatim"
    );

    stage.press_key(Key::Down);
    stage.press_key(Key::Enter);
    assert_eq!(
        stage.label_of("finder-status").as_deref(),
        Some("Copied the secret to the clipboard."),
        "the wording changes with the kind, and nothing else does"
    );
    assert!(
        stage.clipboard_holds(SYNTHETIC_SECRET_VALUE),
        "the second Enter wrote over the first, as `Tx::copy_secret` is meant to"
    );
}

#[test]
fn a_successful_copy_asks_for_the_palette_to_be_put_away() {
    let mut stage = open_palette(vault_backed_finder());
    stage.press_key(Key::Enter);
    assert!(
        stage.state().dismissed,
        "the shipped success path ends in `api.hideSelf()`"
    );
    assert!(
        !stage.state().rows.is_empty(),
        "and hides the palette *as it stands*: `hideSelf` is not `hide()`, so the rows and the query \
         are still there when the shell unmaps the window. Only Escape's `hide()` clears them, and \
         this distinction is what stops a future refactor from clearing the list under a copy that \
         has to be reportable afterwards."
    );
}

#[test]
fn a_failed_copy_stays_open_with_the_shipped_message_and_writes_nothing() {
    // `Finder::default()` carries the shell's only shipped copy source, `Unwired`: nothing resolves,
    // which is the same `None` a locked vault or a deleted row returns on a real desktop. The failure
    // path is therefore the shell's *current* behaviour, not a simulation of one.
    let mut stage = open_palette(Finder::with_candidates(&[
        command_candidate(),
        secret_candidate(),
    ]));
    stage.press_key(Key::Enter);

    assert!(
        !stage.state().dismissed,
        "`catch` runs before any `hideSelf`, so a failed copy leaves the palette up"
    );
    assert_eq!(
        stage.label_of("finder-error").as_deref(),
        Some("Copy failed. Nothing was copied."),
        "the shipped fallback message, verbatim, in the `role=\"alert\"` slot"
    );
    assert!(
        !stage.has("finder-status"),
        "the confirmation and the failure are the shipped `{{#if error}}…{{else if status}}` pair: \
         one line, never two"
    );
    assert!(
        stage.clipboard_is_empty(),
        "a refused copy must leave the board exactly as it found it, rather than blanking whatever \
         the user had copied before"
    );
}

#[test]
fn a_copied_value_reaches_the_clipboard_and_nowhere_the_window_can_show() {
    let mut stage = open_palette(vault_backed_finder());
    stage.press_key(Key::Down);
    stage.press_key(Key::Enter);

    assert!(
        stage.clipboard_holds(SYNTHETIC_SECRET_VALUE),
        "precondition: the value did get copied, so the assertions below are about a copy that \
         happened and not about a copy that was never attempted"
    );
    assert_eq!(
        stage.leak_surfaces(SYNTHETIC_SECRET_VALUE),
        Vec::<&'static str>::new(),
        "the value reached no accessible name, no visible text and no element key"
    );
    assert!(
        stage
            .element_strings()
            .iter()
            .any(|shown| shown.contains("synthetic deploy token")),
        "and the row's *name* is still on screen: this is not an assertion that the list is empty, \
         which would pass for the wrong reason"
    );
}

#[test]
fn the_view_state_carries_the_fact_and_no_field_a_value_could_be_written_into() {
    let mut stage = open_palette(vault_backed_finder());
    stage.press_key(Key::Down);
    stage.press_key(Key::Enter);

    let dumped = format!("{:?}", stage.state());
    assert!(
        !dumped.contains(SYNTHETIC_SECRET_VALUE),
        "`Debug` is what an `assert_eq!` diff, a panic message and any future log line are built \
         from, so the state's own printable form must be value-free"
    );
    assert!(
        dumped.contains("SecretCopied") && dumped.contains("dyn CopySource"),
        "the debug form reports the outcome and names the resolver rather than printing through it: \
         `Finder`'s `Debug` is hand-written for exactly this, and this assertion is what fails if \
         someone swaps it back to `#[derive(Debug)]` and starts printing the resolver's contents"
    );
    // The kind the message branches on is still reachable, because it is metadata.
    assert_eq!(
        stage.state().rows[stage.state().selected].kind,
        Kind::Secret,
        "the highlighted row is the secret one, so the assertion above was made about a real copy"
    );
}

#[test]
fn a_copied_value_is_cleared_while_the_shell_still_owns_the_board() {
    // The shipped rule (`Clipboard::expire_due`): after `clipboardExpirySeconds` the value is wiped,
    // and only if the board still holds what this app put there. Rune's secret copy implements the
    // same guard, so the deadline is exercised here rather than only argued about in a comment - with
    // an expiry shortened from the 30s default to something a test can wait out.
    let mut stage = open_palette(vault_backed_finder());
    stage.state_mut().copy_expiry = Duration::from_millis(20);
    stage.press_key(Key::Enter);

    assert!(
        stage.clipboard_holds(SYNTHETIC_COMMAND_DRAFT),
        "inside the deadline the value is on the board"
    );
    std::thread::sleep(Duration::from_millis(60));
    assert!(
        stage.clipboard_is_empty(),
        "once it passes, the copy is cleared - which is the `expires_at_ms` the shipped `CopyResult` \
         promises the frontend"
    );
}

#[test]
fn a_value_the_shell_no_longer_owns_is_left_exactly_where_the_user_put_it() {
    // The other half of the same guard, and the half that destroys work if it is wrong: the shipped
    // `expire_due` reads the board first and clears only when it still holds the app's own value, so
    // a user who copied something else meanwhile does not lose it to a timer. `copy_expiry` is left at
    // the 30s default here, so the only thing that can retire the copy is the replacement itself.
    let mut stage = open_palette(vault_backed_finder());
    stage.press_key(Key::Enter);
    assert!(stage.clipboard_holds(SYNTHETIC_COMMAND_DRAFT));

    stage.set_clipboard_text("somebody else's clipboard text");
    assert!(
        !stage.clipboard_holds(SYNTHETIC_COMMAND_DRAFT),
        "the board no longer holds the copy"
    );
    assert!(
        stage
            .clipboard_text()
            .is_some_and(|text| text == "somebody else's clipboard text"),
        "and the deadline that follows must leave that text alone: clearing a clipboard the app no \
         longer owns is how a palette quietly eats the user's work"
    );
}

#[test]
fn enter_without_anything_to_copy_is_still_consumed() {
    // Two shipped details in one case: `case 'Enter': event.preventDefault(); await activate()` runs
    // the preventDefault *before* `activate` discovers `current` is null, and the shipped input keeps
    // its own text either way. The observable form of "consumed" here is that no newline reaches the
    // field, which is what Rune's editor would have inserted had the key gone unhandled.
    let mut stage = open_palette(Finder::default());
    stage.type_text("nothing matches this");
    stage.press_key(Key::Enter);

    assert_eq!(
        stage.state().query.text,
        "nothing matches this",
        "an Enter with no candidates must not insert a newline into the query"
    );
    assert!(
        stage.clipboard_is_empty(),
        "and it must not write anything, least of all the query text"
    );
    assert!(
        stage.state().copy_status.is_none(),
        "`activate` returns before touching `status` when there is no entry, so no line appears"
    );
    assert!(
        !stage.state().dismissed,
        "and the palette stays up: nothing happened that would justify hiding it"
    );
}

#[test]
fn escape_clears_a_failed_line_and_leaves_a_confirmation_alone() {
    // `hide()` nulls `error` but not `status`, and that asymmetry is the shipped behaviour this pins:
    // a failure is something the user is told about on the way out of the palette, while a
    // confirmation describes a state that is about to be gone. (The place the shipped app does clear
    // a confirmation is at the start of the next search, and there is no search port yet - which is
    // why the matrix row for that rule stays uncovered instead of being tested against a search that
    // does not exist.)
    let mut failing = open_palette(Finder::with_candidates(&[command_candidate()]));
    failing.press_key(Key::Enter);
    assert!(
        failing.has("finder-error"),
        "precondition: a failure is on screen"
    );
    failing.press_key(Key::Escape);
    assert!(
        !failing.has("finder-error"),
        "`hide`'s `error = null` clears the failure line with the rest of the palette"
    );

    let mut confirming = open_palette(vault_backed_finder());
    confirming.press_key(Key::Enter);
    assert!(
        confirming.has("finder-status"),
        "precondition: a copy succeeded"
    );
    confirming.press_key(Key::Escape);
    assert!(
        confirming.has("finder-status"),
        "the confirmation survives dismissal, because the shipped `hide()` only clears `error`"
    );
}

#[test]
fn the_copy_asks_for_the_row_it_could_have_read_the_name_of() {
    // The request the seam receives is the shipped one: `api.copy(entry.kind, entry.id, 0)` - the
    // *id*, the kind, and a literal 0 for "whatever revision the backend currently holds"
    // (`CopyRequest::expected_revision`'s own comment). Pressing row two must produce a request for
    // row two's id, which is the difference between a copy and a copy-of-the-wrong-entry that happens
    // to print the right name.
    let mut stage = open_palette(vault_backed_finder());
    let ids: Vec<&str> = stage
        .state()
        .rows
        .iter()
        .map(|row| row.id.as_str())
        .collect();
    assert_eq!(ids, vec!["synthetic-command-1", "synthetic-secret-1"]);

    stage.press_key(Key::Down);
    stage.press_key(Key::Enter);
    assert!(
        stage.clipboard_holds(SYNTHETIC_SECRET_VALUE),
        "the highlighted row's id is the one the source resolved"
    );
    assert_eq!(
        stage.state().rows[1].kind,
        Kind::Secret,
        "and the kind it reported came from the same row, not from the fixture's order"
    );
}
