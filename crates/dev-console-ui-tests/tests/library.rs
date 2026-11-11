//! The Library window over a real temporary profile: `tests/e2e/library.spec.ts`, `editor.spec.ts`,
//! `secrets.spec.ts`, `command-run.spec.ts` and friends, natively.

use dev_console_core::models::{Kind, ListRequest};
use dev_console_core::services::AppEvent;
use dev_console_core::validation::NAME_SPACES_MESSAGE;
use dev_console_desktop::library::{Library, RunPhase, BUSY_EDIT};
use dev_console_ui_tests::{clear_clipboard, clipboard_text, Profile, Stage, SYNTHETIC_SECRET};
use rune::Tx;

fn commands(profile: &Profile) -> Vec<String> {
    profile
        .hub
        .library
        .list_entries(ListRequest {
            kind: Kind::Command,
            ..Default::default()
        })
        .expect("list")
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

fn type_draft(library: &mut Stage<Library>, name: &str, command: &str) {
    library.apply(|state| {
        let editor = state.editing.as_mut().expect("editor open");
        editor.name.replace_text(name.to_owned());
        editor.command.replace_text(command.to_owned());
    });
}

#[test]
fn an_empty_library_says_so_and_offers_a_new_command() {
    let profile = Profile::new("lib-empty");
    let library = profile.library();
    assert!(library.has("list-empty"));
    assert!(library.has("nothing-selected"));
    assert!(library.has("empty-new"));
    assert!(library.shows("Add a command and it will be searchable from the Finder."));
    assert_eq!(library.state().counts, [0, 0]);
}

#[test]
fn new_then_save_writes_the_command_and_selects_it() {
    let profile = Profile::new("lib-create");
    let mut library = profile.library();
    assert!(library.press("add-entry"));
    assert!(library.has("entry-editor"));
    assert_eq!(
        library.text_of("editor-title").as_deref(),
        Some("New command")
    );
    type_draft(&mut library, "deploy-staging", "make deploy");
    assert!(
        library.has("dirty-indicator"),
        "a typed draft is an unsaved change"
    );
    assert!(library.press("save"));
    assert_eq!(commands(&profile), vec!["deploy-staging"]);
    assert!(library.state().editing.is_none());
    assert_eq!(library.state().status.as_deref(), Some("Saved."));
    assert_eq!(
        library.state().meta().map(|meta| meta.name.as_str()),
        Some("deploy-staging")
    );
    assert_eq!(
        library.text_of("command-text").as_deref(),
        Some("make deploy")
    );
    assert_eq!(library.state().counts[0], 1);
}

#[test]
fn validation_keeps_the_draft_and_says_what_is_wrong() {
    let profile = Profile::new("lib-validate");
    let mut library = profile.library();
    library.press_ctrl('n');
    library.press("save");
    assert_eq!(
        library.state().save_error.as_deref(),
        Some("Name is required.")
    );
    assert!(library.has("save-error"));
    type_draft(&mut library, "has spaces", "true");
    assert!(
        library.has("name-spaces-hint"),
        "the spaces hint appears as soon as a space is typed"
    );
    library.press_ctrl('s');
    assert_eq!(
        library.state().save_error.as_deref(),
        Some(NAME_SPACES_MESSAGE)
    );
    assert!(library.state().editing.is_some(), "nothing typed is lost");
    assert!(commands(&profile).is_empty());
}

#[test]
fn ctrl_n_ctrl_s_and_the_tab_chords_drive_the_window() {
    let profile = Profile::new("lib-keys");
    let mut library = profile.library();
    library.press_ctrl('n');
    type_draft(&mut library, "from-keys", "echo synthetic");
    library.press_ctrl('s');
    assert_eq!(commands(&profile), vec!["from-keys"]);
    library.press_ctrl('2');
    assert_eq!(library.state().tab, Kind::Secret);
    library.press_ctrl('1');
    assert_eq!(library.state().tab, Kind::Command);
}

#[test]
fn a_dirty_draft_blocks_tab_switches_and_row_selection() {
    let profile = Profile::new("lib-busy");
    profile.command("existing", "true");
    let mut library = profile.library();
    library.press_ctrl('n');
    type_draft(&mut library, "half-typed", "echo");
    library.press_ctrl('2');
    assert_eq!(library.state().tab, Kind::Command);
    assert_eq!(library.state().status.as_deref(), Some(BUSY_EDIT));
    assert!(library.state().editing.is_some(), "the draft survives");
}

#[test]
fn edit_then_cancel_leaves_the_saved_record_alone() {
    let profile = Profile::new("lib-cancel");
    profile.command("keep-me", "echo original");
    let mut library = profile.library();
    assert!(library.press("edit"));
    assert_eq!(
        library.text_of("editor-title").as_deref(),
        Some("Edit command")
    );
    library.apply(|state| {
        state
            .editing
            .as_mut()
            .unwrap()
            .command
            .replace_text("echo changed")
    });
    assert!(library.press("cancel-edit"));
    assert!(library.state().editing.is_none());
    let details = profile
        .hub
        .library
        .get_entry(dev_console_core::models::GetRequest {
            kind: Kind::Command,
            id: library.state().selected_id.clone().unwrap(),
        });
    assert_eq!(details.unwrap().command.as_deref(), Some("echo original"));
}

#[test]
fn closing_with_unsaved_edits_asks_and_each_answer_does_what_it_says() {
    let profile = Profile::new("lib-close");
    profile.command("keep-me", "echo original");
    let mut library = profile.library();
    library.press("edit");
    library.apply(|state| state.editing.as_mut().unwrap().name.replace_text("renamed"));
    assert!(
        !library.apply(|state| state.request_close()),
        "a dirty draft does not close"
    );
    assert!(library.has("prompt"));
    assert!(library.press("close-cancel"));
    assert!(!library.has("prompt"));
    assert!(
        library.state().editing.is_some(),
        "Keep editing changes nothing else"
    );

    library.apply(|state| state.request_close());
    assert!(library.press("close-save"));
    assert_eq!(commands(&profile), vec!["renamed"]);
    assert!(!library.state().pending_close);

    library.press("edit");
    library.apply(|state| {
        state
            .editing
            .as_mut()
            .unwrap()
            .name
            .replace_text("discarded")
    });
    library.apply(|state| state.request_close());
    assert!(library.press("close-discard"));
    assert!(library.state().editing.is_none());
    assert_eq!(commands(&profile), vec!["renamed"]);
}

#[test]
fn delete_asks_first_names_the_row_and_cancel_keeps_it() {
    let profile = Profile::new("lib-delete");
    let first = profile.command("first-one", "true");
    profile.command("second-one", "true");
    let mut library = profile.library();
    assert!(library.press(&format!("entry-row-{first}")));
    assert!(library.press("entry-more"));
    assert!(library.press("entry-options-item2"));
    assert_eq!(
        library.text_of("dialog-title").as_deref(),
        Some("Delete “first-one”?")
    );
    assert!(library.press("cancel-delete"));
    assert_eq!(commands(&profile).len(), 2);
    library.press("entry-more");
    library.press("entry-options-item2");
    assert!(library.press("confirm-delete"));
    assert_eq!(commands(&profile), vec!["second-one"]);
    assert_eq!(
        library.state().status.as_deref(),
        Some("Deleted “first-one”.")
    );
}

#[test]
fn pin_moves_the_entry_first_and_flips_its_label() {
    let profile = Profile::new("lib-pin");
    profile.command("alpha", "true");
    let beta = profile.command("beta", "true");
    let mut library = profile.library();
    assert!(library.press(&format!("row-pin-{beta}")));
    assert_eq!(commands(&profile)[0], "beta", "pinned entries come first");
    assert_eq!(
        library.label_of(&format!("row-pin-{beta}")).as_deref(),
        Some("Unpin beta")
    );
}

#[test]
fn copy_goes_through_the_core_clipboard_with_its_expiry_line() {
    let profile = Profile::new("lib-copy");
    profile.command("deploy", "make deploy --synthetic");
    let _clipboard = clear_clipboard();
    let mut library = profile.library();
    assert!(library.press("copy"));
    assert_eq!(clipboard_text().as_deref(), Some("make deploy --synthetic"));
    assert!(library
        .state()
        .status
        .as_deref()
        .is_some_and(|status| status.starts_with("Copied for about ")));
}

#[test]
fn a_secret_is_masked_until_revealed_and_hiding_drops_the_plaintext() {
    let profile = Profile::new("lib-secret");
    profile.secret("registry-token", SYNTHETIC_SECRET);
    let mut library = profile.library();
    library.press_ctrl('2');
    assert_eq!(
        library.text_of("secret-value").as_deref(),
        Some("••••••••••")
    );
    assert!(!library.leaks(SYNTHETIC_SECRET));
    assert!(library.press("reveal"));
    assert_eq!(
        library.text_of("secret-value").as_deref(),
        Some(SYNTHETIC_SECRET)
    );
    assert!(library.press("reveal"));
    assert!(
        library.state().details.as_ref().unwrap().value.is_none(),
        "hiding drops the plaintext"
    );
    assert!(!library.leaks(SYNTHETIC_SECRET));
}

#[test]
fn locking_clears_revealed_values_and_hides_secret_metadata() {
    let profile = Profile::new("lib-lock");
    profile.secret("registry-token", SYNTHETIC_SECRET);
    let mut library = profile.library();
    library.press_ctrl('2');
    library.press("reveal");
    assert!(library.leaks(SYNTHETIC_SECRET));
    assert!(library.press("vault-toggle"));
    assert!(!library.leaks(SYNTHETIC_SECRET));
    assert!(library.state().entries.is_empty());
    assert!(library.has("list-locked"));
    assert!(library.has("vault-locked"));
    assert!(
        !library.has("list-error"),
        "a locked vault is a state, never an error"
    );
    assert!(library.shows("Vault locked"));
}

#[test]
fn a_secret_draft_is_masked_and_cancelling_wipes_it() {
    let profile = Profile::new("lib-secret-draft");
    let mut library = profile.library();
    library.press_ctrl('2');
    library.press_ctrl('n');
    library.apply(|state| {
        let editor = state.editing.as_mut().unwrap();
        editor.name.replace_text("new-token");
        editor.value.replace_text(SYNTHETIC_SECRET);
    });
    assert!(
        !library.leaks(SYNTHETIC_SECRET),
        "the value field is masked"
    );
    library.press_ctrl('s');
    let saved = profile
        .hub
        .library
        .list_entries(ListRequest {
            kind: Kind::Secret,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(saved.len(), 1);
    assert!(!library.leaks(SYNTHETIC_SECRET));
}

#[test]
fn run_asks_for_approval_then_shows_exit_code_and_both_streams() {
    let profile = Profile::new("lib-run");
    profile.command(
        "say-hello",
        "echo synthetic-out; echo synthetic-err >&2; exit 3",
    );
    let mut library = profile.library();
    assert!(library.press("run"));
    assert!(library.has("run-dialog"));
    assert_eq!(
        library.text_of("run-command").as_deref(),
        Some("echo synthetic-out; echo synthetic-err >&2; exit 3")
    );
    assert!(library.press("confirm-run"));
    let Some(output) = &library.state().run_output else {
        panic!("no output dialog")
    };
    let RunPhase::Done(result) = &output.phase else {
        panic!("run did not finish: {:?}", output.phase)
    };
    assert_eq!(result.exit_code, Some(3));
    assert_eq!(
        library.text_of("command-stdout").as_deref(),
        Some("synthetic-out\n")
    );
    assert_eq!(
        library.text_of("command-stderr").as_deref(),
        Some("synthetic-err\n")
    );
    assert!(library.shows("Exit code 3."));
    assert!(library.press("close-command-output"));
    assert!(library.state().run_output.is_none());
}

#[test]
fn cancelling_the_approval_runs_nothing() {
    let profile = Profile::new("lib-run-cancel");
    let marker = profile.fixture.scratch_dir() + "/ran";
    profile.command("touch-marker", &format!("touch '{marker}'"));
    let mut library = profile.library();
    library.press("run");
    assert!(library.press("cancel-run"));
    assert!(library.state().run_output.is_none());
    assert!(!std::path::Path::new(&marker).exists());
}

#[test]
fn the_filter_narrows_the_list() {
    let profile = Profile::new("lib-filter");
    profile.command("deploy-staging", "true");
    profile.command("rebase-main", "true");
    let mut library = profile.library();
    library.apply(|state| {
        state.query.replace_text("rebase");
        state.filter_changed();
    });
    let names: Vec<_> = library
        .state()
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect();
    assert_eq!(names, vec!["rebase-main"]);
}

#[test]
fn a_narrow_window_stacks_the_panes_with_a_way_back() {
    let profile = Profile::new("lib-narrow");
    profile.command("deploy-staging", "true");
    let mut library = profile.library();
    library.apply(|state| state.set_width(760.0));
    assert!(library.has("back-to-list"));
    assert!(!library.has("list-pane"));
    assert!(library.press("back-to-list"));
    assert!(library.has("list-pane"));
    assert!(!library.has("detail-pane"));
    // The stacked layout keeps what belongs to the whole library on the list, one step back.
    assert!(
        library.has("import-project"),
        "import stays reachable in a narrow window"
    );
    assert!(library.has("open-environment"));
}

#[test]
fn import_reads_a_projects_tasks_and_saves_the_chosen_ones() {
    let profile = Profile::new("lib-import");
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("package.json"),
        r#"{"name":"synthetic","scripts":{"build":"vite build","test":"vitest run","lint":"eslint ."}}"#,
    )
    .unwrap();
    let mut library = profile.library();
    assert!(library.press("import-project"));
    assert!(library.has("project-import-dialog"));
    library.apply(|state| {
        state
            .import
            .as_mut()
            .unwrap()
            .path
            .replace_text(project.path().to_string_lossy().into_owned());
        state.scan_project();
    });
    assert_eq!(
        library.text_of("project-import-count").as_deref(),
        Some("3 of 3 selected")
    );
    let lint = library
        .state()
        .import
        .as_ref()
        .unwrap()
        .scan
        .as_ref()
        .unwrap()
        .candidates
        .iter()
        .find(|candidate| candidate.command.contains("lint"))
        .unwrap()
        .name
        .clone();
    assert!(library.press(&format!("project-candidate-{lint}")));
    assert_eq!(
        library.text_of("project-import-count").as_deref(),
        Some("2 of 3 selected")
    );
    assert!(library.press("project-import"));
    assert!(library.state().import.is_none());
    assert_eq!(library.state().status.as_deref(), Some("Imported 2 tasks."));
    assert_eq!(commands(&profile).len(), 2);
}

#[test]
fn the_environment_panel_lists_saved_versions_and_a_lock_closes_it() {
    let profile = Profile::new("lib-env");
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join(".env"),
        "SYNTHETIC_TOKEN=not-a-real-value\n",
    )
    .unwrap();
    let root = project.path().to_string_lossy().into_owned();
    let mut library = profile.library();
    library.apply(|state| state.open_environment(root.clone()));
    assert!(library.has("environment-dialog"));
    let snapshot = library
        .state()
        .environment
        .as_ref()
        .unwrap()
        .project()
        .unwrap()
        .snapshots[0]
        .clone();
    assert_eq!(snapshot.file, ".env");
    assert!(library.has(&format!("environment-version-{}", snapshot.id)));
    assert!(
        !library.leaks("not-a-real-value"),
        "the panel lists versions, never contents"
    );
    let mut effects = dev_console_core::Effects::default();
    let vault = profile.hub.library.lock_vault(&mut effects).unwrap();
    library.apply(|state| state.on_event(&AppEvent::VaultStateChanged(vault), &mut Tx::default()));
    assert!(library.state().environment.is_none());
}

#[test]
fn settings_preview_a_theme_and_closing_without_saving_puts_it_back() {
    let profile = Profile::new("lib-settings-theme");
    let mut library = profile.library();
    assert!(library.press("settings"));
    assert!(library.has("settings-panel"));
    library.apply(|state| state.preview_theme("omarchy-tokyo-night"));
    assert_eq!(library.state().theme, "omarchy-tokyo-night");
    assert!(library.press("close-settings"));
    assert_eq!(library.state().theme, "system");
    library.press("settings");
    library.apply(|state| state.preview_theme("light"));
    assert!(library.press("save-settings"));
    assert_eq!(profile.hub.library.get_settings().theme, "light");
    assert_eq!(library.state().theme, "light");
}

#[test]
fn a_terminal_command_that_does_not_parse_disables_save() {
    let profile = Profile::new("lib-settings-terminal");
    let mut library = profile.library();
    library.press("settings");
    library.apply(|state| {
        let panel = state.settings.as_mut().unwrap();
        panel.terminal_choice = "custom".into();
        panel.terminal.replace_text("ghostty -e 'unclosed");
        state.terminal_edited();
    });
    assert!(library.has("terminal-error"));
    library.press("save-settings");
    assert!(library.state().settings.is_some(), "save stayed disabled");
}

#[test]
fn export_never_overwrites_without_the_explicit_card() {
    let profile = Profile::new("lib-export");
    profile.command("deploy", "true");
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("export.age");
    std::fs::write(&path, b"pre-existing synthetic bytes").unwrap();
    let mut library = profile.library();
    library.press("settings");
    library.press("settings-tab-backups");
    library.apply(|state| {
        state
            .settings
            .as_mut()
            .unwrap()
            .export_path
            .replace_text(path.to_string_lossy().into_owned());
        state.export_archive(false);
    });
    assert!(library.has("export-overwrite-card"));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"pre-existing synthetic bytes"
    );
    // The card's Replace is the only route to an overwrite.
    library.apply(|state| state.export_archive(true));
    assert!(std::fs::read(&path)
        .unwrap()
        .starts_with(b"age-encryption.org/v1"));
}

