//! Real X11 clipboard behaviour for the secret-copy path.
//!
//! These tests drive a genuine X selection through `xclip`, so they are `#[ignore]`d by
//! default and are run by `scripts/test-clipboard.sh` inside Xvfb. Run them single-threaded:
//! the X selection is one shared resource per display.
//!
//! They prove the parts that matter for C15: a copied secret is actually pasteable, the app
//! only ever clears content it still owns, and the expiry ticker really retracts it. They do
//! not prove exclusion from a clipboard history manager - no manager runs here, and the app
//! claims `supported: false` for exactly that reason.

use dev_console_lib::clipboard::{exclusion_status, read_clipboard, write_clipboard, Clipboard};
use dev_console_lib::models::Kind;

fn synthetic(label: &str) -> String {
    format!("dev-console-{label}-{}", uuid::Uuid::new_v4())
}

/// There is deliberately no silent skip here: an ignored test that reports `ok` without ever
/// touching an X selection would turn a missing environment into a false pass. These tests are
/// `#[ignore]`d, so the only way to run them is on purpose - and running them on purpose without
/// a display must say so loudly. `scripts/test-clipboard.sh` is what supplies Xvfb, and it exits
/// 77 (UNVERIFIED) when Xvfb is missing rather than pretending.
fn require_display() {
    if std::env::var_os("DISPLAY").is_none() {
        panic!("no DISPLAY: these tests must run through scripts/test-clipboard.sh (which reports UNVERIFIED rather than passing when Xvfb is unavailable)");
    }
}

#[test]
#[ignore = "needs a real X display and xclip"]
fn a_copied_secret_is_pasteable_and_is_cleared_when_the_vault_locks() {
    require_display();
    let clip = Clipboard::new();
    let value = synthetic("secret");
    let result = clip
        .copy(Kind::Secret, "record-1", &value, 5)
        .expect("copy to the X selection");
    assert_eq!(result.id, "record-1");
    assert_eq!(result.kind, Kind::Secret);
    assert!(result.expires_at_ms > 0);
    assert!(
        !result.excluded,
        "the app must not claim history exclusion it cannot provide"
    );
    assert_eq!(read_clipboard().expect("paste"), value);

    assert_eq!(
        clip.clear_owned(),
        1,
        "locking clears a copy the app still owns"
    );
    assert_eq!(
        read_clipboard()
            .expect("paste after clear")
            .trim()
            .to_string(),
        String::new(),
        "the secret must not survive the clear"
    );
}

#[test]
#[ignore = "needs a real X display and xclip"]
fn the_app_never_clears_content_it_does_not_own() {
    require_display();
    let clip = Clipboard::new();
    let secret = synthetic("replaced");
    clip.copy(Kind::Secret, "record-2", &secret, 60)
        .expect("copy");

    let user_text = synthetic("user-copied");
    write_clipboard(&user_text).expect("user write");

    assert_eq!(
        clip.clear_owned(),
        0,
        "a value the user copied afterwards must survive a vault lock"
    );
    assert_eq!(read_clipboard().expect("paste"), user_text);
}

#[test]
#[ignore = "needs a real X display and xclip"]
fn the_expiry_ticker_retracts_the_copy_and_only_then() {
    require_display();
    let clip = Clipboard::new();
    let secret = synthetic("expiry");
    // copy() clamps the window to at least five seconds, which is the shortest honest test.
    clip.copy(Kind::Secret, "record-3", &secret, 5)
        .expect("copy");

    assert_eq!(clip.expire_due(), 0, "not due yet");
    assert_eq!(read_clipboard().expect("paste"), secret);

    std::thread::sleep(std::time::Duration::from_secs(7));
    assert_eq!(clip.expire_due(), 1, "the due copy must be retracted");
    assert!(read_clipboard()
        .expect("paste after expiry")
        .trim()
        .is_empty());
    assert_eq!(clip.expire_due(), 0, "nothing is pending any more");
}

#[test]
#[ignore = "needs a real X display and xclip"]
fn an_x11_session_reports_that_history_exclusion_is_unavailable() {
    require_display();
    let status = exclusion_status();
    assert!(!status.supported);
    assert_eq!(status.mechanism, "none");
    assert!(status.detail.contains("x-kde-passwordManagerHint"));
}
