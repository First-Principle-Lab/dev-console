# Release 0.0.9 review

Reviewed the six open PR heads against dev `561e396` on 2026-11-07.
None was already included in that dev head.

| PR | Disposition |
| --- | --- |
| #20 | Included unchanged through #23. |
| #21 | Superseded by #22/#23. Its detection starts after the palette takes focus and lacks the session token that rejects stale replies. Keep #23's implementation. |
| #22 | Included unchanged through #23, including focus capture before showing the palette and stale detection rejection. |
| #23 | Integrated Finder project ranking and documentation moves. |
| #24 | Integrated shell history suggestions and the existing review fixes. |
| #25 | Integrated project task import and the existing review fixes. |

The integration resolves overlapping settings, icons, API methods, mock handlers,
Library actions, native tests, screenshot tests and Cargo dependencies. Both settings
remain available. History suggestions belong in 0.0.9, not the published 0.0.8 notes.

## Findings fixed

- #25's failing harness run 42174242894 ran all ignored library tests from the keyring
  harness. That included the optional installed-just smoke test and failed without
  just. The harness now selects only `keyring::tests::real_keyring_round_trip`.
  It also avoids running the compositor timing test introduced by #22.
- Project script and Make target names were interpolated unquoted into saved shell
  commands. Task arguments are now quoted when needed, with an option terminator
  for names beginning with a dash. A regression test invokes a synthetic shell
  function to prove spaces, quotes, semicolons and command substitutions stay literal.
- Generated entry names containing spaces could not pass the frontend editor's
  validation. Names now use dashes for whitespace while retaining the actual task
  argument and canonical working directory.

## Validation

- Bun frozen install, Svelte check with zero errors/warnings, ESLint, 24 Vitest tests
  and production frontend build passed.
- Combined browser suite: 171 passed, no failures or skips. Uses synthetic IPC data.
- Rust formatting, Clippy across all targets with warnings denied and locked Rust
  tests passed. Final count: 164 library tests and eight integration tests passed;
  14 environment-dependent tests ignored by default.
- Staged packaging install, desktop/policy validation and exact uninstall passed.
- Keyring harness returned 77 because this host lacks python3-dbus. Provider behavior
  remains unverified locally; GitHub's harness job installs that dependency.
- Native X11 clipboard, native desktop smoke and provider-backed integration remain
  unverified locally because Xvfb/xclip and python3-dbus are absent. The real Hyprland
  terminal-to-palette check must be run from a focused terminal with the candidate
  build installed. This automated session does not establish that acceptance check.
- Arch static checks passed, but the pre-existing real-build harness failed because
  it invokes makepkg from a directory without PKGBUILD. The PKGBUILD also still targets
  0.1.0 and carries a placeholder checksum. An Arch/AUR artifact is not release-ready;
  the repository's release workflow publishes a signed AppImage.

## Promotion

The batch is staged in dev for a dev-to-main promotion PR. Main was already an ancestor
of this batch, so merging main into the candidate required no additional merge commit.
GitHub verification must be green before merge. Merging the promotion PR publishes
0.0.9 through the existing workflow. No release is claimed until the main verification
and publish jobs pass and all three signed updater assets are present.

## Folder picker follow-up

Project import now has a Browse action using the Tauri dialog plugin, with directory
selection enabled and the open-dialog capability granted only to the library window.
Choosing a folder fills its path and scans it; cancelling preserves the reviewed tasks.
Manual path entry remains available. No commands are imported until confirmation.

After this change, all 174 browser tests passed, including selection, cancellation and
picker failure cases. The final focused project-import suite passed all 31 tests. Svelte
check, ESLint, 24 frontend unit tests, production build, Rust formatting, Clippy and the
locked Rust suite passed. Browser tests use a picker stub; the actual native chooser
still needs the owner's desktop test. The running local dev app rebuilt successfully.

## Native Hyprland hover repair

On the owner's Hyprland 0.56.2, WebKitGTK 2.54.1 and NVIDIA 615 setup, Finder
stayed mapped, visible and floating at 680×420 while its transparent native
surface stopped painting. A region capture showed the windows behind it instead
of Finder. Setting `transparent(false)` on the same running build restored the
palette during pointer hover. The page background now matches the opaque shell.
No compositor configuration was changed. The existing float/size/center rule and
DMABUF renderer workaround remain in place.

The corrected native build passed 20 open/dismiss cycles and 40 pointer-out/in
body/footer paint checks using Hyprland client geometry and region captures. The
library stayed tiled at its original position and size. Header focus and row hover
colors were excluded from the paint probes because those are expected to change.
Captures containing the owner's entry metadata were kept out of the repository.
Native typing and keyboard activation remain for the owner's acceptance test.

All 174 browser tests, 24 frontend unit tests, Svelte check, lint, production build,
Rust formatting, Clippy and the locked Rust suite passed. AGENTS.md now records the
opaque-surface requirement, earlier DMABUF workaround, compositor sizing rules and
mandatory native hover checks for future Finder changes.

## Project-based library and recommendations

The owner's follow-up replaces global history suggestions with per-project recording
and groups the command sidebar by canonical project root. The command editor now
assigns project folders. Details, limits, privacy behavior, source-hook setup and
validation are in [project-command-groups.md](project-command-groups.md). The zsh
integration is installed locally; counts start in new terminals, with no inferred
backfill from cwd-less history. The full browser gate passed 180 tests and the final
history/project layout passed 19 focused tests. Rust passed 168 library tests and
nine integration tests. Fourteen environment-dependent tests remain ignored by default.

## Settings Save

Successful Settings saves now close the panel after committing the saved theme and
notifying the library. Failed saves retain the form and show the error. Save is
disabled while pending. The full browser suite passed 181 tests, including failure
retention and successful-save dismissal. Svelte check, lint, 24 frontend unit tests,
production build, Rust formatting, Clippy and the locked Rust suite also passed.
