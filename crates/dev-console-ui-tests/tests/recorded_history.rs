//! The view loads real recorded project counts, rather than injected suggestions.
use dev_console_core::{
    models::{SaveRequest, Settings},
    project_history::ProjectHistory,
};
use dev_console_ui_tests::Profile;

#[test]
fn rescan_ranks_five_projects_filters_launchers_and_suppresses_only_the_saved_project() {
    let profile = Profile::new("history-recorded");
    let settings = profile.hub.library.get_settings();
    profile
        .fixture
        .state
        .settings
        .save(Settings {
            history_suggestions_enabled: true,
            ..settings
        })
        .unwrap();
    let db = dev_console_core::project_history::database_path().unwrap();
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let history = ProjectHistory::open(&db).unwrap();
    let base = std::path::PathBuf::from(profile.fixture.scratch_dir());
    let mut roots = Vec::new();
    for i in 0..6 {
        let root = base.join(format!("synthetic-project-{i}"));
        std::fs::create_dir_all(root.join(".git")).unwrap();
        for _ in 0..(i + 2) {
            assert!(history
                .record(root.to_str().unwrap(), "cargo test --workspace")
                .unwrap());
        }
        assert!(!history.record(root.to_str().unwrap(), "cd ..").unwrap());
        roots.push(root);
    }
    let mut library = profile.library();
    assert_eq!(library.state().suggestions.len(), 5);
    assert_eq!(
        library.state().suggestions[0].project_root.as_deref(),
        roots[5].to_str()
    );
    assert!(library
        .state()
        .suggestions
        .iter()
        .all(|s| s.command == "cargo test --workspace"));
    let mut draft = profile
        .fixture
        .draft("saved-tests", "cargo test --workspace");
    draft.working_directory = roots[5].to_string_lossy().into_owned();
    profile
        .hub
        .library
        .save_entry(SaveRequest { draft }, &mut Default::default())
        .unwrap();
    for _ in 0..100 {
        if library.focused_key().as_deref() == Some("history-suggestions-rescan") {
            break;
        }
        library.press_key(rune::Key::Tab);
    }
    assert_eq!(
        library.focused_key().as_deref(),
        Some("history-suggestions-rescan")
    );
    library.press_key(rune::Key::Text(" ".into()));
    assert!(library
        .state()
        .suggestions
        .iter()
        .all(|s| s.project_root.as_deref() != roots[5].to_str()));
    assert!(library
        .state()
        .suggestions
        .iter()
        .any(|s| s.project_root.as_deref() == roots[4].to_str()));
    assert!(history
        .record(roots[4].to_str().unwrap(), "cargo clippy --workspace")
        .unwrap());
    for _ in 0..100 {
        if library.focused_key().as_deref() == Some("history-suggestions-rescan") {
            break;
        }
        library.press_key(rune::Key::Tab);
    }
    assert_eq!(
        library.focused_key().as_deref(),
        Some("history-suggestions-rescan")
    );
    library.press_key(rune::Key::Text(" ".into()));
    assert!(library
        .state()
        .suggestions
        .iter()
        .any(|s| s.command == "cargo clippy --workspace"));
    assert!(library.shows("cargo clippy --workspace"));
}
