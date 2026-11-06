//! Manual harness: does project detection work against a live Hyprland session?
//!
//! Run it from the terminal you are actually looking at, inside a git checkout:
//! `bash scripts/test-project-context.sh`. It asks the real compositor which window has focus,
//! follows that terminal to the shell it started, and compares the project the detection reports
//! with the repository the harness itself was started in.
//!
//! Ignored by `cargo test`: with no Hyprland session the honest answer is always "no context", so
//! passing here would prove nothing. `scripts/test-project-context.sh` reports UNVERIFIED (77) for
//! exactly that case, which is why this is a harness and not a test.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use dev_console_lib::context::detect;

#[test]
#[ignore = "needs a live Hyprland session and this terminal focused"]
fn the_focused_terminal_reports_its_project() {
    let started = Instant::now();
    let context = detect().expect(
        "no project detected: run scripts/test-project-context.sh from the terminal window you \
         are looking at, inside a git checkout",
    );
    let elapsed = started.elapsed();

    let toplevel = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .expect("git is on PATH");
    let repository = String::from_utf8_lossy(&toplevel.stdout).trim().to_string();
    assert!(
        !repository.is_empty(),
        "the harness must run inside a git checkout"
    );

    assert_eq!(
        context.root.to_string_lossy(),
        repository,
        "detection followed the focused terminal to a different folder than the one the harness \
         runs in"
    );
    assert_eq!(
        context.name,
        Path::new(&repository)
            .file_name()
            .expect("a repository has a name")
            .to_string_lossy(),
        "the name the palette would show"
    );
    // The palette is presented beside this lookup, so the budget is not about correctness but
    // about the finder never feeling slower because of it.
    assert!(
        elapsed < Duration::from_millis(30),
        "detection took {} µs, over the 30 ms the palette is allowed to spend on it",
        elapsed.as_micros()
    );
    println!(
        "detected {} ({}) in {} µs",
        context.root.display(),
        context.name,
        elapsed.as_micros()
    );
}
