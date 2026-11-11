# UI revamp scorecard (native Rune shell)

Status: three rounds done on `feat/ui-revamp`, self-scored 90 out of 100 against a baseline of 41.
The native check on Hyprland has not been run, so read that 90 as "headless evidence says so".
Scope is the Rune shell in `crates/dev-console-desktop`. The Svelte/Tauri frontend is untouched
and still ships.

This file is the record of a hill climb: what was wrong, what was changed, how each change was
measured, and what the interface scores now. The target was 90.

## How to read the scores

The scores are a self-assessment by the agent that made the changes. That is a weak kind of
evidence, so three rules keep it honest:

1. A category only moves on a named change with evidence: a measured number, a test in
   `crates/dev-console-ui-tests/tests/revamp.rs`, or a capture you can open.
2. Nothing a headless render cannot show is scored above 9. Pointer feel, real frame pacing on
   NVIDIA and Wayland, and compositor behaviour are listed under [Not verified](#not-verified).
3. The owner's eye is the final judge. Where this file and your reaction disagree, the file is
   wrong; say which screen, and it becomes the next round.

## How the interface was reviewed

Every screen is rendered offscreen by Rune's own headless renderer from the same views and state
the binary uses, over a synthetic profile (two projects, ten commands, three secrets):

```sh
cargo run -p dev-console-ui-tests --example capture -- SCENE OUT.png [WxH] [dark|light] [PALETTE]
# scenes: library, library-empty, library-filter, secrets, secrets-revealed, secrets-locked,
#         editor-new, editor-edit, editor-secret, delete, run, settings, import, environment,
#         status, history, history-empty, finder, finder-query, finder-empty
# CAPTURE_ARGS passes Rune tooling flags, e.g. --scale 1, --script taps.json, --strip motion.png
```

No window is mapped and nothing on the desktop is touched, so the loop can run while the machine
is in use. It proves layout, paint, text and motion on Rune's clock. It does not prove anything
about Hyprland.

Before and after captures at 1x, the scale of the owner's 1080p display, are in
`artifacts/ui-review/native-revamp/before/` and `after/`. The two sets use the same synthetic
profile, except that `after/` has one more command (the long-name stress entry).

## What clean desktop apps do, and what Dev Console did instead

The references are the apps the owner named as feeling clean: Apple's own Mac apps (Passwords,
Notes, Mail, System Settings), Things 3, and Raycast. No colours, icons or measurements were copied
from them. What was taken is the set of habits they share, each of which the baseline broke:

| Habit of the reference apps | Baseline Dev Console | Now |
| --- | --- | --- |
| A list row is a name. Actions on a row appear under the pointer, or live in the toolbar for the selection. | Copy, pin and a red delete on every row, always drawn: three buttons per entry. | Rows are names. Copy and pin appear on hover or keyboard focus. Delete is only in the detail bar. |
| One filled button per view; everything else is plain or an icon. | Five boxed buttons in a bar under every entry, three more in the toolbar, two in the header. | One filled button (Copy). Run or Reveal beside it, plain. Edit, pin and delete are icons. |
| The sidebar is the navigation: a segmented switch, a filter, the list. The app does not print its own name inside its window. | A header band with a logo and "Dev Console", a tab strip with pill counters and an underline, then a toolbar row. Three stacked bands before the first entry. | One column: segmented switch, filter and New, the list, and the vault at the foot. |
| State is shown once. | Vault state as a pill with an icon, a label and a button; "Command" as a tile, a badge and a tab; Reveal twice on the same screen. | Each fact appears once. The vault line is both the state and the control. |
| Selecting something replaces the content at once. Motion is saved for things that move. | Every selection cross-faded the old entry through the new one for half a second. | Content swaps are instant. Dialogs still fade. |
| Labels are sentence case and quiet. One large size, for the thing you are looking at. | Tracked capitals for every label, semibold on every row, half-point font sizes. | Sentence-case labels, medium-weight rows, one 20 pt title, whole-point sizes. |
| Settings are short pages of rows: label on the left, control on the right. | One sheet of six bordered cards, each opening with a paragraph, three window-heights long. | Three pages (General, Backups, Keyboard) of rows with switches; one or two sentences per group; the sheet keeps its size between pages. |
| Feedback floats and leaves. | A banner under the toolbar that pushed the whole window down and never went away. | A toast over the bottom of the window that moves nothing and leaves after 4 s (8 s if it is not a confirmation). |
| Paths are shortened the way a person says them. | `/home/you/long/path/to/project` in full, wherever it appeared. | `~/code/storefront`; long ones keep their last two folders, the full path is the tooltip. |

## Measurements

