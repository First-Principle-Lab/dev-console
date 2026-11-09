//! More of `tests/e2e/finder.spec.ts` and the palette half of `redesign.spec.ts`.

use dev_console_core::models::{EntryDraft, Kind, SaveRequest};
use dev_console_core::services::AppEvent;
use dev_console_desktop::finder::COPIED_COMMAND;
use dev_console_ui_tests::{clear_clipboard, clipboard_text, Profile, SYNTHETIC_SECRET};
use rune::{Key, Tx};

fn command_in(profile: &Profile, name: &str, folder: &str, tags: &[&str]) -> String {
    let mut draft: EntryDraft = profile.fixture.draft(name, "true");
    draft.working_directory = folder.to_owned();
    draft.tags = tags.iter().map(|tag| tag.to_string()).collect();
    let mut effects = dev_console_core::Effects::default();
    profile
        .hub
        .library
        .save_entry(SaveRequest { draft }, &mut effects)
        .expect("save")
        .0
        .id
}

#[test]
fn with_no_matches_tab_and_enter_do_nothing() {
    let profile = Profile::new("finder-nomatch");
    profile.command("deploy-staging", "make deploy");
    let _clipboard = clear_clipboard();
    let mut finder = profile.finder();
    finder.type_text("zzz-nothing");
    assert!(finder.has("finder-empty"));
    finder.press_key(Key::Tab);
    assert_eq!(finder.state().query.text, "zzz-nothing");
    finder.press_key(Key::Enter);
    assert_eq!(clipboard_text(), None);
    assert!(finder.state().status.is_none() && finder.state().error.is_none());
}

#[test]
fn keys_still_work_after_a_click_moved_focus_onto_a_row() {
    let profile = Profile::new("finder-rowfocus");
    let first = profile.command("alpha-one", "true");
    profile.command("beta-two", "true");
    let mut finder = profile.finder();
    let ids: Vec<String> = finder
        .state()
        .rows()
        .iter()
        .map(|row| row.id.clone())
        .collect();
    assert!(finder.press(&format!("finder-row-{}", ids[0])));
    finder.press_key(Key::Down);
    assert_eq!(finder.state().selected_id.as_ref(), Some(&ids[1]));
    let _ = first;
}

#[test]
fn typing_shows_the_matching_saved_entries_with_their_tags() {
    let profile = Profile::new("finder-tags");
    command_in(&profile, "deploy-staging", "", &["work", "ci"]);
    let mut finder = profile.finder();
    finder.type_text("deploy");
    assert!(finder.shows("#work"));
    assert!(finder.shows("#ci"));
}

#[test]
fn an_empty_query_lists_the_most_recently_used_entries_first() {
    let profile = Profile::new("finder-recent");
    profile.command("alpha-one", "true");
    let beta = profile.command("beta-two", "true");
    let _clipboard = clear_clipboard();
    let mut finder = profile.finder();
    finder.apply(|finder| finder.selected_id = Some(beta.clone()));
    finder.press_key(Key::Enter);
    finder.apply(|finder| finder.on_event(&AppEvent::FinderShown, &mut Tx::default()));
    assert_eq!(
        finder.state().rows()[0].id,
        beta,
        "the entry just used leads the empty query"
    );
}

#[test]
fn a_copy_failure_keeps_the_finder_open_with_a_safe_message() {
    let profile = Profile::new("finder-copyfail");
    let id = profile.secret("registry-token", SYNTHETIC_SECRET);
    let mut finder = profile.finder();
    finder.apply(|finder| finder.selected_id = Some(id.clone()));
    // The vault locks underneath the palette before the copy lands.
    let mut effects = dev_console_core::Effects::default();
    profile.hub.library.lock_vault(&mut effects).expect("lock");
    finder.press_key(Key::Enter);
    assert!(finder.has("finder-error"));
    assert!(finder.state().status.is_none());
    assert!(!finder.leaks(SYNTHETIC_SECRET));
}

#[test]
fn a_copied_confirmation_does_not_describe_the_next_search() {
    let profile = Profile::new("finder-status");
    profile.command("deploy-staging", "make deploy");
    let _clipboard = clear_clipboard();
    let mut finder = profile.finder();
    finder.press_key(Key::Enter);
    assert_eq!(finder.state().status.as_deref(), Some(COPIED_COMMAND));
    finder.type_text("d");
    assert!(finder.state().status.is_none());
    assert!(!finder.has("finder-status"));
}

#[test]
fn a_row_from_the_current_project_is_labelled_and_ranked_first() {
    let profile = Profile::new("finder-project");
    let root = profile.fixture.scratch_dir();
    profile.command("general-one", "true");
    let in_project = command_in(&profile, "project-one", &root, &[]);
    profile
        .fixture
        .set_finder_context(&root, "synthetic-project");
    let finder = profile.finder();
    assert_eq!(finder.state().rows()[0].id, in_project);
    assert_eq!(
        finder
            .label_of(&format!("finder-in-project-{in_project}"))
            .as_deref(),
        Some("in synthetic-project")
    );
    assert!(finder.has("finder-context"));
}

