# Native feasibility ledger: what is decided, where, and by what

Plan [`../plans/rune-migration.md`](../plans/rune-migration.md) §1 and §4 milestone 1 ask for a
native two-window app with tray, background residency, control-socket wakeups, portal folder
selection, keyboard input and opaque Finder rendering, "tested repeatedly on the real
Hyprland/NVIDIA desktop". Some of that a headless test can decide; some only a real session can.

This ledger names which is which, row by row. It is a gate, not a note:
`cargo test -p dev-console-desktop --lib ledger_tests` parses this file and fails when a row names a
test that no longer exists, when a row's status leaves the vocabulary below, when a desktop run does
not say what ran and when, or when an UNVERIFIED row arrives without the command that settles it.

## Vocabulary

| Status | What it claims |
| --- | --- |
| `HEADLESS-TEST` | A `#[test]` in this repository asserts it with no display. |
| `HEADLESS-RUN` | The real `dev-console` binary was executed with no display and the recorded output is what it printed. |
| `DESKTOP-RUN (owner desktop)` | The real `dev-console` binary was run on the owner's Hyprland/NVIDIA session (Omarchy, WebKitGTK 2.54 beside it) on the date given, and the row records what was done and what was seen. Evidence, not a test: it does not re-run. |
| `UNVERIFIED (owner desktop)` | Not decided yet. The named command settles it on the Hyprland/NVIDIA session. |
| `NOT BUILT` | The Rune shell has no such capability yet, so nobody can check it. |

Nothing in the `UNVERIFIED` or `NOT BUILT` rows is claimed as passed anywhere. `scripts/verify.sh`
reads exit 77 as UNVERIFIED, which is why `app::EXIT_NO_DISPLAY` is 77.

The desktop runs below used a throwaway profile (`DEV_CONSOLE_DATA_DIR`, `DEV_CONSOLE_CONFIG_DIR`
and `DEV_CONSOLE_RUNTIME_DIR` under `/tmp`), synthetic entries only, and drove the windows with
`hyprctl`, `wtype` and `ydotool`. The installed Tauri app kept running beside it on its own socket.

## The ledger