| Measure | Baseline | Now | How it is measured |
| --- | --- | --- | --- |
| Controls on the Library screen, besides the rows themselves | 24 with 3 rows listed; 3 more for every further row | 16 with all 11 rows listed | Focusable, drawn elements in Rune's `--tree` dump of the `library` scene |
| Controls on the Secrets screen | 22 | 12 | Same, `secrets` scene |
| Buttons added per list row | 3, always drawn | 0 at rest; copy and pin under the pointer | `a_resting_library_shows_a_dozen_controls_however_many_rows_it_has` |
| Worded buttons in the detail pane | 5 | 2 (one filled) | `the_detail_pane_has_one_filled_button` |
| Frames spent animating after selecting a row | 31 (0.5 s), old and new text overlapping | 0 | `--script` tap on a row, Rune's "animating frames" report |
| Frames spent animating after switching tab | 31 | 0 | Same, tap on the other tab |
| Opening narrower than 900 pt | Cross-fade between the wide and the stacked layout | 0 frames | `library` scene at 760x520, `--frames 4` |
| Layout shift when a status appears | List and detail pushed down by the banner's height | 0 pt | `a_status_floats_over_the_window_and_moves_nothing` |
| How long a status stays | Until the next one replaces it | 4 s for a confirmation, 8 s otherwise | `a_confirmation_leaves_on_its_own_and_other_messages_stay_longer` |
| Rows visible before touching anything (2 projects, 10 commands) | 3: groups start folded, hiding the selected entry | All of them | The `library` capture |
| Height above the list | About 136 pt in three bands (header, tabs, toolbar) | 88 pt in one column | The `library` capture |
| Font sizes in use | 11, 11.5, 12.5, 13.5, 15, 17, 18 | 11, 12, 13, 15, 17, 20 | `theme::text` |
| Keys that work in the list | Tab | Up, Down, Home, End, type-to-select, Ctrl+F, Ctrl+E, Escape | `arrows_walk_the_list_and_the_detail_pane_follows` and two more |
| Text contrast on the surfaces the revamp introduced | Not measured | At least 4.5:1 for every pairing drawn, in all 24 palettes | `revamp_text_clears_contrast_on_every_surface_it_is_drawn_on` |
| Settings sheet | One page, about 1,500 pt of content in a 560 pt viewport | Three pages; the sheet is 440 pt whichever is open | `settings_is_three_short_pages_and_opens_on_general` |

### Where "slow" comes from

The largest part of it is the build, not the interface. Measured with Rune's frame report on the
same scripted session (two selections and a scroll), on this machine:

| Build | Library size | First frame | Frame, median | Frame, 95th percentile |
| --- | --- | --- | --- | --- |
| Debug (`target/debug`) | 10 commands | 78 ms | 2.0 ms | 3.7 ms |
| Release | 10 commands | 18 ms | 0.21 ms | 0.49 ms |
| Debug | 410 commands | 367 ms | 16.6 ms | 42 ms |
| Release | 410 commands | 41 ms | 1.7 ms | 7.1 ms |

A 100 Hz display gives a frame 10 ms. The debug binary misses that on every frame once the library
is large, and takes a third of a second to draw the window at all. The release binary is about
ten times faster and stays inside the budget at 410 commands without virtualising the list.
`target/debug/dev-console` is a 141 MB unoptimised build; to judge the interface, run:

```sh
cargo run --release -p dev-console-desktop -- --library
```

These are CPU times from a headless run. They leave out GPU upload and presentation, which only a
real window on the real compositor shows.

## Scores

Each category is scored out of 10 and weighted. The weights follow the owner's complaint: too many
buttons and too much flutter come first.

| Category | Weight | Baseline | Round 1 | Round 2 | Round 3 | What holds it below 10 |
| --- | --- | --- | --- | --- | --- | --- |
| Action density | 20 | 3 | 8.5 | 9 | 9 | The three icons at the foot of the sidebar are explained only by tooltips. The environment dialog still has four buttons. |
| Motion and responsiveness | 15 | 3 | 8 | 9 | 9 | Measured on Rune's clock, not on Hyprland. Capped at 9 until it is. |
| Hierarchy and layout | 15 | 4 | 8 | 8.5 | 9 | A long name is cut about 48 pt early, because the row keeps room for its two hover buttons. A wide window leaves the right half of the detail pane empty. |
| Typography | 12 | 5 | 8 | 8.5 | 9 | One embedded family. The Notes editor is not markdown-aware. |
| Colour, surfaces and depth | 10 | 5 | 8 | 9 | 9 | Six of the 24 palettes were looked at; the rest pass the contrast test but no one has seen them. In several dark palettes the sidebar and the page are almost the same shade. |
| Copy | 8 | 4 | 8 | 8.5 | 9 | The clipboard caveat after copying a secret is still a long sentence, kept on purpose. |
| Consistency | 8 | 5 | 8 | 8.5 | 9 | The theme and terminal pickers are hand-built lists, not the kit's menu. The command field is a hand-built well next to kit fields. |
| Keyboard and flow | 7 | 5 | 6 | 8.5 | 9 | No key copies the selected entry from the list; that is the Finder's job, but it is still a gap. |
| States: empty, locked, error, narrow | 5 | 6 | 8 | 8.5 | 9 | The 410-command case was measured, not looked at for long. No capture of a failed save. |
| **Total** | **100** | **41** | **80** | **87** | **90** | |