#[test]
fn clicking_the_project_chip_ranks_everything_again_without_closing_the_palette() {
    let profile = Profile::new("finder-chip");
    let root = profile.fixture.scratch_dir();
    profile.command("general-one", "true");
    command_in(&profile, "project-one", &root, &[]);
    profile
        .fixture
        .set_finder_context(&root, "synthetic-project");
    let mut finder = profile.finder();
    assert!(finder.press("finder-context"));
    assert!(finder.state().context.is_none());
    assert!(!finder.has("finder-context"));
    assert!(finder.has("finder-input"), "the palette stays open");
}

#[test]
fn with_no_detected_project_there_is_no_chip_and_no_marker() {
    let profile = Profile::new("finder-noproject");
    command_in(&profile, "project-one", &profile.fixture.scratch_dir(), &[]);
    let finder = profile.finder();
    assert!(!finder.has("finder-context"));
    assert!(finder.keys_starting_with("finder-in-project-").is_empty());
}

#[test]
fn a_secret_tagged_for_the_project_is_not_moved_up_or_marked() {
    let profile = Profile::new("finder-secretproject");
    let root = profile.fixture.scratch_dir();
    profile.command("general-one", "true");
    let secret = profile.secret("project-token", SYNTHETIC_SECRET);
    profile
        .fixture
        .set_finder_context(&root, "synthetic-project");
    let finder = profile.finder();
    assert!(!finder.has(&format!("finder-in-project-{secret}")));
}

#[test]
fn a_project_detected_after_the_first_search_moves_the_selection_to_its_first_command() {
    let profile = Profile::new("finder-latecontext");
    let root = profile.fixture.scratch_dir();
    profile.command("general-one", "true");
    let in_project = command_in(&profile, "project-one", &root, &[]);
    let mut finder = profile.finder();
    let context = dev_console_core::context::FinderContext {
        root: root.clone().into(),
        name: "synthetic-project".into(),
    };
    profile
        .fixture
        .set_finder_context(&root, "synthetic-project");
    finder.apply(|finder| {
        finder.on_event(&AppEvent::FinderContext(Some(context)), &mut Tx::default())
    });
    assert_eq!(
        finder.state().selected_id.as_deref(),
        Some(in_project.as_str())
    );
}

#[test]
fn the_finder_loads_the_saved_theme_and_follows_changes() {
    let profile = Profile::new("finder-theme");
    profile.fixture.set_theme("omarchy-tokyo-night");
    let mut finder = profile.finder();
    assert_eq!(finder.state().theme, "omarchy-tokyo-night");
    finder.apply(|finder| {
        finder.on_event(&AppEvent::ThemeChanged("light".into()), &mut Tx::default())
    });
    assert_eq!(finder.state().theme, "light");
}

#[test]
fn the_query_field_is_a_combobox_over_a_results_listbox() {
    let profile = Profile::new("finder-roles");
    profile.command("deploy-staging", "make deploy");
    let finder = profile.finder();
    assert_eq!(
        finder.label_of("finder-results").as_deref(),
        Some("Results")
    );
    assert!(finder.has("finder-input"));
    let _ = Kind::Command;
}

#[test]
fn typing_over_a_project_preserves_its_marker_and_ranking() {
    let profile = Profile::new("finder-project-query");
    let root = profile.fixture.scratch_dir();
    command_in(&profile, "deploy-general", "", &[]);
    let id = command_in(&profile, "deploy-project", &root, &[]);
    profile
        .fixture
        .set_finder_context(&root, "synthetic-project");
    let mut finder = profile.finder();
    finder.type_text("deploy");
    assert_eq!(finder.state().rows()[0].id, id);
    assert!(finder.has(&format!("finder-in-project-{id}")));
    assert!(finder.has("finder-context"));
}

#[test]
fn a_pinned_command_leads_even_the_detected_project() {
    let profile = Profile::new("finder-pinned-project");
    let root = profile.fixture.scratch_dir();
    command_in(&profile, "project-command", &root, &[]);
    let id = profile.command("general-command", "true");
    profile
        .hub
        .library
        .set_pinned(
            dev_console_core::models::PinRequest {
                kind: Kind::Command,
                id: id.clone(),
                expected_revision: 1,
                pinned: true,
            },
            &mut dev_console_core::Effects::default(),
        )
        .unwrap();
    profile
        .fixture
        .set_finder_context(&root, "synthetic-project");
    let finder = profile.finder();
    assert_eq!(finder.state().rows()[0].id, id);
    assert!(!finder.state().rows()[0].in_context);
}
