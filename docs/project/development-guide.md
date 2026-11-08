# Development and verification

This guide covers the current Tauri 2 and Svelte 5 application. For the product overview
and installation quick start, see the [README](../../README.md). The Rune rewrite is
[planned separately](../plans/rune-migration.md) and has not shipped.

Paths in command examples are relative to the repository root.

## Running it

```bash
bun install --frozen-lockfile   # JS/TS toolchain and task runner is Bun
bun run dev                     # frontend only, in a browser (no native IPC)
bun run tauri dev               # the real app against the Vite dev server
bunx tauri build --no-bundle    # release binary at src-tauri/target/release/dev-console
```

The first run creates a vault key in your keyring; if no Secret Service is running, secret
features report themselves unavailable and commands keep working.

## Verifying

```bash
bash scripts/verify.sh
```

That is the whole gate: Bun install, `svelte-check`, ESLint, unit tests (which include the token
contrast audit below), production build,
Playwright (library + Finder + project-task import + redesign + visual review captures),
`cargo fmt --check`, clippy with warnings denied, the Rust test suite, plus the
environment-dependent harnesses:

| Script | What it proves | Without the environment |
| --- | --- | --- |
| `scripts/test-keyring.sh` | the real Secret Service round trip through the app's own keyring adapter | exit 77 = UNVERIFIED |
| `scripts/test-native-ipc.sh` | real SQLite + real age vault + real provider + real X11 selection, across a restart | exit 77 = UNVERIFIED |
| `scripts/test-clipboard.sh` | real X11 selection ownership, expiry retraction, exclusion reporting | exit 77 = UNVERIFIED |
| `scripts/test-packaging.sh` | staged install, desktop file, polkit policy, exact uninstall | exit 77 = UNVERIFIED |
| `scripts/test-arch-package.sh` | Arch package layout from `packaging/arch/PKGBUILD` | exit 77 = UNVERIFIED |
| `scripts/smoke-native.sh <binary>` | the compiled app: control socket, single instance, lock survives restart, private files | exit 77 = UNVERIFIED |

Each harness exits **77** when its environment is missing; that is reported as UNVERIFIED and never
silently converted into a pass. `docs/project/TEST_REPORT.md` records which acceptance criteria are proven, where,
and what is still open.

## Continuous integration

`.github/workflows/ci.yml` runs the frontend, Rust, harness, and native smoke checks on pushes to
`main` and on pull requests. A harness that cannot run in CI exits 77 and is reported as
UNVERIFIED. On a `main` push, the `verify` workflow builds a signed Linux AppImage alongside the
checks and publishes a GitHub release only after all jobs pass. The first release is `0.0.1`; each later successful push
increments the patch version. The version is set in the release build, so no version-bump commit
or extra CI run is created. The repository must be public so the desktop updater can download its
release assets. The release workflow needs the `TAURI_SIGNING_PRIVATE_KEY` repository secret.

Run the AppImage from the GitHub release to receive in-app updates. The library checks at startup,
when focused, and every 15 minutes. An **Update to x.y.z** button appears when a newer signed
release is available. Clicking it downloads, verifies, installs, and relaunches the AppImage.
Linux deb/rpm packages and locally built binaries do not support this in-app AppImage update path.

## Visual review

`artifacts/ui-review/before/` and `artifacts/ui-review/after/` hold the review grid — eleven UI
states captured at 1440, 768 and 390 px (`SHOT_SET=before|after bun run test:e2e -- screenshots`).
These are **UI-shell** checks: the browser preview cannot reach native IPC, so they run against the
documented IPC test double; the native binary is exercised separately by `scripts/smoke-native.sh`.

`raycast-baseline/` and `raycast-after/` are the before/after pair for the Raycast-inspired pass
(`docs/design/raycast-ui-redesign.md`, `docs/design/raycast-ui-research-2026-11-02.md`); the baseline set is 41
PNGs and the redesign set 42 - the extra one, `17-library-minimum-window`, is the state the visual
pass needed and the old grid did not have (the smallest real library window, one pane, action area
on screen). Every other filename is shared, so `diff` between the two directory listings names the
captures that moved. The default `SHOT_SET` is `raycast-after`, which keeps a plain `bunx playwright test` from
rewriting the frozen `after/` set; `SHOT_HTML=1` writes each captured DOM alongside, and any
synthetic secret value reaching a capture fails the run.