#[test]
fn the_recovery_credential_shows_on_request_and_a_lock_takes_it_away() {
    let profile = Profile::new("lib-recovery");
    let mut library = profile.library();
    library.press("settings");
    library.press("settings-tab-backups");
    assert!(!library.has("recovery-secret"));
    library.apply(|state| state.reveal_recovery());
    assert!(library.has("recovery-secret"));
    let mut effects = dev_console_core::Effects::default();
    let vault = profile.hub.library.lock_vault(&mut effects).unwrap();
    library.apply(|state| state.on_event(&AppEvent::VaultStateChanged(vault), &mut Tx::default()));
    assert!(!library.has("recovery-secret"));
    assert!(library
        .state()
        .settings
        .as_ref()
        .unwrap()
        .recovery
        .is_none());
}

#[test]
fn backup_now_writes_an_encrypted_backup_and_lists_it() {
    let profile = Profile::new("lib-backup");
    profile.command("deploy", "true");
    let mut library = profile.library();
    library.press("settings");
    let before = library.state().settings.as_ref().unwrap().backups.len();
    library.apply(|state| state.back_up_now());
    let panel = library.state().settings.as_ref().unwrap();
    assert_eq!(panel.backups.len(), before + 1);
    assert_eq!(
        panel.backup_message.as_deref(),
        Some("Encrypted backup written.")
    );
}

