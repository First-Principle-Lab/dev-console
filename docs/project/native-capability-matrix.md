# Native capability matrix: every shipped behaviour, and what the Rune shell has behind it

Checkpoint C6 of plan [`../plans/rune-migration.md`](../plans/rune-migration.md) ("port all feature
screens") asks for more than a list of screens that exist. It asks for **every user-visible
behaviour the shipped app has** to be mapped either to the native test that now decides it or to an
explicit statement of what is still missing. This document is that map.

**Read the summary first.** Of the 195 behaviours inventoried below, **120 are decided by a native
test**, **17 are partly decided** and **58 are not decided by a native test**. The screens are built:
Library (list, project groups, details, editor, extra fields, run approval and output, delete and
unsaved-changes dialogs, narrow layout), Finder (search, completion, copy, project context, locked
vault), history suggestions, project import, project environment and Settings (themes, terminal,
backups, export, import review, recovery credential). They run on the real core services against a
real profile, and the scripted tests in `crates/dev-console-ui-tests/tests/` drive them headlessly
over temporary profiles.

Of the 58 rows no native test decides, 27 are **paint**: what a row, a focus ring, a truncation or a
screenshot looks like. A headless runtime cannot decide that, and those rows stay open here even
though the screens were captured and checked by hand on the owner's Hyprland/NVIDIA desktop on
2026-11-08 (`docs/project/native-feasibility-ledger.md`). The other 31 are named below with the seam
each waits on. The updater (4 rows) is not ported; it is plan milestone 5.

## Why a document this blunt is a gate rather than a confession

A matrix written once rots in three ways, all of them flattering: a row quietly keeps claiming
coverage after the test behind it is renamed; a new spec appears and is never mapped; and a vague
"not there yet" hides *which* missing seam it is waiting on. So the file is parsed by
`cargo test -p dev-console-desktop --lib matrix_tests` (`crates/dev-console-desktop/src/matrix_tests.rs`),
which fails when:

- a `test(...)`/`it(...)` title in `tests/e2e/*.spec.ts` or `tests/unit/*.test.ts` has no row here,
  or a row names a title the specs no longer contain;
- a row cites a backticked `snake_case` name that is not a `#[test]` function in this workspace, so
  a renamed or deleted test turns the row red instead of leaving it confidently wrong;
- a `NOT COVERED` or `PARTIAL` row does not name its missing seams from a fixed vocabulary, or a
  `NATIVE-TEST` row names one (a row cannot both claim coverage and admit a gap);
- the summary tables at the bottom stop matching the rows above them.

A row moves up only by the test existing first.

## What counts as covered

| Status | What it claims |
| --- | --- |
| `NATIVE-TEST` | A `#[test]` in this workspace decides the same observable the spec asserts, on a host with no display. |
| `PARTIAL` | Part of the observable is decided by a named native test and the rest is not; the cell says which is which and names the rest as a seam. |
| `NOT COVERED` | No test decides it. The cell names the seam it waits on, and says when it was checked by hand instead. |

Two rules keep the labels honest in both directions:

- **A `NATIVE-TEST` row may not name a missing seam.** If something is missing, the row is
  `PARTIAL` or `NOT COVERED`.
- **A hand check is not a test.** A behaviour seen working on the owner's desktop is recorded as
  such in its cell and in the feasibility ledger, and its row keeps the status its tests earn.

## What the inventory covers, and where it is thinner than the app

| Source | What it stands for | Titles here | Runs it describes |
| --- | --- | --- | --- |
| `tests/e2e/*.spec.ts` (Playwright) | the shipped app's user-visible behaviour, as the repository itself defines it | 145 | 187 |
| `tests/unit/*.test.ts` (vitest) | the pure logic the views call: argv, validation, Finder selection, tokens, contrast | 24 | 24 |

Six of those e2e titles are loops, so a row can stand for several runs: the Omarchy palette render
covers 22 palettes, the long-list footer 12 (4 themes x 3 sizes), and three screenshot sets 3 or 4
viewports/themes each.

The inventory lists what the specs assert. Where the Svelte frontend does something no spec pins
down, this matrix cannot see it either, so the real gap is at least as wide as the `NOT COVERED`
count - never narrower.

## The Playwright specs

Behaviour titles are quoted exactly as the spec file writes them, so `grep` finds the test.

### tests/e2e/environment.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `project Environment shows dated versions, copies old files and pauses sync` | NATIVE-TEST | `the_environment_panel_lists_saved_versions_and_a_lock_closes_it` lists the dated version, `the_environment_view_copies_a_saved_version_through_the_secret_clipboard` copies it, `pausing_sync_is_stored_and_a_locked_vault_refuses_the_view` pauses sync. |
| `a missing project folder still allows copying saved versions` | NATIVE-TEST | `a_missing_project_folder_still_allows_copying_saved_versions` deletes the folder after a sync and copies the kept version. |
| `Browse starts an empty project and a locked vault rejects access` | PARTIAL | The locked refusal is decided by `pausing_sync_is_stored_and_a_locked_vault_refuses_the_view`. Browse goes through the XDG portal picker, which a headless runtime cannot answer; Checked by hand on the owner's Hyprland/NVIDIA desktop on 2026-11-08 (see the feasibility ledger), which is evidence but not a test. Seam: folder-picker |
| `locking the vault closes the environment view and clears its metadata` | NATIVE-TEST | `the_environment_panel_lists_saved_versions_and_a_lock_closes_it` delivers the lock event and the panel is gone. |

### tests/e2e/finder.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `the first result is selected and labelled for assistive tech` | PARTIAL | Selection and the row's accessible name are decided by `the_first_result_is_selected_and_labelled_for_assistive_tech`; Rune at this revision has no `aria-selected` equivalent to assert. Seam: a11y-semantics |
| `Up and Down move the selection and clamp at both ends` | NATIVE-TEST | `up_and_down_move_the_selection_and_clamp_at_both_ends` drives the keys through the real field. |
| `Tab completes the selected name without copying anything` | NATIVE-TEST | `tab_completes_the_selected_name_without_copying_anything` checks the query and an untouched clipboard. |
| `Enter copies exactly the selected record and hides only after success` | PARTIAL | The copy, its target and the status line are decided by `enter_copies_the_command_through_the_core_clipboard_and_hides_nothing_of_it` and `copying_a_secret_says_so_and_never_puts_the_value_in_the_tree`; the hide is a window request, checked by hand on the owner's Hyprland/NVIDIA desktop on 2026-11-08 (see the feasibility ledger), which is evidence but not a test. Seam: window-lifecycle |
| `a copy failure keeps the finder open with a safe message` | NATIVE-TEST | `a_copy_failure_keeps_the_finder_open_with_a_safe_message` locks the vault under the palette and checks the error line and no value. |
| `Ctrl+O hands the selected entry to the library` | PARTIAL | The Library's half, opening the named entry, is `a_finder_intent_opens_the_named_entry`; the hand-off between two windows checked by hand on the owner's Hyprland/NVIDIA desktop on 2026-11-08 (see the feasibility ledger), which is evidence but not a test. Seam: window-lifecycle |
| `Escape clears the query and hides the finder` | PARTIAL | Clearing is `escape_clears_the_query_and_the_rows`; the hide is a window request. Checked by hand on the owner's Hyprland/NVIDIA desktop on 2026-11-08 (see the feasibility ledger), which is evidence but not a test. Seam: window-lifecycle |
| `keys still work after a click moved focus onto a row` | NATIVE-TEST | `keys_still_work_after_a_click_moved_focus_onto_a_row` presses a row, then Down. |
| `with no matches Tab and Enter do nothing` | NATIVE-TEST | `with_no_matches_tab_and_enter_do_nothing`. |
| `a stale asynchronous response cannot replace newer results` | NATIVE-TEST | `a_stale_asynchronous_response_cannot_replace_newer_results` hands the palette the answer to a replaced query. |
| `a locked vault removes all secret metadata from the results` | NATIVE-TEST | `a_locked_vault_hides_secrets_and_offers_an_unlock_row`. |
| `reopening starts with a fresh query instead of the previous search` | NATIVE-TEST | `every_appearance_starts_from_an_empty_query_and_a_fresh_search`. |
| `typing shows the matching saved entries with their tags` | NATIVE-TEST | `typing_shows_the_matching_saved_entries_with_their_tags` and `typing_searches_and_shows_the_inline_completion`. |
| `an empty query lists the most recently used entries first` | NATIVE-TEST | `an_empty_query_lists_the_most_recently_used_entries_first` copies one entry and reopens. |
| `the palette says which project it was opened from, and ranks it first` | NATIVE-TEST | `a_row_from_the_current_project_is_labelled_and_ranked_first`. |
| `a row from the current project is labelled for assistive tech` | NATIVE-TEST | `a_row_from_the_current_project_is_labelled_and_ranked_first` reads the marker's accessible name. |
| `clicking the project chip ranks everything again, without closing the palette` | NATIVE-TEST | `clicking_the_project_chip_ranks_everything_again_without_closing_the_palette`. |
| `Ctrl+P ignores the project from the keyboard` | NATIVE-TEST | `a_detected_project_shows_a_chip_resets_the_selection_and_ctrl_p_ignores_it`. |
| `with no detected project, the palette looks and orders exactly as it used to` | NATIVE-TEST | `with_no_detected_project_there_is_no_chip_and_no_marker`. |
| `typing narrows the list without dropping the project marker` | NATIVE-TEST | `typing_over_a_project_preserves_its_marker_and_ranking`. |
| `with project ranking switched off, a detected project changes nothing` | PARTIAL | The switch is stored (`settings_offer_the_project_ranking_switch_and_store_it`) and `Hub::show_finder` skips detection when it is off, but no native test opens the palette with it off. Seam: search-ranking |
| `a secret tagged for the project is not moved up or marked` | NATIVE-TEST | `a_secret_tagged_for_the_project_is_not_moved_up_or_marked`. |
| `a pinned command still leads the empty query, ahead of the project` | NATIVE-TEST | `a_pinned_command_leads_even_the_detected_project`. |
| `a project detected after the first search moves the selection to its first command` | NATIVE-TEST | `a_project_detected_after_the_first_search_moves_the_selection_to_its_first_command`. |

### tests/e2e/history.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `offers the ranked history above the commands, minus what is already saved` | NATIVE-TEST | `rescan_ranks_five_projects_filters_launchers_and_suppresses_only_the_saved_project`. |
| `saving opens the editor prefilled and writes nothing until it is confirmed` | NATIVE-TEST | `saving_a_suggestion_opens_the_editor_prefilled_and_writes_nothing`. |
| `dismissing takes the suggestion away and is the only thing remembered` | NATIVE-TEST | `dismissing_takes_the_suggestion_away_and_only_its_digest_is_remembered`. |
| `the panel is absent entirely when the setting is off` | NATIVE-TEST | `the_history_panel_is_absent_when_the_setting_is_off`. |
| `turning the setting off in Settings hides the panel without a reload` | NATIVE-TEST | `turning_history_off_in_settings_hides_the_panel_without_a_reload`. |
| `the panel is keyboard-only reachable: focus, Enter saves, Delete dismisses` | NATIVE-TEST | `history_rows_accept_enter_and_delete_but_ignore_backspace`. |
| `rescan picks up commands added to the history afterwards` | NATIVE-TEST | `rescan_ranks_five_projects_filters_launchers_and_suppresses_only_the_saved_project`. |
| `the panel folds away without touching the list underneath it` | NATIVE-TEST | `the_history_panel_folds_away_without_touching_the_list`. |
| `without project activity the panel explains how to enable tracking` | NATIVE-TEST | `without_project_activity_the_panel_explains_how_to_enable_tracking`. |
| `Settings brings every dismissed suggestion back, and the panel offers it again` | NATIVE-TEST | `settings_bring_dismissed_suggestions_back_only_when_there_are_some`. |
| `with nothing dismissed, Settings shows no bring-back button` | NATIVE-TEST | `settings_bring_dismissed_suggestions_back_only_when_there_are_some`. |
| `Backspace on a suggestion does nothing: only Delete dismisses` | NATIVE-TEST | `history_rows_accept_enter_and_delete_but_ignore_backspace`. |
| `every row keeps its name, count and buttons on one line, under the command, in the narrow list pane` | NOT COVERED | A layout property of drawn rows. Seam: paint |
| `project history has an unboxed pointer state and a visible keyboard focus ring` | NOT COVERED | Hover and focus rings are paint. Seam: paint |

### tests/e2e/library.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `library lists both same-named commands and shows the selected record` | NATIVE-TEST | `two_commands_with_the_same_name_are_both_listed` and `new_then_save_writes_the_command_and_selects_it`. |
| `creating a command persists through the backend and refreshes the list` | NATIVE-TEST | `new_then_save_writes_the_command_and_selects_it` reads the record back through the service. |
| `a newly saved ordinary command shows stdout, stderr and its exit code in the app` | NATIVE-TEST | `run_asks_for_approval_then_shows_exit_code_and_both_streams` runs a real process. |
| `error-only command output can be copied and clipboard failures stay visible` | PARTIAL | Copying stderr is `error_only_output_can_be_copied`; no native test makes the clipboard provider fail. Seam: clipboard |
| `Escape closes confirmation, output, and unsaved changes dialogs` | PARTIAL | Escape on the output view is `closing_the_output_view_drops_a_late_answer`; Escape on the delete confirmation was checked by hand on the owner's desktop. The unsaved prompt's Escape route is Rune's modal dismiss with no native test. Seam: focus-management |
| `sudo commands use the configured terminal after confirmation` | NATIVE-TEST | `installed_terminal_selection_is_saved_and_sudo_runs_only_after_approval`. |
| `a cancelled edit leaves the saved record untouched` | NATIVE-TEST | `edit_then_cancel_leaves_the_saved_record_alone`. |
| `a failed write keeps the draft on screen` | NATIVE-TEST | `a_failed_write_keeps_the_draft_on_screen` loses a revision race. |
| `pin, copy and delete confirmation all act on the intended record` | NATIVE-TEST | `delete_asks_first_names_the_row_and_cancel_keeps_it`, `pin_moves_the_entry_first_and_flips_its_label` and `copy_goes_through_the_core_clipboard_with_its_expiry_line`. |
| `sidebar actions target their row without selecting it, including secret pinning` | NATIVE-TEST | `row_actions_target_their_row_without_selecting_it`, `pinning_a_secret_from_its_row_preserves_selection_and_value`. |
| `secrets are masked in details and only appear through an explicit reveal` | NATIVE-TEST | `a_secret_is_masked_until_revealed_and_hiding_drops_the_plaintext`. |
| `a new secret needs only name, value and notes` | NATIVE-TEST | `a_secret_draft_is_masked_and_cancelling_wipes_it` saves one from name and value. |
| `a secret can carry optional custom fields added with the plus button` | NATIVE-TEST | `a_secret_carries_custom_fields_added_with_the_plus_button`. |
| `a locked vault clears the secret list and blocks secret reads` | NATIVE-TEST | `locking_clears_revealed_values_and_hides_secret_metadata` and `a_revealed_value_is_gone_the_frame_it_is_hidden_or_locked`. |
| `editing the short form preserves older entry metadata` | NATIVE-TEST | `editing_the_short_form_preserves_older_entry_metadata`. |
| `narrow layout shows one pane instead of squeezing both` | NATIVE-TEST | `a_narrow_window_stacks_the_panes_with_a_way_back`. |
| `settings exposes encrypted backups: create, list, reviewed restore, and a rejected import` | NATIVE-TEST | `backup_now_writes_an_encrypted_backup_and_lists_it`, `import_review_shows_the_archive_and_the_pre_backup_is_taken` and `a_damaged_archive_is_rejected_at_review`. |
| `backups are refused while the vault is locked` | NATIVE-TEST | `backups_and_secret_reads_are_refused_while_the_vault_is_locked`. |
| `a URL field opens through the backend and only for http(s) links` | PARTIAL | Only http(s) values become links and anything else is refused at save (`a_url_field_is_a_link_only_for_http_links`); opening the browser is not exercised by a test. Seam: details |
| `URL hit and focus areas fit short text while long URLs wrap inside the field` | NOT COVERED | Hit areas and wrapping are layout of drawn text. Seam: paint |

### tests/e2e/omarchy-themes.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `theme choices preview immediately and Save keeps the choice` | NATIVE-TEST | `settings_preview_a_theme_and_closing_without_saving_puts_it_back` previews and saves. |
| `closing Settings discards a theme preview` | NATIVE-TEST | `settings_preview_a_theme_and_closing_without_saving_puts_it_back`. |
| `${name} renders library, secrets, editor, settings and finder` | PARTIAL | Every palette resolves every token (`every_registered_palette_resolves_every_token_the_views_paint`) and both windows build under each (`every_palette_draws_the_library_and_the_finder`); what each looks like is paint. Seam: paint |
| `Finder loads the saved theme without access to full settings and follows changes` | NATIVE-TEST | `the_finder_loads_the_saved_theme_and_follows_changes`. |

### tests/e2e/project-import.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `offers the action beside New and explains what a scan reads` | NATIVE-TEST | `the_action_sits_beside_new_and_explains_what_a_scan_reads`. |
| `refuses an empty folder without reading anything` | NATIVE-TEST | `an_empty_folder_field_is_refused_without_reading_anything`. |
| `shows the backend reason when the path is not a folder` | NATIVE-TEST | `a_path_that_is_not_a_folder_shows_the_backend_reason`. |
| `says so when a folder defines nothing it can offer` | NATIVE-TEST | `a_folder_that_defines_nothing_says_so`. |
| `groups a scanned folder and pre-selects every task it defines` | NATIVE-TEST | `import_reads_a_projects_tasks_and_saves_the_chosen_ones`. |
| `marks a task the library already has and leaves it out of the count` | NATIVE-TEST | `a_task_the_library_already_has_is_marked_and_left_out_of_the_count`. |
| `selections drive the count: one row, a whole group, then everything` | NATIVE-TEST | `selections_drive_the_count_one_row_a_group_then_everything`. |
| `imports the selection as commands that run in the folder, and shows them in the library` | NATIVE-TEST | `imported_commands_run_in_the_folder` and `import_reads_a_projects_tasks_and_saves_the_chosen_ones`. |
| `a task that reached the library after the scan is reported, never written twice` | NATIVE-TEST | `tasks_saved_after_a_scan_are_reported_without_duplicates`. |
| `an untouched folder still lists its tasks as already saved on the second scan` | NATIVE-TEST | `a_task_the_library_already_has_is_marked_and_left_out_of_the_count`. |
| `Enter in the folder field scans, and Escape leaves nothing behind` | NATIVE-TEST | `enter_in_the_folder_field_scans_and_cancelling_writes_nothing`, `escape_dismisses_import_and_restores_focus_to_its_opener`. |
| `cancelling writes nothing` | NATIVE-TEST | `enter_in_the_folder_field_scans_and_cancelling_writes_nothing`. |
| `the action is not offered while the secrets tab is showing` | NATIVE-TEST | Changed on purpose in the native shell: Import and Environment show on every tab, and Import moves to Commands first. `both_tabs_offer_the_same_action_row`, `import_from_the_secrets_tab_moves_back_to_commands`. |
| `a folder with far more tasks than fit on screen scrolls instead of growing` | NATIVE-TEST | `a_long_task_list_scrolls_and_the_footer_stays_reachable`. |
| `keeps keyboard focus inside the modal and restores it on close` | NOT COVERED | Rune's modal owns the trap and the restore; no native test walks Tab through this dialog. Seam: focus-management |
| `reports a partially vanished selection and rescans the saved rows` | NATIVE-TEST | `a_vanished_task_is_reported_and_the_remaining_tasks_are_rescanned`. |
| `long-list footer stays fully visible in ${theme} at ${width}` | PARTIAL | One palette at the default size is `a_long_task_list_scrolls_and_the_footer_stays_reachable`; the theme-by-width grid is paint. Seam: paint |
| `Browse selects and scans a folder without typing or importing` | NOT COVERED | The XDG portal picker cannot be answered headlessly. Checked by hand on the owner's Hyprland/NVIDIA desktop on 2026-11-08 (see the feasibility ledger), which is evidence but not a test. Seam: folder-picker |
| `cancelling Browse preserves the path and reviewed selection` | NOT COVERED | Cancelling the portal picker leaves the panel alone in code (`Library::browse_project`), with no native test. Seam: folder-picker |
| `a folder picker failure leaves manual path entry available` | NOT COVERED | No native test makes the portal fail. Seam: folder-picker |

### tests/e2e/projects.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `eighteen commands share one collapsed project and search expands only matches` | NATIVE-TEST | `search_expands_only_the_project_groups_that_match`. |
| `same-named projects at different roots stay separate and General stays available` | NATIVE-TEST | `same_named_projects_stay_separate_and_general_stays_available`. |
| `a tag-only command joins a uniquely identified project without changing its data` | NATIVE-TEST | `commands_group_by_project_and_general_sorts_last`. |
| `a manually saved command can be assigned with the project folder picker` | PARTIAL | The field and Browse are present (`a_fresh_editor_offers_an_optional_project_folder`); the portal picker checked by hand on the owner's Hyprland/NVIDIA desktop on 2026-11-08 (see the feasibility ledger), which is evidence but not a test. Seam: folder-picker |
| `history shows the five busiest projects and filters routine launchers` | NATIVE-TEST | `rescan_ranks_five_projects_filters_launchers_and_suppresses_only_the_saved_project`. |
| `saving a history command retains its project and does not suppress another project` | NATIVE-TEST | `saving_a_suggestion_opens_the_editor_prefilled_and_writes_nothing`, `rescan_ranks_five_projects_filters_launchers_and_suppresses_only_the_saved_project`. |

### tests/e2e/redesign.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `tabs carry entry counts and expose one selected tab` | PARTIAL | Counts are `tabs_carry_entry_counts_and_one_selected_tab`; the selected state is a checked flag with no `aria-selected` to assert. Seam: a11y-semantics |
| `secret rows show only the name and actions without exposing the value` | NATIVE-TEST | `a_secret_is_masked_until_revealed_and_hiding_drops_the_plaintext` searches the whole tree for the value. |
| `the editor reports unsaved changes and clears them on cancel` | NATIVE-TEST | `new_then_save_writes_the_command_and_selects_it` (the chip) and `edit_then_cancel_leaves_the_saved_record_alone`. |
| `locked vault hides the list and offers one clear unlock action` | NATIVE-TEST | `locking_clears_revealed_values_and_hides_secret_metadata`. |
| `finder keeps combobox/listbox semantics while typing` | PARTIAL | `the_query_field_is_a_combobox_over_a_results_listbox` checks the names; the roles themselves are not observable through the runtime's element list. Seam: a11y-semantics |
| `copying from the finder reports a copied state without the value` | NATIVE-TEST | `copying_a_secret_says_so_and_never_puts_the_value_in_the_tree`. |
| `a locked vault turns the finder into one keyboard-reachable unlock row` | PARTIAL | The unlock row is `a_locked_vault_hides_secrets_and_offers_an_unlock_row`; Enter on it raises polkit, which no test may answer. Seam: vault-state |
| `the finder footer presents keyboard hints as keycaps` | PARTIAL | `the_footer_names_the_bindings_the_field_routes`; keycaps are paint. Seam: paint |
| `narrow windows keep one pane with an explicit way back to the list` | NATIVE-TEST | `a_narrow_window_stacks_the_panes_with_a_way_back`. |
| `a locked vault is presented as a state, never as an error` | NATIVE-TEST | `locking_clears_revealed_values_and_hides_secret_metadata` asserts the state view and no error line. |
| `minimal list rows show names and actions without overflowing` | NOT COVERED | Overflow is layout of drawn text. Seam: paint |
| `a fresh editor offers an optional project folder before saving` | NATIVE-TEST | `a_fresh_editor_offers_an_optional_project_folder`. |
| `a copied confirmation does not describe the next search` | NATIVE-TEST | `a_copied_confirmation_does_not_describe_the_next_search`. |
| `the selected finder row is perceivable without colour alone, and hover is not selection` | NOT COVERED | Seam: paint |
| `a locked finder keeps secret names, types and tags out of the DOM` | NATIVE-TEST | `a_locked_vault_hides_secrets_and_offers_an_unlock_row` searches the tree for the secret's name. |
| `a long name truncates without losing the right-aligned action` | NOT COVERED | Seam: paint |
| `both themes apply through the same tokens and stay above 4.5:1 in the live DOM` | PARTIAL | Tokens resolve for every palette (`every_registered_palette_resolves_every_token_the_views_paint`); contrast is measured on the token data in core, not on painted pixels. Seam: paint |
| `reduced motion collapses the transitions and the shipped shell stays opaque` | NOT COVERED | Seam: paint |
| `keyboard focus draws the ring from --focus-ring` | NOT COVERED | Seam: paint |
| `the inspector keeps every metadata value but only one chip` | NOT COVERED | Seam: paint |
| `the action area stays put at the minimum window and keyboard focus lands on it` | NOT COVERED | Seam: responsive-layout |
| `settings, the delete dialog and the palette draw one shared shell frame` | NOT COVERED | Seam: paint |
| `the empty library and populated inspector use the same icon style and chips` | NOT COVERED | Seam: paint |

### tests/e2e/screenshots.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `finder at the real palette size (680x420)` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |
| `finder with a detected project, at the real palette size (${theme})` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |
| `library with a long name and long values (overflow check)` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |
| `settings at the real library size and at the minimum window size` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |
| `settings import review card at the real library size` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |
| `library record at the real minimum window size` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |
| `project task review at the real library size and at the minimum window size` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |
| `project task review with far more tasks than fit on screen` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |
| `library review set @ ${viewport.name}` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |
| `finder review set @ ${viewport.name}` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |
| `shell history suggestions @ desktop-1440 and mobile-390` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |
| `project import review captures in ${theme}` | NOT COVERED | Screenshots are paint. Native captures of this screen were taken by hand on the owner's desktop on 2026-11-08; they are evidence, not a test. Seam: paint |

### tests/e2e/settings.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `import requires a review that shows the archive date, counts and what gets replaced` | NATIVE-TEST | `import_review_shows_the_archive_and_the_pre_backup_is_taken`. |
| `the confirm step offers a pre-restore backup and says it took one` | NATIVE-TEST | `import_review_shows_the_archive_and_the_pre_backup_is_taken`. |
| `export refuses an existing file and only replaces after explicit confirmation` | NATIVE-TEST | `export_never_overwrites_without_the_explicit_card` checks the bytes. |
| `the recovery credential stays hidden until revealed and shows the restore steps` | NATIVE-TEST | `the_recovery_credential_shows_on_request_and_a_lock_takes_it_away` and `the_recovery_credential_is_not_kept_across_a_close_and_reopen`. |
| `Settings loads in a locked profile and a general setting can still be changed and saved` | NATIVE-TEST | `settings_load_in_a_locked_profile_and_a_general_setting_still_saves`. |
| `a revealed recovery credential disappears the moment the vault locks` | NATIVE-TEST | `the_recovery_credential_shows_on_request_and_a_lock_takes_it_away`. |
| `a revealed recovery credential is not kept across a close and reopen` | NATIVE-TEST | `the_recovery_credential_is_not_kept_across_a_close_and_reopen`. |
| `the recovery credential copies through the backend clipboard path, never the browser one` | NATIVE-TEST | `the_recovery_credential_copies_through_the_core_clipboard`. |
| `at the minimum window size the footer and a confirmation card stay reachable` | NOT COVERED | Seam: responsive-layout |
| `the terminal field parses quoted arguments instead of splitting on spaces` | NATIVE-TEST | `the_terminal_field_parses_quoted_arguments_and_saves_them`. |
| `Settings lists installed terminals and saves the selected launch command` | NATIVE-TEST | `installed_terminal_selection_is_saved_and_sudo_runs_only_after_approval`. |
| `an unbalanced quote blocks saving the terminal setting` | NATIVE-TEST | `a_terminal_command_that_does_not_parse_disables_save`. |
| `Settings offers the project ranking switch and stores it` | NATIVE-TEST | `settings_offer_the_project_ranking_switch_and_store_it`. |
| `a failed settings save stays open with the changed value and error` | NATIVE-TEST | `a_failed_settings_write_keeps_the_changed_value_and_error`. |

### tests/e2e/updates.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `Settings shows the running version and says when it is current` | NOT COVERED | The updater is not ported (plan milestone 5); the native Settings says updates need the installed app. Seam: updates |
| `Check for updates finds a release that appeared after launch and installs it` | NOT COVERED | The updater is not ported (plan milestone 5); the native Settings says updates need the installed app. Seam: updates |
| `a failed check shows the reason in Settings and a way there from the header` | NOT COVERED | The updater is not ported (plan milestone 5); the native Settings says updates need the installed app. Seam: updates |
| `a browser preview says updates need the installed app` | NOT COVERED | The updater is not ported (plan milestone 5); the native Settings says updates need the installed app. Seam: updates |

## The vitest units

The pure logic the views call, ported into `dev-console-core` with its own tests.

### tests/unit/argv.test.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `splits plain words like the old splitter did` | NATIVE-TEST | ported one for one with the old splitter as the reference: `splits_plain_words_like_the_old_splitter_did` |
| `keeps paths and arguments that contain spaces when quoted` | NATIVE-TEST | `keeps_paths_and_arguments_that_contain_spaces_when_quoted` |
| `supports escapes and empty quoted words` | NATIVE-TEST | `supports_escapes_and_empty_quoted_words` |
| `rejects unbalanced quotes instead of guessing` | NATIVE-TEST | `rejects_unbalanced_quotes_instead_of_guessing`, and the quoting side that only touches ambiguous words: `only_ambiguous_words_get_quoted` |
| `round-trips argv through the input field` | NATIVE-TEST | `round_trips_argv_through_the_input_field` |

### tests/unit/contrast.test.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `measures contrast from tokens.css and clears the thresholds in both themes` | NATIVE-TEST | the same audit runs in Rust over all 24 palettes and cross-checks the shipped script pairing-for-pairing and minimum-for-minimum: `the_contrast_audit_clears_every_threshold_in_all_twenty_four_palettes`, `the_audit_measures_the_pairings_the_shipped_script_measures`, `the_rust_audit_measures_the_shipped_scripts_pairings`, `the_measured_minima_are_the_ones_the_node_audit_reports`, `the_value_layer_parses_what_the_shipped_script_parses` |

### tests/unit/design-tokens.test.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `declares the same token names in the dark and both light palettes` | NATIVE-TEST | `every_palette_restates_the_same_token_names`, `the_two_light_palettes_in_tokens_css_are_identical`, `the_shared_token_table_is_the_tokens_css_root_block` |
| `gives every Omarchy theme the complete color token set` | NATIVE-TEST | `the_palette_registry_is_every_stylesheet_palette_in_order`, `every_generated_palette_matches_its_stylesheet_block_row_for_row`, `every_declared_token_in_every_palette_maps_to_a_kind`, `the_completeness_sweep_gates_both_measured_and_exempt_tokens` |
| `keeps colour literals out of components: tokens.css is the only source` | NATIVE-TEST | the same rule, over the corpus that matters to the native shell - every Rust view source is scanned for a hex triplet or an `rgb`/`hsl` call, and the detector is self-checked so a token name cannot read as a literal: `no_view_source_hard_codes_a_colour`, `the_colour_detector_sees_a_literal_and_ignores_a_token_name`. The Svelte corpus stays under this vitest, which is the right gate for it |
| `ships material disabled: blur is zero unless transparency is requested` | NATIVE-TEST | the shadow/blur block is compared to the stylesheet row for row and its shape is pinned: `a_palette_block_is_forty_four_colours_four_shadows_and_a_blur`, `every_generated_palette_matches_its_stylesheet_block_row_for_row`, `the_shared_block_carries_the_shapes_the_views_ask_for`, `a_length_keeps_its_unit_and_an_unknown_unit_is_an_error` |

### tests/unit/finder-state.test.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `selects the first result automatically` | NATIVE-TEST | `selects_the_first_result_automatically` |
| `moves and clamps without wrapping past the ends` | NATIVE-TEST | `moves_and_clamps_without_wrapping_past_the_ends`, and the same rule driven through the real palette's keys: `up_and_down_move_the_selection_and_clamp_at_both_ends` |
| `keeps the stable id when the list is replaced with a matching row` | NATIVE-TEST | `keeps_the_stable_id_when_the_list_is_replaced_with_a_matching_row` |
| `has no selection when there are no results` | NATIVE-TEST | `has_no_selection_when_there_are_no_results` |
| `shows a muted completion only for a real name prefix` | NATIVE-TEST | `shows_a_muted_completion_only_for_a_real_name_prefix` |
| `Tab accepts a prefix completion and replaces the query for a fuzzy match` | NATIVE-TEST | `tab_accepts_a_prefix_completion_and_replaces_the_query_for_a_fuzzy_match`, and the palette's Tab writing the highlighted name into the field: `tab_completes_the_selected_name_without_copying_anything` |
| `never lets completion change the selection identity` | NATIVE-TEST | `never_lets_completion_change_the_selection_identity` |
| `ignores keys delivered during composition` | NATIVE-TEST | `ignores_keys_delivered_during_composition`. The view-side consequence is recorded in the feasibility ledger rather than papered over: Rune's composition flag is not readable from a handler, so the shell leaves `Space` unbound instead of bound wrongly |

### tests/unit/validation.test.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `accepts a complete command draft` | NATIVE-TEST | `accepts_a_complete_command_draft` |
| `requires name, type and command text` | NATIVE-TEST | `requires_name_type_and_command_text` |
| `rejects spaces inside a name because Space copies in the finder` | NATIVE-TEST | `rejects_spaces_inside_a_name_because_space_copies_in_the_finder` |
| `requires a secret value so an empty reveal cannot wipe a stored secret` | NATIVE-TEST | `requires_a_secret_value_so_an_empty_reveal_cannot_wipe_a_stored_secret` |
| `rejects blank labels and non-http URLs in custom fields` | NATIVE-TEST | `rejects_blank_labels_and_non_http_urls_in_custom_fields` |
| `only treats http(s) as openable URLs` | NATIVE-TEST | `only_treats_http_s_as_openable_urls`, plus the service-side gate that hands nothing else to an opener: `only_an_http_or_https_link_is_handed_to_the_opener`, `only_an_http_or_https_link_may_be_opened` |

### tests/e2e/cliproxy.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `CLI Proxy shows cost, tokens, period breakdown and remaining quota` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `missing quota does not hide token history` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `empty data and a failed request are explicit` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |

### tests/unit/usage.test.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `formats USD with two decimals` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `formats token counts with suffixes` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `shows a sub-threshold percent with a < sign` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `formats reset durations in coarse units` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `reports remaining quota, clamped` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `classifies pace against elapsed window time` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `returns null pace without a reset clock` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `prices a plain request from the rate table` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `bills cached input at the cached rate` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `bills cache writes at 1.25x input` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `applies the long-context tier` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `leaves reviewer models unpriced` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `prices dated model snapshots like their family prefix` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `totals tokens and counts distinct sessions` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `splits models by share and marks unpriced ones` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `buckets records by local day and hour, sorted` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `keys hourly records to the same local hours the 24h window lists` | NATIVE-TEST | `every_window_ends_with_the_current_bucket`. |
| `credits cache savings against the uncached rate` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `builds four cost and token segments in T3 order` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `picks a nice tick scale above the peak` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `maps zero to the baseline and the peak to the plot top` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `keeps monotone tangents flat across local extrema` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |
| `zero-fills quiet periods so the axis never skips days` | NOT COVERED | Browser/unit coverage exists; equivalent native assertions for this precise observable are pending. Seam: usage-analytics |

## Where the gaps cluster

Machine-checked against the rows above: `the_matrix_summary_counts_match_its_rows`.

| Status | Rows |
| --- | --- |
| NATIVE-TEST | 120 |
| PARTIAL | 17 |
| NOT COVERED | 58 |
| **Total** | **195** |

| Missing seam | Rows waiting on it | What the rows still need |
| --- | --- | --- |
| `usage-analytics` | 25 | Native assertions for the web usage math and data-state fixtures. |
| `paint` | 27 | rasterised appearance: what a row, a ring, a truncation or a screenshot looks like. A headless runtime cannot decide it; the owner's desktop checks in the feasibility ledger are evidence, not tests |
| `folder-picker` | 5 | the XDG portal file chooser, which a headless runtime cannot answer; the picker itself is built and was used by hand on the owner's desktop |

| `updates` | 4 | the updater, not ported yet (plan milestone 5) |
| `a11y-semantics` | 3 | roles and selected states the runtime's element list does not expose to a test |
| `window-lifecycle` | 3 | show and hide requests between the two windows, which need a real event loop |
| `search-ranking` | 1 | palette orderings decided by the core ranker that no native test sets up (pinning against a project, typing over a detected project) |
| `focus-management` | 2 | Escape and Tab routes owned by Rune's modal that no native test walks |



| `responsive-layout` | 2 | minimum-window layouts asserted as geometry |

| `clipboard` | 1 | a clipboard provider that fails |

| `details` | 1 | opening a URL in the browser |
| `vault-state` | 1 | authenticated unlock, which raises a real polkit prompt |

| Spec | Rows | NATIVE-TEST | PARTIAL | NOT COVERED |
| --- | --- | --- | --- | --- |
| `tests/e2e/cliproxy.spec.ts` | 3 | 0 | 0 | 3 |
| `tests/unit/usage.test.ts` | 23 | 1 | 0 | 22 |
| `tests/e2e/environment.spec.ts` | 4 | 3 | 1 | 0 |
| `tests/e2e/finder.spec.ts` | 24 | 19 | 5 | 0 |
| `tests/e2e/history.spec.ts` | 14 | 12 | 0 | 2 |
| `tests/e2e/library.spec.ts` | 20 | 16 | 3 | 1 |
| `tests/e2e/omarchy-themes.spec.ts` | 4 | 3 | 1 | 0 |
| `tests/e2e/project-import.spec.ts` | 20 | 15 | 1 | 4 |
| `tests/e2e/projects.spec.ts` | 6 | 5 | 1 | 0 |
| `tests/e2e/redesign.spec.ts` | 23 | 9 | 5 | 9 |
| `tests/e2e/screenshots.spec.ts` | 12 | 0 | 0 | 12 |
| `tests/e2e/settings.spec.ts` | 14 | 13 | 0 | 1 |
| `tests/e2e/updates.spec.ts` | 4 | 0 | 0 | 4 |
| `tests/unit/argv.test.ts` | 5 | 5 | 0 | 0 |
| `tests/unit/contrast.test.ts` | 1 | 1 | 0 | 0 |
| `tests/unit/design-tokens.test.ts` | 4 | 4 | 0 | 0 |
| `tests/unit/finder-state.test.ts` | 8 | 8 | 0 | 0 |
| `tests/unit/validation.test.ts` | 6 | 6 | 0 | 0 |

## How to keep this file honest

- Add or rename a spec title: add or rename its row in the same commit, or
  `every_shipped_behaviour_title_has_exactly_one_matrix_row` fails.
- Rename or delete a native test: update every row that cites it, or
  `every_matrix_citation_names_a_test_that_actually_exists` fails.
- Move a row up only when the test that decides it exists, then update the tables above;
  `the_matrix_summary_counts_match_its_rows` recounts them.