Contrast is measured rather than asserted: `bun run check:contrast`
(`scripts/contrast-check.mjs`) reads the palettes out of `src/styles/tokens.css` and
`src/styles/omarchy.css`, composites every
translucent layer over the surface it is painted on and checks 76 real pairings per theme at
4.5:1 for text and 3:1 for state indicators, printing each ratio and each deliberate exemption.
`tests/unit/contrast.test.ts` runs it inside `bun run test`, so a token retune that breaks
contrast fails the gate.

The native app reads the same palette from Rust rather than from CSS.
`crates/dev-console-core/src/theme/` carries the 52 shared tokens and the 24 palettes as generated
tables, the same 76 pairings at the same 4.5:1 / 3.0:1 thresholds, and the same 27 exemptions with
their reasons. `python3 scripts/generate-theme-data.py` regenerates those tables from
`src/styles/tokens.css`, `src/styles/omarchy.css` and `scripts/contrast-check.mjs` (it fails loudly if
a block renames or drops a token rather than reporting a palette it cannot check); `--report` prints
what it parsed without writing. Two rules keep the port honest instead of merely similar:
`theme::inventory` re-reads both stylesheets in the test run and fails if a generated row no longer
matches it, and the audit tests pin all 76 pairings and the measured minima against the Node script's
own numbers, so a pairing cannot quietly vanish from the Rust side. If the CSS palette is retuned,
regenerate and re-run `bun run check:contrast` in the same change; if they disagree, fix the port, not
the numbers. The Rust tables are not a second source of truth - the stylesheets stay it.

Settings includes the 22 stock Omarchy themes installed on this host. Their colors are
snapshotted in `src/styles/omarchy.css` and can be refreshed with
`python scripts/generate-omarchy-themes.py /path/to/omarchy/themes`. The generated roles retain
each theme's background and accent while adjusting text and state colors to meet the app's contrast
checks. `tests/e2e/omarchy-themes.spec.ts` captures the library, secrets, editor, settings and Finder
for each theme in `artifacts/ui-review/omarchy-themes/`.

`artifacts/ui-review/native-redesign/` holds the redesign as actually painted by the compiled
release binary under Xvfb (`scripts/native-capture.sh`): library and palette over a dark and a
busy X11 background, in both themes, with pixel probes proving the opaque shell never bleeds the
background through, keeps square window corners, maps without a flash frame and does not repaint
at rest. Wayland/Hyprland compositor blur cannot be exercised here and stays a manual checklist
in `docs/design/raycast-ui-research-2026-11-02.md`; the shipped material is an opaque shell, not glass.

## Keyboard

| Where | Keys |
| --- | --- |
| Desktop | `Super+F` Finder (configured in your compositor, see below) |
| Library | `Ctrl+1`/`Ctrl+2` tabs, `Ctrl+N` new, `Ctrl+S` save, `Esc` dismiss dialogs |
| Finder | `↑`/`↓` select, `Tab` complete, `Enter` copy, `Ctrl+O` open in library, `Ctrl+P` ignore the detected project, `Esc` close |

## Desktop integration (Hyprland and other Wayland compositors)

Tauri cannot register a *global* shortcut on wlroots compositors, so the binding lives in the
compositor. For Hyprland:

```ini
# hyprland.conf
bind = SUPER, F, exec, dev-console --finder
```

`dev-console --finder` talks to the already-running instance over
`$XDG_RUNTIME_DIR/dev-console/control.sock` (mode 0600) and raises the Finder; if nothing is
running it starts one in the background. The same socket serves the CLI (`dev-console vault lock`,
`dev-console vault status`), which is how you wire a lock-on-screen-lock rule:

```ini
# hyprland.conf — lock the vault when the session locks
bindl = , exec, dbus-send --session --dest=org.freedesktop.DBus \
  /org/freedesktop/DBus org.freedesktop.DBus.AddMatch string \
  "type='signal',interface='org.freedesktop.DBus.Properties'"
exec-once = bash -c 'while true; do busctl --user wait org.freedesktop.ScreenSaver.Active true >/dev/null 2>&1 \
  && dev-console vault lock; done'
```