| Area | Check | Status | Evidence | Owner-desktop command |
| --- | --- | --- | --- | --- |
| two windows | The Library spec is 1040x700 with a 760x520 minimum, hidden at start and resizable | HEADLESS-TEST | `the_two_window_specs_match_the_tauri_shell`, `the_library_opens_hidden_and_cannot_be_shrunk_below_the_tauri_minimum`, `the_sizes_match_the_windows_the_tauri_shell_opened` | - |
| two windows | The Finder spec is 680x420, opaque, borderless, non-resizable, always-on-top and hidden until asked for | HEADLESS-TEST | `the_finder_asks_for_an_opaque_borderless_fixed_size_window`, `the_finder_identity_is_the_string_the_compositor_rule_matches` | - |
| two windows | Both windows build their real views over a real profile and answer presses and keys | HEADLESS-TEST | `an_empty_library_says_so_and_offers_a_new_command`, `the_first_result_is_selected_and_labelled_for_assistive_tech`, `every_palette_draws_the_library_and_the_finder` | - |
| two windows | What the compositor reports for the mapped windows | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console --finder` then `hyprctl -j clients`: the Finder is title `dev-console-finder`, class `dev-console`, floating, 680x420, on every open that was checked; the Library is title `Dev Console`, class `dev-console`, tiled. | - |
| two windows | The Library stays tiled and the windows around it keep their layout when the Finder appears and disappears | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console --finder` repeatedly beside the tiled Library and a T3 Code window: `hyprctl -j clients` showed the same tiled geometry before and after each open and dismiss. | - |
| two windows | Opaque Finder on NVIDIA/Wayland survives the pointer entering, crossing rows and leaving (the bug AGENTS.md records for the transparent WebKit window) | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console --finder`, cursor moved into, across and out of the palette with `hyprctl` and captured with `grim` each time: the paint stayed whole. | - |
| two windows | Narrow layout on a real window: floated to 760x560, the panes stack with a way back to the list | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console --library`, floated and resized with `hyprctl`, then tiled again: one pane, the List button, back to the list, then the original 941x1004 tile. | - |
| tray | The menu the host is shown, in the shipped order, follows the vault | HEADLESS-TEST | `the_menu_matches_the_shipped_tray_and_follows_the_vault`, `every_menu_item_has_an_action` | - |
| tray | The icon buffer matches its dimensions, and the tray id is the application identity | HEADLESS-TEST | `the_icon_buffer_matches_its_own_dimensions`, `the_tray_id_is_the_application_identity`, `the_app_id_is_the_tray_id` | - |
| tray | A real StatusNotifierItem host lists the entry and its menu, and menu events drive the app | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console` registered with the session's StatusNotifierWatcher as id `dev-console`; `busctl` read the dbusmenu layout (Open library, Open finder, Add command, Add secret, Lock vault, Settings, Quit) and its `Event` calls opened a new secret draft and locked the vault, after which the item read Unlock vault and the tooltip said the vault was locked. | - |
| tray | The icon renders in the bar and a pointer click on it delivers `TrayEvent::Activate` | UNVERIFIED (owner desktop) | The registration and the menu are checked over D-Bus; the bar's own rendering and its primary click were not exercised. | Run `target/debug/dev-console --finder`, dismiss the Finder, then click its disc in the Omarchy bar. The Finder must open. |
| control socket | One JSON request per connection, one JSON reply line, a known kebab-case intent, and nothing else | HEADLESS-TEST | `an_intent_round_trips_over_the_socket`, `the_wire_shape_is_a_stable_kebab_case_intent_and_nothing_else`, `a_request_can_carry_nothing_but_a_window_name` | - |
| control socket | Bad, unknown-shaped and over-long requests are skipped and the next intent still arrives | HEADLESS-TEST | `an_unknown_intent_is_rejected_without_ending_the_wakeup_path`, `an_oversized_request_is_capped_and_the_next_intent_still_arrives` | - |
| control socket | Access control is the filesystem: the runtime directory is 0700 and the socket 0600 | HEADLESS-TEST | `the_runtime_directory_holds_the_private_permissions` | - |
| control socket | A second launch wakes the running UI loop and maps the window it names | DESKTOP-RUN (owner desktop) | 2026-11-08, with the resident `target/debug/dev-console` running, `target/debug/dev-console --library` and `--finder` mapped those windows within a second and exited 0; a second `--finder` while the palette showed hid it. | - |
| control socket | The CLI subcommands and shell-hook protocol of `src-tauri/src/control.rs` on the same socket | NOT BUILT | The Rune shell answers window intents only, on `rune-instance.sock`; the full control protocol moves with the cutover (plan milestone 6). | - |
| single instance | An `flock` the kernel drops when the holder dies, and the exclusive `AF_UNIX` bind | HEADLESS-TEST | `the_second_holder_of_the_lock_is_told_to_forward_instead`, `binding_a_socket_a_live_instance_listens_on_fails`, `a_socket_file_left_by_a_dead_instance_is_replaced` | - |
| single instance | A launch with nowhere to forward to becomes the resident process; only two intents exist | HEADLESS-TEST | `an_intent_with_no_running_instance_is_reported_rather_than_retried_forever`, `flags_name_intents_and_only_the_known_ones`, `flags_parse_into_at_most_one_intent` | - |
| single instance | Exactly one process holds the vault and the windows across repeated launches | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console` with no flag, `--library` and `--finder` launched repeatedly during the session: each forwarded and exited, and `pgrep` found the one resident process every time it was checked. | - |
| residency | The exit-code contract: usage is 0 or 2, and no display is 77 | HEADLESS-TEST | `exit_codes_do_not_collide_with_the_repos_unverified_convention`, `flags_parse_into_at_most_one_intent` | - |
| residency | Starting with no display is reported, not passed | HEADLESS-RUN | On 2026-11-08 `target/debug/dev-console` with `WAYLAND_DISPLAY` and `DISPLAY` unset and a throwaway profile printed `the native shell could not start: ... neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set` and exited 77. | - |
| residency | Closing the Library hides it and the process stays resident; a dirty draft brings the window back with the unsaved-changes prompt | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console --library`, then `hyprctl` window close: clean, the window hid and the process stayed; with an edited name, the window came back with Keep editing, Discard and Save, and Save wrote revision 2. | - |
| residency | Background duties start with the resident process: a sealed startup backup, clipboard expiry, environment sync | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console` printed `sealed backup written`; a copied command was on the Wayland clipboard (`wl-paste`) and gone 32 seconds later, with the 30-second expiry. | - |
| residency | Tray Quit with a dirty editor supports Discard and Save all the way to process exit | UNVERIFIED (owner desktop) | The 2026-11-09 native harness passed Keep editing via Escape, subsequent saving, clean Quit and synthetic-secret clipboard cleanup. The native Discard and Save buttons in the quit prompt still need a desktop check; their draft actions are covered by `closing_with_unsaved_edits_asks_and_each_answer_does_what_it_says`. | Run `target/debug/dev-console --library` with a disposable profile, change a draft, pick Quit, then test Discard and Save separately. Check that only Save writes the draft and each exits. |
| keyboard | The palette's keys: arrows clamp, Tab completes, Enter copies, Escape clears, Ctrl+O and Ctrl+P | HEADLESS-TEST | `up_and_down_move_the_selection_and_clamp_at_both_ends`, `tab_completes_the_selected_name_without_copying_anything`, `enter_copies_the_command_through_the_core_clipboard_and_hides_nothing_of_it`, `escape_clears_the_query_and_the_rows`, `a_detected_project_shows_a_chip_resets_the_selection_and_ctrl_p_ignores_it` | - |
| keyboard | Keys still reach the palette after a click moved focus onto a row | HEADLESS-TEST | `keys_still_work_after_a_click_moved_focus_onto_a_row` | - |
| keyboard | The Library's chords: Ctrl+N, Ctrl+S, Ctrl+1 and Ctrl+2, refused while a draft is dirty | HEADLESS-TEST | `ctrl_n_ctrl_s_and_the_tab_chords_drive_the_window`, `a_dirty_draft_blocks_tab_switches_and_row_selection` | - |
| keyboard | Real key delivery from a Wayland seat to the focused field and window chords | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console` with `wtype` into the focused windows: typing searched and showed the inline completion, Tab completed, Escape dismissed, Ctrl+P dropped the project, Ctrl+O opened the Library on the row, Ctrl+N, Ctrl+S and Ctrl+2 drove the Library. | - |
| keyboard | Text typed through an input method, and Space binding beside Enter | NOT BUILT | Rune's composition flag is not readable from a handler, so Space stays unbound in the palette rather than bound imperfectly. | - |
| modal dialogs | Delete, run approval, command output and unsaved changes, each acting on the record it named | HEADLESS-TEST | `delete_asks_first_names_the_row_and_cancel_keeps_it`, `run_asks_for_approval_then_shows_exit_code_and_both_streams`, `closing_with_unsaved_edits_asks_and_each_answer_does_what_it_says`, `cancelling_the_approval_runs_nothing` | - |
| modal dialogs | On screen: the scrim, the panel, Escape dismissing, focus starting on the safe action | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console --library`: Run and Delete dialogs drawn over a dimmed window, Escape closed them, the unsaved prompt opened with Keep editing focused. | - |
| library shell | Create, edit, pin, copy, delete, run, filter, project groups, import, environment and Settings over the real services | HEADLESS-TEST | `new_then_save_writes_the_command_and_selects_it`, `edit_then_cancel_leaves_the_saved_record_alone`, `pin_moves_the_entry_first_and_flips_its_label`, `copy_goes_through_the_core_clipboard_with_its_expiry_line`, `search_expands_only_the_project_groups_that_match`, `import_reads_a_projects_tasks_and_saves_the_chosen_ones`, `the_environment_panel_lists_saved_versions_and_a_lock_closes_it`, `settings_preview_a_theme_and_closing_without_saving_puts_it_back` | - |
| library shell | The same flows by hand on the desktop | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console --library`: commands created, run (exit code 3 with both streams), copied, pinned, deleted and filtered; 10 tasks imported from this repository; a `.env` snapshot synced and copied; a backup, an export, an overwrite refusal and a reviewed import with pre-backup; themes previewed, saved and followed by the Finder; Ghostty detected as the terminal. | - |
| opaque Finder rendering | Every palette resolves every token the views paint | HEADLESS-TEST | `every_registered_palette_resolves_every_token_the_views_paint`, `system_follows_the_desktop_and_unknown_names_fall_back_to_it` | - |
| opaque Finder rendering | Project context from the focused terminal: the chip, the ranking and the marker | DESKTOP-RUN (owner desktop) | 2026-11-08, a Ghostty window in this repository focused, then `target/debug/dev-console --finder`: the chip read Dev Console, the project's commands led with the folder marker, and the selection started on the first of them. | - |
| portal folder selection | The XDG portal folder picker opens, returns a folder, and the panel uses it | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console --library`, Environment, Browse: `xdg-desktop-portal-gtk` opened "Choose a project folder" on the current folder; Select returned `/tmp/dcproj` and its `.env` was synced. | - |
| portal folder selection | Cancelling the picker leaves the panel as it was | UNVERIFIED (owner desktop) | The cancel branch leaves the draft and the panel untouched in code; no run cancelled the portal dialog. | Run `target/debug/dev-console --library`, open Import from project, Browse, then Cancel in the portal dialog. The path field must be unchanged. |
| secret hygiene | A secret never reaches the element tree, a key or `Debug` unless revealed, and hiding or locking removes it in the same frame | HEADLESS-TEST | `a_secret_is_masked_until_revealed_and_hiding_drops_the_plaintext`, `a_revealed_value_is_gone_the_frame_it_is_hidden_or_locked`, `copying_a_secret_says_so_and_never_puts_the_value_in_the_tree`, `a_secret_draft_is_masked_and_cancelling_wipes_it` | - |
| secret hygiene | The recovery credential appears only on request and is gone on lock and on close | HEADLESS-TEST | `the_recovery_credential_shows_on_request_and_a_lock_takes_it_away`, `the_recovery_credential_is_not_kept_across_a_close_and_reopen` | - |
| secret hygiene | Keyring auto-unlock, manual lock and the authenticated-unlock prompt on the real session | DESKTOP-RUN (owner desktop) | 2026-11-08, `target/debug/dev-console` opened the vault from the Secret Service key at start; Lock hid secret metadata in both windows; Unlock raised the polkit prompt, and cancelling it left the vault locked with the backend's message. | - |
| secret hygiene | Authenticated unlock succeeding through polkit | UNVERIFIED (owner desktop) | The prompt was raised and cancelled; nobody but the owner may answer it. | Lock the vault, press Unlock in `dev-console --library` and authenticate: secrets must reappear. |
| secret hygiene | A clipboard-history manager does not keep a copied secret | UNVERIFIED (owner desktop) | The session exposes no clipboard-history exclusion protocol, which the app reports in Settings and in its copy line. | Copy a synthetic secret from `dev-console --finder` and check the clipboard manager's history after expiry. |
| secret hygiene | Screenshots, logs and retained undo history never hold a value | NOT BUILT | No test inspects a rendered frame or the text widget's undo stack; the log was checked by hand for the recovery credential only. | - |

## Repeatable native validation on 2026-11-09

`python3 scripts/test-rune-desktop.py --output /tmp/dev-console-native-final-20261109`
built on the current branch's `target/debug/dev-console`. The script used private temporary
HOME, data, config and runtime directories. It created one synthetic command and one synthetic
secret through the native editor, then completed 14 Finder appearances, query/completion/arrow/
Escape delivery, five pointer crossings, exact command and secret clipboard copies, tray menu
registration, cancelling a dirty-draft Quit with Escape, saving afterwards, and clean tray Quit.
The process exited 0 and its owned synthetic secret was no longer on the clipboard. Finder stayed
floating at 680x420 with class `dev-console`; Library stayed tiled and neighbouring geometry did
not change. The script's report and captures are local evidence under the output directory.

A no-display run with all three display variables removed exited 77. Authenticated polkit success,
the bar's physical icon click, portal cancellation and clipboard-history exclusion remain outside
these observations. These runs do not award the missing matrix rows or the migration's completion
reward.
