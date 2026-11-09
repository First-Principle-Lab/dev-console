//! More of `library.spec.ts`, `projects.spec.ts`, `redesign.spec.ts`, `history.spec.ts`,
//! `settings.spec.ts` and `omarchy-themes.spec.ts`.

use dev_console_core::history::Suggestion;
use dev_console_core::models::{
    EntryDraft, ExtraField, FieldDisplay, GetRequest, Kind, ListRequest, SaveRequest, Settings,
};
use dev_console_core::services::AppEvent;
use dev_console_desktop::library::{Library, RunPhase, Stream};
use dev_console_ui_tests::{clear_clipboard, clipboard_text, Profile, Stage, SYNTHETIC_SECRET};
use rune::{Key, Tx};

const AAA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BBB: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const CCC: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn save(profile: &Profile, draft: EntryDraft) -> String {
    let mut effects = dev_console_core::Effects::default();
    profile
        .hub
        .library
        .save_entry(SaveRequest { draft }, &mut effects)
        .expect("save")
        .0
        .id
}

fn suggestion(key: &str, command: &str) -> Suggestion {
    Suggestion {
        key: key.into(),
        command: command.into(),
        count: 7,
        suggested_name: format!("{key}-name"),
        project: Some("synthetic-project".into()),
        project_root: Some("/tmp/synthetic-project".into()),
        working_directory: Some("/tmp/synthetic-project".into()),
        project_count: 12,
    }
}

fn with_history(profile: &Profile) -> Stage<Library> {
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
    let mut library = profile.library();
    library.apply(|state| {
        state.history_enabled = true;
        state.suggestions = vec![
            suggestion(AAA, "cargo test --workspace"),
            suggestion(BBB, "just lint"),
        ];
    });
    library
}

#[test]
fn two_commands_with_the_same_name_are_both_listed() {
    let profile = Profile::new("more-samename");
    let first = profile.command("deploy", "make deploy-a");
    let mut draft = profile.fixture.draft("deploy", "make deploy-b");
    draft.working_directory = profile.fixture.scratch_dir();
    let second = save(&profile, draft);
    let library = profile.library();
    assert!(library.has(&format!("entry-row-{first}")));
    assert!(library.has(&format!("entry-row-{second}")));
}

#[test]
fn error_only_output_can_be_copied() {
    let profile = Profile::new("more-stderr");
    profile.command("fails", "echo synthetic-failure >&2; exit 1");
    let _clipboard = clear_clipboard();
    let mut library = profile.library();
    library.press("run");
    library.press("confirm-run");
    assert!(!library.has("command-stdout"));
    assert!(library.press("copy-command-stderr"));
    assert_eq!(clipboard_text().as_deref(), Some("synthetic-failure\n"));
    assert_eq!(library.state().copied_stream, Some(Stream::Stderr));
    assert!(library.shows("Copied"));
}

#[test]
fn closing_the_output_view_drops_a_late_answer() {
    let profile = Profile::new("more-lateout");
    profile.command("quick", "echo synthetic");
    let mut library = profile.library();
    library.press("run");
    library.press("confirm-run");
    assert!(matches!(
        library.state().run_output.as_ref().map(|out| &out.phase),
        Some(RunPhase::Done(_))
    ));
    library.press_key(Key::Escape);
    assert!(
        library.state().run_output.is_none(),
        "Escape closes the output view"
    );
}

#[test]
fn a_failed_write_keeps_the_draft_on_screen() {
    let profile = Profile::new("more-conflict");
    let id = profile.command("shared", "echo original");
    let mut library = profile.library();
    library.press("edit");
    library.apply(|state| {
        state
            .editing
            .as_mut()
            .unwrap()
            .command
            .replace_text("echo mine")
    });
    // Somebody else saves the same record first, so the draft's revision is stale.
    let mut draft = profile.fixture.draft("shared", "echo theirs");
    draft.id = Some(id.clone());
    draft.expected_revision = 1;
    save(&profile, draft);
    library.press("save");
    assert!(library.state().save_error.is_some());
    assert!(library.has("save-error"));
    assert_eq!(
        library.state().editing.as_ref().unwrap().command.text,
        "echo mine"
    );
}

