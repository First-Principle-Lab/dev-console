//! A synthetic terminal records argv and never executes the saved command.
use dev_console_ui_tests::{isolate, Profile};
use std::{os::unix::fs::PermissionsExt, path::PathBuf};

#[test]
fn installed_terminal_selection_is_saved_and_sudo_runs_only_after_approval() {
    isolate();
    let profile = Profile::new("native-terminal");
    let capture = PathBuf::from(profile.fixture.scratch_dir()).join("terminal-argv");
    let terminal = PathBuf::from(std::env::var_os("HOME").unwrap()).join("bin/ghostty");
    std::fs::write(
        &terminal,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            capture.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&terminal, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut library = profile.library();
    library.press("settings");
    assert!(library
        .state()
        .settings
        .as_ref()
        .unwrap()
        .terminals
        .iter()
        .any(|t| t.id == "ghostty"));
    library.apply(|state| state.choose_terminal("ghostty"));
    library.apply(|state| state.save_settings());
    assert_eq!(
        profile.hub.library.get_settings().terminal[0],
        terminal.to_str().unwrap()
    );
    assert!(library.state().settings.is_none());
    profile.command("sudo-synthetic", "sudo printf synthetic-terminal-only");
    library.apply(|state| state.load_list());
    library.press("run");
    assert!(
        !capture.exists(),
        "opening approval must not launch a terminal"
    );
    library.press("confirm-run");
    assert!(library.state().run_output.is_none());
    assert_eq!(
        library.state().status.as_deref(),
        Some("Launched in a terminal for sudo.")
    );
    for _ in 0..100 {
        if capture.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let argv = std::fs::read_to_string(capture).unwrap();
    assert!(argv.contains("sudo printf synthetic-terminal-only"));
}