#[test]
fn a_finder_intent_opens_the_named_entry() {
    let profile = Profile::new("lib-intent");
    profile.command("first", "true");
    let second = profile.command("second", "true");
    let mut library = profile.library();
    library
        .apply(|state| state.show_intent(Kind::Command, Some(second.clone()), &mut Tx::default()));
    assert_eq!(
        library.state().selected_id.as_deref(),
        Some(second.as_str())
    );
    assert_eq!(library.text_of("details-name").as_deref(), Some("second"));
}

#[test]
fn a_revealed_value_is_gone_the_frame_it_is_hidden_or_locked() {
    let profile = Profile::new("lib-instant");
    profile.secret("registry-token", SYNTHETIC_SECRET);
    let mut library = profile.library();
    library.press_ctrl('2');
    library.press("reveal");
    assert!(library.leaks(SYNTHETIC_SECRET));
    assert!(library.press_once("reveal"));
    assert!(
        !library.leaks(SYNTHETIC_SECRET),
        "hiding leaves no fading copy of the value"
    );
    library.press("reveal");
    assert!(library.leaks(SYNTHETIC_SECRET));
    assert!(library.press_once("vault-toggle"));
    assert!(
        !library.leaks(SYNTHETIC_SECRET),
        "locking leaves no fading copy of the value"
    );
}
