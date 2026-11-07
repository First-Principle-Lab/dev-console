# Dev Console app plan

Status: behavior agreed; implementation has not started. No application or desktop configuration has been changed.

## Product

A lightweight local desktop app built with Tauri 2 and TypeScript. The first release targets Arch Linux and Hyprland. Keep the core app portable so Windows and macOS support can follow without depending on Hyprland.

The app runs in the system tray and has two GUI windows: a full library for storing and managing entries, and a compact finder opened by Super+F. Both use the same backend and saved entries. No account, cloud sync, or external search service is required. The concrete screen design, data contracts, and implementation sequence are specified in [IMPLEMENTATION.md](IMPLEMENTATION.md).

## Tray and window lifecycle

- Start hidden in the tray at login after the user enables Start at login. A normal application-menu launch opens the library.
- Provide tray menu actions Open library, Open finder, Add command, Add secret, Lock vault or Unlock vault, Settings, and Quit. Add secret requires unlocking if the vault is locked.
- Closing the library hides it and keeps the tray app running. Unsaved edits require Save, Discard, or Cancel before hiding. Escape hides the finder.
- Quit is the only normal action that exits the background app. Check unsaved edits, flush completed writes, clear app-owned secret clipboard contents, and release decrypted vault state before exiting.
- Use one running Tauri application and two named windows, `library` and `finder`. Reopening either focuses its existing window.
- The user's Waybar already has a tray module. On Linux, rely on the tray context menu because Tauri does not emit raw tray mouse events there. Do not depend on a left-click or double-click handler to open the library.

## Finder

- Super+F opens a centered floating finder on the active monitor and focuses its search input from any workspace.
- Launch the app if it is closed. If it is already running, show the existing finder instead of starting another instance.
- Super+F toggles the finder. Escape dismisses it.
- Search commands and unlocked secrets together by default. Show each result's name, kind, type, and a short description. Never put secret values in result previews or autocomplete.
- With an empty query, suggest pinned and recently used entries. With a query, rank exact names before name prefixes, token matches, and fuzzy matches. Types and tags also contribute to matching. Usage breaks ties rather than outranking a clearly better match.
- Select the first result automatically. Up and Down move selection and scroll it into view.
- Show a muted inline completion when the selected name extends the typed prefix. Tab accepts that completion; for a fuzzy match, Tab replaces the query with the selected entry's full name. Tab never runs or copies an entry.
- Enter copies the selected command or secret value to the clipboard and dismisses the finder. Command execution is a separate Run action. Secret copy requires an unlocked vault.
- Provide an explicit details action and a visible way to open the library. Reserve ordinary Tab navigation for library forms; autocomplete owns Tab only while the finder input has focus.
- Support ordinary name/type/tag queries and optional filters such as `type:api` and `tag:work`.
- When the vault is locked, hide secret names, types, and other metadata. Keep command search usable and offer an Unlock vault result. During normal use, the system keyring unlocks the vault automatically so combined search includes secrets.

## Library

Keep two tabs, Commands and Secrets. Each supports search, browse, add, inspect, edit, delete, copy, pin, and custom types/tags. Use an entry list beside a details/editor panel. Select a row to inspect it; Edit enables the form, Save commits, and Cancel restores the saved entry. Confirm permanent deletion by entry name.

Shared entry fields:

| Field | Purpose |
| --- | --- |
| ID | Stable internal identifier |
| Name | Human-readable searchable name |
| Kind | Command or secret |
| Type | Editable classification such as Git, Docker, API key, or password |
| Tags | Additional searchable labels |
| Description | Optional context |
| Notes | Optional multiline instructions or reference information |
| Custom fields | Ordered label/value pairs, such as project, URL, environment, or account |
| Pinned | Whether to suggest the entry on an empty query |
| Usage | Local timestamps and counters for ranking |

Each custom field has a stable field ID, label, value, and display kind such as text, URL, or multiline text. Users can add, reorder, and remove fields. All extra information belonging to secrets stays encrypted. Search defaults to names, types, tags, and descriptions; full notes and custom-field values are not silently indexed.

A command also has command text and an optional working directory. A secret has a masked value. Permit duplicate names when their type or description distinguishes them.

Provide copy actions for both kinds. A separate explicit command action can run a saved command in a terminal. Show the full command and working directory before an initial execution. Editing a command resets that initial confirmation. Never execute commands during search or completion.

Secret reveal requires an unlocked vault and a deliberate action. Start with individual text secrets; structured credentials, file secrets, and command placeholders are later extensions.

## Storage and security