or, with `hyprlock`/`swaylock`, add `dev-console vault lock` to the lock command's pre-hook.

**Session-lock behaviour, stated exactly:** locking your *session* does not by itself lock the vault,
and this app does not pretend otherwise — there is no portable way for a sandboxed client to be
notified of session lock on every compositor. What is guaranteed is stronger and local: the vault
key is released from memory by `dev-console vault lock` (or the tray's **Lock**) and the manual lock
persists across restarts until polkit re-authenticates you. Add the hook above and the two lock
states move together.

## Arch Linux install

`packaging/arch/PKGBUILD` builds the release binary plus the frontend and installs the launcher,
polkit policy, helper and icons. Install it with:

```bash
makepkg -si                     # in packaging/arch (builds from the tagged source)
```

For AUR use the same PKGBUILD with `source=("$pkgname-$pkgver.tar.gz::https://github.com/parththirwani/dev-console/archive/refs/tags/v$pkgver.tar.gz")`.
On other distributions `sudo PREFIX=/usr/local bash packaging/install.sh` installs the same
file set (and writes an uninstall manifest, so `sudo PREFIX=/usr/local bash packaging/install.sh uninstall` removes
exactly what it placed). See `docs/packaging/packaging.md`.

## Layout

```
src/                 Svelte 5 frontend (tokens.css is the only source of colour/spacing)
  components/        Button, Badge, Dialog, EmptyState, EntryDetails, EntryEditor, EntryList,
                     ExtraFields, Icon, Keycap, ProjectImport, SettingsPanel, StatusLine, Tabs,
                     VaultStatus
  windows/           Library.svelte, Finder.svelte
crates/              Cargo workspace: the domain layer and the native Rune port
  dev-console-core/  Rust domain: models, paths, repository (SQLite), vault (age), keyring,
                     auth (polkit), backup, recovery, clipboard, context, environment, execution,
                     history, import, project_history, projects (task scan), search, settings,
                     state (the `AppState` service container), services (the two typed surfaces a
                     UI calls - `LibraryActions` / `FinderActions` - and the `Effects` they return),
                     freshness (the ledger a background answer is checked against before a window
                     may paint it) and testing (the scratch-profile fixture the workspace tests
                     share)
  dev-console-desktop/  Rune app windows, tray, control socket, CLI (not the shipped app yet):
                        app/identity (window specs, titles, sizes, exit codes), off_thread (a
                        blocking call on a Rune worker, its answer guarded by core's `freshness`),
                        instance (single-instance lock + control socket), tray, ui (the state the
                        views draw - still placeholders until plan milestone 4)
  dev-console-ui-tests/ scripted element-tree and state-transition tests for the Rune views
src-tauri/src/       Tauri desktop runtime around the core crate: lib (app setup), windows
                     (show/hide/focus, launch intent, relaunch), tray (native menu), ipc (one thin
                     adapter per command: guard the window, call the service, dispatch the effects),
                     effects (the only place an event is emitted or a window action performed),
                     control (Unix socket + CLI replies), cli (intents), autostart, desktop_icons
src-tauri/tests/     native backend integration tests (no test doubles)
scripts/             verification harnesses (each exits 77 when the environment is missing)
packaging/           desktop entry, polkit policy + helper, install script, arch PKGBUILD
docs/                every other Markdown file, in subfolders (see docs/README.md)
artifacts/ui-review/ before/after screenshot grid
```

## Status

`docs/project/TEST_REPORT.md` is the authoritative verdict per acceptance criterion: what is proven by an
executed test, what is only UI-shell covered, and what remains unverified in a headless container.
Items are never marked passed without a run behind them.

## README screenshots

The product README uses unchanged copies of UI review captures generated by
`tests/e2e/screenshots.spec.ts` with synthetic demo data and masked secret values.
These show the app UI through the browser test double, not native Wayland validation.
Keep the original review records intact when refreshing the product images.

| Product asset | Source under `artifacts/ui-review/raycast-after/` |
| --- | --- |
| `docs/assets/screenshots/library.png` | `01-library-commands__1440.png` |
| `docs/assets/screenshots/finder.png` | `18-finder-project-context__680.png` |
| `docs/assets/screenshots/project-import.png` | `20-project-import-dark__1440.png` |