The total is the weighted sum divided by 10: baseline 41.2, round 1 79.6, round 2 87.3, round 3
90.0. Round 3 scores every category 9, which is the ceiling rule 2 sets for anything not checked
on the real desktop; the reasons in the last column are why none of them is a 10.

## Rounds

### Round 1: structure and density

- Replaced the header band, tab strip and toolbar with one sidebar: segmented switch, filter, New,
  the list, and a foot holding the vault, Import, Environment and Settings.
- Made list rows names. Copy and pin are drawn under the pointer or keyboard focus; a pinned row
  keeps its pin; delete left the rows.
- Moved the entry's actions to a bar at the top of the detail pane: Copy filled, Run or Reveal
  plain, edit, pin and delete as icons. Removed the duplicate Reveal, the kind badge and the
  revision number (now in the tooltip of "Updated").
- Project groups start open. The project's environment button sits on its heading.
- New type scale and sizes; sentence-case labels; plates without borders; buttons without boxes.
- Selecting, switching tab, folding and editing commit with no animation.
- Status became a toast.
- Dialogs, Settings, Import, Environment and the Finder restyled and their copy shortened.

Caught by the tests in this round: `hover_alone` on a row button made the press fall through to
the row, so pinning from a row selected it instead. Removed.

### Round 2: what a user trips on

- The toast now leaves on its own (`Library::poll`); before, "Copied" stayed until something
  replaced it.
- A copied command is confirmed in one sentence. A copied secret keeps the full clipboard caveat.
- The list is a source list: arrows, Home, End and type-to-select move the selection. Ctrl+F
  focuses the filter, Ctrl+E edits, Escape leaves an editor nothing was typed in.
- Opening the window at a narrow width no longer cross-fades between layouts.
- Finder results replace the list at once when they arrive from the worker thread.
- Fields got a visible well: on the dark palette they were the same colour as the page.
- Added the contrast test for the new pairings, the control budget test, and the release-build
  measurements above.

### Round 3: the long tail

- Settings split into General, Backups and Keyboard pages at a fixed sheet height, with the
  version in the footer and a list of every shortcut the window answers (tested against the
  router, so the list cannot name a key that does nothing).
- The command editor is monospace.
- The clipboard-history note in Settings became a setting row with its consequence under it.
- Stress scene: a 65-character name, a 280-character command, five tags, long notes and a long
  URL field, at 1040 and 760 pt wide. The field label wraps instead of cutting.
- Looked at six Omarchy palettes; no changes needed.

## Review of the change

A second pass over the diff as a reviewer, and over the app as a first-time user. The scores above
were not raised for the fixes; they were already claiming what the defects contradicted.

### Defects found and fixed

| Found | Effect on a user | Fix | Test |
| --- | --- | --- | --- |
| Every row's hidden copy button was a Tab stop. | Tab from the list walked one invisible button per entry before reaching the detail pane: 8 extra stops with 8 commands, 400 with 400. | Row buttons are pointer controls; Tab goes list, then detail bar. | `tab_does_not_stop_on_every_rows_hidden_buttons` |
| Window shortcuts ran behind an open dialog. | Ctrl+E or Ctrl+N under a delete confirmation opened an editor behind it; Ctrl+2 switched tab. Ctrl+N and Ctrl+1/2 did this before the revamp; Ctrl+E and Ctrl+F were new ways to hit it. | A dialog owns the keyboard. | `a_dialog_owns_the_keyboard` |
| A copied secret's three-line message left after 4 s. | Not enough time to read the clipboard caveat. | Reading time is added for every word past eight, up to 20 s. | `a_confirmation_leaves_on_its_own_and_other_messages_stay_longer` |
| The toast layer was `inert`. | Rune reports inert elements as inert to assistive technology, so a screen reader lost every status and error. | Not inert; presses still pass through. | `a_status_floats_over_the_window_and_moves_nothing` |
| Escape on an edited draft did nothing, silently. | The key looked broken. | It says the entry has unsaved edits. | `escape_leaves_a_clean_editor_and_keeps_a_dirty_one` |
| Settings switches look like they apply at once, but nothing is written until Save. | Flip "Start at login", press Close, and it is silently thrown away. | The footer says "Not saved yet", Close becomes Discard, and Save is filled only when there is something to save. | `settings_says_when_a_change_is_not_saved_yet` |