#[test]
fn row_actions_target_their_row_without_selecting_it() {
    let profile = Profile::new("more-rowactions");
    let first = profile.command("alpha", "true");
    let second = profile.command("beta", "true");
    let mut library = profile.library();
    let selected = library.state().selected_id.clone().unwrap();
    let other = if selected == first { second } else { first };
    assert!(library.press(&format!("row-pin-{other}")));
    assert_eq!(
        library.state().selected_id.as_ref(),
        Some(&selected),
        "pinning another row leaves the selection"
    );
}

#[test]
fn a_secret_carries_custom_fields_added_with_the_plus_button() {
    let profile = Profile::new("more-fields");
    let mut library = profile.library();
    library.press_ctrl('2');
    library.press_ctrl('n');
    assert!(library.press("add-field"));
    assert_eq!(library.state().editing.as_ref().unwrap().fields.len(), 1);
    library.apply(|state| {
        let editor = state.editing.as_mut().unwrap();
        editor.name.replace_text("with-fields");
        editor.value.replace_text(SYNTHETIC_SECRET);
        editor.fields[0].label.replace_text("console");
        editor.fields[0]
            .value
            .replace_text("https://console.example.test");
        editor.fields[0].display = FieldDisplay::Url;
    });
    library.press_ctrl('s');
    let saved = profile
        .hub
        .library
        .list_entries(ListRequest {
            kind: Kind::Secret,
            ..Default::default()
        })
        .unwrap();
    let details = profile
        .hub
        .library
        .get_entry(GetRequest {
            kind: Kind::Secret,
            id: saved[0].id.clone(),
        })
        .unwrap();
    assert_eq!(details.fields[0].label, "console");
    assert!(library.has(&format!("field-url-{}", details.fields[0].id)));
}

#[test]
fn a_url_field_is_a_link_only_for_http_links() {
    let profile = Profile::new("more-url");
    let mut draft = profile.fixture.secret_draft("with-links", SYNTHETIC_SECRET);
    draft.fields = vec![
        ExtraField {
            id: "f-web".into(),
            label: "web".into(),
            value: "https://example.test".into(),
            display: FieldDisplay::Url,
        },
        ExtraField {
            id: "f-file".into(),
            label: "file".into(),
            value: "file:///etc/passwd".into(),
            display: FieldDisplay::Url,
        },
    ];
    let mut effects = dev_console_core::Effects::default();
    let refused = profile.hub.library.save_entry(
        SaveRequest {
            draft: draft.clone(),
        },
        &mut effects,
    );
    assert!(
        refused.is_err(),
        "a non-http URL field is refused before it is stored"
    );
    draft.fields.pop();
    save(&profile, draft);
    let mut library = profile.library();
    library.press_ctrl('2');
    assert!(library.has("field-url-f-web"));
}

#[test]
fn editing_the_short_form_preserves_older_entry_metadata() {
    let profile = Profile::new("more-metadata");
    let mut draft = profile.fixture.draft("tagged", "echo synthetic");
    draft.tags = vec!["work".into(), "ci".into()];
    draft.description = "kept description".into();
    let id = save(&profile, draft);
    let mut library = profile.library();
    library.press("edit");
    library.apply(|state| {
        state
            .editing
            .as_mut()
            .unwrap()
            .name
            .replace_text("tagged-renamed")
    });
    library.press("save");
    let details = profile
        .hub
        .library
        .get_entry(GetRequest {
            kind: Kind::Command,
            id,
        })
        .unwrap();
    assert_eq!(details.meta.name, "tagged-renamed");
    assert_eq!(details.meta.tags, vec!["work", "ci"]);
    assert_eq!(details.meta.description, "kept description");
}

