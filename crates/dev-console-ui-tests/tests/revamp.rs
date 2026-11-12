//! The interface rules `docs/design/ui-revamp-scorecard.md` scores, as checks.
//!
//! A score in a document drifts; these do not. Each test states one rule of the revamp in the
//! terms a person would notice it in: how many controls are on screen, what the list shows before
//! anything is touched, what a status does to the layout, and how a copy is confirmed.

use dev_console_core::models::{Kind, SaveRequest};
use dev_console_desktop::library::Library;
use dev_console_desktop::widgets::short_path;
use dev_console_ui_tests::{Profile, Stage};
use rune::Tx;

/// Three projects' worth of commands and two secrets: a library with every kind of row in it.
fn busy_library(profile: &Profile) -> (Stage<Library>, Vec<String>, String) {
    let root = profile.fixture.scratch_dir();
    std::fs::create_dir_all(format!("{root}/.git")).unwrap();
    let mut ids = Vec::new();
    for index in 0..6 {
        let mut draft = profile
            .fixture
            .draft(&format!("project-task-{index}"), "true");
        draft.working_directory = root.clone();
        ids.push(
            profile
                .hub
                .library
                .save_entry(SaveRequest { draft }, &mut Default::default())
                .unwrap()
                .0
                .id,
        );
    }
    for index in 0..6 {
        ids.push(profile.command(&format!("loose-task-{index}"), "true"));
    }
    profile.secret("first-token", "synthetic-first-value");
    profile.secret("second-token", "synthetic-second-value");
    (profile.library(), ids, root)
}

#[test]
fn a_resting_library_shows_a_dozen_controls_however_many_rows_it_has() {
    let profile = Profile::new("revamp-budget");
    let (library, ids, _) = busy_library(&profile);
    let controls = library.visible_controls();
    let rows = library
        .keys_starting_with("entry-row-")
        .iter()
        .filter(|key| key.starts_with("entry-row-"))
        .count();
    assert_eq!(rows, ids.len(), "every row is listed without unfolding");
    let chrome: Vec<&String> = controls
        .iter()
        .filter(|key| !key.starts_with("entry-row-"))
        .collect();
    // Two tabs, the filter, New; the project heading; Copy, Run, Edit, Pin, Delete; the vault,
    // Import, Environment and Settings. Twelve rows must not bring twelve sets of row buttons.
    assert!(
        chrome.len() <= 15,
        "{} controls besides the rows: {chrome:?}",
        chrome.len()
    );
    assert!(
        !controls.iter().any(|key| key.starts_with("row-copy-")),
        "row actions are not drawn until the pointer or the keyboard is on the row"
    );
    assert!(
        !controls.iter().any(|key| key.starts_with("row-delete-")),
        "a list row never carries a delete button"
    );
}

#[test]
fn a_rows_own_actions_appear_under_the_pointer_and_only_there() {
    let profile = Profile::new("revamp-hover");
    let (mut library, ids, _) = busy_library(&profile);
    let (first, second) = (&ids[0], &ids[1]);
    assert!(library.hover(&format!("entry-row-{first}")));
    let controls = library.visible_controls();
    assert!(controls.contains(&format!("row-copy-{first}")));
    assert!(controls.contains(&format!("row-pin-{first}")));
    assert!(!controls.contains(&format!("row-copy-{second}")));
}

#[test]
fn a_pinned_row_keeps_its_pin_on_show() {
    let profile = Profile::new("revamp-pinned");
    let (mut library, ids, _) = busy_library(&profile);
    let target = ids[9].clone();
    assert!(library.press(&format!("row-pin-{target}")));
    // Away from the row again: the pin stays, the copy button does not.
    assert!(library.hover("tab-search"));
    let controls = library.visible_controls();
    assert!(controls.contains(&format!("row-pin-{target}")));
    assert!(!controls.contains(&format!("row-copy-{target}")));
}

#[test]
fn project_groups_start_open_and_fold_on_request() {
    let profile = Profile::new("revamp-groups");
    let (mut library, ids, root) = busy_library(&profile);
    assert!(library.state().collapsed.is_empty());
    assert!(library.has(&format!("entry-row-{}", ids[0])));
    assert!(library.press(&format!("project-toggle-{root}")));
    assert!(!library.has(&format!("entry-row-{}", ids[0])));
    assert!(
        library.has(&format!("entry-row-{}", ids[7])),
        "General has no fold: it is where everything else lives"
    );
    assert!(library.press(&format!("project-toggle-{root}")));
    assert!(library.has(&format!("entry-row-{}", ids[0])));
}

