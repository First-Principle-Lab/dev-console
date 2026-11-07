# Changelog

Every release of Dev Console, newest first. Each version matches a GitHub release tag
(`v0.0.N`) and the AppImage the in-app updater installs.

## Unreleased

- Environment sync discovers project folders when entered in zsh, without adding them in Dev Console first.

- Project Environment sections keep encrypted, dated versions of local `.env` files, with automatic sync, pause/resume and copying of older versions.

## 0.0.9 — 2026-11-07

### Added

- Collapsible project groups for saved commands. Assign or move a command with the editor's project-folder picker.
- History recommendations for the five busiest recorded projects, with up to five commands each and separate counts per working folder.
- Local zsh tracking through **Enable project tracking for zsh**. Open a new terminal after setup. Existing history is not backfilled.
- Import selected package.json, Make, just and Docker Compose tasks through **Import from project** and its native folder picker.
- Finder ranks commands for the focused terminal's project. Its project chip and `Ctrl+P` let you ignore that context.
- A documentation index and organised `docs/` subfolders.

### Fixed

- Finder stays painted when hovered on Hyprland by using an opaque native surface.
- Settings closes after a successful save. Failed saves retain your changes and show the error.
- Project history uses a history icon and a quiet heading hover, with visible keyboard focus.
- Imported task names are quoted safely, and generated command names use dashes for spaces.
- Import dialogs keep keyboard focus and their confirmation buttons visible at small window sizes.
- Keyring verification runs its own provider test without requiring unrelated optional tools.

## 0.0.8 — 2026-11-06

### Added
- 22 Omarchy themes with immediate preview in Settings and an explicit Save action.
- A selector for installed terminals, including Ghostty and Kitty, with a custom command option.
- Command output and errors in the app, with separate copy buttons. Commands containing `sudo` open the selected terminal.
- Copy, pin, and delete buttons on each library row. All app icons now use Lucide.

### Fixed
- Escape closes confirmation, output, and unsaved-change dialogs even when focus moves outside them.
- URL fields open in the browser with a click target that fits the text; long URLs wrap, and hover changes only the pointer.
- A missing legacy `x-terminal-emulator` setting falls back to an installed terminal.
- Narrow library windows show the list without an empty detail pane.

## 0.0.5 — 2026-11-04

### Added
- **Settings → Updates.** Shows the running version, has a **Check for updates** button,
  and offers **Update to X and restart** when a newer release exists. If a check fails, the full
  reason is shown there. The header's "Update check failed" button opens that section.
- `CHANGELOG.md`, listing what changed in every version.

### Fixed
- **The finder opened as a large black window on Hyprland.** The compositor mapped the 680×420
  palette in a 950×1008 window, and the empty part was painted dark. The finder window is now
  transparent and resizes itself to the palette every time it opens.

## 0.0.4 — 2026-11-04

### Fixed
- **The finder stayed on "Searching…" and showed no commands.** Search results from the backend
  had no tags, so the finder failed while drawing the list.
- **The desktop launcher kept the old logo after an update.** The AppImage now refreshes its
  launcher icons in `~/.local/share/icons` on startup.

### Changed
- An empty finder search lists pinned entries first, then the most recently used, then the
  most used.
- Entry names can't contain spaces. Space copies the selected result in the finder, so a name
  like `wireguard off` couldn't be typed there. The editor warns as soon as a space is typed
  and suggests a dash (`wireguard-off`).
- Releases are built in parallel with the checks and reuse cached Rust builds, instead of
  compiling twice in a row.

## 0.0.3 — 2026-11-04

### Added
- Dev Console logo, and distinct icons for commands, collections and empty states.

## 0.0.2 — 2026-11-04

### Added
- Optional custom fields in the secret editor.

## 0.0.1 — 2026-11-04

First release: a tray app with a library window and a Super+F finder for saved commands and
encrypted secrets, shipped as a signed AppImage that updates itself.
