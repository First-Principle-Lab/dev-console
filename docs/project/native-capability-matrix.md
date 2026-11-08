# Native capability matrix: every shipped behaviour, and what the Rune shell has behind it

Checkpoint C6 of plan [`../plans/rune-migration.md`](../plans/rune-migration.md) ("port all feature
screens") asks for more than a list of screens that exist. It asks for **every user-visible
behaviour the shipped app has** to be mapped either to the native test that now decides it or to an
explicit statement that nothing does. This document is that map, and it is written to be read as a
gap list rather than as a score.

**Read the summary before anything else.** Of the 169 behaviours inventoried below:
**26 are decided by a native test** (ported pure logic - argv parsing, draft validation, Finder
selection, the token tables, the contrast audit - plus, since the clipboard seam landed, two palette
behaviours a headless runtime can really settle: a copy failure keeping the palette open with the
shipped line, and a copied value reaching no window surface at all), **10 are partially decided** (the
keyboard behaviour inside the palette's own field and its footer, the copy itself - the highlighted
row, the success-only hide, the expiry and replacement guards - plus, since the first real modal
landed, the shared confirmation's focus trap, its Escape routes and its focus restore, and the delete
gate that removes the record it named), and **133 are not decided anywhere in the native shell.** The Playwright suite describes 187 browser runs of a shipped app
whose screens - entry list from the repository, editor, inspector, settings, project import, history
suggestions, environment, updater, and every painted pixel of all of them - the Rune shell does not
have. One shared dialog is not a screen: it is the surface four of the app's confirmations sit on,
and three of those four still have no native view. Checkpoint C6 is therefore **not earned**, and
nothing in this file should be quoted as if it were.

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

A row moves up only by the test existing first. That is also the whole answer to "how do we know the
matrix is not padded": the padding would have to compile.

## What counts as covered

| Status | What it claims |
| --- | --- |
| `NATIVE-TEST` | A `#[test]` in this workspace decides the same observable the spec asserts, on this host, with no display. |
| `PARTIAL` | Part of the observable is decided by a named native test and the rest is not; the cell says which is which and names the rest as a seam. |
| `NOT COVERED` | Nothing in the native shell decides it. The cell names the screen or seam that is missing, and cites any lower-layer rule that *is* already ported and tested, so the next reader knows where to hang the view. |

Two rules keep the labels honest in both directions:

- **A `NATIVE-TEST` row may not name a missing seam.** If something is missing, the row is
  `PARTIAL` or `NOT COVERED` - which is why every painted or composited property below is a gap even
  where the underlying data is ported and measured.
- **Painting is never "covered" from this container.** No GPU, no Vulkan ICD, no Wayland or X11
  session and no compositor here, so a row whose observable is a rendered pixel, a focus ring, a
  truncation or a screenshot stays `NOT COVERED` with the `paint` seam, exactly as the feasibility
  ledger keeps the mapped-window checks `UNVERIFIED (owner desktop)`.

## What the inventory covers, and where it is thinner than the app

| Source | What it stands for | Titles here | Runs it describes |
| --- | --- | --- | --- |
| `tests/e2e/*.spec.ts` (Playwright) | the shipped app's user-visible behaviour, as the repository itself defines it | 145 | 187 |
| `tests/unit/*.test.ts` (vitest) | the pure logic the views call: argv, validation, Finder selection, tokens, contrast | 24 | 24 |

Six of those e2e titles are loops, so a row can stand for several runs: the Omarchy palette render
covers 22 palettes, the long-list footer 12 (4 themes x 3 sizes), and three screenshot sets 3 or 4
viewports/themes each. The row says how many runs it stands for.

The inventory is also, in one direction, thinner than the app: it lists what the specs assert. Where
the Svelte frontend does something no spec pins down, this matrix cannot see it either, so the real
gap is at least as wide as the `NOT COVERED` count - never narrower.


## The Playwright specs

Behaviour titles are quoted exactly as the spec file writes them, so `grep` finds the test.

### tests/e2e/environment.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `project Environment shows dated versions, copies old files and pauses sync` | NOT COVERED | no environment view exists in `crates/dev-console-desktop/src/ui.rs`, so nothing renders versions or a pause control. Seam: environment, run-output. The version store underneath it is ported and tested in `dev-console-core`: `versions_are_per_project_file_deduplicated_and_survive_missing_sources_and_restarts`, `pause_blocks_automatic_sync_and_symlinks_and_nested_files_are_not_read` |
| `a missing project folder still allows copying saved versions` | NOT COVERED | the environment view does not exist, so "copy a saved version" has nothing to click - and the write it would perform now has a tested shape to follow (`the_copy_asks_for_the_row_it_could_have_read_the_name_of` resolves by entry id, which is the same request shape a version copy needs). The file-dedup rule that lets a version survive a missing source is tested: `versions_are_per_project_file_deduplicated_and_survive_missing_sources_and_restarts`. Seam: environment |
| `Browse starts an empty project and a locked vault rejects access` | NOT COVERED | there is no folder picker and no environment view, so Browse cannot be exercised; the refusal itself is a service answer with no view to show it. Seam: folder-picker, environment, vault-state. `a_closed_vault_is_refused_as_a_backup_and_as_an_export_rather_than_written_empty` and `the_guard_names_the_window_that_was_refused` decide the refusal at the service layer |
| `locking the vault closes the environment view and clears its metadata` | NOT COVERED | the invalidation rule exists and is tested where the answer is judged, but no environment view consumes that answer, so there is nothing here that closes or clears. Seam: environment, vault-state, events. `a_response_from_before_a_manual_lock_cannot_restore_the_secret_rows`, `the_vault_is_judged_before_the_record_so_a_post_lock_answer_says_the_truth` |

### tests/e2e/finder.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `the first result is selected and labelled for assistive tech` | PARTIAL | the auto-selection rule is ported and tested (`selects_the_first_result_automatically`) and a real element tree answers injected keys, but the palette has no search results to select from and `El` carries no selected/activedescendant wiring, so the assistive-tech half is undecided. Seam: search-ranking, a11y-semantics |
| `Up and Down move the selection and clamp at both ends` | PARTIAL | the one behaviour the shell decides end to end: injected keys run through the shipped `nextSelection` rule on a real element tree, the caret and text are proven untouched, and clamping and the restart-after-shrink case are asserted (`arrows_move_the_highlight_and_never_reach_the_caret`, `the_highlight_clamps_at_both_ends_instead_of_wrapping`, `arrows_do_nothing_when_there_is_nothing_to_highlight`, `a_highlight_that_fell_out_of_a_shortened_list_restarts_where_it_was_heading`). Seam: a11y-semantics, search-ranking - the rows are placeholder names and the aria-selected mirror the spec reads is not wired |
| `Tab completes the selected name without copying anything` | PARTIAL | both completion rules are decided on a real tree and the completion half of "does not copy" now has teeth: `tab_writes_the_highlighted_row_into_the_field`, `tab_replaces_the_whole_query_when_the_match_is_fuzzy`, and `completing_with_tab_never_copies_anything`, which is only checkable now that the shell has a clipboard at all. Seam: search-ranking - the re-search behind the shipped `acceptCompletion` is still absent, so Tab completes into a list that never updates |
| `Enter copies exactly the selected record and hides only after success` | PARTIAL | the copy is now the highlighted row's and nobody else's (`enter_copies_the_highlighted_row_and_not_the_row_above_it`, `the_copy_asks_for_the_row_it_could_have_read_the_name_of`: Down then Enter resolves the *secret's* id and value, not the row above it), and the hide is a success-only transition on the right conditions - `a_successful_copy_asks_for_the_palette_to_be_put_away` also pins that hideSelf is not hide, so the rows survive the copy being reported. Seam: search-ranking, window-lifecycle - the highlighted row belongs to a placeholder list rather than a ranked result, and this container decides the *request* to put the palette away, never the unmap |
| `a copy failure keeps the finder open with a safe message` | NATIVE-TEST | the shell's own resolver answers "nothing" today (`copy::Unwired`), so this is the live path rather than a simulated one: `a_failed_copy_stays_open_with_the_shipped_message_and_writes_nothing` pins that `dismissed` stays false, that the line is the shipped `Copy failed. Nothing was copied.` in the `role="alert"` slot, that no confirmation slot is drawn beside it, and that the clipboard is untouched - while `a_copied_value_reaches_the_clipboard_and_nowhere_the_window_can_show` is what makes "safe message" more than an adjective: no value reaches any accessible name, visible text or element key |
| `Ctrl+O hands the selected entry to the library` | NOT COVERED | `Ctrl+O` is unbound and there is no details panel to hand anything to; the cross-window intent the shipped app emits has no native equivalent. Seam: details, keyboard-delivery, events |
| `Escape clears the query and hides the finder` | PARTIAL | the clear half is decided on a real tree: query, rows and selection are emptied, the shell is asked to hide through explicit state, and the field keeps focus because Rune blurs on an unhandled Escape (`escape_clears_the_palette_without_taking_focus_out_of_the_field`). Seam: window-lifecycle - the unmap itself needs a mapped window, which is an UNVERIFIED (owner desktop) row in the feasibility ledger, and the request-counter half of the shipped `hide` waits on the search port |
| `keys still work after a click moved focus onto a row` | NOT COVERED | the palette bindings live on the query field (the only Rune seam that sees a focused input's own keys), so a row that took focus would route keys away from them, and nothing asserts the shipped behaviour of re-focusing the field on row activation. Seam: keyboard-delivery, a11y-semantics. Row presses do reach their handler: `pressing_a_result_selects_it` |
| `with no matches Tab and Enter do nothing` | PARTIAL | both no-op guards are decided: `tab_and_enter_do_nothing_when_there_is_nothing_to_act_on` (Tab completes nothing and inserts nothing; Enter inserts no newline, reports no outcome, dismisses nothing and writes no clipboard) and `enter_without_anything_to_copy_is_still_consumed`. Seam: search-ranking - "no matches" is only reachable today as "no rows at all", because typing still cannot produce an empty result set |
| `a stale asynchronous response cannot replace newer results` | NOT COVERED | the guard is ported, unit-tested and has a Rune worker stream that refuses superseded answers, but no native view runs a search through it, so nothing in a tree can be replaced by a stale answer. Seam: search-ranking, events. `an_answer_for_a_query_the_user_replaced_is_not_applied`, `a_superseded_answer_never_lands_and_never_wakes_the_loop`, `a_debounced_request_is_tagged_with_the_generation_it_ran_under` |
| `a locked vault removes all secret metadata from the results` | NOT COVERED | the palette shows placeholder names, and no window state or element offers a secret capability, so the locked-vault filtering the spec reads in the DOM has nothing native to be read from. Seam: vault-state, search-ranking. The service-side rule is ported and tested: `secret_metadata_never_leaves_a_locked_vault`, `the_secret_list_matches_its_type_box_as_a_prefix` |
| `reopening starts with a fresh query instead of the previous search` | NOT COVERED | there is no shown/hidden lifecycle in the shell: `dismissed` is set by Escape and nothing re-opens or resets a palette, so the shipped `finder-shown` reset has no native counterpart. Seam: window-lifecycle, events |
| `typing shows the matching saved entries with their tags` | NOT COVERED | no search runs behind the field, so typing changes only the field. Seam: search-ranking, events, library-list. The ranking payload the rows would need is ported and tested: `results_carry_tags_for_the_finder_rows`, `ranks_exact_before_prefix_token_and_fuzzy` |
| `an empty query lists the most recently used entries first` | NOT COVERED | the ordering rule is ported and tested at the ranker (`empty_query_puts_recently_used_before_frequently_used`, `empty_query_prefers_pinned_then_usage`) but the palette never asks the ranker, so no list is ordered anywhere native. Seam: search-ranking, events |
| `the palette says which project it was opened from, and ranks it first` | NOT COVERED | the chip element exists and stays absent when no context was detected (`an_empty_finder_shows_the_query_field_and_no_rows_or_context_chip`), but nothing detects a context for a native window and nothing ranks by it. Seam: search-ranking, events. Detection itself is ported and tested: `detection_runs_the_whole_chain_and_stops_where_a_link_is_missing`, `an_answer_for_a_finder_open_that_has_already_gone_is_dropped` |
| `a row from the current project is labelled for assistive tech` | NOT COVERED | no row carries a project marker and `El` exposes no accessible description wiring in these views, so the label the spec asserts has no native counterpart. Seam: a11y-semantics, search-ranking |
| `clicking the project chip ranks everything again, without closing the palette` | NOT COVERED | the chip is not clickable in the shell (no press handler), so neither the re-rank nor the "stays open" half is reachable. Seam: search-ranking, events. The invalidation it depends on is tested: `dismissing_the_project_context_invalidates_a_detection_still_in_flight` |
| `Ctrl+P ignores the project from the keyboard` | NOT COVERED | unbound, and it would have nothing to ignore: no context and no ranking behind the rows. Seam: keyboard-delivery, search-ranking |
| `with no detected project, the palette looks and orders exactly as it used to` | NOT COVERED | the regression rule is ported and tested at the ranker (`no_context_ranks_exactly_as_it_did_before`) and the empty chip is asserted (`an_empty_finder_shows_the_query_field_and_no_rows_or_context_chip`), but there is no populated palette to compare. Seam: search-ranking |
| `typing narrows the list without dropping the project marker` | NOT COVERED | typing does not search, and no row can carry a marker. Seam: search-ranking, a11y-semantics |
| `with project ranking switched off, a detected project changes nothing` | NOT COVERED | the setting is ported and enforced in the service layer (`history_suggestions_are_off_by_default_and_clamped_when_on`, `the_tray_settings_action_addresses_the_library_only` cover the neighbouring rules) but there is no settings view and no ranked palette. Seam: settings, search-ranking |
| `a secret tagged for the project is not moved up or marked` | NOT COVERED | the ranking rules are ported (`a_project_is_recognised_by_its_saved_directory_or_its_tag`, `within_a_match_tier_the_current_project_beats_usage`) and the secret list matches its type box (`the_secret_list_matches_its_type_box_as_a_prefix`), but no palette row is ranked or marked anywhere native. Seam: search-ranking, vault-state |
| `a pinned command still leads the empty query, ahead of the project` | NOT COVERED | the order itself is decided at the ranker (`empty_query_prefers_pinned_then_usage`, `an_empty_query_puts_the_current_project_straight_after_pinned`); what is missing is any palette that asks it. Seam: search-ranking |
| `a project detected after the first search moves the selection to its first command` | NOT COVERED | the shipped palette re-runs its search when the context arrives late and restarts the selection. Neither half exists here: no late context reaches a view and no search re-runs, though the identity-stable selection rule is ported (`keeps_the_stable_id_when_the_list_is_replaced_with_a_matching_row`). Seam: events, search-ranking |

### tests/e2e/history.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `offers the ranked history above the commands, minus what is already saved` | NOT COVERED | there is no history panel and the entry list is a placeholder, so nothing is offered above it. Seam: history-suggestions, library-list. The ranking and de-duplication are ported and tested: `counts_and_recency_rank_a_frequent_recent_command_first`, `skips_commands_already_in_the_library_whatever_their_spacing` |
| `saving opens the editor prefilled and writes nothing until it is confirmed` | NOT COVERED | no editor view exists, so a prefilled draft cannot open and no confirmation gate exists to be honoured. Seam: editor, dialog, history-suggestions. The draft validation the editor would run is ported and tested: `accepts_a_complete_command_draft`, `an_empty_draft_is_rejected_until_the_user_types` |
| `dismissing takes the suggestion away and is the only thing remembered` | NOT COVERED | the dismissal store is ported, keyed by hash and spacing-tolerant (`dismissed_suggestions_are_matched_by_hash_not_by_spacing`, `a_dismissal_key_is_stored_lowercase_and_anything_else_is_refused`) but no panel offers a suggestion to dismiss. Seam: history-suggestions |
| `the panel is absent entirely when the setting is off` | NOT COVERED | the setting is ported and clamped (`history_suggestions_are_off_by_default_and_clamped_when_on`); the view that would honour it is not ported. Seam: history-suggestions, settings |
| `turning the setting off in Settings hides the panel without a reload` | NOT COVERED | no settings view, no panel, and no event that would re-render one. Seam: settings, history-suggestions, events |
| `the panel is keyboard-only reachable: focus, Enter saves, Delete dismisses` | NOT COVERED | nothing in the shell takes focus beyond the query field and no row binds Enter or Delete, so keyboard reachability has no substrate. Seam: keyboard-delivery, history-suggestions, focus-management |
| `rescan picks up commands added to the history afterwards` | NOT COVERED | the history readers are ported and tested (`the_history_written_most_recently_is_read_last_and_ranks_as_recent`, `histfile_replaces_the_zsh_default_and_a_missing_file_is_no_source`); no view triggers a rescan. Seam: history-suggestions, events |
| `the panel folds away without touching the list underneath it` | NOT COVERED | no panel and no list built from repository rows. Seam: history-suggestions, library-list |
| `without project activity the panel explains how to enable tracking` | NOT COVERED | no panel, so no empty-state copy to assert; the project-history store that decides "no activity" is ported (`launchers_secrets_and_unassigned_directories_are_never_stored`). Seam: history-suggestions |
| `Settings brings every dismissed suggestion back, and the panel offers it again` | NOT COVERED | neither the settings view nor the panel exists; the dismissal rows a bring-back would clear are ported and tested. Seam: settings, history-suggestions |
| `with nothing dismissed, Settings shows no bring-back button` | NOT COVERED | no settings view to hold the button. Seam: settings, history-suggestions |
| `Backspace on a suggestion does nothing: only Delete dismisses` | NOT COVERED | no suggestion row exists to receive either key, and the shell binds neither Delete nor Backspace anywhere but the editor. Seam: keyboard-delivery, history-suggestions |
| `every row keeps its name, count and buttons on one line, under the command, in the narrow list pane` | NOT COVERED | the row layout is a rasterised property of a panel that does not exist; nothing here measures a line or a pane. Seam: paint, history-suggestions, responsive-layout |
| `project history has an unboxed pointer state and a visible keyboard focus ring` | NOT COVERED | paint and focus-ring appearance are undecidable on this host even for the views that exist, and this one does not. Seam: paint, focus-management, history-suggestions |

### tests/e2e/library.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `library lists both same-named commands and shows the selected record` | NOT COVERED | the list is a placeholder of synthetic strings and there is no details pane, so same-named records cannot be listed or disambiguated. Seam: library-list, details. The store underneath keeps both rows and refuses a stale writer: `two_sources_naming_the_same_task_both_survive`, `two_editors_cannot_both_save_the_same_record` |
| `creating a command persists through the backend and refreshes the list` | NOT COVERED | no editor to type into and no list built from the repository, so a save has neither an origin nor a visible result. Seam: editor, library-list, events. The persistence and its announcement are ported and tested: `saving_a_new_entry_reports_created_and_a_change_reports_updated`, `a_saved_command_survives_closing_and_reopening_the_database` |
| `a newly saved ordinary command shows stdout, stderr and its exit code in the app` | NOT COVERED | no output panel exists and no run is reachable from the shell. Seam: run-output, dialog. Execution, its bounds and its timeout are ported and tested: `ordinary_commands_capture_output_and_exit_status`, `in_app_command_output_is_bounded_and_interactive_waits_time_out`, `command_output_is_capped_at_what_execution_was_allowed_to_capture` |
| `error-only command output can be copied and clipboard failures stay visible` | NOT COVERED | no output panel to show it in and nothing to copy out of it. What has stopped being missing is the other half of the sentence: a clipboard failure *is* visible in the shell now, on the palette's own line (`a_failed_copy_stays_open_with_the_shipped_message_and_writes_nothing`), so the failure surface exists and only the run-output screen in front of it does not. What is decided about output is its bound, not its display: `in_app_command_output_is_bounded_and_interactive_waits_time_out`, `command_output_is_capped_at_what_execution_was_allowed_to_capture`. Seam: run-output |
| `Escape closes confirmation, output, and unsaved changes dialogs` | PARTIAL | the shell's one real modal honours the shipped branch: Escape, the Cancel button and a press off the panel all reach the same cancel handler, and none of them removes anything (`escape_backs_out_of_the_confirmation_without_touching_the_list`, `cancelling_removes_nothing`, `a_press_off_the_panel_backs_out_of_the_confirmation_too`). Two of the three dialogs this title names have no native surface at all, so their Escape is not a half-decided thing but a missing screen. Seam: run-output, editor |
| `sudo commands use the configured terminal after confirmation` | NOT COVERED | no confirmation card and no terminal launch. Seam: run-output, dialog, settings. The terminal argv parsing is ported and tested: `discovered_terminals_pass_the_shell_as_argv`, `metadata_never_reaches_the_argv`, `missing_working_directory_is_an_error_not_a_fallback` |
| `a cancelled edit leaves the saved record untouched` | NOT COVERED | no editor, so no draft to cancel; the revision conflict a cancel avoids is ported and tested. Seam: editor, library-list. `a_draft_the_store_refuses_produces_no_event`, `deleting_with_a_stale_revision_is_refused_and_keeps_the_record` |
| `a failed write keeps the draft on screen` | NOT COVERED | no draft state exists in the shell at all. Seam: editor, dialog. The freshness rule for a record that moved underneath the draft is ported: `a_draft_loaded_before_someone_else_saved_is_not_painted_over_their_record`, `a_record_that_vanished_while_the_work_ran_is_reported_as_gone` |
| `pin, copy and delete confirmation all act on the intended record` | PARTIAL | the destructive one is decided, including the half that matters: the confirmation removes the record it *named*, not the slot it was pressed on, so a list that reloaded underneath the dialog cannot point it at somebody else's row (`a_record_that_moved_in_the_list_is_still_the_record_that_gets_removed`, `confirming_removes_the_named_record_and_closes_the_confirmation`). The copy half now has the same property in the palette (`enter_copies_the_highlighted_row_and_not_the_row_above_it` acts on the highlighted record, not `rows[0]`); what the *Library* still lacks is any row action at all, and pin is absent everywhere, so "the intended record" is decided at the gate and in the palette but not yet at the store. Seam: library-list. The identity and revision rules the last half leans on are ported: `pinning_or_deleting_at_a_stale_revision_conflicts_and_says_nothing`, `a_pinned_row_and_a_deleted_row_both_announce_themselves` |
| `sidebar actions target their row without selecting it, including secret pinning` | NOT COVERED | no sidebar, no rows, and the placeholder list has no per-row actions. Seam: library-list, details, vault-state |
| `secrets are masked in details and only appear through an explicit reveal` | NOT COVERED | no details panel. The shell does hold the stronger structural rule: no window state can hold a value and no Finder element offers a secret capability. Seam: details, vault-state, clipboard. `a_reveal_is_the_only_call_that_carries_the_value`, `a_reveal_is_the_only_call_that_hands_over_the_value`, `no_window_state_can_hold_a_secret_value` |
| `a new secret needs only name, value and notes` | NOT COVERED | no editor and no required-field gate in the shell, though the rules are ported and tested. Seam: editor, details, library-list. `requires_name_type_and_command_text`, `requires_a_secret_value_so_an_empty_reveal_cannot_wipe_a_stored_secret` |
| `a secret can carry optional custom fields added with the plus button` | NOT COVERED | no editor, no extra-field rows, and no field validation reachable from a view. Seam: editor, details. `rejects_blank_labels_and_non_http_urls_in_custom_fields` |
| `a locked vault clears the secret list and blocks secret reads` | NOT COVERED | the shell has one vault_unlocked boolean on the Library state and no lock/unlock interaction; the metadata filtering and the single-shot unlock event are ported and tested. Seam: vault-state, library-list, events. `secret_metadata_never_leaves_a_locked_vault`, `unlocking_reports_the_state_and_never_returns_stale_metadata`, `manual_lock_survives_a_restart_and_blocks_every_secret_operation` |
| `editing the short form preserves older entry metadata` | NOT COVERED | no editor, so no short form; the write path that preserves untouched columns is ported and tested. Seam: editor, library-list. `pinning_a_secret_preserves_its_value_and_rejects_stale_revisions`, `saving_a_new_entry_reports_created_and_a_change_reports_updated` |
| `narrow layout shows one pane instead of squeezing both` | NOT COVERED | the native views have no breakpoint behaviour and one pane is never swapped for two. Seam: responsive-layout, paint. The measured invariant the shell does hold is that each view fills the surface it was given: `the_library_measures_the_surface_it_was_asked_to_fill` |
| `settings exposes encrypted backups: create, list, reviewed restore, and a rejected import` | NOT COVERED | no settings view and no dialog to review an archive in; the archive rules themselves are ported and tested. Seam: settings, backups, dialog. `a_backup_round_trips_commands_secrets_and_usage`, `inspect_reports_date_and_counts_without_touching_anything`, `corrupt_truncated_and_newer_archives_are_refused_without_changing_anything`, `a_closed_vault_is_refused_as_a_backup_and_as_an_export_rather_than_written_empty` |
| `backups are refused while the vault is locked` | NOT COVERED | the refusal is ported and tested at the service layer; the view that would report it is not ported. Seam: backups, vault-state, settings. `a_closed_vault_is_refused_as_a_backup_and_as_an_export_rather_than_written_empty` |
| `a URL field opens through the backend and only for http(s) links` | NOT COVERED | no field and no opener call in the shell; the allow-list is ported and tested. Seam: details, run-output. `only_an_http_or_https_link_is_handed_to_the_opener`, `only_an_http_or_https_link_may_be_opened`, `only_treats_http_s_as_openable_urls` |
| `URL hit and focus areas fit short text while long URLs wrap inside the field` | NOT COVERED | a rasterised property of a field that does not exist natively. Seam: paint, responsive-layout, details |

### tests/e2e/omarchy-themes.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `theme choices preview immediately and Save keeps the choice` | NOT COVERED | the palette data is ported and structurally pinned, but the views still draw Rune's built-in theme and there is no settings view to pick one in. Seam: theme-switch, settings. `the_palette_registry_is_every_stylesheet_palette_in_order`, `every_generated_palette_matches_its_stylesheet_block_row_for_row` |
| `closing Settings discards a theme preview` | NOT COVERED | no settings view, no preview state and no theme-changed emission from a native window. Seam: settings, theme-switch, events. The event shape a preview would use is ported and tested: `a_theme_changed_event_is_a_bare_string_like_the_old_emit`, `saving_settings_announces_the_theme_before_handing_the_result_back` |
| `${name} renders library, secrets, editor, settings and finder` | NOT COVERED | one row standing for 22 palettes: nothing native can be rendered in a chosen palette, and three of the five surfaces do not exist as views. Seam: theme-switch, editor, settings, library-list. What is decided here is the data: `every_generated_palette_matches_its_stylesheet_block_row_for_row`, `the_contrast_audit_clears_every_threshold_in_all_twenty_four_palettes`, `every_declared_token_in_every_palette_maps_to_a_kind` |
| `Finder loads the saved theme without access to full settings and follows changes` | NOT COVERED | the native Finder has no theme source at all yet, and no theme-changed delivery to follow. Seam: theme-switch, events. The delivery rule it would need is ported and tested: `window_targeted_events_keep_their_delivery`, `a_theme_changed_event_is_a_bare_string_like_the_old_emit` |

### tests/e2e/project-import.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `offers the action beside New and explains what a scan reads` | NOT COVERED | no library toolbar and no import entry point in the shell. Seam: project-import, library-list. The scanners a review would list are ported and tested: `reads_scripts_and_skips_npm_lifecycle_hooks`, `make_targets_are_discovered_with_their_help_text`, `just_recipes_survive_parameters_privacy_and_directives`, `compose_services_become_up_logs_and_one_down` |
| `refuses an empty folder without reading anything` | NOT COVERED | no dialog to type a path into; the refusal is ported and tested at the service layer. Seam: project-import, dialog. `an_empty_selection_is_refused_and_a_stale_one_is_reported`, `a_file_and_a_missing_folder_are_reported_not_scanned` |
| `shows the backend reason when the path is not a folder` | NOT COVERED | no view to show a reason in. Seam: project-import, dialog. `a_file_and_a_missing_folder_are_reported_not_scanned` |
| `says so when a folder defines nothing it can offer` | NOT COVERED | the empty-scan case is ported and tested; the message has nowhere to appear. Seam: project-import. `a_package_without_scripts_and_a_binary_manifest_offer_nothing`, `a_compose_file_without_services_offers_nothing`, `an_import_that_saved_nothing_churns_no_list` |
| `groups a scanned folder and pre-selects every task it defines` | NOT COVERED | no review list. Seam: project-import, dialog. `sources_group_and_sort_in_a_stable_order`, `an_imported_task_is_recognised_on_rescan_by_command_and_folder` |
| `marks a task the library already has and leaves it out of the count` | NOT COVERED | no list and no count widget. Seam: project-import, library-list. `a_second_import_reports_nothing_new_and_never_duplicates`, `a_task_the_user_already_wrote_by_hand_counts_as_saved` |
| `selections drive the count: one row, a whole group, then everything` | NOT COVERED | no rows to select and no name that would be written. Seam: project-import, dialog. `only_the_named_tasks_are_imported`, `importing_saves_the_command_with_its_folder_tag_and_type` |
| `imports the selection as commands that run in the folder, and shows them in the library` | NOT COVERED | neither the import call nor the list it would refresh exists in the shell. Seam: project-import, library-list, events. `importing_saves_the_command_with_its_folder_tag_and_type`, `a_task_that_cannot_be_saved_skips_without_losing_the_rest` |
| `a task that reached the library after the scan is reported, never written twice` | NOT COVERED | the partial-stale report is ported and tested; the dialog that would show it is not. Seam: project-import, dialog. `an_empty_selection_is_refused_and_a_stale_one_is_reported`, `partially_stale_selections_report_each_missing_task` |
| `an untouched folder still lists its tasks as already saved on the second scan` | NOT COVERED | no scan UI. Seam: project-import. `an_imported_task_is_recognised_on_rescan_by_command_and_folder`, `a_second_import_reports_nothing_new_and_never_duplicates` |
| `Enter in the folder field scans, and Escape leaves nothing behind` | NOT COVERED | no dialog, so no field that binds Enter to a scan and no Escape stack that would abandon one; the shell binds neither key outside the palette. Seam: dialog, keyboard-delivery, project-import |
| `cancelling writes nothing` | NOT COVERED | no dialog and no reviewed selection to discard. Seam: dialog, project-import. `an_empty_selection_is_refused_and_a_stale_one_is_reported` |
| `the action is not offered while the secrets tab is showing` | NOT COVERED | the tabs exist and switch state, but nothing gates an import action on the tab because there is no action. Seam: project-import, library-list. `switching_tabs_changes_the_state_the_list_is_built_from` |
| `a folder with far more tasks than fit on screen scrolls instead of growing` | NOT COVERED | a rasterised property of a dialog that does not exist. Seam: project-import, dialog, paint |
| `keeps keyboard focus inside the modal and restores it on close` | PARTIAL | decided on the shell's one real modal, and it is the native twin of the rules `Dialog.svelte` hand-rolls: opening moves the keyboard to the first footer action, Tab and Shift+Tab cycle the two actions and never reach the inert window behind, and cancelling hands the keyboard back to the row that asked (`the_confirmation_takes_the_keyboard_to_its_first_action`, `tab_cycles_the_confirmations_actions_and_never_leaves_it`, `closing_the_confirmation_puts_the_keyboard_back_where_it_came_from`). Two halves are outstanding and both are named rather than waved through. The shipped guard `if (trigger?.isConnected) trigger.focus()` has no native twin while list rows are keyed by index: after a deletion the restore lands on whichever record now sits in that slot, which `a_removed_record_leaves_the_focus_on_the_row_key_that_now_holds_someone_else` pins as the shell's behaviour and does not endorse. And whether a user can *see* where the keyboard is is a focus ring, which is paint. Seam: paint, library-list |
| `reports a partially vanished selection and rescans the saved rows` | NOT COVERED | no dialog to report into. Seam: project-import, dialog. `partially_stale_selections_report_each_missing_task` |
| `long-list footer stays fully visible in ${theme} at ${width}` | NOT COVERED | one row standing for 12 runs: no import dialog, no palette selection, and no rasterised footer to measure. Seam: dialog, theme-switch, paint, responsive-layout |
| `Browse selects and scans a folder without typing or importing` | NOT COVERED | the shell opens no file dialog at all - plan §3 schedules the XDG portal replacement for the Tauri dialog plugin, and until it lands neither this host nor the desktop can check it (the feasibility ledger carries the row). Seam: folder-picker, project-import |
| `cancelling Browse preserves the path and reviewed selection` | NOT COVERED | no picker, no dialog state. Seam: folder-picker, dialog, project-import |
| `a folder picker failure leaves manual path entry available` | NOT COVERED | no picker to fail and no field to fall back to. Seam: folder-picker, dialog, project-import |

### tests/e2e/projects.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `eighteen commands share one collapsed project and search expands only matches` | NOT COVERED | the native list is one flat placeholder with no grouping and no search to expand it. Seam: library-list, search-ranking. The grouping rules are ported and tested: `sources_group_and_sort_in_a_stable_order`, `nested_folders_group_by_root_but_keep_execution_and_saved_identity` |
| `same-named projects at different roots stay separate and General stays available` | NOT COVERED | no project grouping exists in a view; the identity rules behind it are ported and tested. Seam: library-list, search-ranking. `a_scan_of_a_second_folder_never_sees_the_first_one_saves`, `the_folder_is_named_after_its_repository_root_when_there_is_one`, `the_folder_name_is_used_when_nothing_above_is_a_repository` |
| `a tag-only command joins a uniquely identified project without changing its data` | NOT COVERED | the recognition rule is ported and tested (`a_project_is_recognised_by_its_saved_directory_or_its_tag`); no view assigns or displays a project. Seam: search-ranking, editor |
| `a manually saved command can be assigned with the project folder picker` | NOT COVERED | no editor and no folder picker. Seam: folder-picker, editor. `tilde_folder_paths_expand_to_home`, `a_saved_tilde_means_what_execution_would_run_it_as` |
| `history shows the five busiest projects and filters routine launchers` | NOT COVERED | no history panel. The limit and the launcher filter are ported and tested: `only_five_busiest_projects_and_five_commands_each`, `launchers_secrets_and_unassigned_directories_are_never_stored`. Seam: history-suggestions |
| `saving a history command retains its project and does not suppress another project` | NOT COVERED | no panel and no editor. `counts_and_dismissals_are_project_and_folder_specific`, `importing_saves_the_command_with_its_folder_tag_and_type` decide the rules underneath. Seam: history-suggestions, editor |

### tests/e2e/redesign.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `tabs carry entry counts and expose one selected tab` | NOT COVERED | the tabs switch the state the list is built from and keep the list mounted (`switching_tabs_changes_the_state_the_list_is_built_from`, `a_tab_switch_keeps_the_list_present`, `a_library_with_entries_lists_exactly_those_rows`), but a count comes from placeholder rows and `El` exposes no selected state on a tab at this revision, so the accessibility half the spec asserts cannot be decided. Seam: a11y-semantics, library-list |
| `secret rows show only the name and actions without exposing the value` | NOT COVERED | no secret rows exist. The structural guarantee is decided and is stronger than the DOM check: no window state can hold a value and no element in the Finder's tree offers a secret capability. Seam: library-list, details, vault-state. `no_window_state_can_hold_a_secret_value`, `no_finder_element_offers_a_secret_capability`, `a_reveal_is_the_only_call_that_carries_the_value` |
| `the editor reports unsaved changes and clears them on cancel` | NOT COVERED | no editor and no draft state. Seam: editor, dialog. The revision-conflict rules a cancel avoids are ported and tested: `a_draft_the_store_refuses_produces_no_event`, `two_editors_cannot_both_save_the_same_record` |
| `locked vault hides the list and offers one clear unlock action` | NOT COVERED | the Library state carries one vault_unlocked flag that the header text reflects, and no lock/unlock interaction exists. Seam: vault-state, library-list, dialog. `unlocking_reports_the_state_and_never_returns_stale_metadata`, `secret_metadata_never_leaves_a_locked_vault`, `manual_lock_survives_a_restart_and_blocks_every_secret_operation` |
| `finder keeps combobox/listbox semantics while typing` | NOT COVERED | the native tree labels the field (`role`/`label` on the query field, asserted through the element tree) and lists rows with positions in a set, but there is no combobox/listbox pairing: no expanded state, no controls relationship, no active descendant, and typing does not search. Seam: a11y-semantics, search-ranking. `an_empty_finder_shows_the_query_field_and_no_rows_or_context_chip`, `a_finder_with_candidates_lists_exactly_those_rows` |
| `copying from the finder reports a copied state without the value` | NATIVE-TEST | the fact is reported and the value is not: `a_command_and_a_secret_copy_the_same_way_and_say_different_things` pins the two shipped sentences verbatim (`Copied the command to the clipboard.` / `Copied the secret to the clipboard.`, the branch being the kind alone), and `a_copied_value_reaches_the_clipboard_and_nowhere_the_window_can_show` scans every element's accessible name, visible text and key for the value while *requiring the row's name to be on screen* so it cannot pass on an empty tree. `the_view_state_carries_the_fact_and_no_field_a_value_could_be_written_into` closes the third surface - the state's `Debug`, which is what a panic or an `assert_eq!` diff would print - and fails if `Finder`'s hand-written impl is ever swapped back to a derive that prints through the resolver |
| `a locked vault turns the finder into one keyboard-reachable unlock row` | NOT COVERED | the palette has no unlock affordance and no rows are ever generated from a locked state. Seam: vault-state, search-ranking, keyboard-delivery |
| `the finder footer presents keyboard hints as keycaps` | PARTIAL | the footer draws labelled hint chips and the check now runs in both directions: `every_footer_chip_describes_a_binding_the_palette_actually_honours` asserts the four chips the shell honours in the shipped order and the shipped words (adding `Enter copy` now that `Enter` is bound), rejects a chip for any binding that is not made (`Ctrl+O` details, `Ctrl+P` ignore-context, Space - declined for the IME reason recorded in `ui.rs`), then *drives* each advertised binding once so a chip cannot outlive the key underneath it. Seam: keyboard-delivery, a11y-semantics - none of this is a key that came from a device, and a labelled chip is not the shipped `<Keycap>` glyph markup |
| `narrow windows keep one pane with an explicit way back to the list` | NOT COVERED | no breakpoint behaviour and no second pane to leave. Seam: responsive-layout, library-list, details. The size invariants that are decided are the window specs themselves: `the_two_window_specs_match_the_tauri_shell`, `the_library_opens_hidden_and_cannot_be_shrunk_below_the_tauri_minimum` |
| `a locked vault is presented as a state, never as an error` | NOT COVERED | no lock state reaches a view; the header text is the only vault surface. Seam: vault-state, library-list. `the_palette_sees_the_same_shared_read_model_as_the_library`, `a_search_carries_the_request_id_and_generation_it_must_be_rendered_at` |
| `minimal list rows show names and actions without overflowing` | NOT COVERED | the rows the tree does build carry exactly the entries in state and one press target each, and that press now opens a confirmation instead of touching the list (`a_library_with_entries_lists_exactly_those_rows`, `pressing_a_row_reaches_that_rows_handler`, `the_list_behind_an_open_confirmation_cannot_act`), but overflow is a painted property and the rows are placeholder names, not repository records. Seam: paint, library-list |
| `a fresh editor offers an optional project folder before saving` | NOT COVERED | no editor. Seam: editor, folder-picker. The folder rules a save would apply are ported and tested: `tilde_folder_paths_expand_to_home`, `spaces_in_working_directory_are_preserved` |
| `a copied confirmation does not describe the next search` | NOT COVERED | the surface exists and its lifecycle is now partly pinned - `escape_clears_a_failed_line_and_leaves_a_confirmation_alone` holds the shipped asymmetry exactly (`hide()` nulls `error`, not `status`) - but the rule this row states is the *other* half: `runSearch` clearing `status` so a confirmation cannot survive into a new query's results. That needs the search behind the field. Seam: search-ranking, events |
| `the selected finder row is perceivable without colour alone, and hover is not selection` | NOT COVERED | selection is drawn from a token fill and no non-colour marker or aria-selected state is wired, so neither half is decided; the state behind it is (`pressing_a_result_selects_it`, `the_highlight_clamps_at_both_ends_instead_of_wrapping`). Seam: a11y-semantics, paint |
| `a locked finder keeps secret names, types and tags out of the DOM` | NOT COVERED | the palette never shows secret rows at all, and the structural no-value guarantee is decided; the filtered-by-lock behaviour the spec asserts needs the search port. Seam: vault-state, search-ranking. `no_window_state_can_hold_a_secret_value`, `secret_metadata_never_leaves_a_locked_vault`, `the_secret_list_matches_its_type_box_as_a_prefix` |
| `a long name truncates without losing the right-aligned action` | NOT COVERED | a painted property of rows that exist only as placeholders. Seam: paint, library-list, responsive-layout |
| `both themes apply through the same tokens and stay above 4.5:1 in the live DOM` | NOT COVERED | the numbers are ported and measured - all 76 shipped pairings across all 24 palettes clear every threshold (`the_contrast_audit_clears_every_threshold_in_all_twenty_four_palettes`, `the_audit_measures_the_pairings_the_shipped_script_measures`, `the_rust_audit_measures_the_shipped_scripts_pairings`) - but nothing paints them, so the live-DOM half is undecidable here even for the views that exist. Seam: theme-switch, paint |
| `reduced motion collapses the transitions and the shipped shell stays opaque` | NOT COVERED | the Finder's opaque, fixed-size, borderless spec is asserted from the window options (`the_finder_asks_for_an_opaque_borderless_fixed_size_window`), which is exactly the half the repo's own checklist cares about; motion preferences and any painted transition are undecidable here. Seam: paint, theme-switch. The owner-desktop rows for the mapped window are in the feasibility ledger |
| `keyboard focus draws the ring from --focus-ring` | NOT COVERED | the token is ported and structurally pinned, but no native view requests focus, draws a ring, or reads the palette at all. Seam: theme-switch, focus-management, paint. `focus_starts_empty_and_a_press_puts_it_in_the_field` is the whole of what is decidable about focus today |
| `the inspector keeps every metadata value but only one chip` | NOT COVERED | no details or inspector surface exists in either native window, so the metadata the shipped inspector keeps has nowhere to be laid out and no chip to be reduced to. Seam: details, library-list |
| `the action area stays put at the minimum window and keyboard focus lands on it` | NOT COVERED | the minimum size is asserted as a spec (`the_library_opens_hidden_and_cannot_be_shrunk_below_the_tauri_minimum`) and the view fills whatever surface it is given (`the_library_measures_the_surface_it_was_asked_to_fill`); painted position and focus placement are undecided here. Seam: paint, focus-management |
| `settings, the delete dialog and the palette draw one shared shell frame` | NOT COVERED | two of the three surfaces exist and neither shares a frame primitive with the other: the delete confirmation now builds a real panel from the card/fill/border/radius tokens, but the palette has no matching surface call and the settings view does not exist. Whether any of it is the *same* frame is a painted question anyway. Seam: dialog, settings, theme-switch, paint |
| `the empty library and populated inspector use the same icon style and chips` | NOT COVERED | the empty state exists as text only and there is no inspector; the SVG set the shipped UI uses is not ported. Seam: details, paint, library-list. `an_empty_library_shows_the_empty_state_and_no_list` covers the empty state's structure |

### tests/e2e/screenshots.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `finder at the real palette size (680x420)` | NOT COVERED | the palette draws and measures 680x420 as a spec and a surface (`the_finder_identity_is_the_string_the_compositor_rule_matches`, `the_sizes_match_the_windows_the_tauri_shell_opened`, `the_finder_measures_the_surface_it_was_asked_to_fill`); what a capture would show - text, fill, shadows - is the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: paint |
| `finder with a detected project, at the real palette size (${theme})` | NOT COVERED | one row standing for 4 themes: the chip element renders only when a context is set (`an_empty_finder_shows_the_query_field_and_no_rows_or_context_chip`), but no context detection reaches a native window and nothing ranks by it. the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: theme-switch, search-ranking, paint |
| `library with a long name and long values (overflow check)` | NOT COVERED | no repository rows and no secret values in reach, so there is nothing long to overflow, and the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: library-list, details, paint |
| `settings at the real library size and at the minimum window size` | NOT COVERED | the Library's 1040x700 spec and 760x520 minimum are asserted from the window options (`the_library_opens_hidden_and_cannot_be_shrunk_below_the_tauri_minimum`) and the Library fills its surface (`the_library_measures_the_surface_it_was_asked_to_fill`); there is no settings view at either size, and the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: settings, paint |
| `settings import review card at the real library size` | NOT COVERED | no settings view and no restore-review card to capture, and the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: settings, backups, dialog, paint |
| `library record at the real minimum window size` | NOT COVERED | no record surface exists; the entry list is a placeholder of synthetic names, and the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: library-list, details, paint |
| `project task review at the real library size and at the minimum window size` | NOT COVERED | no import review to lay out at any width, and the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: project-import, dialog, responsive-layout, paint |
| `project task review with far more tasks than fit on screen` | NOT COVERED | no review list, so no scroll area to bound - the bounds that do exist are on what a scan reads at all: `a_definition_file_over_the_size_cap_is_ignored_not_read`, `just_dump_rejects_failed_and_oversized_output`. the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: project-import, paint |
| `library review set @ ${viewport.name}` | NOT COVERED | one row standing for 3 viewports of a surface that is not ported at all. the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: library-list, responsive-layout, paint |
| `finder review set @ ${viewport.name}` | NOT COVERED | one row standing for 3 viewports. The palette view does draw and measure its surface (`an_empty_finder_shows_the_query_field_and_no_rows_or_context_chip`, `a_finder_with_candidates_lists_exactly_those_rows`, `the_finder_measures_the_surface_it_was_asked_to_fill`), so the structure behind a capture exists while the capture itself does not. the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: paint, search-ranking |
| `shell history suggestions @ desktop-1440 and mobile-390` | NOT COVERED | no history panel at any width, and the state it would fold into is absent too. the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: history-suggestions, responsive-layout, paint |
| `project import review captures in ${theme}` | NOT COVERED | one row standing for 4 themes of an unported review, and no palette to render it in either. the thing captured is painted pixels, and this host has no GPU, no Vulkan ICD, no session and no compositor, so no frame is ever composited here. Seam: project-import, theme-switch, dialog, paint |

### tests/e2e/settings.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `import requires a review that shows the archive date, counts and what gets replaced` | NOT COVERED | no settings view and no review card; the inspect step a review would show is ported and tested. Seam: settings, backups, dialog. `inspect_reports_date_and_counts_without_touching_anything`, `a_bulk_reload_announces_the_list_before_the_vault`, `corrupt_truncated_and_newer_archives_are_refused_without_changing_anything` |
| `the confirm step offers a pre-restore backup and says it took one` | NOT COVERED | no dialog and no confirm step. Seam: settings, backups, dialog. `a_backup_round_trips_commands_secrets_and_usage`, `environment_versions_round_trip_in_encrypted_backups` |
| `export refuses an existing file and only replaces after explicit confirmation` | NOT COVERED | no export UI; the never-overwrite rule it would rely on is ported and tested. Seam: settings, backups. `an_export_never_silently_overwrites_and_leaves_no_staging_files`, `a_destination_that_appears_mid_export_is_never_overwritten` |
| `the recovery credential stays hidden until revealed and shows the restore steps` | NOT COVERED | no settings view and no reveal control; the credential handling is ported and tested in the service layer. Seam: settings, recovery, clipboard. `a_recovery_key_is_created_once_kept_private_and_reusable`, `wrong_or_missing_recovery_credentials_are_rejected`, `backup_files_never_contain_plaintext` |
| `Settings loads in a locked profile and a general setting can still be changed and saved` | NOT COVERED | no settings view at all; saving and its event order are ported and tested. Seam: settings, vault-state, events. `saving_settings_announces_the_theme_before_handing_the_result_back`, `missing_legacy_terminal_uses_an_installed_one`, `history_suggestions_are_off_by_default_and_clamped_when_on` |
| `a revealed recovery credential disappears the moment the vault locks` | NOT COVERED | no reveal surface exists to clear. Seam: recovery, vault-state, clipboard. The lock-side rules are ported and tested: `a_failed_unlock_keeps_the_manual_lock_and_the_flag_file`, `manual_lock_survives_a_restart_and_blocks_every_secret_operation`, `nothing_sensitive_appears_in_the_vault_file` |
| `a revealed recovery credential is not kept across a close and reopen` | NOT COVERED | no view, so no reveal lifetime to test. Seam: recovery, settings, window-lifecycle. `a_recovery_credential_restores_an_archive_into_a_fresh_profile` |
| `the recovery credential copies through the backend clipboard path, never the browser one` | NOT COVERED | no settings view and no reveal control, so there is no path to take. The divergence this row guards against cannot occur in the native shell at all - there is no webview and therefore no browser clipboard API anywhere in it - which is a structural fact, not a test, so the row stays open until the credential surface exists and can be pointed at the seam. What is decided about that seam: no copied value ever reaches a window (`a_copied_value_reaches_the_clipboard_and_nowhere_the_window_can_show`) and no element or state surface in either window can carry one (`no_window_state_can_hold_a_secret_value`). Seam: settings, recovery |
| `at the minimum window size the footer and a confirmation card stay reachable` | NOT COVERED | the minimum size is asserted as a spec (`the_library_opens_hidden_and_cannot_be_shrunk_below_the_tauri_minimum`) and each view fills the surface it is given; painted reachability and a confirmation card are not decidable here. Seam: paint, dialog, responsive-layout |
| `the terminal field parses quoted arguments instead of splitting on spaces` | NOT COVERED | no settings field exists to type into, while the parser it would call is ported and tested one for one: `splits_plain_words_like_the_old_splitter_did`, `keeps_paths_and_arguments_that_contain_spaces_when_quoted`, `supports_escapes_and_empty_quoted_words`, `round_trips_argv_through_the_input_field`. Seam: settings, argv |
| `Settings lists installed terminals and saves the selected launch command` | NOT COVERED | no settings view; discovery and the argv shape are ported and tested. Seam: settings, run-output. `discovered_terminals_pass_the_shell_as_argv`, `missing_legacy_terminal_uses_an_installed_one`, `metadata_never_reaches_the_argv` |
| `an unbalanced quote blocks saving the terminal setting` | NOT COVERED | no form to block; the rejection is ported and tested in the parser: `rejects_unbalanced_quotes_instead_of_guessing`, `only_ambiguous_words_get_quoted`. Seam: settings, argv |
| `Settings offers the project ranking switch and stores it` | NOT COVERED | no settings view; the ranking the switch controls is ported and tested. Seam: settings, search-ranking. `no_context_ranks_exactly_as_it_did_before`, `within_a_match_tier_the_current_project_beats_usage` |
| `a failed settings save stays open with the changed value and error` | NOT COVERED | no dialog to stay open. Seam: settings, dialog, events. The refused-write rules are ported and tested: `a_draft_the_store_refuses_produces_no_event`, `a_refused_answer_loses_its_payload_without_naming_it` |

### tests/e2e/updates.spec.ts

| Behaviour (the test title) | Status | What the native shell has, or what is missing |
| --- | --- | --- |
| `Settings shows the running version and says when it is current` | NOT COVERED | no settings view, and the native shell has no updater wiring at all: the Tauri updater plugin stays on the shipped side. Seam: updates, settings |
| `Check for updates finds a release that appeared after launch and installs it` | NOT COVERED | no updater call exists in the shell to check or install. Seam: updates |
| `a failed check shows the reason in Settings and a way there from the header` | NOT COVERED | no settings view, no header, and no updater to fail. Seam: updates, settings |
| `a browser preview says updates need the installed app` | NOT COVERED | a property of the browser preview build of the shipped frontend; the native shell has no update path to compare it with. Seam: updates |

## The vitest units

Every one of these has a ported Rust twin, which is what the `NATIVE-TEST` label below
means: the rule itself moved, with its test, into `dev-console-core`.

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
| `moves and clamps without wrapping past the ends` | NATIVE-TEST | `moves_and_clamps_without_wrapping_past_the_ends`, and the same rule exercised through injected keys on a real tree: `the_highlight_clamps_at_both_ends_instead_of_wrapping` |
| `keeps the stable id when the list is replaced with a matching row` | NATIVE-TEST | `keeps_the_stable_id_when_the_list_is_replaced_with_a_matching_row` |
| `has no selection when there are no results` | NATIVE-TEST | `has_no_selection_when_there_are_no_results` |
| `shows a muted completion only for a real name prefix` | NATIVE-TEST | `shows_a_muted_completion_only_for_a_real_name_prefix` |
| `Tab accepts a prefix completion and replaces the query for a fuzzy match` | NATIVE-TEST | `tab_accepts_a_prefix_completion_and_replaces_the_query_for_a_fuzzy_match`, and the view-side write through the zeroising replacement: `tab_writes_the_highlighted_row_into_the_field`, `tab_replaces_the_whole_query_when_the_match_is_fuzzy` |
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

## Where the gaps cluster

Machine-checked against the rows above: `the_matrix_summary_counts_match_its_rows`.

| Status | Rows |
| --- | --- |
| NATIVE-TEST | 26 |
| PARTIAL | 10 |
| NOT COVERED | 133 |
| **Total** | **169** |

| Missing seam | Rows waiting on it | What has to be built for the rows to move |
| --- | --- | --- |
| `dialog` | 27 | one shared shell exists and the delete confirmation sits on it; the import review, the restore review and the unsaved-changes prompt still have no native surface |
| `library-list` | 31 | an entry list built from the repository instead of a placeholder of names |
| `paint` | 29 | nothing this host can decide: rasterised appearance, and the owner-desktop rows in the feasibility ledger |
| `settings` | 28 | the settings view and its save/error round trip |
| `search-ranking` | 29 | wiring the ported ranker behind the palette field so typing searches |
| `project-import` | 21 | the project scan/import review and its selection round trip |
| `events` | 19 | the service event stream reaching a view (entries-changed, vault-state, theme-changed) |
| `history-suggestions` | 17 | the shell-history suggestion panel and its dismissal round trip |
| `vault-state` | 15 | the lock/unlock interaction in a view, on top of the vault rules already ported |
| `details` | 14 | the inspector/details panel: metadata rows, extra fields, reveal |
| `clipboard` | 3 | the write seam itself exists and is hygiene-tested (`Enter` resolves an id, writes a value, and reports the fact without the value); what still waits on it is a screen with something to reveal - the details panel's reveal-to-copy and the recovery credential, three rows, all of which need a surface that shows a value in order to copy it |
| `editor` | 14 | the entry editor: draft state, validation gate, save and cancel |
| `a11y-semantics` | 9 | combobox/listbox/tab/option semantics on native elements (Rune exposes no `selected`) |
| `theme-switch` | 11 | handing `dev_console_core::theme` to Rune instead of the built-in palette |
| `responsive-layout` | 10 | one-pane and breakpoint behaviour in the native views |
| `keyboard-delivery` | 8 | the bindings the shell still does not route (Space, Ctrl+O, Ctrl+P, Delete), and the fact that no key in any headless suite came from a device |
| `backups` | 6 | the encrypted backup/export/restore round trip in a view |
| `focus-management` | 4 | placing focus: initial focus, focus restore, focus follows activation |
| `folder-picker` | 6 | the XDG portal folder picker that replaces the Tauri dialog plugin |
| `run-output` | 7 | the command runner and its output, stderr and exit-code panel |
| `environment` | 4 | the project environment view: dated versions, copy, pause |
| `recovery` | 4 | the recovery credential surface: reveal, restore steps, clearing on lock |
| `updates` | 4 | the updater, which still lives only on the shipped side |
| `window-lifecycle` | 4 | show/hide/re-show lifecycle: the reset a re-opened window would run |
| `argv` | 2 | nothing to type into: the parser is ported, the settings field is not |

| Spec | Rows | NATIVE-TEST | PARTIAL | NOT COVERED |
| --- | --- | --- | --- | --- |
| `tests/e2e/environment.spec.ts` | 4 | 0 | 0 | 4 |
| `tests/e2e/finder.spec.ts` | 24 | 1 | 6 | 17 |
| `tests/e2e/history.spec.ts` | 14 | 0 | 0 | 14 |
| `tests/e2e/library.spec.ts` | 20 | 0 | 2 | 18 |
| `tests/e2e/omarchy-themes.spec.ts` | 4 | 0 | 0 | 4 |
| `tests/e2e/project-import.spec.ts` | 20 | 0 | 1 | 19 |
| `tests/e2e/projects.spec.ts` | 6 | 0 | 0 | 6 |
| `tests/e2e/redesign.spec.ts` | 23 | 1 | 1 | 21 |
| `tests/e2e/screenshots.spec.ts` | 12 | 0 | 0 | 12 |
| `tests/e2e/settings.spec.ts` | 14 | 0 | 0 | 14 |
| `tests/e2e/updates.spec.ts` | 4 | 0 | 0 | 4 |
| `tests/unit/argv.test.ts` | 5 | 5 | 0 | 0 |
| `tests/unit/contrast.test.ts` | 1 | 1 | 0 | 0 |
| `tests/unit/design-tokens.test.ts` | 4 | 4 | 0 | 0 |
| `tests/unit/finder-state.test.ts` | 8 | 8 | 0 | 0 |
| `tests/unit/validation.test.ts` | 6 | 6 | 0 | 0 |

## What closing these gaps takes, in the order that unblocks the most rows

The seam counts above are not independent: a handful of builds move dozens of rows, and one of them
(the clipboard) is the single largest unlock in the Finder and Settings columns. In dependency order:

1. **The clipboard seam - landed, and narrower than the row it was written for.** What exists now:
   `Enter` on the palette's highlighted row resolves that row's id through a `CopySource` seam, the
   value goes to the clipboard through Rune's secret-copy path (so the expiry window and the "is this
   still ours to clear?" guard are the toolkit's, exercised by test rather than described), and the
   window reports only *that* it copied, in one of two fixed strings. The secret-hygiene half arrived
   with it, as far as this host can enumerate it: no copied value reaches any element's accessible
   name, visible text or element key, or the view state's `Debug`. What the seam does *not* have is the
   other end - the shell's own resolver answers "nothing" (`copy::Unwired`), so today's binary shows
   `Copy failed. Nothing was copied.` rather than inventing a value - and four screens still want a
   copy control to point at it (details reveal, recovery credential, command output, environment
   versions). Logs, screenshots and retained undo stay on the owner-desktop side, and the ledger keeps
   them `NOT BUILT` for want of an editor to hold a draft.
2. **The ranked search behind the palette** - `Finder::rows` becomes the ported ranker's
   `SearchResult`s, so `selects_the_first_result_automatically` and the rest of the ported
   selection rules are finally driving visible rows.
3. **A repository-backed entry list, then the details panel, then the editor** - in that order,
   because the editor's validation rules and the details panel's masking rules are already ported
   and only need a surface.
4. **The shared dialog shell - started, not finished.** `crate::dialog` is the one modal the shell
   has; it owns the focus trap, the initial focus, the Escape/Cancel/press-off-panel routes and the
   restore, and the delete confirmation is the screen standing on it. What still waits on it is the
   other three confirmations the app shows - the import review, the restore review and the
   unsaved-changes prompt - and whatever the settings panel needs it for.
5. **The focus-management gap that goes with it - halved.** A modal now places focus, keeps it inside
   itself and hands it back. What the shell still cannot do is place it *by identity*: list rows are
   keyed `entry-{index}`, so the restore after a deletion lands on whichever record now occupies that
   slot instead of dropping it the way the shipped guard does (`if (trigger?.isConnected)
   trigger.focus()`). The divergence is named in the `keeps keyboard focus inside the modal` row, kept
   as a `NOT BUILT` line in the feasibility ledger, and pinned by
   `a_removed_record_leaves_the_focus_on_the_row_key_that_now_holds_someone_else` - asserted as
   behaviour to replace, not behaviour to keep. The same identity-keying is what the `library-list`
   seam asks for, so it is one build and not two.
6. **Settings, then backups and the recovery credential**, then **project import**, **history
   suggestions** and **the environment view**. Each is a view on top of rules that already have
   tests, which is why their rows quote those tests: the port is done, the screen is not.
7. **Handing the palette to Rune** (`dev_console_core::theme` to `tok::theme`) and **the portal
   folder picker**, both of which are prerequisites for whole spec files rather than rows
   (`omarchy-themes.spec.ts`, the Browse rows in `project-import.spec.ts`).
8. **A11y semantics last, not first.** `El` at this Rune revision exposes no `selected` state on a
   tab or an option, so the combobox/listbox rows cannot be finished by porting alone; they need
   either a Rune feature or a documented, honest divergence. Until then those rows stay
   `NOT COVERED` even where the behaviour underneath them works.

Everything above this line is decidable in this container. The `paint` and `window-lifecycle` rows
are not, and no amount of work here will decide them: they are the owner-desktop checks the
feasibility ledger already lists, and they stay `NOT COVERED`/`UNVERIFIED` however good the
headless evidence for the state behind them gets.

## How to keep this file honest while the screens land

- Run `cargo test -p dev-console-desktop --lib matrix_tests` after editing either the specs or this
  file. It re-reads the specs, so a new `test(...)` title with no row here fails the build.
- When a screen lands, move the row rather than deleting it: change the status, cite the new test by
  name, and drop the seams that are no longer missing. `NATIVE-TEST` rows may not name a seam - if a
  seam is still missing, the row is `PARTIAL`.
- Never cite a test that only shares a word with the observable. The gate checks the name resolves,
  not that it decides the right thing: that judgement is the reviewer's, which is why each cell says
  in one sentence what the cited test has to do with the row.
- Do not move a `paint`, `window-lifecycle` or `folder-picker` row up on container evidence. Those
  are the owner-desktop's to settle, and the ledger is the place that records how.
