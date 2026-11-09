//! The Finder palette over a real temporary profile: `tests/e2e/finder.spec.ts`, natively.

use dev_console_core::context::FinderContext;
use dev_console_core::models::Kind;
use dev_console_core::services::AppEvent;
use dev_console_desktop::finder::{COPIED_COMMAND, COPIED_SECRET, UNLOCK_ID};
use dev_console_ui_tests::{clear_clipboard, clipboard_text, Profile, SYNTHETIC_SECRET};
use rune::Key;

fn row(id: &str) -> String {
    format!("finder-row-{id}")
}

#[test]
fn the_first_result_is_selected_and_labelled_for_assistive_tech() {
    let profile = Profile::new("finder-first");
    let deploy = profile.command("deploy-staging", "make deploy");
    profile.command("rebase-main", "git rebase main");
    let finder = profile.finder();
    let first = finder.state().rows()[0].id.clone();
    assert_eq!(finder.state().selected_id.as_deref(), Some(first.as_str()));
    assert_eq!(
        finder.label_of(&row(&deploy)).as_deref(),
        Some("deploy-staging")
    );
    assert_eq!(
        finder.label_of("finder-input").as_deref(),
        Some("Search commands and secrets")
    );
    assert_eq!(finder.text_of("finder-count").as_deref(), Some("2"));
}

#[test]
fn typing_searches_and_shows_the_inline_completion() {
    let profile = Profile::new("finder-type");
    profile.command("deploy-staging", "make deploy");
    profile.command("rebase-main", "git rebase main");
    let mut finder = profile.finder();
    finder.type_text("dep");
    assert_eq!(finder.state().query.text, "dep");
    let names: Vec<_> = finder
        .state()
        .rows()
        .iter()
        .map(|row| row.name.clone())
        .collect();
    assert_eq!(names, vec!["deploy-staging"]);
    assert_eq!(finder.state().completion_tail(), "loy-staging");
    assert_eq!(
        finder.text_of("finder-completion").as_deref(),
        Some("loy-staging")
    );
}

#[test]
fn up_and_down_move_the_selection_and_clamp_at_both_ends() {
    let profile = Profile::new("finder-arrows");
    for name in ["alpha-one", "beta-two", "gamma-three"] {
        profile.command(name, "true");
    }
    let mut finder = profile.finder();
    let ids: Vec<String> = finder
        .state()
        .rows()
        .iter()
        .map(|row| row.id.clone())
        .collect();
    finder.press_key(Key::Up);
    assert_eq!(
        finder.state().selected_id.as_ref(),
        Some(&ids[0]),
        "Up clamps at the top"
    );
    finder.press_key(Key::Down);
    finder.press_key(Key::Down);
    finder.press_key(Key::Down);
    assert_eq!(
        finder.state().selected_id.as_ref(),
        Some(&ids[2]),
        "Down clamps at the bottom"
    );
    finder.press_key(Key::Up);
    assert_eq!(finder.state().selected_id.as_ref(), Some(&ids[1]));
}

#[test]
fn tab_completes_the_selected_name_without_copying_anything() {
    let profile = Profile::new("finder-tab");
    profile.command("deploy-staging", "make deploy");
    let _clipboard = clear_clipboard();
    let mut finder = profile.finder();
    finder.type_text("dep");
    finder.press_key(Key::Tab);
    assert_eq!(finder.state().query.text, "deploy-staging");
    assert_eq!(finder.state().completion_tail(), "");
    assert_eq!(clipboard_text(), None, "Tab never copies");
    assert!(finder.state().status.is_none());
}

#[test]
fn enter_copies_the_command_through_the_core_clipboard_and_hides_nothing_of_it() {
    let profile = Profile::new("finder-enter");
    profile.command("deploy-staging", "make deploy --synthetic");
    let _clipboard = clear_clipboard();
    let mut finder = profile.finder();
    finder.press_key(Key::Enter);
    assert_eq!(clipboard_text().as_deref(), Some("make deploy --synthetic"));
    assert_eq!(finder.state().status.as_deref(), Some(COPIED_COMMAND));
    assert!(finder.has("finder-status"));
    assert!(
        !finder.leaks("make deploy --synthetic"),
        "the copied command is never drawn by the palette"
    );
}

#[test]
fn copying_a_secret_says_so_and_never_puts_the_value_in_the_tree() {
    let profile = Profile::new("finder-secret");
    let id = profile.secret("registry-token", SYNTHETIC_SECRET);
    let _clipboard = clear_clipboard();
    let mut finder = profile.finder();
    finder.apply(|finder| finder.selected_id = Some(id.clone()));
    finder.press_key(Key::Enter);
    assert_eq!(clipboard_text().as_deref(), Some(SYNTHETIC_SECRET));
    assert_eq!(finder.state().status.as_deref(), Some(COPIED_SECRET));
    assert!(!finder.leaks(SYNTHETIC_SECRET));
    assert!(
        !format!("{:?}", finder.state()).contains(SYNTHETIC_SECRET),
        "Debug never prints a value"
    );
}

