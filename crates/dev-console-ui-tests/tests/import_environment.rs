//! `tests/e2e/project-import.spec.ts` and `environment.spec.ts`, natively.

use dev_console_core::models::{Kind, ListRequest};
use dev_console_desktop::library::Library;
use dev_console_ui_tests::{clear_clipboard, clipboard_text, Profile, Stage};
use rune::Key;

fn project(scripts: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let body: Vec<String> = scripts
        .iter()
        .map(|(name, run)| format!("\"{name}\":\"{run}\""))
        .collect();
    std::fs::write(
        dir.path().join("package.json"),
        format!(
            "{{\"name\":\"synthetic\",\"scripts\":{{{}}}}}",
            body.join(",")
        ),
    )
    .unwrap();
    dir
}

fn scan(library: &mut Stage<Library>, path: &str) {
    library.apply(|state| {
        state
            .import
            .as_mut()
            .unwrap()
            .path
            .replace_text(path.to_owned());
        state.scan_project();
    });
}

fn commands(profile: &Profile) -> usize {
    profile
        .hub
        .library
        .list_entries(ListRequest {
            kind: Kind::Command,
            ..Default::default()
        })
        .unwrap()
        .len()
}

#[test]
fn the_action_sits_beside_new_and_explains_what_a_scan_reads() {
    let profile = Profile::new("imp-offer");
    let mut library = profile.library();
    assert!(library.has("import-project"));
    library.press("import-project");
    assert!(library.shows("Nothing inside the folder is executed by scanning or by importing."));
}

#[test]
fn an_empty_folder_field_is_refused_without_reading_anything() {
    let profile = Profile::new("imp-empty");
    let mut library = profile.library();
    library.press("import-project");
    library.apply(|state| state.scan_project());
    assert_eq!(
        library.state().import.as_ref().unwrap().error.as_deref(),
        Some("Enter a project folder first.")
    );
    assert!(library.state().import.as_ref().unwrap().scan.is_none());
}

#[test]
fn a_path_that_is_not_a_folder_shows_the_backend_reason() {
    let profile = Profile::new("imp-notfolder");
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut library = profile.library();
    library.press("import-project");
    scan(&mut library, &file.path().to_string_lossy());
    assert!(library.has("project-error"));
    assert!(library.state().import.as_ref().unwrap().scan.is_none());
}

#[test]
fn a_folder_that_defines_nothing_says_so() {
    let profile = Profile::new("imp-nothing");
    let dir = tempfile::tempdir().unwrap();
    let mut library = profile.library();
    library.press("import-project");
    scan(&mut library, &dir.path().to_string_lossy());
    assert!(library.has("project-empty"));
}

#[test]
fn a_task_the_library_already_has_is_marked_and_left_out_of_the_count() {
    let profile = Profile::new("imp-dup");
    let dir = project(&[("build", "vite build"), ("test", "vitest run")]);
    let path = dir.path().to_string_lossy().into_owned();
    let mut library = profile.library();
    library.press("import-project");
    scan(&mut library, &path);
    library.press("project-import");
    library.press("import-project");
    scan(&mut library, &path);
    assert_eq!(
        library.text_of("project-import-count").as_deref(),
        Some("0 of 0 selected")
    );
    assert_eq!(
        library.keys_starting_with("project-already-saved-").len(),
        2
    );
    assert!(library.shows("2 of these already run in this folder, so they are left alone."));
}

#[test]
fn selections_drive_the_count_one_row_a_group_then_everything() {
    let profile = Profile::new("imp-select");
    let dir = project(&[
        ("build", "vite build"),
        ("test", "vitest run"),
        ("lint", "eslint ."),
    ]);
    let mut library = profile.library();
    library.press("import-project");
    scan(&mut library, &dir.path().to_string_lossy());
    assert!(library.press("project-select-all"));
    assert_eq!(
        library.text_of("project-import-count").as_deref(),
        Some("0 of 3 selected")
    );
    assert!(library.press("project-group-all-package.json scripts"));
    assert_eq!(
        library.text_of("project-import-count").as_deref(),
        Some("3 of 3 selected")
    );
    let first = library
        .state()
        .import
        .as_ref()
        .unwrap()
        .scan
        .as_ref()
        .unwrap()
        .candidates[0]
        .name
        .clone();
    library.press(&format!("project-candidate-{first}"));
    assert_eq!(
        library.text_of("project-import-count").as_deref(),
        Some("2 of 3 selected")
    );
}

