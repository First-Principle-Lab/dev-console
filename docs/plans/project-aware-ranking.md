# Plan: Project-aware Finder ranking

When the Finder opens, Dev Console works out which project the terminal you were using is in, and
ranks that project's commands first.

## Goal

- Press `Super+F` from a terminal that's inside `~/projects/dev-console`. The Finder shows a small
  **dev-console** context chip, and commands tied to that project come first, both with an empty
  query and within each match tier.
- A command is tied to a project when either:
  - its `working_directory` is inside the project root; or
  - it has a `project:<name>` tag, as created by the project-scripts import.
- With no detectable context (a browser was focused, or detection failed), ranking is exactly what
  it is today.
- Detection takes under 30 ms and never delays the Finder from showing.

## Detecting the context (`src-tauri/src/context.rs`, new)

Capture the context **before** the Finder takes focus, at the moment the shortcut fires or
`window.finder` arrives on the control socket.

1. **Hyprland** (primary target): run `hyprctl activewindow -j` (argv, 200 ms timeout) and read
   `pid` and `class`. Under Hyprland, `$HYPRLAND_INSTANCE_SIGNATURE` is set, so the IPC socket at
   `$XDG_RUNTIME_DIR/hypr/<sig>/.socket.sock` can be queried directly to avoid spawning a process.
   Send `j/activewindow` over a `UnixStream` from std; no new crate is needed.
2. **Other X11 desktops**: `_NET_ACTIVE_WINDOW` → `_NET_WM_PID`. The project has no X11 crate,
   so leave this out of v1. Without Hyprland, the context is `None`.
3. **From the window pid to a directory**: terminals run the shell as a child process. Walk
   `/proc/<pid>/task/*/children` breadth-first, at most 3 levels, looking for a known shell
   (`zsh`, `bash`, `fish`, `nu`). Prefer the youngest such process, because tabs and splits create
   several. Read `/proc/<shell>/cwd` with `read_link`.
   - Ghostty and Kitty fit this pattern. With tmux, the shell's parent is the tmux server, so
     detection fails. Accept that for v1 and note it in the docs.
4. **From a directory to a project**: go up to the nearest folder containing `.git`, stopping at
   `$HOME`. If there's none, use the directory itself only when it contains a project file
   (`package.json`, `Makefile`, `justfile`, `compose.yaml`, `Cargo.toml`).
5. Result: `FinderContext { root: PathBuf, name: String }`, stored in `AppState` and replaced on
   every Finder open.
6. Add a setting, `project_context_enabled: bool`, default `true`, in Settings → Finder.

Everything reads local `/proc` and the compositor socket only. Nothing is persisted.

## Ranking (`src-tauri/src/search.rs`)

- `EntryMeta` has no working directory today, and it's deliberately limited to safe metadata.
  Add `working_directory: String` for commands only. It's not secret, but check the "Deliberately
  has no value" comment still holds. Secrets keep an empty string.
- Add `context: Option<&FinderContext>` to `rank()`, defaulting to `None` so existing callers and
  tests are untouched.
- `in_context(entry)`: true when `working_directory` resolves (with `~` expanded, like
  `execution::working_directory`) to a path under `root`, or when the tags contain
  `project:<name>` (case-insensitive).
- Ordering:
  - **Empty query**: pinned, then **in context**, then most recently used, then most used, then
    name.
  - **With a query**: within the same match tier, in-context entries come before usage. Tiers
    themselves still decide first, so an exact match outside the project still beats a fuzzy
    match inside it.
- `SearchResult` gets `in_context: bool` so the UI can mark rows.

## Frontend

- `Finder.svelte`: show a compact chip with the project name at the right of the search field
  (Lucide `folder-git-2`, muted text, using tokens only). Clicking it or pressing `Ctrl+P` clears
  the context for this session, and ranking goes back to normal.
- In-context rows get a subtle marker (a small dot or the folder icon) with an accessible label,
  "in dev-console".
- `ipc-mock.js`: add a way to seed a fake context, `__devConsoleSeed.context`.

## Steps to complete

1. Write `context.rs`: pure functions for walking the process tree over an injectable `/proc`
   root, the git-root walk-up, and the Hyprland reader behind a trait so tests can fake it.
2. Capture the context in `windows::toggle_finder` / `show_finder` before presenting the window.
   Run it on a background thread, emit `finder-context` to the Finder when ready, and show the
   Finder immediately without waiting.
3. Add `working_directory` to `EntryMeta`. Thread `context` through `rank()` and
   `ipc::search_entries`.
4. Build the Finder chip, the row marker, the `Ctrl+P` clear, and the setting.
5. Add a CHANGELOG line and a README Finder section update.
6. Take screenshots of the Finder with and without a context in four themes.

## Tests to add