#[test]
fn a_projects_environment_opens_from_its_heading() {
    let profile = Profile::new("revamp-env");
    let (mut library, _, root) = busy_library(&profile);
    assert!(library.press(&format!("project-environment-{root}")));
    assert_eq!(
        library
            .state()
            .environment
            .as_ref()
            .map(|panel| panel.root.as_str()),
        Some(root.as_str())
    );
    assert!(
        !library.state().collapsed.contains(&root),
        "the heading's own button does not also fold the group"
    );
}

#[test]
fn a_status_floats_over_the_window_and_moves_nothing() {
    let profile = Profile::new("revamp-toast");
    let (mut library, ids, _) = busy_library(&profile);
    let row = format!("entry-row-{}", ids[3]);
    let before = (library.box_of(&row), library.box_of("copy"));
    assert!(library.press("copy"));
    assert!(library.has("status"));
    assert_eq!(
        (library.box_of(&row), library.box_of("copy")),
        before,
        "a confirmation must not push the list or the bar it was pressed in"
    );
    // The toast's layer covers the window. What is under it still takes the pointer.
    assert!(library.press("settings"));
    assert!(library.state().settings.is_some());
    library.press("close-settings");
    assert!(library.press(&format!("entry-row-{}", ids[11])));
    assert_eq!(
        library.state().selected_id.as_deref(),
        Some(ids[11].as_str())
    );
}

#[test]
fn a_copied_command_is_confirmed_in_one_sentence_and_a_secret_keeps_the_caveat() {
    let profile = Profile::new("revamp-copyline");
    let (mut library, _, _) = busy_library(&profile);
    let _guard = dev_console_ui_tests::clear_clipboard();
    assert!(library.press("copy"));
    let full = library.state().status.clone().unwrap();
    let shown = library
        .strings()
        .into_iter()
        .find(|text| text.starts_with("Copied") && !text.contains(". "))
        .expect("the one-sentence confirmation");
    assert!(full.starts_with(shown.trim_end_matches('.')));
    assert_eq!(
        library.label_of("status").as_deref(),
        Some(full.as_str()),
        "assistive technology still gets the whole message"
    );
    library.press_ctrl('2');
    assert!(library.press("copy"));
    let secret_status = library.state().status.clone().unwrap();
    assert!(
        library.shows(&secret_status),
        "a copied secret shows the clipboard caveat in full"
    );
}

#[test]
fn a_confirmation_leaves_on_its_own_and_other_messages_stay_longer() {
    let profile = Profile::new("revamp-expiry");
    let mut library = profile.library();
    let mut tx = Tx::default();
    library.apply(|state| {
        state.status = Some("Copied for about 30s.".into());
        assert!(!state.poll(10.0, &mut tx), "first sight starts the clock");
        assert!(!state.poll(13.9, &mut tx));
        assert!(state.status.is_some());
        assert!(state.poll(14.0, &mut tx));
        assert!(state.status.is_none());

        state.status = Some("Finish or cancel the edit first.".into());
        assert!(!state.poll(20.0, &mut tx));
        assert!(
            !state.poll(27.9, &mut tx),
            "not a confirmation: eight seconds"
        );
        assert!(state.poll(28.0, &mut tx));

        // A new message while one is up restarts the clock for the new one.
        state.status = Some("Saved.".into());
        assert!(!state.poll(30.0, &mut tx));
        state.status = Some("Deleted “x”.".into());
        assert!(!state.poll(33.0, &mut tx));
        assert!(!state.poll(36.9, &mut tx));
        assert!(state.poll(37.0, &mut tx));
    });
}

#[test]
fn the_detail_pane_has_one_filled_button() {
    let profile = Profile::new("revamp-primary");
    let (mut library, _, _) = busy_library(&profile);
    for tab in [Kind::Command, Kind::Secret] {
        library.apply(|state| {
            state.select_tab(tab);
        });
        let labelled: Vec<String> = ["copy", "run", "reveal", "edit", "pin", "delete"]
            .into_iter()
            .filter(|key| library.text_inside(key))
            .map(str::to_owned)
            .collect();
        let expected: Vec<String> = if tab == Kind::Command {
            vec!["copy".into(), "run".into(), "edit".into()]
        } else {
            vec!["copy".into(), "reveal".into(), "edit".into()]
        };
        assert_eq!(
            labelled, expected,
            "Copy and Run/Reveal sit with the content; Edit stays visible"
        );
    }
}