#[test]
fn imported_commands_run_in_the_folder() {
    let profile = Profile::new("imp-workdir");
    let dir = project(&[("build", "vite build")]);
    let path = dir.path().to_string_lossy().into_owned();
    let mut library = profile.library();
    library.press("import-project");
    scan(&mut library, &path);
    library.press("project-import");
    let entry = &profile
        .hub
        .library
        .list_entries(ListRequest {
            kind: Kind::Command,
            ..Default::default()
        })
        .unwrap()[0];
    assert!(entry
        .working_directory
        .starts_with(path.trim_end_matches('/')));
}

#[test]
fn enter_in_the_folder_field_scans_and_cancelling_writes_nothing() {
    let profile = Profile::new("imp-cancel");
    let dir = project(&[("build", "vite build")]);
    let mut library = profile.library();
    library.press("import-project");
    library.apply(|state| {
        state
            .import
            .as_mut()
            .unwrap()
            .path
            .replace_text(dir.path().to_string_lossy().into_owned())
    });
    // The folder field opens focused, so Enter reaches its submit, which scans.
    library.press_key(Key::Enter);
    assert!(library.state().import.as_ref().unwrap().scan.is_some());
    assert!(library.press("project-cancel"));
    assert!(library.state().import.is_none());
    assert_eq!(commands(&profile), 0);
}

#[test]
fn import_from_the_secrets_tab_moves_back_to_commands() {
    let profile = Profile::new("imp-secrets");
    let mut library = profile.library();
    library.press_ctrl('2');
    // The action row is the same on both tabs.
    assert!(library.has("open-environment"));
    assert!(library.has("tab-cliproxy"));
    assert!(library.press("import-project"));
    assert_eq!(library.state().tab, Kind::Command);
    assert!(library.has("project-import-dialog"));
}

#[test]
fn a_long_task_list_scrolls_and_the_footer_stays_reachable() {
    let profile = Profile::new("imp-long");
    let scripts: Vec<(String, String)> = (0..40)
        .map(|index| (format!("task{index:02}"), "true".to_string()))
        .collect();
    let pairs: Vec<(&str, &str)> = scripts
        .iter()
        .map(|(name, run)| (name.as_str(), run.as_str()))
        .collect();
    let dir = project(&pairs);
    let mut library = profile.library();
    library.press("import-project");
    scan(&mut library, &dir.path().to_string_lossy());
    let (footer, _) = library.box_of("project-import").unwrap();
    assert!(
        footer[1] + footer[3] <= 700.0,
        "the Import button stays inside the window"
    );
}

#[test]
fn the_environment_view_copies_a_saved_version_through_the_secret_clipboard() {
    let profile = Profile::new("env-copy");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".env"),
        "SYNTHETIC_TOKEN=not-a-real-value\n",
    )
    .unwrap();
    let _clipboard = clear_clipboard();
    let mut library = profile.library();
    library.apply(|state| state.open_environment(dir.path().to_string_lossy().into_owned()));
    let id = library
        .state()
        .environment
        .as_ref()
        .unwrap()
        .project()
        .unwrap()
        .snapshots[0]
        .id
        .clone();
    assert!(library.press(&format!("environment-copy-{id}")));
    assert_eq!(
        clipboard_text().as_deref(),
        Some("SYNTHETIC_TOKEN=not-a-real-value\n")
    );
    assert!(library.has("environment-notice"));
}

#[test]
fn a_missing_project_folder_still_allows_copying_saved_versions() {
    let profile = Profile::new("env-missing");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".env"), "SYNTHETIC_TOKEN=kept\n").unwrap();
    let root = dir.path().to_string_lossy().into_owned();
    profile.hub.library.sync_environment(root.clone()).unwrap();
    drop(dir);
    let _clipboard = clear_clipboard();
    let mut library = profile.library();
    library.apply(|state| state.open_environment(String::new()));
    library.apply(|state| state.environment.as_mut().unwrap().root = root.clone());
    let id = library
        .state()
        .environment
        .as_ref()
        .unwrap()
        .project()
        .unwrap()
        .snapshots[0]
        .id
        .clone();
    library.press(&format!("environment-copy-{id}"));
    assert_eq!(clipboard_text().as_deref(), Some("SYNTHETIC_TOKEN=kept\n"));
}

