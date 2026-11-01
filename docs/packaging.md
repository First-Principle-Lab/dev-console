# Packaging and desktop integration

Two installation paths exist, and both install the same five things: the binary, the launcher, the
polkit policy, the helper the policy authorises, and the icons.

## Arch Linux

`packaging/arch/PKGBUILD` builds from a release tarball with `bun install --frozen-lockfile`,
`bun run build:frontend` and `cargo build --release --locked --features tauri/custom-protocol`
(the feature flag is required: without it Tauri compiles in dev mode and the binary needs the
Vite dev server), then installs:

| Path | From |
| --- | --- |
| `/usr/bin/dev-console` | `src-tauri/target/release/dev-console` |
| `/usr/share/applications/dev-console.desktop` | `packaging/dev-console.desktop` |
| `/usr/share/polkit-1/actions/ai.codemate.devconsole.policy` | `packaging/polkit/…` |
| `/usr/lib/dev-console/dev-console-auth-helper` | `packaging/dev-console-auth-helper` |
| `/usr/share/icons/hicolor/{32x32,128x128,256x256,512x512}/apps/dev-console.png` | `src-tauri/icons/` |

```bash
cd packaging/arch
makepkg -si
```

Publishing to AUR means editing only `source=` (and setting the real `sha256sums`):

```bash
source=("$pkgname-$pkgver.tar.gz::https://github.com/parththirwani/dev-console/archive/refs/tags/v$pkgver.tar.gz")
```

The policy's `exec.path` and the launcher's `Exec=` are rewritten to this prefix at package time,
because polkit policies resolve helper paths absolutely and ignore `PATH`.

`scripts/test-arch-package.sh` validates the PKGBUILD (required variables, that every `install -D`
destination is absolute under `$pkgdir`, that the desktop entry and policy are covered, and that
the policy XML parses after the prefix rewrite). With `makepkg` available it also runs a staged
`makepkg --repackage -d` inside a scratch directory; without it the script exits **77** and reports
UNVERIFIED rather than pretending the package was built.

## Other distributions (Debian/Ubuntu and friends)

```bash
bunx tauri build --no-bundle
sudo PREFIX=/usr/local bash packaging/install.sh install    # stages, validates, installs
sudo PREFIX=/usr/local bash packaging/install.sh uninstall  # removes exactly what it placed
```

`packaging/install.sh` writes a manifest (`/usr/local/share/dev-console/installed-files`) so
uninstall never guesses, and it refuses to run without one. (`install` and `uninstall` are
positional arguments - `--uninstall` is not accepted and exits with a usage error.)

## Launcher entry

`packaging/dev-console.desktop` starts the app hidden in the tray (`--background`), sets
`X-GNOME-Autostart-enabled`-friendly metadata, and is validated by `desktop-file-validate` in
`scripts/test-packaging.sh`.

## Global shortcut (Hyprland and other wlroots compositors)

Tauri's global-shortcut support does not cover wlroots compositors, so the binding belongs to the
compositor:

```ini
# hyprland.conf
bind = SUPER, F, exec, dev-console --finder
```

The second (and every later) invocation connects to the running instance over
`$XDG_RUNTIME_DIR/dev-console/control.sock` — the socket is created 0600 inside a 0700 directory —
asks it to raise the Finder, and exits. If no instance is running, the CLI fails closed with exit
code 30 for operations that require one instead of silently decrypting somewhere else.

CLI exit codes are stable and scriptable: `0` ok, `1` error, `10` locked, `20` unavailable,
`30` no running instance.

## Session lock

Locking the *session* does not lock the *vault* by itself, and Dev Console does not claim it does:
there is no portable notification a sandboxed client can rely on across compositors. What the app
guarantees locally:

- **Lock** (tray, library, or `dev-console vault lock`) drops the decrypted vault from memory and
  writes a persistent manual-lock flag.
- The manual lock survives restarts and Finder opens. Nothing but a verified polkit
  authentication clears it — and the flag is removed **only after the vault has actually decrypted
  successfully**, so a failed unlock leaves the lock exactly where it was (covered by
  `vault::tests::a_failed_unlock_keeps_the_manual_lock_and_the_flag_file`).

To move both states together, put the lock command in your **locker's own** pre-hook (not in
`hyprland.conf`):

```ini
# hyprlock.cfg  (hyprlock >= 0.6)
[GENERAL]
before_lock_cmd = dev-console vault lock
```

with `swaylock` it is a command hook in its config or wrapper:

```ini
# swaylock-style wrapper script bound to your lock key
dev-console vault lock
exec swaylock
```

`dev-console vault status` (exit 10 means locked) lets you verify from a terminal that the hook
actually fired. This pairing is **UNVERIFIED on real hardware**: the vault-side guarantees are
covered by tests (`vault::tests::a_failed_unlock_keeps_the_manual_lock_and_the_flag_file`,
`manual_lock_survives_a_restart_and_blocks_every_secret_operation`), but no Hyprland/hyprlock
session has run the hook yet, so do not treat screen lock and vault lock as proven to be paired.

## Desktop acceptance checklist (must be run on the owner's Arch/Hyprland machine)

Every item below is UNVERIFIED by design - this container has no Wayland compositor, no
Waybar, no polkit agent and no logind - and the release stays a draft until they pass:

1. `Super+F` (the `bind = SUPER, F, exec, dev-console --finder` line above) raises the Finder
   from anywhere, toggles it away, and never steals focus from a fullscreen window.
2. Tray menu under the status-notifier: Show Library, Show Finder, Lock, Quit all act on the
   resident instance.
3. `before_lock_cmd = dev-console vault lock` fires on screen lock: after locking and
   unlocking the screen, `dev-console vault status` reports `locked (manual)` (exit 10), and
   the Finder refuses secret search until unlock.
4. Unlock through the UI prompts polkit (the `ai.codemate.devconsole.unlock` action), a
   cancelled prompt keeps the vault locked, and unrelated keyring items are untouched.
5. Copy a secret, paste it into an app, wait past the configured expiry: the selection is
   retracted; confirm whether your clipboard history manager kept a copy regardless (the app
   reports exclusion as unavailable on X11 and cannot prevent it).
6. Login keyring: after a fresh graphical login (no manual start), opening the library
   auto-unlocks the vault without any prompt beyond the session's own.
7. `makepkg -si` from `packaging/arch` builds and installs; the desktop entry launches it;
   `dev-console vault lock` + relaunch still shows `locked (manual)`.

## polkit

`packaging/polkit/ai.codemate.devconsole.policy` declares action
`ai.codemate.devconsole.unlock` (`allow_any=no`, `allow_inactive=no`, active sessions must
authenticate, result retained for no more than 5 minutes) and points `exec.path` at the installed
helper. The application never asks polkit to run a privileged binary itself: the helper exists so
the authentication prompt is bound to a concrete, auditable action id. When no polkit agent is
present (a headless container, for instance) the app reports the unlock mechanism as unavailable
instead of falling back to "just open the vault".