#[test]
fn paths_read_from_the_home_folder_and_keep_their_last_two_parts() {
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() {
        assert_eq!(short_path(&format!("{home}/code/api")), "~/code/api");
        assert_eq!(short_path(&home), "~");
    }
    assert_eq!(short_path("/srv/app"), "/srv/app");
    assert_eq!(
        short_path("/var/lib/some/very/deeply/nested/folder/structure/project/api"),
        "…/project/api"
    );
    assert_eq!(short_path(""), "");
}

#[test]
fn arrows_walk_the_list_and_the_detail_pane_follows() {
    let profile = Profile::new("revamp-arrows");
    let (mut library, _, _) = busy_library(&profile);
    let order: Vec<String> = library.keys_starting_with("entry-row-");
    let start = library.state().selected_id.clone().unwrap();
    let at = order
        .iter()
        .position(|key| key.ends_with(&start))
        .expect("the selected row is listed");
    assert!(library.press(&order[at]));
    library.press_key(rune::Key::Down);
    assert_eq!(
        library.state().selected_id.as_deref(),
        order[at + 1].strip_prefix("entry-row-"),
        "Down selects the next row, not the row's own buttons and not a heading"
    );
    library.press_key(rune::Key::End);
    assert_eq!(
        library.state().selected_id.as_deref(),
        order.last().unwrap().strip_prefix("entry-row-")
    );
    library.press_key(rune::Key::Home);
    assert_eq!(
        library.state().selected_id.as_deref(),
        order[0].strip_prefix("entry-row-")
    );
    assert!(
        library.state().collapsed.is_empty(),
        "walking the list never folds a group"
    );
}

#[test]
fn the_filter_and_the_editor_have_their_own_keys() {
    let profile = Profile::new("revamp-keys");
    let (mut library, _, _) = busy_library(&profile);
    library.press_ctrl('k');
    // Typing lands in the filter, which is what having the focus means.
    library.type_text("loose");
    assert_eq!(library.state().query.text, "loose");
    assert_eq!(library.state().entries.len(), 6);
    library.press_ctrl('e');
    assert!(
        library.state().editing.is_some(),
        "Ctrl+E edits the selection"
    );
}

#[test]
fn escape_leaves_a_clean_editor_and_keeps_a_dirty_one() {
    let profile = Profile::new("revamp-escape");
    let (mut library, _, _) = busy_library(&profile);
    library.press_ctrl('e');
    assert!(library.state().editing.is_some());
    library.press_key(rune::Key::Escape);
    assert!(
        library.state().editing.is_none(),
        "nothing typed: Escape closes"
    );
    library.press_ctrl('e');
    library.type_text("x");
    assert!(library.state().draft_dirty());
    library.press_key(rune::Key::Escape);
    assert!(
        library.state().editing.is_some(),
        "typed text is never thrown away by Escape"
    );
    assert!(
        library.shows("unsaved edits"),
        "and the key says why it did nothing"
    );
}

#[test]
fn tab_does_not_stop_on_every_rows_hidden_buttons() {
    let profile = Profile::new("revamp-tabstops");
    let (mut library, ids, _) = busy_library(&profile);
    let mut stops = Vec::new();
    for _ in 0..80 {
        library.press_key(rune::Key::Tab);
        let key = library.focused_key().unwrap_or_default();
        if stops.first() == Some(&key) {
            break;
        }
        stops.push(key);
    }
    assert!(
        !stops.iter().any(|key| key.starts_with("row-")),
        "Tab reached a row button: {stops:?}"
    );
    let rows = stops
        .iter()
        .filter(|key| key.starts_with("entry-row-"))
        .count();
    assert_eq!(rows, 1, "the list is one stop; arrows move inside it");
    assert!(
        stops.len() < ids.len() + 12,
        "{} stops: {stops:?}",
        stops.len()
    );
    for key in ["copy", "edit", "entry-more", "settings"] {
        assert!(
            stops.iter().any(|stop| stop == key),
            "Tab never reached {key}"
        );
    }
}

#[test]
fn a_dialog_owns_the_keyboard() {
    let profile = Profile::new("revamp-modal-keys");
    let (mut library, _, _) = busy_library(&profile);
    assert!(library.press("entry-more"));
    assert!(library.press("entry-options-item2"));
    assert!(library.state().pending_delete.is_some());
    library.press_ctrl('e');
    library.press_ctrl('n');
    assert!(
        library.state().editing.is_none(),
        "no editor opens behind a confirmation"
    );
    library.press_ctrl('2');
    assert_eq!(library.state().tab, Kind::Command, "and the tab stays put");
    assert!(library.state().pending_delete.is_some());
}