#[test]
fn pausing_sync_is_stored_and_a_locked_vault_refuses_the_view() {
    let profile = Profile::new("env-pause");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".env"), "A=1\n").unwrap();
    let mut library = profile.library();
    library.apply(|state| state.open_environment(dir.path().to_string_lossy().into_owned()));
    assert!(library.press("environment-toggle"));
    assert!(
        !library
            .state()
            .environment
            .as_ref()
            .unwrap()
            .project()
            .unwrap()
            .enabled
    );
    library.press("environment-close");
    let mut effects = dev_console_core::Effects::default();
    profile.hub.library.lock_vault(&mut effects).unwrap();
    library.apply(|state| state.open_environment(String::new()));
    assert!(library.has("environment-error"));
    assert!(library
        .keys_starting_with("environment-version-")
        .is_empty());
}

#[test]
fn tasks_saved_after_a_scan_are_reported_without_duplicates() {
    let profile = Profile::new("import-race");
    let dir = project(&[("build", "true")]);
    let mut library = profile.library();
    library.press("import-project");
    scan(&mut library, dir.path().to_str().unwrap());
    profile
        .hub
        .library
        .import_project(
            dev_console_core::models::ImportProjectRequest {
                path: dir.path().to_string_lossy().into_owned(),
                names: Vec::new(),
                all: true,
            },
            &mut dev_console_core::Effects::default(),
        )
        .unwrap();
    library.apply(|state| state.confirm_import());
    assert_eq!(commands(&profile), 1);
    assert!(library
        .state()
        .import
        .as_ref()
        .unwrap()
        .notice
        .as_ref()
        .unwrap()
        .contains("already saved"));
    assert!(library
        .state()
        .import
        .as_ref()
        .unwrap()
        .scan
        .as_ref()
        .unwrap()
        .candidates
        .iter()
        .all(|row| row.already_imported));
}

#[test]
fn a_vanished_task_is_reported_and_the_remaining_tasks_are_rescanned() {
    let profile = Profile::new("import-vanished");
    let dir = project(&[("build", "true"), ("test", "true")]);
    let mut library = profile.library();
    library.press("import-project");
    scan(&mut library, dir.path().to_str().unwrap());
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"synthetic","scripts":{"build":"true"}}"#,
    )
    .unwrap();
    library.apply(|state| state.confirm_import());
    assert_eq!(commands(&profile), 1);
    let panel = library.state().import.as_ref().unwrap();
    assert!(panel.error.as_ref().unwrap().contains("could not be saved"));
    assert_eq!(panel.scan.as_ref().unwrap().candidates.len(), 1);
    assert!(panel.scan.as_ref().unwrap().candidates[0].already_imported);
}

#[test]
fn escape_dismisses_import_and_restores_focus_to_its_opener() {
    let profile = Profile::new("import-focus");
    let mut library = profile.library();
    library.press("import-project");
    assert!(library.state().import.is_some());
    library.press_key(Key::Escape);
    assert!(library.state().import.is_none());
    assert_eq!(library.focused_key().as_deref(), Some("import-project"));
    assert_eq!(commands(&profile), 0);
}

#[test]
fn the_import_footer_fits_the_minimum_window_with_a_long_task_list() {
    let profile = Profile::new("import-minimum");
    let tasks: Vec<_> = (0..40).map(|i| (format!("task-{i:02}"), "true")).collect();
    let refs: Vec<_> = tasks
        .iter()
        .map(|(name, command)| (name.as_str(), *command))
        .collect();
    let dir = project(&refs);
    let state = Library::new(profile.hub.clone(), dev_console_desktop::hub::Exec::Inline);
    let mut library = Stage::from_spec(
        state,
        &dev_console_desktop::app::library_spec(),
        (760.0, 520.0),
    );
    library.press("import-project");
    scan(&mut library, dir.path().to_str().unwrap());
    let ([_, y, _, h], _) = library
        .box_of("project-import-dialog-footer")
        .expect("import footer");
    assert!(
        y >= 0.0 && y + h <= 520.0,
        "footer is outside viewport: {y} + {h}"
    );
}

#[test]
fn both_tabs_offer_the_same_action_row() {
    let profile = Profile::new("import-both-tabs");
    let mut library = profile.library();
    let actions = ["import-project", "open-environment", "tab-cliproxy"];
    for key in actions {
        assert!(library.has(key), "{key} is missing on Commands");
    }
    library.press_ctrl('2');
    for key in actions {
        assert!(library.has(key), "{key} is missing on Secrets");
    }
    assert!(library.state().import.is_none());
}
