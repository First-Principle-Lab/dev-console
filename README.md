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

**Vault** — secrets are masked everywhere by default. A manual lock is persistent: it survives
restarting the app, and *only* a verified system authentication (polkit) can clear it. An unlock
never clears the lock unless the vault actually decrypted.

**Clipboard** — values are copied as ordinary text (so you can paste them anywhere), the app
claims ownership of the selection and clears it after the expiry window you configure (default
30s). It reports honestly when history exclusion is unavailable instead of claiming a protection
it cannot deliver.

**Execution** — commands run in your configured terminal only after an explicit approval, and the
approval is invalidated when the command text changes.

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

That is the whole gate: Bun install, `svelte-check`, ESLint, unit tests, production build,
Playwright (library + Finder + redesign + visual review captures), `cargo fmt --check`, clippy with
warnings denied, the Rust test suite, plus the environment-dependent harnesses:

| Script | What it proves | Without the environment |
| --- | --- | --- |
| `scripts/test-keyring.sh` | the real Secret Service round trip through the app's own keyring adapter | exit 77 = UNVERIFIED |
| `scripts/test-native-ipc.sh` | real SQLite + real age vault + real provider + real X11 selection, across a restart | exit 77 = UNVERIFIED |
| `scripts/test-clipboard.sh` | real X11 selection ownership, expiry retraction, exclusion reporting | exit 77 = UNVERIFIED |
| `scripts/test-packaging.sh` | staged install, desktop file, polkit policy, exact uninstall | exit 77 = UNVERIFIED |
| `scripts/test-arch-package.sh` | Arch package layout from `packaging/arch/PKGBUILD` | exit 77 = UNVERIFIED |
| `scripts/smoke-native.sh <binary>` | the compiled app: control socket, single instance, lock survives restart, private files | exit 77 = UNVERIFIED |

Each harness exits **77** when its environment is missing; that is reported as UNVERIFIED and never
silently converted into a pass. `TEST_REPORT.md` records which acceptance criteria are proven, where,
and what is still open.

## Visual review

`artifacts/ui-review/before/` and `artifacts/ui-review/after/` hold the review grid — eleven UI
states captured at 1440, 768 and 390 px (`SHOT_SET=before|after bun run test:e2e -- screenshots`).
These are **UI-shell** checks: the browser preview cannot reach native IPC, so they run against the
documented IPC test double; the native binary is exercised separately by `scripts/smoke-native.sh`.

## Keyboard

| Where | Keys |
| --- | --- |
| Desktop | `Super+F` Finder (configured in your compositor, see below) |
| Library | `Ctrl+1`/`Ctrl+2` tabs, `Ctrl+N` new, `Ctrl+S` save, `Esc` dismiss dialogs |
| Finder | `↑`/`↓` select, `Tab` complete, `Enter` copy, `Ctrl+O` open in library, `Esc` close |

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
file set (and writes an uninstall manifest, so `bash packaging/install.sh --uninstall` removes
exactly what it placed). See `docs/packaging.md`.

## Layout

```
src/                 Svelte 5 frontend (tokens.css is the only source of colour/spacing)
  components/        Button, Badge, Dialog, EmptyState, EntryDetails, EntryEditor, EntryList,
                     ExtraFields, Icon, Keycap, SettingsPanel, StatusLine, Tabs, VaultStatus
  windows/           Library.svelte, Finder.svelte
src-tauri/src/       Rust: repository (SQLite), vault (age), keyring, auth (polkit), backup,
                     clipboard, control (Unix socket), execution, ipc, search, settings, tray
src-tauri/tests/     native backend integration tests (no test doubles)
scripts/             verification harnesses (each exits 77 when the environment is missing)
packaging/           desktop entry, polkit policy + helper, install script, arch PKGBUILD
docs/                packaging and desktop-integration notes
artifacts/ui-review/ before/after screenshot grid
```

## Status

`TEST_REPORT.md` is the authoritative verdict per acceptance criterion: what is proven by an
executed test, what is only UI-shell covered, and what remains unverified in a headless container.
Items are never marked passed without a run behind them.
