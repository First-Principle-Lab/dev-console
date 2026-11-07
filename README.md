# Dev Console

A local-first desktop tray app for the two things you copy all day: **commands** and **secrets**.
Built with Tauri 2 (Rust backend, Svelte 5 frontend) for Linux desktops — X11 and Wayland, tested
against Hyprland.

Everything stays on your machine: commands live in a local SQLite database, secrets live in an
age-encrypted vault whose key never leaves the OS Secret Service (gnome-keyring / KWallet), and
there is no account, no sync, no telemetry and no cloud storage anywhere in the product.

## What it does

**Library window** — two tabs (Commands, Secrets) with counts, a filterable list pane (pinned
first, type/tag chips, usage and "updated" info) and an inspector pane for the selected entry:
copy, reveal, edit, pin, run and delete, with confirmations for destructive actions.

**Finder** — a 680×420 command palette on a global shortcut (`Super+F` by default). One field
searches commands and unlocked secrets together; first result is preselected, `Tab` completes the
name inline, `↑`/`↓` move the selection, `Enter` copies the value, `Ctrl+O` opens the entry in the
library, `Esc` dismisses. Everything is reachable from the keyboard, and the list keeps proper
combobox/listbox semantics.

**Project context** — when the Finder opens, Dev Console asks the compositor which window had
focus (before the palette takes it), follows that window to the shell it started, and reads the
folder that shell is sitting in. Commands saved with a working directory inside that project — or
tagged `project:<name>` — are listed first and marked with a folder icon, and the palette names the
project it is ranking by. `Ctrl+P`, or clicking that name, ignores it for this open; Settings →
Finder turns it off entirely, and then neither the compositor nor `/proc` is read. Nothing is
written to disk. On a desktop other than Hyprland (where focus is unreadable without a compositor
API) the palette is exactly what it has always been: no chip, no marks, the old ordering.

Which shell counts: when one terminal has several tabs or windows, the one whose folder the window
title shows, else the newest. Any focused app with a shell under it counts, so an editor's built-in
terminal does too. Under tmux, the folder is the one tmux was started from, not the pane's.

**Vault** — secrets are masked everywhere by default. A manual lock is persistent: it survives
restarting the app, and *only* a verified system authentication (polkit) can clear it. An unlock
never clears the lock unless the vault actually decrypted.

**Clipboard** — values are copied as ordinary text (so you can paste them anywhere), the app
claims ownership of the selection and clears it after the expiry window you configure (default
30s). It reports honestly when history exclusion is unavailable instead of claiming a protection
it cannot deliver.

**Execution** — after explicit approval, ordinary commands run with `/bin/sh` and show stdout,
stderr and exit status inside the library. Commands containing `sudo` open the configured terminal
for authentication. In-app output is limited to 64 KiB per stream, and commands time out after
120 seconds. The approval is invalidated when the command text changes. Settings lists supported
terminals found in `PATH` and keeps a custom command option. A saved `x-terminal-emulator` setting
falls back to an installed terminal when that executable is missing.

**Suggestions from shell history** — the Commands tab offers the most-used commands from your
zsh, bash or fish history as starting points for a new entry, ranked by count weighted by recency.
The file is parsed in Rust on request (when the library opens, on **Rescan**), never watched, never
written to, and never opened at all while the setting is off (Settings → General). Lines that look
like they carry a credential are dropped before anything reaches the screen: an exported
`*_TOKEN`, `-p<password>` or `sshpass -p …`/`docker login -p …`, `--password`/`--token`/`--api-key`
and their kin, `Authorization:` and other credential headers (`X-Api-Key:`, `Cookie:`), a
`user:pass@host` URL or a token as the user part of an `https://` URL, a setting such as
`aws_secret_access_key …`, and a long hex or base64 blob (except a full commit id where git expects
a revision). **Save** only prefills the normal editor; nothing is written until you confirm. A
**Dismissal** is stored as the SHA-256 of the command, never the command itself, and Settings →
General can bring every dismissed suggestion back. Each suggestion is a keyboard stop as well:
`Tab` to the command, `Enter` saves it, `Delete` dismisses it. When several history files exist,
the one written most recently counts as the most recent.

**Project tasks** — the library can read the runnable tasks a project folder already defines instead
of making you retype them: `package.json` scripts (run through whichever package manager the
folder's lockfile names — bun, pnpm, yarn or npm), Makefile targets, `justfile` recipes and Docker
Compose services. Scanning lists them grouped by source, ticks the new ones, and marks anything the
library already has as "Already saved". Importing writes the ticked ones as ordinary commands whose
working directory is that folder and whose tag is `project:<repo>`. Reading and importing execute
nothing in the folder — the commands are only written down, and running one still goes through the
approval above. The folder field accepts absolute paths and `~/` paths. Makefile discovery
uses GNU Make's file precedence and ignores `define` blocks. Compose files are validated as YAML.
The optional read-only `just --dump` process has a three-second deadline covering output and exit.
If selected tasks disappear before import, the review reports them alongside the saved tasks.

**Backups** — sealed age archives of the command table plus the vault, rotated to the newest eight
in `~/.local/share/dev-console/backups`, plus an explicit encrypted export/import. A wrong key, a
truncated file, a corrupted file or a newer format is rejected before anything is applied.

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
src-tauri/src/       Rust: repository (SQLite), vault (age), keyring, auth (polkit), backup,
                     clipboard, control (Unix socket), execution, ipc, projects (task scan), search,
                     settings, tray
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
