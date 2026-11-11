# Changelog

Every release of Dev Console, newest first. Each version matches a GitHub release tag
(`v0.0.N`) and the AppImage the in-app updater installs.

## Unreleased

### Added

- Port Library, Finder, entry editing, project import, environment versions, history suggestions, and Settings to the native Rune shell.
- Ship the native Rune shell as the released app. The AppImage is now built from it with Cargo and pinned AppImage tooling, and is about a tenth of the size. An installed AppImage updates to it through the existing in-app updater: the signing key, signature format, endpoint and the three release assets are unchanged.
- Add signed updates to the native shell. Settings shows **Update to x.y.z and restart** when a release is available. The update is verified before it replaces the installed AppImage, the previous image is kept beside it as `Dev-Console.AppImage.previous`, and a failed download or check leaves the installed app as it was.
- Give the native shell the full command line: `--background`, `--finder`, `vault lock|unlock|status`, `import bitwarden`, `import project` and `history setup`, with the same exit codes and the same control socket as before.
- Apply **Start at login** from the native shell. From an AppImage the login entry starts the installed image, and an entry left pointing at a previous AppImage mount is repaired at the next start.
- Add native scripted UI tests over disposable encrypted profiles and a live Hyprland check with synthetic entries, repeated Finder opens, hover, keyboard copy, tray Quit, and clipboard cleanup.
- Move through the Library list with Up, Down, Home, End and by typing a name. Ctrl+F focuses the filter, Ctrl+E edits the selected entry, and Escape leaves an editor nothing was typed in.
- Add `cargo run -p dev-console-ui-tests --example capture`, which renders any screen of the native shell to a PNG offscreen over a synthetic profile, for design review without mapping a window.

### Changed

- Build the Arch package, the source install and the `.deb` from the native shell. They no longer need Bun or WebKitGTK.
- Keep Suggested above saved commands, initially showing three compact rows with Show all. Selecting a suggestion previews its full command; Save opens a draft for review.
- Balance the native Library list and detail panes with a resizable divider and distinct surfaces. Give saved entries consistent name/context rows, a clear selection, and full text space when row actions are hidden.
- Compose entry details around the title, command/value, and nearby Copy and Run/Reveal actions. Keep Edit visible and move Pin and Delete into a dismissible More menu. Label Import and Environment beside the list controls and Settings beside vault status. Strengthen form labels and empty-state guidance.
- Redesign the staged Rune shell around one sidebar and one detail pane. The Library drops its header band, tab strip and toolbar for a segmented Commands/Secrets switch, a filter and a New button above the list; project tools sit beside the list controls, while vault status and Settings sit at the foot of the sidebar. Project groups start open.
- Cut the buttons. A list row shows a name and context: its copy and pin controls appear under the pointer or keyboard focus. The detail pane has one filled button (Copy) with Run or Reveal beside it; Edit and More sit in its header.
- Replace content at once instead of cross-fading it. Selecting an entry, switching tabs, folding a group, opening the editor and resizing the window across the narrow layout no longer animate; Finder results replace the list as they arrive.
- Show status as a toast over the bottom of the window that moves nothing and leaves on its own, instead of a banner that pushed the Library down and stayed. A copied command is confirmed in one sentence; a copied secret keeps the clipboard caveat.
- Lay Settings out as grouped rows with switches, shorten its explanations, and list the keyboard shortcuts. Dialogs lose their boxed sections and say what they do in a sentence or two.
- Use whole-point font sizes, sentence-case labels and one large title. Paths read from `~` and keep their last two folders; the full path is the tooltip.
- Trim the Finder: the vault badge shows only when the vault is locked, the selected row loses its outline, and the footer names Tab, Ctrl+O and Enter instead of five bindings.

### Fixed

- Close More when the vault locks so keyboard shortcuts keep working. Clear the selected suggestion when history suggestions are disabled so narrow windows return to the library list.
- Center both lines vertically within suggestion highlights so the top and bottom padding match.
- Paint native dialogs and the Settings sheet in the selected palette's shell surface instead of a raised surface, so a theme whose two shades differ (Kanagawa, Tokyo Night) no longer shows a dialog body in one colour and its footer in another. The native Settings theme list drops its `Omarchy: ` prefix and names palettes exactly as the shipped app does.
- Reject late secret and recovery reveals after closing, cancelling, locking, switching tabs, or reopening Settings. Reload the Secrets tab after unlock and secure its editor before the first frame.
- Keep modal footers and project actions reachable in narrow native windows, support Enter on history suggestions, and hide project import on the Secrets tab.
- Preserve unsaved edits when quitting the native shell, and clear decrypted state and owned clipboard content before exit.
- Closing the finder while Dev Console is quitting finishes the quit at once instead of waiting out the five-second grace period.

### Testing

- Run Cargo workspace formatting, Clippy, and tests in CI alongside the existing Tauri checks. Isolate the native test fixtures' XDG directories as well as their home directory.

## 0.0.12 — 2026-11-08

- Add a standalone installer for the latest signed AppImage, with a user command, desktop launcher, icon, and optional authenticated-unlock helper.

## 0.0.11 — 2026-11-08

- Refresh the product README with demo screenshots, installation steps, and keyboard shortcuts; move detailed development notes into the documentation.
- Document the planned Arch Linux migration from Tauri/Svelte to Rune.

## 0.0.10 — 2026-11-07

- Finder loads the same theme as Library and follows saved theme changes without restarting.

- Serialize concurrent environment scans and reject queued vault operations after locking.

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