### Open findings

Not fixed, because each is a decision or needs your eye.

1. **The vault line locks on one click.** "Vault unlocked" is a state label that is also the Lock
   button. A stray click locks the vault, and unlocking asks the system for authentication. The
   tooltip says "Lock the vault"; nothing else does. Options: a separate small Lock button, or keep
   it and accept the risk.
2. **Buttons and switches have no 3:1 edge.** `raycast-ui-redesign.md` promised that a meaningful
   control boundary clears 3.0:1. Plain buttons are now a 6% wash with no border, and an off switch
   is a 12% track. Their labels pass 4.5:1 and the contrast test only measures text. This is the
   look the references use, but it is a step back from the earlier promise.
3. **Copy and pin on a row are found only by hovering.** Nothing at rest says they exist. Both are
   also in the detail bar, so nothing is lost by not finding them.
4. **The three icons at the foot of the sidebar** (Import, Environment, Settings) have tooltips and
   no labels.
5. **Clicking the padding of the Command field does not focus it.** Only the text area inside the
   well takes the click; the padding is about 10 pt on each side.
6. **`settings.rs` is 1,600 lines** (1,426 before). The view half should move to its own file
   before more is added.
7. **Project groups start open, so opening one no longer triggers an environment sync.** The 30 s
   background sync still runs; the first sync after launch can now be up to 30 s later.
8. **Copying the same thing twice within a few seconds does not restart the toast's clock.**

### Ease of use, by task

| Task | Baseline | Now | Verdict |
| --- | --- | --- | --- |
| Find a command and copy it | Unfold its project, click the row, click Copy | Click the row (or arrow to it), click Copy; or hover the row and click its copy icon | Easier |
| Same, from the keyboard | Tab through every row and its three buttons | Ctrl+F, type, Down, Tab to Copy, Space | Easier, but no single key copies |
| Add a command | New, fill, Save | The same, Ctrl+N; Escape backs out of an untouched form | Same |
| Delete an entry | Trash on the row | Select it, then the trash in the bar | One step more, on purpose |
| Lock the vault | Lock button in the header | Click "Vault unlocked" | Fewer controls, less obvious (finding 1) |
| Import a project's tasks | Labelled button in the toolbar | Unlabelled icon at the foot of the sidebar | Harder to find the first time (finding 4) |
| Change a setting | Scroll one long sheet, Save | Pick a page, change, Save; the sheet says when a change is unsaved | Easier |
| See why something failed | Banner stays until replaced | Toast for 8 s or more, then gone | Cleaner, but a missed error is gone |

## Deliberate departures

These are choices, not accidents. Each is cheap to reverse if you disagree.

- **The Finder footer names three bindings, not five.** `docs/design/raycast-ui-redesign.md` asked
  for Up/Down, Tab, Enter, Ctrl+O and Escape. Arrows and Escape still work and are no longer
  spelled out. `the_footer_names_the_bindings_the_field_routes` was changed to say so.
- **No delete button on list rows.** Delete is in the detail bar. One test moved to that route.
- **In the stacked (narrow) layout, Import, Environment and Settings are on the list**, one step
  back from an open entry, not on every screen.
- **History suggestions sit under the commands**, as a "Suggested" group, not above them.
- **The native shell now differs from the Svelte frontend.** The capability matrix still passes,
  because it maps behaviours, not layout. The matrix rows about painted details of the old layout
  are not re-read here.

## Not verified

- **Nothing was run on Hyprland.** The live check drives the real desktop with `wtype` and
  `ydotool` and takes the keyboard focus for about half a minute, so it was not started while the
  machine was in use. Finder content changed (footer, vault badge, selected row), the window setup
  did not: opaque, borderless, 680x420, same title. AGENTS.md still requires the check:

  ```sh
  cargo build -p dev-console-desktop
  python3 scripts/test-rune-desktop.py --output /tmp/dev-console-native-revamp
  ```

  That script tabs through the editor by position. The folder field's Browse control is now an
  icon button in the same place in the order; if the script fails at the first save, look there
  first.
- Hover: the row buttons and their tooltips were exercised by the scripted pointer, never by a
  hand. Whether they appear fast enough to feel instant is a judgement only you can make.
- Frame pacing, GPU upload and presentation on NVIDIA.
- `bun` checks were not rerun: no file under `src/` or `tests/` changed. `scripts/generate-icons.py`
  gained three icons for the native shell.
- 18 of the 24 palettes, by eye.