- Store commands locally in SQLite through the Rust backend.
- Use an established encrypted vault implementation for secrets; evaluate Tauri Stronghold before choosing the storage API. Do not design custom cryptography.
- Encrypt secret values, names, types, tags, descriptions, notes, custom fields, and usage metadata. Search decrypted metadata in memory only while unlocked. Do not keep a plaintext secret metadata index.
- Generate a random vault credential and store it in the system keyring. Read it through the native backend using the Linux Secret Service API. No separate app master password or inactivity timeout is required for ordinary use.
- Automatically open the vault when the desktop session and keyring are unlocked, unless the user has manually locked the vault. An unlocked desktop alone does not prove the keyring is unlocked. If the keyring is unavailable or locked, show a native unlock/setup path and keep commands usable. Never silently fall back to plaintext storage.
- Provide `dev-console vault lock`, `dev-console vault unlock`, and `dev-console vault status`, plus a Lock action in the app. These are planned app commands, not existing tools.
- Manual lock clears decrypted data references and secret results, blocks backend secret actions, cancels pending secret copies, and clears any clipboard value still owned by the app. Persist the manual lock flag so finder invocation, app restart, and login cannot silently undo it. Only an explicit authenticated unlock clears the flag.
- Unlock after a manual lock must require system authentication. Reading a credential from an already unlocked keyring is not authentication. Evaluate a supported OS authentication mechanism or a dedicated locked keyring collection before implementation; do not lock the shared login collection and disrupt other apps. If no supported verification mechanism is available, report that limitation instead of presenting an immediately reversible hide action as a secure lock.
- Follow desktop session lock and suspend with vault locking where OS integration permits it. After the user returns and authenticates to the desktop, automatically restore ordinary vault access unless the persistent manual lock flag is set. Verify these hooks on Hyprland.
- Copy secret values from the native backend. Clear the clipboard after a short timeout only if it still contains the copied value, so newer clipboard contents are preserved.
- Check the actual clipboard managers on this machine and implement and verify their exclusion mechanism. A clipboard timeout alone does not erase clipboard history. Document any limitation if an active manager cannot exclude secrets.
- Keep secret values out of logs, errors, notifications, command arguments, and exports. Minimize how long revealed values remain in the webview; JavaScript garbage collection cannot guarantee memory erasure.
- Include encrypted vault backup and restore with a user-held recovery credential or password-protected export. A vault copy alone is insufficient if its credential is lost with the machine's keyring. Never include the unprotected vault credential in a backup. Command import/export can use ordinary JSON; secret exports remain encrypted.

## Implementation

- Tauri 2 shell with Svelte and TypeScript for the interface, built with Vite.
- Create the system tray in Rust with `TrayIconBuilder`, enable Tauri's `tray-icon` feature, and dispatch menu actions through the same window/vault services used by the CLI.
- Give library and finder separate routes, capabilities, and frontend state. Keep persistence and vault state in one Rust `AppState`. Publish metadata-only change events so both windows refresh after edits.
- A small Rust backend owns persistence, vault state, native clipboard actions, and explicit terminal execution.
- Use Tauri's single-instance plugin to route repeated launches and CLI vault operations to the running backend. CLI operations return success only after the requested action completes; `vault lock` must also persist its flag without automatically unlocking when the app is not running.
- Use Hyprland's native keybinding to launch `dev-console --finder`. This avoids relying on an application-level global shortcut implementation on Wayland.
- Install the desktop entry and executable, optionally start the app hidden at login, and add the shortcut in the user's Hyprland configuration.
- Super+F currently means fullscreen. Add an `unbind` before the finder binding, back up the user configuration, and validate with `hyprctl reload` and `hyprctl configerrors` when implementing the change.
- Consult the current official Hyprland window-rule documentation before adding floating, sizing, or placement rules. Leave Omarchy-managed source files untouched.
- Use a local-only webview with narrow Tauri capabilities and no remote content. Restrict finder IPC to the actions it needs.

## Delivery order

1. Build the tray lifecycle, both GUI windows, finder keyboard behavior, and command library with optional notes/custom fields and local persistence.
2. Add encrypted secret storage, keyring integration, authenticated manual unlock, and combined metadata search.
3. Add native clipboard handling, backups, and separate command execution.
4. Package the app, integrate Super+F, and test focus and monitor behavior on this desktop.

## Acceptance checks

- Super+F opens and focuses the finder from another app or workspace; repeated presses never create duplicate instances.
- Tray menu opens both windows, closing the library leaves the app running, Quit exits, and start-at-login launches hidden. Verify this in the actual Waybar tray, including its expander.
- Add, view, edit, delete, and copy work in both tabs. Optional notes and custom fields survive restart; cancelled edits do not change saved data. Finder results refresh after a library change.
- Names, types, and tags find the expected entries; ties remain stable; Up/Down and Tab work with empty, prefix, fuzzy, and no-result searches.
- Tab completes only a name. Enter performs exactly the selected action.
- Saved entries survive app restarts. Failed writes and invalid imports do not destroy existing data.
- Locked or unavailable keyrings cannot open the vault. Lock removes secret results and blocks secret actions in the backend, including actions already queued by the interface.
- Verify that manual lock survives finder invocation, process restart, and login; only an explicit successful system-authenticated unlock clears it. Failed or cancelled authentication leaves the vault locked. Confirm that unrelated keyring users remain unaffected.
- Inspect saved files and diagnostic output for plaintext secret values and, with the default metadata policy, plaintext secret names.
- Verify clipboard timeout, clipboard-history exclusion, explicit execution, vault backup/restore, and session lock/suspend behavior.
- Measure release startup time, finder response latency, and idle memory on this machine before claiming that the app is lightweight.

## References

- Tauri single instance: https://v2.tauri.app/plugin/single-instance/
- Tauri Stronghold: https://v2.tauri.app/plugin/stronghold/
- Tauri architecture: https://v2.tauri.app/concept/architecture/
- Linux Secret Service interface: https://gnome.pages.gitlab.gnome.org/libsecret/class.Service.html
- Tauri system tray, including Linux mouse-event limitations: https://v2.tauri.app/learn/system-tray/
- Tauri window customization: https://v2.tauri.app/learn/window-customization/

## Agreed choices and environment checks

- Enter copies commands; Run is separate.
- Use the system keyring for normal automatic unlocking. Keep the vault available during ordinary unlocked desktop use.
- Provide an explicit manual vault lock command for leaving the machine. Hide secrets while locked and prevent automatic reopening until authenticated manual unlock.
- GNOME Keyring, KWallet, and libsecret are installed on this machine. SDDM has keyring session startup configuration. The inspection sandbox cannot access the user's D-Bus service, so the active Secret Service provider and login unlock behavior still need verification on the desktop.
- Before vault implementation, verify the selected encrypted storage library can use the keyring-held random credential and identify a supported mechanism for system authentication after manual lock.