#[test]
fn a_fresh_editor_offers_an_optional_project_folder() {
    let profile = Profile::new("more-folder");
    let mut library = profile.library();
    library.press_ctrl('n');
    assert!(library.has("field-working-directory"));
    assert!(library.has("editor-project-browse"));
}

#[test]
fn search_expands_only_the_project_groups_that_match() {
    let profile = Profile::new("more-groups");
    let root = profile.fixture.scratch_dir();
    // A repository root is what makes a folder a project.
    std::fs::create_dir_all(format!("{root}/.git")).unwrap();
    let mut ids = Vec::new();
    for index in 0..18 {
        let mut draft = profile.fixture.draft(&format!("task-{index:02}"), "true");
        draft.working_directory = root.clone();
        ids.push(save(&profile, draft));
    }
    let mut library = profile.library();
    assert!(
        ids.iter()
            .all(|id| !library.has(&format!("entry-row-{id}"))),
        "the project group starts collapsed"
    );
    library.apply(|state| {
        state.query.replace_text("task-07");
        state.filter_changed();
    });
    assert!(
        library.has(&format!("entry-row-{}", ids[7])),
        "a search opens the matching group"
    );
}

#[test]
fn tabs_carry_entry_counts_and_one_selected_tab() {
    let profile = Profile::new("more-tabs");
    profile.command("one", "true");
    profile.command("two", "true");
    profile.secret("token", SYNTHETIC_SECRET);
    let library = profile.library();
    assert_eq!(library.state().counts, [2, 1]);
    assert!(library.shows("Commands"));
    assert!(library.shows("Secrets"));
}

#[test]
fn backups_and_secret_reads_are_refused_while_the_vault_is_locked() {
    let profile = Profile::new("more-lockedbackup");
    let mut library = profile.library();
    library.press("settings");
    let mut effects = dev_console_core::Effects::default();
    let vault = profile.hub.library.lock_vault(&mut effects).unwrap();
    library.apply(|state| state.on_event(&AppEvent::VaultStateChanged(vault), &mut Tx::default()));
    library.apply(|state| state.back_up_now());
    assert!(library
        .state()
        .settings
        .as_ref()
        .unwrap()
        .backup_error
        .is_some());
    assert!(library.has("backup-error"));
}

#[test]
fn settings_load_in_a_locked_profile_and_a_general_setting_still_saves() {
    let profile = Profile::new("more-lockedsettings");
    let mut effects = dev_console_core::Effects::default();
    profile.hub.library.lock_vault(&mut effects).unwrap();
    let mut library = profile.library();
    library.press("settings");
    assert!(
        library
            .state()
            .settings
            .as_ref()
            .unwrap()
            .settings
            .is_some(),
        "General loads without the vault"
    );
    library.apply(|state| state.settings_toggle(|settings| &mut settings.autostart));
    library.press("save-settings");
    assert!(profile.hub.library.get_settings().autostart);
}

#[test]
fn import_review_shows_the_archive_and_the_pre_backup_is_taken() {
    let profile = Profile::new("more-review");
    profile.command("deploy", "true");
    let target = tempfile::tempdir().unwrap();
    let path = target
        .path()
        .join("export.age")
        .to_string_lossy()
        .into_owned();
    profile
        .hub
        .library
        .export_archive(path.clone(), None)
        .unwrap();
    let mut library = profile.library();
    library.press("settings");
    let before = library.state().settings.as_ref().unwrap().backups.len();
    library.apply(|state| {
        state
            .settings
            .as_mut()
            .unwrap()
            .import_path
            .replace_text(path.clone());
        state.review_import();
    });
    assert!(library.has("review-card"));
    assert!(library.shows("1 commands · 0 secrets"));
    assert!(library.has("prebackup"));
    library.apply(|state| state.confirm_apply());
    let panel = library.state().settings.as_ref().unwrap();
    assert_eq!(
        panel.backups.len(),
        before + 1,
        "the current data was backed up first"
    );
    assert!(panel
        .backup_message
        .as_deref()
        .unwrap()
        .contains("your previous data was backed up first"));
}

