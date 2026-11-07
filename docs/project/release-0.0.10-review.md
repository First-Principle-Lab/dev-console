# Release 0.0.10 review

This release adds encrypted local history for project `.env` and `.env.*` files.
Directory visits in zsh discover projects automatically. Every changed file gets a
version and date; missing sources keep their saved history. Copy uses the native
secret clipboard. Sync respects vault locking and per-project pause settings.

## Review findings resolved

- Terminal, manual and background scans could read concurrently and commit older
  contents after a newer capture. A per-vault mutex now covers reads and commits.
  A regression test queues a scan while the source changes and verifies that it
  saves the latest content after acquiring the mutex.
- Vault reads and writes checked authorization before waiting for their file lock.
  Queued operations now check again after acquiring it, so a manual lock blocks
  them before any read, copy or mutation. Regression tests cover reads and writes.

The review also checked bounded reads, symlink exclusion, encrypted persistence,
archive round trips, older vault compatibility, window authorization, directory
hooks, per-project versioning, missing source recovery and the environment UI.
Finder's startup theme request also incorrectly used the Library-only full Settings
API. Both windows now use a theme-only read command, with saved-theme events and a
refresh on Finder reopen. Native IPC tests preserve the full-settings restriction;
the browser mock now enforces it too. Native Hyprland validation confirmed the saved
Kanagawa palette, six open/dismiss cycles, 24 hover paint checks, query entry,
clearing, arrow navigation and Escape. Finder stayed floating at 680×420 and Library
stayed tiled. No further release blockers were found.

## Validation

- Svelte check: no errors or warnings; ESLint and frontend production build passed.
- Frontend unit tests: 24 passed. Browser tests: 187 passed.
- Rust tests: 176 library tests and 9 integration tests passed; 14 tests requiring
  external providers or a live desktop were ignored locally, not counted as passed.
- Rust formatting and Clippy with warnings denied passed.
- Real zsh directory-hook test passed against a disposable Unix socket.
- The promotion PR and main workflow must both pass before publication is confirmed.

## Scope and limits

Capture requires Dev Console to run with an unlocked vault. Existing shell-hook
installations update at app startup; open a new terminal once to load the new
`chpwd` hook. Existing terminals also report a project on their next command.
Only zsh is integrated automatically. Files in an entered monorepo subfolder are
tracked separately from the repository root. There is no recursive disk scan.

Files are limited to 1 MiB each and saved contents to 32 MiB total. Background
checks run every 30 seconds. Changes lost between scans cannot be recovered.
The vault format is now version 2; older app versions refuse it rather than
silently dropping environment history. Existing version 1 vaults remain readable
by this release. Packages continue to ship as a signed Linux AppImage.