#[test]
fn settings_says_when_a_change_is_not_saved_yet() {
    let profile = Profile::new("revamp-settings-unsaved");
    let mut library = profile.library();
    library.press("settings");
    assert!(!library.has("settings-unsaved"));
    assert_eq!(library.label_of("close-settings").as_deref(), Some("Close"));
    assert!(library.press("autostart"));
    assert!(library.has("settings-unsaved"));
    assert_eq!(
        library.label_of("close-settings").as_deref(),
        Some("Discard"),
        "a switch here does not apply until Save, and the sheet says so"
    );
    assert!(library.press("autostart"));
    assert!(!library.has("settings-unsaved"), "back to what is on disk");
}

#[test]
fn every_chord_settings_lists_is_one_the_window_answers() {
    use dev_console_core::keybindings::ACTIONS;
    let profile = Profile::new("revamp-shortcuts");
    let (mut library, _, _) = busy_library(&profile);
    library.press("settings");
    library.press("settings-tab-keyboard");
    for (_, what, _) in ACTIONS {
        assert!(library.shows(what), "Settings does not list {what:?}");
    }
    library.press("close-settings");
    let mut tx = Tx::default();
    for (_, what, chord) in ACTIONS {
        let key = chord.chars().last().unwrap();
        let event = rune::KeyEvent {
            key: rune::Key::Command(key),
            mods: rune::Mods {
                cmd: true,
                ctrl: true,
                ..rune::Mods::default()
            },
        };
        let taken = library.apply(|state| state.route_key(&event, &mut tx));
        assert!(taken, "{what:?} is listed but {chord} is not routed");
    }
}

#[test]
fn settings_is_three_short_pages_and_opens_on_general() {
    let profile = Profile::new("revamp-settings-pages");
    let mut library = profile.library();
    library.press("settings");
    assert!(library.has("clipboard-expiry"));
    assert!(!library.has("backup-now"), "one page at a time");
    assert!(library.press("settings-tab-backups"));
    assert!(library.has("backup-now") && library.has("recovery-reveal"));
    assert!(!library.has("clipboard-expiry"));
    assert!(
        library.has("save-settings") && library.has("current-version"),
        "Save and the version stay in the footer of every page"
    );
    library.press("close-settings");
    library.press("settings");
    assert!(
        library.has("clipboard-expiry"),
        "a reopened sheet starts on General again"
    );
}

#[test]
fn sidebar_resizes_with_drag_and_keyboard_and_leaves_room_for_details() {
    let profile = Profile::new("revamp-resize");
    profile.command("synthetic task", "true");
    let mut library = profile.library();
    let initial = library.box_of("list-pane").unwrap().0[2];
    assert_eq!(initial, 420.0);
    assert!(library.drag("sidebar-resize", 100.0, 0.0));
    assert_eq!(library.box_of("list-pane").unwrap().0[2], initial + 100.0);
    assert!(library.press("sidebar-resize"));
    library.press_key(rune::Key::Left);
    assert_eq!(library.box_of("list-pane").unwrap().0[2], initial + 84.0);
    library.press_key(rune::Key::Home);
    assert_eq!(library.box_of("list-pane").unwrap().0[2], initial);
    assert!(library.drag("sidebar-resize", 1000.0, 0.0));
    assert_eq!(library.box_of("list-pane").unwrap().0[2], 560.0);
    assert!(library.box_of("detail-pane").unwrap().0[2] >= 400.0);
    assert!(library.drag("sidebar-resize", -1000.0, 0.0));
    assert_eq!(library.box_of("list-pane").unwrap().0[2], 360.0);
}

#[test]
fn saved_commands_have_a_section_heading_and_nest_below_their_project() {
    let profile = Profile::new("revamp-hierarchy");
    let (library, ids, root) = busy_library(&profile);
    let heading = library.box_of("saved-entries-heading").unwrap().0;
    let project = library.box_of(&format!("project-toggle-{root}")).unwrap().0;
    let command = library.box_of(&format!("entry-name-{}", ids[0])).unwrap().0;
    assert!(heading[1] + heading[3] <= project[1]);
    assert!(
        command[0] > project[0] + 24.0,
        "commands nest beyond the project name"
    );
    assert!(library.shows("Saved commands"));
}