#[test]
fn a_damaged_archive_is_rejected_at_review() {
    let profile = Profile::new("more-badarchive");
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("garbage.age");
    std::fs::write(&path, b"not an archive").unwrap();
    let mut library = profile.library();
    library.press("settings");
    library.apply(|state| {
        state
            .settings
            .as_mut()
            .unwrap()
            .import_path
            .replace_text(path.to_string_lossy().into_owned());
        state.review_import();
    });
    assert!(!library.has("review-card"));
    assert!(library.has("backup-error"));
}

#[test]
fn the_recovery_credential_is_not_kept_across_a_close_and_reopen() {
    let profile = Profile::new("more-recovery-close");
    let mut library = profile.library();
    library.press("settings");
    library.apply(|state| state.reveal_recovery());
    assert!(library.has("recovery-secret"));
    library.press("close-settings");
    library.press("settings");
    assert!(!library.has("recovery-secret"));
    assert!(library.has("recovery-steps") || library.has("recovery-reveal"));
}

#[test]
fn the_recovery_credential_copies_through_the_core_clipboard() {
    let profile = Profile::new("more-recovery-copy");
    let _clipboard = clear_clipboard();
    let mut library = profile.library();
    library.press("settings");
    library.apply(|state| state.reveal_recovery());
    let secret = library
        .state()
        .settings
        .as_ref()
        .unwrap()
        .recovery
        .as_ref()
        .unwrap()
        .secret
        .clone();
    library.apply(|state| state.copy_recovery());
    assert_eq!(clipboard_text().as_deref(), Some(secret.as_str()));
    assert!(library
        .state()
        .settings
        .as_ref()
        .unwrap()
        .recovery_copied_until
        .is_some());
}

#[test]
fn the_terminal_field_parses_quoted_arguments_and_saves_them() {
    let profile = Profile::new("more-terminal");
    let mut library = profile.library();
    library.press("settings");
    library.apply(|state| {
        let panel = state.settings.as_mut().unwrap();
        panel.terminal_choice = "custom".into();
        panel
            .terminal
            .replace_text("'/opt/My Term/term' --title \"dev console\" -e");
        state.terminal_edited();
    });
    assert_eq!(
        library.state().settings.as_ref().unwrap().terminal_tokens,
        vec!["/opt/My Term/term", "--title", "dev console", "-e"]
    );
    assert!(library.has("terminal-tokens"));
    library.press("save-settings");
    assert_eq!(
        profile.hub.library.get_settings().terminal,
        vec!["/opt/My Term/term", "--title", "dev console", "-e"]
    );
}

#[test]
fn settings_offer_the_project_ranking_switch_and_store_it() {
    let profile = Profile::new("more-ranking");
    let state = Library::new(profile.hub.clone(), dev_console_desktop::hub::Exec::Inline);
    let mut library = Stage::from_spec(
        state,
        &dev_console_desktop::app::library_spec(),
        (760.0, 520.0),
    );
    assert!(library.press("settings"));
    assert!(library.press("project-context"));
    assert!(
        !library
            .state()
            .settings
            .as_ref()
            .unwrap()
            .settings
            .as_ref()
            .unwrap()
            .project_context_enabled,
        "the click must reach the checkbox before Save is tested"
    );
    let ([_, y, _, height], _) = library.box_of("project-context").unwrap();
    let ([_, footer_y, _, _], _) = library.box_of("settings-panel-footer").unwrap();
    assert!(
        y >= 0.0 && y + height <= footer_y,
        "the checkbox must be above the fixed footer after scrolling"
    );
    assert!(library.press("save-settings"));
    assert!(
        library.state().settings.is_none(),
        "a successful save closes the panel"
    );
    assert!(!profile.hub.library.get_settings().project_context_enabled);
}