| Kind | Where | What it proves |
| --- | --- | --- |
| Rust unit | `context.rs` | process-tree walk over a fake `/proc` dir: terminal → shell found; multiple shells → youngest; depth limit; non-shell children ignored |
| Rust unit | `context.rs` | git-root walk-up stops at `$HOME`; marker-file fallback; no marker → `None` |
| Rust unit | `context.rs` | Hyprland JSON parsing (sample `activewindow` output) and failure → `None` |
| Rust unit | `search.rs` | `rank(.., None)` identical to current behaviour (existing tests stay green unchanged) |
| Rust unit | `search.rs` | empty query: pinned > in-context > recency; with query: exact out-of-context beats fuzzy in-context, in-context beats usage within the same tier; tag-based and working-directory-based membership; `~` expansion |
| Rust unit | `search.rs` | determinism: same input, same order, ties broken by name then id |
| Native harness | `scripts/test-native-ipc.sh` or a new script | under Hyprland, start a known shell in a temp git repo, focus it, toggle the Finder, assert the context name. Exits 77 (UNVERIFIED) outside Hyprland |
| Playwright | `tests/e2e/finder.spec.ts` | seeded context shows the chip and puts in-context results first; `Ctrl+P` clears it and restores the default order; no context → no chip and default order; row marker has an accessible label |
| Playwright | `screenshots.spec.ts` | Finder with context at 680×420 |
| Benchmark (manual) | `context.rs` `#[ignore]` test | detection runs in under 30 ms on the target machine; record the timing in `docs/project/TEST_REPORT.md` |

## How to verify it's complete

- [ ] All local checks in `AGENTS.md` pass. The new native harness passes on Hyprland, or reports
      UNVERIFIED with a reason elsewhere.
- [ ] From a Ghostty or Kitty window inside this repo, `Super+F` shows the **dev-console** chip and
      puts this repo's commands first.
- [ ] From a browser window, there's no chip and the order is the same as before.
- [ ] Opening the Finder doesn't feel slower: it appears at once and the chip pops in within a
      frame or two.
- [ ] Turning the setting off removes the chip and the reordering, and `/proc` is not read.
- [ ] Screenshots reviewed.

## Status as built

Deliberate deviations from the sketch above:

* **Focus is read before the palette is shown, the rest beside it.** Asking the compositor on the
  detection thread raced `present()`, which moves focus to the palette, so the answer could name
  the palette itself. `windows::detect_context` now reads `j/activewindow` over the socket in line
  (measured at 0.04 ms on Hyprland, bounded at 50 ms) and leaves only the `/proc` walk and the
  repository walk-up to the thread.
* **Socket first, `hyprctl` second.** The plan had `hyprctl activewindow -j` as the primary read.
  It is the other way round: the socket needs no process spawn, which is what makes the in-line
  read above affordable. `hyprctl` (200 ms bound) is only the fallback for a session whose socket
  is missing, and it runs on the thread - after the palette is shown, so it is best effort.
* **The window title picks the shell, then age.** The plan said "prefer the youngest shell". A
  terminal with several tabs, or a single-instance Ghostty with several windows, has many shells
  under one pid and the youngest is often not the one being looked at. The shell whose directory
  the window title shows wins (longest match first); youngest is only the fallback.
* **Commands only.** A secret tagged `project:<name>` is not moved up or marked.
* **The escape hatch is a key, not just a setting.** `Ctrl+P` (or clicking the chip) ignores the
  context for the current open. A trip to Settings to unblock one palette is too much friction.
* **Lifetime.** The detected project is in memory only and is replaced on the next open (or by
  `Ctrl+P`); it is not cleared on hide, which nothing can observe since only the finder ranks by it.
  The chip and the ranking both read it through `AppState::active_finder_context`, so switching the
  setting off removes both at once, even for a project detected just before.

Known limits, documented in `context.rs` and the README rather than worked around: any focused app
with a shell up to three levels below it counts (an editor's integrated terminal does, on purpose);
tmux started from a shell yields that shell's directory, not the pane's; with a title that names no
folder, the youngest shell is a guess.

Verified - see the "Finder project context" section of `docs/project/TEST_REPORT.md` for the numbers:

- [x] All local checks in `AGENTS.md` pass (fmt, clippy `-D warnings`, `cargo test`, svelte-check,
      eslint, vitest, Playwright).
- [x] The detection chain: process-tree walk over a synthetic `/proc` (including title-based
      shell choice), project resolution (`.git` walk-up, `$HOME` stop line, manifest fallback) and
      `activewindow` parsing - 16 unit tests in `context.rs`; 7 ranking tests in `search.rs`; 2
      token tests in `state.rs`.
- [x] Ranking rules, including that a match tier still outranks context and that "no context" leaves
      the previous order untouched.
- [x] From a browser window there is no context: checked on a live Hyprland session (Brave and Zen
      have no shell under them), and in Playwright for the no-context palette.
- [x] Turning the setting off removes the chip and the reordering (Playwright, including a project
      detected before the switch). That `/proc` is then never read rests on the early return in
      `windows::detect_context`, which is code review, not a test.
- [x] In-process detection on a live Hyprland session, through the same two calls `show_finder`
      makes: 2.5 ms end to end.
- [x] Screenshots reviewed in four themes (dark, light, Tokyo Night, Catppuccin Latte):
      `artifacts/ui-review/raycast-after/18-finder-project-context*__680.png`.

Still needs the real desktop, and reported as UNVERIFIED rather than assumed:

- [ ] `bash scripts/test-project-context.sh`, run by hand at an idle prompt in a Ghostty or Kitty
      window inside this repo, **with this branch's build of Dev Console running**: it opens the
      real palette over the control socket (`window.finder`, the Super+F path), reads
      `finder.context` back and checks it names this repo. It exits 77 with the reason when the
      session, the focused window, the shells in between or the running app cannot make that
      meaningful - which is always the case from `verify.sh`, so the gate can never fail on focus.
- [ ] Super+F by hand with two tabs in different repos, to see the title rule pick the right one.
