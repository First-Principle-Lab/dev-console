# Plan: Import secrets from `.env` files

Pick a `.env` file. Each key becomes a secret in the vault, and keys that are already saved can be
skipped or updated.

## Goal

- **Import .env…** in the Secrets tab (and `dev-console import env <file>` from the CLI) parses the
  file and shows a review list.
- Each row shows the key, the value masked, and a status of **New**, **Same as saved**, or
  **Different from saved**.
- You choose per row: import, skip, or (for different values) replace. "Same as saved" rows are
  skipped by default.
- Secret values never reach the frontend during review. The UI shows only masks and statuses.

## Parsing rules (`src-tauri/src/envfile.rs`, new)

Follow the common dotenv conventions:

- `KEY=value`, and `export KEY=value`.
- Keys match `[A-Za-z_][A-Za-z0-9_.-]*`. Other lines produce a per-line warning, not a failure.
- Blank lines and lines starting with `#` are ignored. An inline ` #` comment ends an unquoted
  value.
- Single-quoted values are literal. Double-quoted values support `\n`, `\t`, `\"` and `\\`, and
  may span multiple lines until the closing quote.
- Values are trimmed unless quoted. An empty value (`KEY=`) is kept but flagged "empty" and is not
  selected by default.
- `${VAR}` interpolation is **not** expanded. Store the literal text and mention this in the
  review.
- A duplicate key within the file: the last one wins, with a warning.
- Files over 1 MiB are refused. Invalid UTF-8 is refused with a clear message.

## Design

### Naming and identity

- Default secret name: the key itself (`GITHUB_TOKEN`).
- An optional **prefix** field defaults to the parent folder name, e.g.
  `dev-console/GITHUB_TOKEN`. The prefix also goes in as a tag, `env:<folder>`.
- `entry_type` = `Env var`. A `Source` extra field (display Text) holds the absolute file path, so
  you can see where it came from.
- A saved secret matches a row when they have the same final name (case-sensitive, because env
  keys are case-sensitive). The status compares values in constant time in the backend.

### Backend flow

1. `inspect_env(path, prefix) -> EnvReview`:
   - Parses the file and compares each key with the vault.
   - Returns `{ token, rows: [{ key, name, status, masked, warning }], warnings }`. `masked` is
     `••••` plus the length bucket, never characters of the value.
   - Keeps the parsed values in memory under the random `token`, with a 5-minute expiry.
   - Clears them when the vault locks, using the same lock hook that clears clipboard copies.
   - Requires an unlocked vault.
2. `apply_env(token, decisions: [{ key, action: import|replace|skip }]) -> ImportOutcome`:
   - Uses the values held under `token`, so the frontend never sends values back.
   - Saves through `state.vault.save`. `replace` updates the existing record with its
     `expected_revision`.
   - Drops the token afterwards, on success or failure.
3. The CLI path `dev-console import env <file> [--prefix p] [--replace]` goes through the control
   socket like `import bitwarden`, so only the path crosses the socket. It prints counts only:
   imported, replaced, skipped and warnings. It never prints values.

### Frontend

- `src/components/EnvImport.svelte`, a dialog built on `Dialog.svelte`:
  - a file path field, like archive import (or the dialog plugin, if one was added later);
  - the prefix field;
  - a review table with per-row action selectors;
  - **Import N / Replace M** buttons.
- Show a short note above the table: "Values stay hidden. Interpolations like `${VAR}` are stored
  as written."
- After import, offer **Delete the .env file?**. It's off by default, needs a confirmation, and
  explains that the plaintext stays on disk unless you remove it. Deleting it is optional and
  needs its own confirmation step.

## Security

- No value is logged, put in an error message, sent to the webview, or kept in a review after
  lock or expiry. Error types may carry key names only.
- The review state lives in `AppState` behind a mutex and is wiped on drop. Add `zeroize` for
  this; it's small and has no dependencies, and the project doesn't have it yet.
- All tests use synthetic values only (`AGENTS.md`).

## Steps to complete

1. Write the `envfile.rs` parser and its unit tests.
2. Add `inspect_env` and `apply_env`, the review-token store with expiry and clear-on-lock, and
   the IPC registration and mock.
3. Add the CLI subcommand, its `USAGE` text, and its parse tests.
4. Build `EnvImport.svelte` and the Secrets tab action.
5. Add a CHANGELOG line and a README section, including a sentence about the plaintext file
   staying on disk.
6. Take screenshots at 1440 and 390 in four themes, including a 50-key file and a file that
   produces warnings.

## Tests to add

| Kind | Where | What it proves |
| --- | --- | --- |
| Rust unit | `envfile.rs` | `export`, comments, inline comments, single vs double quotes, escapes, multi-line double-quoted values, empty values, invalid keys → warning, duplicate key → last wins + warning, `${VAR}` kept literal |
| Rust unit | `envfile.rs` | >1 MiB and invalid UTF-8 refused |
| Rust unit | `state.rs` / new module | review token expires; token cleared on lock; `apply_env` with an unknown or expired token fails without saving anything |
| Rust unit | new module | statuses: new / same / different computed correctly; masks contain no value characters |
| Rust integration | `native-crud.rs` | inspect + apply against a real vault in a temp profile: import 3, re-inspect → all "same"; change one value in the file → "different"; replace → vault value updated, revision bumped |
| Rust integration | `native-crud.rs` | locked vault → `inspect_env` refused |
| Rust unit | `cli.rs` | `import env <file>`, `--prefix`, `--replace` parse |
| Playwright | `tests/e2e/env-import.spec.ts` (new) | review shows masked values only (assert the synthetic value text is absent from the DOM); default selections; replace flow; skip flow; Escape closes; vault-lock event while open clears the review |
| Playwright | `screenshots.spec.ts` | dialog at 1440 and 390, long list, warnings state |

## How to verify it's complete

- [ ] All local checks in `AGENTS.md` pass. The native IPC harness passes or reports UNVERIFIED
      with a reason.
- [ ] Importing a synthetic `.env` creates the secrets. They can be found in the Finder, copied,
      and revealed only after unlock.
- [ ] Importing the same file again: everything reads "Same as saved" and nothing changes.
- [ ] Searching the webview DOM, app logs and CLI output for a synthetic value finds nothing.
- [ ] Locking the vault mid-review closes or empties the review, and applying afterwards fails.
- [ ] Screenshots reviewed.