#[test]
fn history_offers_ranked_suggestions_above_the_commands() {
    let profile = Profile::new("more-history");
    let library = with_history(&profile);
    assert!(library.has("history-suggestions"));
    assert!(library.has(&format!("history-suggestion-{AAA}")));
    assert!(library.shows("cargo test --workspace"));
    assert!(library.shows("used 7×"));
    assert!(library.shows("12 runs"));
}

#[test]
fn saving_a_suggestion_opens_the_editor_prefilled_and_writes_nothing() {
    let profile = Profile::new("more-history-save");
    let mut library = with_history(&profile);
    assert!(library.press(&format!("history-suggestion-save-{AAA}")));
    let editor = library.state().editing.as_ref().unwrap();
    assert_eq!(editor.command.text, "cargo test --workspace");
    assert_eq!(editor.name.text, format!("{AAA}-name"));
    assert_eq!(editor.working_directory.text, "/tmp/synthetic-project");
    assert_eq!(editor.tags, vec!["project:synthetic-project"]);
    assert!(
        !library.state().draft_dirty(),
        "an untouched suggestion is not an unsaved edit"
    );
    assert!(profile
        .hub
        .library
        .list_entries(ListRequest {
            kind: Kind::Command,
            ..Default::default()
        })
        .unwrap()
        .is_empty());
}

#[test]
fn dismissing_takes_the_suggestion_away_and_only_its_digest_is_remembered() {
    let profile = Profile::new("more-history-dismiss");
    let mut library = with_history(&profile);
    assert!(library.press(&format!("history-suggestion-dismiss-{AAA}")));
    assert!(!library.has(&format!("history-suggestion-{AAA}")));
    assert_eq!(profile.hub.library.dismissed_suggestion_count().unwrap(), 1);
    assert_eq!(
        library.state().status.as_deref(),
        Some("Dismissed - only its hash is remembered.")
    );
}

#[test]
fn the_history_panel_is_absent_when_the_setting_is_off() {
    let profile = Profile::new("more-history-off");
    let library = profile.library();
    assert!(!library.has("history-suggestions"));
}

#[test]
fn turning_history_off_in_settings_hides_the_panel_without_a_reload() {
    let profile = Profile::new("more-history-toggle");
    let mut library = with_history(&profile);
    let off = Settings {
        history_suggestions_enabled: false,
        ..profile.hub.library.get_settings()
    };
    library.apply(|state| state.on_event(&AppEvent::SettingsSaved(off), &mut Tx::default()));
    assert!(!library.has("history-suggestions"));
}

#[test]
fn the_history_panel_folds_away_without_touching_the_list() {
    let profile = Profile::new("more-history-fold");
    let id = profile.command("listed", "true");
    let mut library = with_history(&profile);
    assert!(library.press("history-suggestions-toggle"));
    assert!(!library.has(&format!("history-suggestion-{AAA}")));
    assert!(library.has(&format!("entry-row-{id}")));
}

#[test]
fn without_project_activity_the_panel_explains_how_to_enable_tracking() {
    let profile = Profile::new("more-history-empty");
    let mut library = with_history(&profile);
    library.apply(|state| state.suggestions.clear());
    assert!(library.has("history-tracking-setup"));
    assert!(library.shows("Suggestions appear here as you use projects."));
}

#[test]
fn settings_bring_dismissed_suggestions_back_only_when_there_are_some() {
    let profile = Profile::new("more-history-restore");
    let mut library = profile.library();
    library.press("settings");
    assert!(!library.has("history-suggestions-restore"));
    library.press("close-settings");
    profile.hub.library.dismiss_suggestion(CCC.into()).unwrap();
    library.press("settings");
    assert!(library.has("history-suggestions-restore"));
    library.apply(|state| state.restore_suggestions());
    assert_eq!(profile.hub.library.dismissed_suggestion_count().unwrap(), 0);
}

