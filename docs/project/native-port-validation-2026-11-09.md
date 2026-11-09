# Native screen port validation, 2026-11-09

The staged Rune shell now contains Library, Finder, editing and custom fields, project groups,
import, environment versions, history suggestions and Settings. It calls the real core services.
Production remains on Tauri. This is a development integration, not the production cutover or a
claim that the migration has passed every acceptance check.

## Regression fixes

- Held UI callbacks cannot restore secret values or editors after close, cancellation, tab
  changes or locking. A recovery reveal cannot land in a reopened Settings panel. Settings loads
  general controls after a lock without restoring secret counts. Unlock reloads the Secrets tab.
- The secret editor is secure before its first frame. Clearing it wipes the baseline value too.
- Tray Quit offers the dirty-draft prompt. Keep editing cancels quitting. Final exit clears
  decrypted vault state and the clipboard value the process still owns.
- Modal bodies shrink and scroll inside the available window height. The native Settings and
  long project-import footers fit the 760x520 viewport. Narrow toolbars keep project actions
  reachable. Import is absent on Secrets, matching the shipped frontend.
- History suggestions accept Enter and Delete while ignoring Backspace. The scripted key helper
  sends both key down and key up, including activation of controls with Space.
- Test isolation overrides XDG and application data/config/runtime paths as well as HOME. The new
  recorded-history test initially inherited the desktop data path and wrote synthetic history
  rows there. Only rows with this test's generated temporary project roots were removed. The
  corrected test passes against its private database.
- CI checks the root workspace as well as Tauri. The keyring harness targets the moved core test
  rather than reporting success after running zero Tauri tests.

## Checks observed

| Check | Result |
| --- | --- |
| Frozen Bun installation, Svelte check, ESLint | Passed; Svelte reported no errors or warnings |
| Frontend unit tests | 24 passed |
| Playwright against the built frontend | 187 passed, no unexpected failures or flaky tests |
| Workspace Rust tests | 439 passed, 5 environment-dependent tests ignored |
| Tauri Rust tests | 16 passed, 10 environment-dependent tests ignored |
| Formatting and Clippy with warnings denied, both Cargo trees | Passed |
| Real keyring provider | One core round-trip test passed on a private Secret Service bus; run with `PATH=/usr/bin:$PATH bash scripts/test-keyring.sh` so Python can import system D-Bus bindings |
| Native Hyprland screen check | Passed with `python3 scripts/test-rune-desktop.py --output /tmp/dev-console-native-final-20261109` |
| No-display startup | Returned 77 with WAYLAND_DISPLAY, WAYLAND_SOCKET and DISPLAY removed |
| Zsh environment hook and staged install/uninstall | Passed |
| Tauri production compile | `bunx tauri build --no-bundle` passed |
| Project-context manual harness | Branch's in-process probe passed in a real Ghostty terminal; the installed Tauri instance's palette reported this repository after the control request |

The native screen check creates a disposable profile and synthetic entries. It verifies native
command and secret saves, masked secret rendering, 14 Finder appearances, five pointer crossings,
query/completion/arrows/Escape, exact command and secret copies, cancelling a dirty-draft Quit,
saving afterwards, clean tray Quit and clipboard cleanup. Finder stays floating at 680x420 with
class `dev-console`; Library stays tiled and neighbouring window geometry stays unchanged. Its
report and synthetic captures are retained locally in the output directory above.

The capability matrix records 119 native-test rows, 17 partial rows and 33 uncovered rows out of
169. Its remaining rows stay open. No checkpoint score or completion reward is awarded here.

## Limits of the full verification run

`CI=1 bun run verify` did not pass end to end. Its manual project-context check cannot run from
this agent's process ancestry; it passed separately in Ghostty. Rerunning with
`SKIP_PROJECT_CONTEXT_TEST=1` reached the pre-existing Arch packaging harness failure:

```text
== real package build ==
==> ERROR: PKGBUILD does not exist.
FAIL makepkg failed
ARCH PACKAGING CHECKS FAILED
```

The script copies `packaging/arch/PKGBUILD` below its temporary working directory, then invokes
`makepkg` at that directory's root. Its release recipe also predates this migration. Neither a
real Arch package build nor release acceptance is claimed by this change.

The host lacks minisign, xvfb-run and dpkg-deb, so installer-signature, X11 clipboard, Xvfb native IPC,
legacy native-smoke and Debian artifact checks remain unverified. The default pyenv Python lacks D-Bus bindings;
the separate system-Python keyring run above does exercise the actual provider test.

Remaining migration work includes the updater, the full CLI/control and shell-hook protocol,
packaging and production cutover. Remaining acceptance includes polkit authentication success,
physical tray clicks, portal cancellation, clipboard-history behaviour, X11 rendering and the
matrix's visual/accessibility checks. Browser screenshots do not establish native Wayland proof.

## CI follow-up

The first backend CI run on `700ffb2` exposed a scripted interaction failure in the Settings
project-ranking test. `Stage::press` used the target's layout coordinates without scrolling its
containing viewport, so a clipped checkbox could be missed while the helper reported a click.
It now scrolls the target into view before pointer delivery. The regression runs at 760x520,
checks the checkbox state before saving, and checks its position above the fixed modal footer.
It fails with scrolling removed and passes with the fix. All 105 native scripted UI tests and
workspace Clippy passed after the change.
