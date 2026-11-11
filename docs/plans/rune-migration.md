# Dev Console migration from Tauri to Rune

Status: milestones 1 to 5 are implemented. Releases are built from the native shell. Milestone 6, removing the Svelte frontend and the Tauri tree, is not done. A real update from an installed Tauri build to the first native release is still to be confirmed on the first release that ships it.

## 1. Target architecture and scope

Replace the Svelte frontend and Tauri desktop runtime with a native Rust application built with Rune. Preserve the current design, features, themes, keyboard behavior, and existing user data.

The target is **Arch Linux on X11 and Wayland**, with the owner's Hyprland/NVIDIA desktop as a required release check. macOS and Windows are excluded.

Develop this as one replacement effort on a feature branch from `dev`. Internal milestones establish working checkpoints, but production switches only after the entire replacement passes acceptance.

Rune renders Rust element trees through its own GPU renderer, layout engine, and text system. Existing HTML, CSS, and Svelte components must therefore be rewritten. Rune is pre-release and recommends pinning a commit. Initially pin both `rune` and `rune-kit` to the inspected revision, `250597b0a591881b5c0138370cbd5f405ed710db`. [Rune repository](https://github.com/anomalyco/rune)

Use a Cargo workspace with three crates:

| Crate | Responsibility |
| --- | --- |
| `dev-console-core` | Models, storage, vault, search, projects, history, environment snapshots, backups, validation, and application services. |
| `dev-console-desktop` | Rune UI, windows, tray, CLI, control socket, desktop integrations, and updater. Produces the existing `dev-console` executable. |
| `dev-console-ui-tests` | Synthetic fixtures, scripted interactions, rendering checks, and visual acceptance scenes. |

The final application contains no Tauri, Svelte, Vite, browser runtime, or JavaScript frontend.

## 2. Component changes and rendering

Build each screen as a Rust view function that produces Rune elements from explicit UI state. Use `rune-kit` controls where they match existing behavior, with app-specific styling and custom compositions where necessary. Rune provides text fields, selection controls, dialogs, lists, badges, and keyboard hint components. [Rune kit documentation](https://github.com/anomalyco/rune/blob/250597b0a591881b5c0138370cbd5f405ed710db/crates/rune-kit/README.md)

| Current component | Rune replacement and rendering |
| --- | --- |
| `App.svelte` | Desktop coordinator with Library and Finder window states. Dispatch CLI intents before starting the GUI. Remove URL hash routing. |
| `Library` | Resizable native window, initially 1040×700 with a 760×520 minimum. Render the current header, tabs, filter, list, inspector, and footer using rows, columns, and scroll containers. Preserve the existing compact layout behavior. |
| `Finder` | Separate opaque, borderless 680×420 native window. Render the search input, completion hint, project context chip, result rows, and keyboard footer. Preserve selection, scrolling, copy, and dismissal behavior. |
| `EntryList` | Keyed, scrollable rows with project headings, collapse state, pinned ordering, metadata, and selection markers. Keep backend ranking and grouping rules. |
| `EntryDetails` | Scrollable inspector with selectable command text, metadata, notes, fields, and action buttons. Render execution output in bounded, selectable monospace panels. |
| `EntryEditor` | Draft state bound to Rune text inputs, multiline editors, type selection, tag controls, and folder picker. Preserve validation, revision conflicts, execution approval, and unsaved-change prompts. |
| `ExtraFields` | Keyed rows for label, value, and display mode. Render text, multiline values, and URL actions according to the saved field type. |
| `ProjectImport` | Modal containing the folder input and picker, scan status, source groups, task checkboxes, duplicate markers, and import results. Scanning and importing continue to execute no project tasks. |
| `ProjectEnvironment` | Modal with project selection, sync toggle, snapshot lists, timestamps, versions, and copy actions. Close and clear its state when the vault locks. |
| `HistorySuggestions` | Collapsible project sections with command previews and save/dismiss actions. Saving fills the ordinary editor draft. |
| `SettingsPanel` | Scrollable modal retaining themes, autostart, clipboard expiry, terminal configuration, Finder context, history, backups, recovery, import/export, and updates. |
| `Dialog` | Shared modal composition with an opaque panel, backdrop, focus trap, Escape handling, initial focus, and focus restoration. Preserve confirmation and cancellation rules. |
| `VaultStatus` | Lock status, availability explanation, and lock/unlock action bound to shared vault state. |
| `Tabs` | Commands/Secrets controls with counts, selected state, and the existing Ctrl+1/Ctrl+2 shortcuts. |
| `StatusLine` | Shared success, warning, error, and busy presentation with accessible labels. |
| `Button` | Themed button wrapper preserving primary, ghost, danger, small, icon-only, disabled, hover, pressed, and focus states. |
| `Badge` | Text/icon chips with the existing command, secret, tag, and status colors. |
| `Keycap` | Styled keyboard hint elements using the app's existing labels and dimensions. |
| `EmptyState` | Centered icon, explanation, and contextual action inside the appropriate pane. |
| `Icon` | Embedded SVG assets matching the current Lucide set, rendered through Rune's SVG support. Preserve accessible labels on icon-only controls. |

### Design tokens

Convert the current CSS tokens and 22 Omarchy palettes into shared theme data consumed by Rust. Map colors, spacing, typography, borders, shadows, radii, and control dimensions to an app theme and Rune kit configuration.

Preserve the existing visual hierarchy rather than adopting Rune's default appearance. Compare rendered states against the existing reference captures, allowing renderer-specific text rasterization differences. Port the contrast checks to the new theme data.

## 3. Backend, state, and desktop integration

### Reuse the existing domain behavior

Move the existing Rust models, SQLite repository, age vault, keyring adapter, polkit authentication, search, imports, project scanners, history, environment snapshots, recovery, and backup logic into the core crate. Refactor dependencies on desktop runtime types without rewriting established storage or security behavior.

Keep unchanged:

- XDG data/config locations, filenames, database schemas, and encrypted archive formats.
- Keyring service `dev-console` and account `vault-key`.
- Persistent manual lock, authenticated unlock, and vault generation checks.
- Command approval rules, 120-second execution timeout, and output limits.
- CLI commands, shell hook protocol, control socket requests, and exit codes.

No user-data conversion is planned. An existing installation must open its current profile directly.

### Replace IPC with typed application services

Replace Tauri command handlers with typed service methods. Keep Rust models as the source of truth and port TypeScript-only drafts, validation, argument parsing, formatting, and Finder selection logic into Rust.

Expose separate `LibraryActions` and `FinderActions` interfaces. Finder receives search metadata, copy, context dismissal, and navigation operations. It receives no secret reveal, editing, execution, archive, or recovery operations.

Replace string-based frontend events with typed events for entries, vault state, Finder context, settings, clipboard expiry, window intents, and update progress.

Run blocking database, crypto, filesystem, authentication, scanning, and execution operations outside the UI thread. Deliver results through channels and Rune wakeups. Retain request IDs, revision checks, and vault/context generations so obsolete results cannot restore stale or sensitive state. Rune supports background wakeups and per-frame polling for this coordination. [Rune runtime documentation](https://github.com/anomalyco/rune/blob/250597b0a591881b5c0138370cbd5f405ed710db/crates/rune/README.md)

### Preserve desktop behavior

| Integration | Replacement |
| --- | --- |
| Windows | Rune window specifications and handles. Keep Library tiled and Finder floating. Closing hides the relevant window while the resident app continues running. |
| Finder presentation | Preserve `dev-console-finder`, application identity, opaque background, post-map size request, context capture before focus, and query reset on every appearance. |
| Single instance | Use an exclusive instance lock plus the existing private Unix control socket. Forward subsequent launch intents to the running process. |
| Tray | Rune's Linux StatusNotifierItem tray. Preserve Library, Finder, vault action, Settings, and Quit behavior. Absence of a tray host must not prevent operation. |
| Global shortcut | Retain the compositor binding invoking `dev-console --finder`. Do not require portal shortcut registration. |
| Clipboard | Retain the existing Linux clipboard service, expiry worker, ownership safeguards, and honest exclusion reporting. Route sensitive copies through it rather than Rune's ordinary text-copy helper. |
| File dialogs | Replace the Tauri dialog plugin with Rune native file/folder dialogs through the XDG portal. Cancellation leaves the draft unchanged. |
| Autostart and authentication | Retain XDG autostart, Secret Service, and the installed polkit helper/policy. |
| Quit/restart | Preserve unsaved-edit handling and bounded quit completion. Clear decrypted state and owned clipboard content before exit. Relaunch updates into a visible Library. |

Rune has Linux tray support and native file-dialog support, but these require their D-Bus-related features. Enable them explicitly. [Tray implementation](https://github.com/anomalyco/rune/blob/250597b0a591881b5c0138370cbd5f405ed710db/crates/rune/src/tray.rs), [Rune feature definitions](https://github.com/anomalyco/rune/blob/250597b0a591881b5c0138370cbd5f405ed710db/crates/rune/Cargo.toml)

Secret editors require an early implementation check. Masked values must not enter accessibility values, element dumps, debug formatting, screenshots, or retained undo history after dismissal or locking. If the pinned text control cannot enforce this, add a dedicated secure editor before migrating secret editing.

## 4. Implementation milestones and release changes

1. **Prove native feasibility.** Build a minimal two-window app with tray, background residency, control-socket wakeups, portal folder selection, keyboard input, and opaque Finder rendering. Test repeatedly on the real Hyprland/NVIDIA desktop. Verify activation, title/class, sizing, hover, dismissal, and surrounding tiled layout. Treat failures here as blockers. Any necessary Rune patch must use a pinned fork with a regression test.
2. **Extract the core and application services.** Move domain code and tests, remove Tauri types from services, and establish typed actions/events. Verify existing profiles and archives before building feature screens.
3. **Port themes and shared controls.** Implement the token mapping, icons, buttons, fields, dialogs, tabs, status presentation, focus rules, and secure editor.
4. **Port all feature screens.** Implement Library and Finder, followed by editing, execution, projects, history, environment snapshots, settings, backups, and recovery. Complete each screen with interaction and rendering coverage.
5. **Replace distribution and updates.** Build the native executable with Cargo. Package it using an explicit AppDir and pinned AppImage tooling. Retain Arch packaging, desktop assets, polkit installation, and the three release assets `Dev-Console.AppImage`, `Dev-Console.AppImage.sig`, and `latest.json`.
6. **Complete the cutover.** Remove the old frontend, Tauri configuration/plugins/build scripts, and obsolete browser tooling. Update documentation, the verification script, changelog, and repository agent instructions to describe the native test workflow.

The new updater must read the existing manifest format and verify artifacts with the existing public key and compatible signature format. Keep the current startup, focus, and 15-minute checks.

Download to staging beside the installed AppImage, verify before replacement, preserve executable permissions, replace atomically, and retain a rollback copy. Authentication, network, signature, or write failures leave the installed app intact. Arch packages remain managed by the package manager and do not overwrite themselves.

A real legacy-Tauri-to-Rune update is mandatory before release. Keep the existing signing identity and endpoint so installed clients can consume the replacement.

CI replaces frontend jobs with Rust checks, scripted Rune UI tests, visual captures, native integration tests, and packaging checks. Build-time version stamping remains automatic. Remove WebKit dependencies and the WebKit-specific environment default only when the Tauri runtime is removed.

Follow the existing `dev` to `main` promotion process. This is a large feature eligible for its own release batch. Confirm successful verification and publication, inspect all three release assets, then merge `main` back into `dev`.

## 5. Acceptance tests and defaults

- Port every existing browser behavior scenario to native Rust/Rune tests, including Library, Finder, projects, history, environment, settings, and updates. Replace Tauri mock-dispatch tests with service and window-action tests.
- Exercise keyboard navigation, completion, modal focus, editing, conflicts, destructive confirmations, command approval, timeout, and output truncation.
- Test lock/reveal/edit races and stale asynchronous responses. Use synthetic secrets exclusively in test output and captures.
- Capture all existing visual review states, minimum window sizes, dark/light appearances, and Omarchy themes. Check contrast, clipping, scrolling, selection, and focus visibility.
- Re-run Secret Service, clipboard, project context, shell hook, packaging, interrupted restore, and resident-process checks against the Rune executable.
- Test existing-data startup and rollback using copied synthetic profiles. Both binaries must understand the same persisted formats.
- Test successful legacy upgrades, subsequent Rune upgrades, corrupted signatures, interrupted downloads, insufficient permissions, and restart behavior.
- Perform native X11 and Wayland checks. On Hyprland/NVIDIA, repeat Finder opening, pointer movement, typing, selection, Escape, and Library transitions. Confirm Finder remains floating at 680×420 and Library remains tiled.
- Treat exit 77 as UNVERIFIED. Headless rendering does not satisfy the native Wayland release gate.

The migration is complete when all current features work in the Rune executable, existing installations retain their data and signed update path, and the production build no longer depends on Tauri or the web frontend.
