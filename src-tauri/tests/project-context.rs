//! Manual harness: does project detection work against a live Hyprland session?
//!
//! Run through `bash scripts/test-project-context.sh` from the terminal you are looking at, at a
//! prompt inside a git checkout. The script checks that the focused window really is that
//! terminal, and that no shell of its own sits between the terminal and this test (which would be
//! the shell detection finds, proving nothing); it then hands the expected repository over in
//! `DEV_CONSOLE_EXPECT_ROOT`.
//!
//! This test takes the same two steps `windows::show_finder` takes - the focused window over the
//! socket, then the `/proc` walk - and compares the project with that repository. The script then
//! drives the running app itself, through the real palette open.
//!
//! Ignored by `cargo test`: with no Hyprland session the honest answer is always "no context", so
//! passing here would prove nothing.

use std::path::Path;
use std::time::{Duration, Instant};

use dev_console_lib::context::{focused_now, resolve};

#[test]
#[ignore = "needs a live Hyprland session and this terminal focused"]
fn the_focused_terminal_reports_its_project() {
    let expected = std::env::var("DEV_CONSOLE_EXPECT_ROOT").expect(
        "run this through scripts/test-project-context.sh, which sets DEV_CONSOLE_EXPECT_ROOT",
    );

    let started = Instant::now();
    let window = focused_now().expect("the compositor socket named no focused window");
    let focus_took = started.elapsed();
    let context = resolve(&window).expect(
        "no project detected: run scripts/test-project-context.sh from the terminal window you \
         are looking at, inside a git checkout",
    );
    let elapsed = started.elapsed();

    assert_eq!(
        context.root.to_string_lossy(),
        expected,
        "detection followed the focused terminal to a different folder than the one the harness \
         was started from"
    );
    assert_eq!(
        context.name,
        Path::new(&expected)
            .file_name()
            .expect("a repository has a name")
            .to_string_lossy(),
        "the name the palette would show"
    );
    // The palette waits on the focus read only; the rest runs beside it. Both are budgeted.
    assert!(
        focus_took < Duration::from_millis(5),
        "reading the focused window took {} µs; the palette waits on this part",
        focus_took.as_micros()
    );
    assert!(
        elapsed < Duration::from_millis(30),
        "detection took {} µs, over the 30 ms the palette is allowed to spend on it",
        elapsed.as_micros()
    );
    println!(
        "detected {} ({}) in {} µs, {} µs of it the focus read",
        context.root.display(),
        context.name,
        elapsed.as_micros(),
        focus_took.as_micros()
    );
}