#[test]
fn escape_clears_the_query_and_the_rows() {
    let profile = Profile::new("finder-escape");
    profile.command("deploy-staging", "make deploy");
    let mut finder = profile.finder();
    finder.type_text("dep");
    finder.press_key(Key::Escape);
    assert!(finder.state().query.text.is_empty());
    assert!(finder.state().results.is_empty());
    assert!(finder.state().selected_id.is_none());
}

#[test]
fn every_appearance_starts_from_an_empty_query_and_a_fresh_search() {
    let profile = Profile::new("finder-shown");
    profile.command("deploy-staging", "make deploy");
    let mut finder = profile.finder();
    finder.type_text("nothing-matches-this");
    assert!(finder.state().results.is_empty());
    assert!(finder.has("finder-empty"));
    finder.apply(|finder| finder.on_event(&AppEvent::FinderShown, &mut rune::Tx::default()));
    assert!(finder.state().query.text.is_empty());
    assert_eq!(finder.state().results.len(), 1);
}

#[test]
fn a_locked_vault_hides_secrets_and_offers_an_unlock_row() {
    let profile = Profile::new("finder-locked");
    profile.command("deploy-staging", "make deploy");
    profile.secret("registry-token", SYNTHETIC_SECRET);
    let mut finder = profile.finder();
    assert!(finder
        .state()
        .rows()
        .iter()
        .any(|row| row.kind == Kind::Secret));
    let mut effects = dev_console_core::Effects::default();
    let vault = profile.hub.finder.lock_vault(&mut effects).expect("lock");
    finder.apply(|finder| {
        finder.on_event(
            &AppEvent::VaultStateChanged(vault),
            &mut rune::Tx::default(),
        )
    });
    let rows = finder.state().rows();
    assert!(
        rows.iter().all(|row| row.kind != Kind::Secret),
        "no secret metadata while locked"
    );
    assert_eq!(rows.last().map(|row| row.id.as_str()), Some(UNLOCK_ID));
    assert!(finder.shows("Vault locked"));
    assert!(finder.shows("Unlock vault to search secrets"));
    assert!(
        !finder.shows("registry-token"),
        "no secret name stays in the tree"
    );
}

#[test]
fn a_detected_project_shows_a_chip_resets_the_selection_and_ctrl_p_ignores_it() {
    let profile = Profile::new("finder-context");
    profile.command("deploy-staging", "make deploy");
    let mut finder = profile.finder();
    let context = FinderContext {
        root: "/tmp/synthetic-project".into(),
        name: "synthetic-project".into(),
    };
    finder.apply(|finder| {
        finder.on_event(
            &AppEvent::FinderContext(Some(context.clone())),
            &mut rune::Tx::default(),
        )
    });
    assert!(finder.has("finder-context"));
    assert_eq!(
        finder.label_of("finder-context").as_deref(),
        Some("Ranking the project synthetic-project first. Activate to ignore it.")
    );
    finder.press_ctrl('p');
    assert!(finder.state().context.is_none());
    assert!(!finder.has("finder-context"));
}

#[test]
fn the_footer_names_the_bindings_the_field_routes() {
    let profile = Profile::new("finder-footer");
    let finder = profile.finder();
    for words in ["select", "complete", "copy", "details", "dismiss"] {
        assert!(finder.shows(words), "footer is missing {words}");
    }
}

#[test]
fn a_stale_asynchronous_response_cannot_replace_newer_results() {
    let profile = Profile::new("finder-stale");
    profile.command("deploy-staging", "make deploy");
    profile.command("rebase-main", "git rebase main");
    let mut finder = profile.finder();
    let superseded = finder.state().request_id();
    finder.type_text("rebase");
    let newer: Vec<String> = finder
        .state()
        .results
        .iter()
        .map(|row| row.name.clone())
        .collect();
    assert_eq!(newer, vec!["rebase-main"]);
    // The answer to the search the typing replaced arrives late, with every row in it.
    let late = profile
        .hub
        .finder
        .search_entries(dev_console_core::models::SearchRequest {
            query: String::new(),
            request_id: superseded,
            limit: None,
        });
    assert_eq!(late.results.len(), 2);
    finder.apply(|finder| finder.apply_search(late));
    let shown: Vec<String> = finder
        .state()
        .results
        .iter()
        .map(|row| row.name.clone())
        .collect();
    assert_eq!(
        shown, newer,
        "an answer for a replaced query is never painted"
    );
}
