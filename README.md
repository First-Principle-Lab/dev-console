# Dev Console

A local desktop console for saved commands and secrets. It runs in the system tray, provides a GUI for managing entries, and opens a combined search finder with Super+F.

The first release targets Arch Linux with Hyprland. The planned stack is Tauri 2, Svelte, TypeScript, and a small Rust backend.

## Project status

Planning stage. This repository currently contains the product plan and implementation specification. The application, installer, keybinding, and CLI commands have not been implemented yet.

## Planned interfaces

### Library GUI

Two tabs, Commands and Secrets, support browsing, viewing, adding, editing, deleting, copying, and pinning entries. Each entry has a name, type, tags, and optional description, notes, and custom fields such as project, URL, environment, or account.

Closing the library keeps the application running in the tray. The tray menu opens the library or finder, adds entries, locks or unlocks the vault, opens settings, and quits the app. Start at login is optional.

### Finder

Super+F opens a compact search window from any workspace. Search commands and unlocked secrets together by name, type, tags, and description. Results show entry metadata; secret values stay masked.

| Key | Planned behavior |
| --- | --- |
| Super+F | Open or hide the finder |
| Up / Down | Select a result and scroll it into view |
| Tab | Complete the selected entry's name |
| Enter | Copy the selected command or secret |
| Ctrl+O | Open the selected entry in the library |
| Escape | Close the finder |

The first matching result is selected automatically. Command execution is a separate Run action in the library; searching or completing a name never executes it.

## Storage and vault behavior

Commands use local SQLite storage. Secrets and their metadata use an encrypted local vault. The system keyring holds the vault credential for automatic unlocking during normal desktop use.

Manual locking hides secret entries and blocks access until an explicit authenticated unlock. The manual lock state must survive app restarts. Commands remain searchable while the vault is locked.

The planned CLI is:

```sh
dev-console vault lock
dev-console vault unlock
dev-console vault status
```

These commands are not available yet. The encrypted storage library, system authentication after manual lock, clipboard-history exclusion, and session lock integration must be verified before storing real secrets.

No account, cloud sync, or external search service is required. Secret values must stay out of logs, notifications, search previews, and plaintext exports. Backups remain encrypted and include a separate recovery strategy for a lost keyring.

## Architecture

One resident Tauri app owns a system tray, a library window, and a finder window. Both windows call the same Rust backend for persistence, search, vault state, and clipboard actions. Metadata-only events keep their results synchronized after edits.

Hyprland launches the finder through `dev-console --finder`. Repeated launches reuse the running application. Super+F currently controls fullscreen in the target setup, so installing the finder binding will replace that shortcut.

## Documentation

- [Project plan](PLAN.md): product behavior, security requirements, delivery order, and acceptance checks.
- [Implementation specification](IMPLEMENTATION.md): screen layouts, window lifecycle, data models, backend commands, source structure, and desktop integration.
- [Cloud agent prompt](CLOUD_AGENT_PROMPT.md): unattended implementation instructions, checkpoint commits, testing, native compilation, CI artifacts, and handoff requirements.

## Delivery order

1. Build the tray lifecycle, library GUI, command storage, and finder.
2. Add keyring integration, encrypted secret storage, and authenticated locking.
3. Add clipboard handling, separate command execution, backups, and settings.
4. Package the app and verify Super+F, the Waybar tray, focus, startup time, and memory use on Arch Linux.

Development and installation instructions will be added when a runnable application exists. Do not commit actual credentials, vault data, or local command databases to this repository.