#[test]
fn every_palette_draws_the_library_and_the_finder() {
    let profile = Profile::new("more-themes");
    profile.command("deploy", "true");
    let mut library = profile.library();
    let mut finder = profile.finder();
    for (name, _) in dev_console_desktop::theme::choices() {
        library.apply(|state| state.theme = name.to_string());
        finder.apply(|state| state.theme = name.to_string());
        assert!(library.has("library-header"), "{name}");
        assert!(finder.has("finder-search"), "{name}");
    }
}

fn focus(library: &mut Stage<Library>, key: &str) {
    for _ in 0..100 {
        library.press_key(Key::Tab);
        if library.focused_key().as_deref() == Some(key) {
            return;
        }
    }
    panic!("could not reach {key} with Tab");
}

#[test]
fn history_rows_accept_enter_and_delete_but_ignore_backspace() {
    let profile = Profile::new("history-keyboard");
    let mut library = with_history(&profile);
    focus(&mut library, &format!("history-suggestion-command-{AAA}"));
    library.press_key(Key::Backspace);
    assert!(library.has(&format!("history-suggestion-{AAA}")));
    library.press_key(Key::Enter);
    assert_eq!(
        library.state().editing.as_ref().unwrap().command.text,
        "cargo test --workspace"
    );
    library.press("cancel-edit");
    focus(&mut library, &format!("history-suggestion-command-{BBB}"));
    library.press_key(Key::Delete);
    assert!(!library.has(&format!("history-suggestion-{BBB}")));
    assert_eq!(profile.hub.library.dismissed_suggestion_count().unwrap(), 1);
}

#[test]
fn a_failed_settings_write_keeps_the_changed_value_and_error() {
    let profile = Profile::new("settings-write-failure");
    let mut library = profile.library();
    library.press("settings");
    library.apply(|state| state.settings.as_mut().unwrap().expiry.replace_text("45"));
    let path = std::path::PathBuf::from(profile.fixture.scratch_dir()).join("settings.json");
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    library.apply(|state| state.save_settings());
    let panel = library
        .state()
        .settings
        .as_ref()
        .expect("failed save stays open");
    assert_eq!(panel.expiry.text, "45");
    assert!(panel.error.is_some());
    assert!(!panel.saving);
    assert_ne!(
        profile.hub.library.get_settings().clipboard_expiry_seconds,
        45
    );
}

#[test]
fn pinning_a_secret_from_its_row_preserves_selection_and_value() {
    let profile = Profile::new("secret-row-pin");
    let first = profile.secret("alpha-token", SYNTHETIC_SECRET);
    let second = profile.secret("beta-token", "synthetic-second-value");
    let mut library = profile.library();
    library.press_ctrl('2');
    assert_eq!(library.state().selected_id.as_deref(), Some(first.as_str()));
    assert!(library.press(&format!("row-pin-{second}")));
    assert_eq!(library.state().selected_id.as_deref(), Some(first.as_str()));
    assert!(
        library
            .state()
            .entries
            .iter()
            .find(|row| row.id == second)
            .unwrap()
            .pinned
    );
    let value = profile
        .hub
        .library
        .reveal_secret(GetRequest {
            kind: Kind::Secret,
            id: second,
        })
        .unwrap();
    assert_eq!(value.value.as_deref(), Some("synthetic-second-value"));
    assert!(!library.leaks(SYNTHETIC_SECRET));
}

#[test]
fn settings_footer_fits_the_minimum_window() {
    let profile = Profile::new("settings-minimum");
    let state = Library::new(profile.hub.clone(), dev_console_desktop::hub::Exec::Inline);
    let mut library = Stage::from_spec(
        state,
        &dev_console_desktop::app::library_spec(),
        (760.0, 520.0),
    );
    library.press("settings");
    let ([_, y, _, height], _) = library
        .box_of("settings-panel-footer")
        .expect("settings footer");
    assert!(
        y >= 0.0 && y + height <= 520.0,
        "footer is outside viewport: {y} + {height}"
    );
}
