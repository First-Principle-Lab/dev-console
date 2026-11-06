# Plan: Suggestions from shell history

Offer the commands you run most often as one-click saves to the library. Dev Console reads the
history file locally and keeps everything on your machine.

## Goal

- The Commands tab shows a **Suggestions** panel listing the most-used commands from your shell
  history that aren't already saved.
- Each suggestion has **Save** (opens the editor prefilled) and **Dismiss** (never suggest it
  again).
- No history line leaves the machine, goes into a log, or is stored unless you save it.

## Scope

In:
- zsh (`$HISTFILE`, else `~/.zsh_history`), including the extended `: <ts>:<dur>;cmd` format and
  multi-line entries that end in `\`.
- bash (`~/.bash_history`), plain and `#<ts>`-prefixed.
- fish (`~/.local/share/fish/fish_history`), YAML-like `- cmd:` entries.

Out (later):
- Atuin or other history databases.
- Watching the history file live. A scan runs when the panel opens or on **Rescan**.

## Design

### Backend (`src-tauri/src/history.rs`, new)

1. `pub fn locate() -> Vec<HistorySource>`: finds which history files exist. Reads `$HISTFILE`
   from the environment, then falls back to the defaults above.
2. `pub fn parse_zsh(bytes: &[u8]) -> Vec<String>` and the bash and fish equivalents:
   - Decode with `String::from_utf8_lossy`, because zsh stores some bytes "metafied". Undo zsh
     metafication (`0x83` followed by a byte XORed with `0x20`).
   - Join continuation lines.
   - Drop blank lines and lines that start with a space (the `HIST_IGNORE_SPACE` convention).
3. `pub fn normalise(cmd: &str) -> String`: trims whitespace and collapses runs of spaces. This is
   the key used for counting and for dedupe.
4. `pub fn suggest(lines, saved: &[EntryRecord], dismissed: &HashSet<String>, limit) -> Vec<Suggestion>`:
   - Counts by normalised command and records the last position each one was seen.
   - Skips trivial commands: those with fewer than 2 tokens or shorter than 6 characters, and a
     built-in deny list (`ls`, `cd`, `clear`, `exit`, `history`, `pwd`, `git status`).
   - Skips commands already saved, comparing normalised `command` text.
   - Skips commands that look like they carry a secret (see *Security*).
   - Sorts by `count * recency_weight`, where `recency_weight = 1 + position / total`. Ties break
     on the command text so the order is stable.
   - Suggests a name: the first two tokens, e.g. `docker compose`, plus the first non-flag
     argument when there is one.
5. `Suggestion { key, command, count, suggested_name }`. `key` is a SHA-256 hex of the normalised
   command, so dismissals never store the command text itself. This needs `sha2` as a direct
   dependency; it's already in the build through `age`, so it adds no new code.

### Storage

- Add a `dismissed_suggestions(key TEXT PRIMARY KEY, dismissed_at TEXT)` table through a schema
  version bump in `Repository::open`. The migration needs a test of its own.
- Add `history_suggestions_enabled: bool` to `Settings`, default `true`, with a toggle in Settings
  → General.

### IPC (`ipc.rs`, register in `lib.rs`)

- `history_suggestions(limit) -> Vec<Suggestion>`: library window only (`require_library`).
- `dismiss_suggestion(key)`.
- Saving uses the existing `save_entry`. The editor opens with `command` and `name` prefilled, and
  you confirm.
- Mirror all of these in `tests/e2e/ipc-mock.js` with a seeded fake history.

### Frontend

- `src/components/HistorySuggestions.svelte`: a collapsible panel at the top of the Commands list.
  It's hidden when empty or disabled.
  - Each row shows the command in monospace, a "used N×" badge, and the **Save** and **Dismiss**
    buttons. Icons come from Lucide via `Icon.svelte`.
  - Full keyboard support: rows are focusable, `Enter` saves, `Delete` dismisses.
- `Library.svelte`: show the panel, and open `EntryEditor` with the draft when **Save** is
  clicked.
- `api.ts` / `models.ts`: types and wrappers.

## Security

- The history file is read in Rust and never logged. Suggestions only reach the library window.
- **Secret filter**: skip lines that match any of the following, and add a unit test for each
  pattern:
  - `export NAME=…` or `NAME=value cmd` where NAME contains `TOKEN|SECRET|PASS|KEY|AUTH`.
  - `-p<value>`, `--password[= ]`, `--token[= ]`, `Authorization:`, `Bearer `.
  - URLs with credentials (`scheme://user:pass@`).
  - Long high-entropy strings (32+ characters of base64 or hex).
- Only the hash of a dismissed command is stored.

## Steps to complete

1. Write `history.rs` with the three parsers, `normalise`, the secret filter and `suggest`, along
   with its unit tests.
2. Add the schema migration and the `history_suggestions_enabled` setting.
3. Add the IPC commands and their mock counterparts.
4. Build `HistorySuggestions.svelte` and wire it into `Library.svelte`.
5. Add the toggle in Settings.
6. Add a CHANGELOG line under `## Unreleased` and a README section.
7. Take screenshots at 1440 and 390 widths in Dark, Light and two Omarchy themes, and look through
   them.

## Tests to add

| Kind | Where | What it proves |
| --- | --- | --- |
| Rust unit | `history.rs` `#[cfg(test)]` | zsh extended format, metafied bytes, continuation lines, leading-space lines dropped; bash `#ts` lines; fish `- cmd:` parsing |
| Rust unit | `history.rs` | `normalise` collapses whitespace; counting and recency ordering are deterministic; saved commands are excluded; dismissed hashes are excluded |
| Rust unit | `history.rs` | every secret-filter pattern rejects its sample, and harmless lookalikes (`git push --tags`) pass |
| Rust unit | `repository.rs` | migration from the current schema version adds `dismissed_suggestions` without touching existing rows |
| Rust integration | `src-tauri/tests/native-crud.rs` | with `HISTFILE` pointed at a temp file: suggest, save one, then re-suggest and confirm it's gone |
| Playwright | `tests/e2e/history.spec.ts` (new) | panel lists seeded suggestions; Save opens a prefilled editor and the saved command disappears from the panel; Dismiss removes it and it stays gone after reload; panel is hidden when the setting is off; keyboard-only flow |
| Playwright | `tests/e2e/screenshots.spec.ts` | adds review captures of the panel at 1440 and 390 |
| Playwright | `tests/e2e/omarchy-themes.spec.ts` | panel readable in every theme (existing contrast checks cover the new tokens) |

## How to verify it's complete

- [ ] All local checks in `AGENTS.md` pass. `bun run verify` doesn't report any new UNVERIFIED
      harness.
- [ ] With a real `~/.zsh_history`, the panel lists familiar commands, and nothing that looks like
      a password or token appears.
- [ ] Saving a suggestion creates a normal command that can be found in the Finder and run.
- [ ] A dismissed suggestion stays hidden after an app restart.
- [ ] Turning the setting off hides the panel, and the history file is not read (check with
      `strace -e openat` or a log-free test hook).
- [ ] Screenshots reviewed with no clipping, overflow or contrast problems.
