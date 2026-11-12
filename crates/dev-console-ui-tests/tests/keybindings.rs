//! Exercise the shipped native settings controls and routing over synthetic profiles.
use dev_console_ui_tests::{isolate, Profile};

#[test]
fn keyboard_settings_save_route_reload_discard_and_reset() {
    isolate();
    let profile = Profile::new("custom-keybindings");
    let mut library = profile.library();
    library.press_ctrl('k');
    library.type_text("default-search");
    assert_eq!(library.state().query.text, "default-search");

    assert!(library.press("settings"));
    assert!(library.press("settings-tab-keyboard"));
    assert!(library.press("shortcut-search"));
    library.press_ctrl('a');
    library.type_text("Ctrl+J");
    library.press("shortcut-cliProxy");
    library.press_ctrl('a');
    library.type_text("Ctrl+L");
    assert!(library.state().settings.as_ref().unwrap().changed());
    assert!(library.press("save-settings"));
    assert!(library.state().settings.is_none());
    assert_eq!(
        profile.hub.library.get_settings().keybindings["search"],
        "ctrl+j"
    );

    // A new window reads the saved settings, and the old default is no longer routed.
    let mut reopened = profile.library();
    let mut tx = rune::Tx::default();
    let old = rune::KeyEvent {
        key: rune::Key::Command('k'),
        mods: rune::Mods {
            cmd: true,
            ctrl: true,
            ..Default::default()
        },
    };
    assert!(!reopened.apply(|state| state.route_key(&old, &mut tx)));
    reopened.apply(|state| state.usage.data = Some(dev_console_ui_tests::usage_fixture()));
    reopened.press_ctrl('l');
    assert!(reopened.state().usage.open);
    reopened.press_ctrl('1');
    assert!(!reopened.state().usage.open);
    reopened.press_ctrl('j');
    reopened.type_text("custom-search");
    assert_eq!(reopened.state().query.text, "custom-search");

    reopened.press("settings");
    reopened.press("settings-tab-keyboard");
    reopened.press("reset-shortcuts");
    reopened.press("close-settings");
    assert_eq!(reopened.state().keybindings["search"], "ctrl+j");
    reopened.press("settings");
    reopened.press("settings-tab-keyboard");
    reopened.press("reset-shortcuts");
    reopened.press("save-settings");
    assert_eq!(reopened.state().keybindings["search"], "ctrl+k");
    assert_eq!(
        profile.hub.library.get_settings().keybindings["search"],
        "ctrl+k"
    );
}

#[test]
fn conflicts_and_invalid_keys_block_save_without_changing_active_bindings() {
    isolate();
    let profile = Profile::new("keybinding-errors");
    let mut library = profile.library();
    library.press("settings");
    library.press("settings-tab-keyboard");
    for invalid in ["Ctrl+N", "Ctrl+3", "k", "Ctrl+Escape"] {
        library.press("shortcut-search");
        library.press_ctrl('a');
        library.type_text(invalid);
        assert!(library.has("shortcut-error"));
        assert!(library
            .state()
            .settings
            .as_ref()
            .unwrap()
            .shortcut_error
            .is_some());
        library.apply(|state| state.save_settings());
        assert!(library.state().settings.is_some());
        assert_eq!(library.state().keybindings["search"], "ctrl+k");
    }
    library.press("reset-shortcuts");
    assert!(!library.has("shortcut-error"));
    library.press("save-settings");
    assert!(library.state().settings.is_none());
}