#[test]
fn hidden_row_actions_do_not_take_space_from_the_name() {
    let profile = Profile::new("revamp-row-width");
    let id = profile.command(
        "a long synthetic command name that needs the available width",
        "true",
    );
    let library = profile.library();
    let row = library.box_of(&format!("entry-row-{id}")).unwrap().0;
    let name = library.box_of(&format!("entry-name-{id}")).unwrap().0;
    assert!(
        name[2] >= row[2] - 24.0,
        "hidden controls must not reserve their width"
    );
}

#[test]
fn empty_selection_explains_suggestions_and_offers_a_new_command() {
    let profile = Profile::new("revamp-empty-selection");
    profile.command("synthetic task", "true");
    let mut library = profile.library();
    library.apply(|state| state.back_to_list());
    assert!(library.shows("Your command library"));
    assert!(library.shows("choose Save to review"));
    assert!(library.press("empty-selection-new"));
    assert!(library.state().editing.is_some());
}

#[test]
fn more_actions_is_dismissible_and_delete_still_requires_confirmation() {
    let profile = Profile::new("production-more-menu");
    profile.command("synthetic command", "true");
    let mut library = profile.library();
    assert!(!library.has("entry-options-item2"));
    assert!(library.press("entry-more"));
    assert!(library.has("entry-options-menu"));
    library.press_ctrl('n');
    assert!(
        library.state().editing.is_none(),
        "a menu owns the keyboard"
    );
    library.press_key(rune::Key::Escape);
    assert!(!library.has("entry-options-menu"));
    assert!(library.press("entry-more"));
    assert!(library.press("entry-options-item0"));
    assert!(library.state().meta().unwrap().pinned);
    assert!(!library.has("entry-options-menu"));
    assert!(library.press("entry-more"));
    assert!(library.press("entry-options-item2"));
    assert!(library.state().pending_delete.is_some());
    assert_eq!(library.state().entries.len(), 1);
}

#[test]
fn command_actions_follow_the_command_and_tools_have_visible_labels() {
    let profile = Profile::new("production-action-placement");
    let id = profile.command("synthetic task", "echo synthetic");
    let library = profile.library();
    let command = library.box_of("command-text").unwrap().0;
    let copy = library.box_of("copy").unwrap().0;
    assert!(copy[1] > command[1] + command[3]);
    assert!(library.text_inside("settings"));
    assert!(library.text_inside("import-project"));
    assert!(library.text_inside("open-environment"));
    assert!(library.has(&format!("entry-caption-{id}")));
}

#[test]
fn saved_command_highlights_have_equal_top_and_bottom_padding() {
    let profile = Profile::new("saved-row-padding");
    let id = profile.command("synthetic command", "true");
    let mut library = profile.library();
    assert!(library.hover(&format!("entry-row-{id}")));
    let row = library.box_of(&format!("entry-row-{id}")).unwrap().0;
    let title = library.box_of(&format!("entry-name-{id}")).unwrap().0;
    let caption = library.box_of(&format!("entry-caption-{id}")).unwrap().0;
    let top = title[1] - row[1];
    let bottom = row[1] + row[3] - caption[1] - caption[3];
    assert!(top >= 6.0);
    assert!(
        (top - bottom).abs() <= 1.0,
        "uneven padding: top={top}, bottom={bottom}"
    );
}

#[test]
fn vault_lock_dismisses_more_and_keeps_keyboard_navigation() {
    let profile = Profile::new("revamp-more-lock");
    profile.secret("synthetic-token", "synthetic-value");
    let mut library = profile.library();
    library.apply(|state| {
        state.select_tab(Kind::Secret);
    });
    assert!(library.press("entry-more"));
    assert!(library.has("entry-options-menu"));
    library.apply(|state| state.lock());
    assert!(!library.has("entry-options-menu"));
    library.press_ctrl('1');
    assert_eq!(library.state().tab, Kind::Command);
    assert!(!library.state().entry_options_open);
}

#[test]
fn a_packaged_install_is_told_who_updates_it() {
    // Only an AppImage replaces itself. Anything else - a package, a build from source, this test
    // binary - shows the Updates group with no button that could download over it.
    let profile = Profile::new("revamp-updates-packaged");
    let mut library = profile.library();
    library.press("settings");
    assert!(library.has("app-updates"));
    assert!(!library.has("check-updates"));
    assert!(!library.has("install-update"));
}
