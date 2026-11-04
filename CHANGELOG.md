# Changelog

Every release of Dev Console, newest first. Each version matches a GitHub release tag
(`v0.0.N`) and the AppImage the in-app updater installs.

## Unreleased

### Added
- **Settings → Updates.** Shows the running version, has a **Check for updates** button,
  and offers **Update to X and restart** when a newer release exists. If a check fails, the full
  reason is shown there. The header's "Update check failed" button opens that section.
- This changelog.

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
