# Overnight implementation prompt

Implement Dev Console in https://github.com/parththirwani/dev-console. This is an unattended coding run. The owner is asleep; use reasonable engineering judgment, resolve routine choices yourself, and continue until the application and required checks are complete or an actual external blocker prevents further progress. Do not stop after writing a plan, scaffolding a UI, or getting only the frontend build to pass.

You have GitHub credentials for preserving progress. Use them only through the configured Git/GitHub tools; never print tokens, inspect unrelated credentials, or put credentials in source, URLs, logs, commits, or artifacts. Auto-approval does not authorize unrelated account changes or destructive system operations.

## Start and working branch

1. Clone or open the repository and inspect Git status, remotes, branches, and recent commits. Read applicable AGENTS.md files, README.md, PLAN.md, and IMPLEMENTATION.md before coding. The README and plans describe the agreed behavior; this prompt adds execution and verification requirements.
2. Read COMPLETION_CHECKLIST.md as the acceptance contract. Preserve existing changes and commits. Start from the latest remote main on a dedicated branch `codex/overnight-mvp`. If that branch already belongs to an active run, choose a distinct `codex/overnight-mvp-<suffix>` branch. Do not overwrite or reset someone else's branch.
3. Create PROGRESS.md with the branch, baseline commit, checkpoint checklist, decisions, verification commands, and environment limitations. Use UTC timestamps in the log to avoid guessing the owner's wake-up time. Do not edit this prompt just to relax acceptance criteria.
4. Confirm the available compiler, Node, Linux distribution, build dependencies, display server, session D-Bus, keyring provider, and CI permissions. Follow current official Tauri documentation when installing dependencies in the disposable cloud development environment. Do not modify the owner's desktop remotely.
5. Push an initial planning/progress checkpoint, then begin implementation immediately.

## Required finished product

Build a real native Tauri 2 desktop application with Svelte, TypeScript, Vite, and a small Rust backend. The first target is Arch Linux with Hyprland/Wayland and Waybar. Keep platform-specific code behind adapters, but do not spend the run implementing Windows/macOS releases. There must be an actual release desktop binary, not just a browser page or screenshots.

The product is one resident tray application with two separate windows backed by the same saved data:

### Library GUI

- Commands and Secrets tabs, with searchable entry lists and a details/editor panel.
- Add, inspect, edit, save, cancel, delete with confirmation, copy, pin, and browse entries.
- Each entry has an ID, revision, name, custom type, tags, description, notes, and ordered optional label/value custom fields. Custom field display kinds are text, multiline text, and validated HTTP/HTTPS URL. Allow duplicate entry names; distinguish by type/description and stable ID.
- Commands have command text and an optional working directory. Secrets have a masked value with deliberate reveal/edit. All metadata and custom information for secret entries belong in encrypted storage.
- Run is a separate command-only action. Enter in the finder copies; it never runs. Show the complete command and working directory before initial execution; invalidate that approval when executable content changes. Execute saved content with argument-safe terminal invocation. Do not execute sample commands during automated tests against the host.
- Preserve unsaved drafts through failed writes. Resolve unsaved changes on close/quit. Do not overwrite newer revisions with stale forms. Both windows update after saved edits through metadata-only events.

### Combined finder

- `dev-console --finder` toggles a compact finder, reusing the running app. No Walker or wofi integration.
- Provide a Hyprland Super+F installation path. Super+F previously controls fullscreen, so the configuration requires an unbind before the new binding. Deliver a reviewed, repeatable installer/config snippet; do not change the cloud host's real window-manager setup.
- Finder is floating, centered on the invoking monitor/workspace, focused immediately, and independent of the library window. Use current official Hyprland window-rule syntax. Verify runtime title/class values before writing matching rules. Account for Wayland limitations on application-controlled placement.
- Search commands and unlocked secrets together by name, type, tags, and description. Support `type:` and `tag:` filters. Never search or preview secret values.
- Suggest pinned/recent entries with an empty query. Rank exact name matches, prefixes, token matches, then fuzzy matches; usage should break ties instead of displacing clearly better matches.
- Select the first result. Up/Down changes selection and scrolls it into view. Tab accepts the selected name completion without copying or running anything. Enter copies the selected value through the backend and hides only after successful copy. Ctrl+O opens the corresponding library entry; Escape hides and clears finder state.
- Correctly handle no matches, duplicate names, asynchronous response ordering, vault locking during search, IME composition, and scrolling. Use an accessible combobox/listbox pattern. Reopening must not show a previous secret query.

### Tray, CLI, and lifecycle

- Tray menu includes Open library, Open finder, Add command, Add secret, Lock/Unlock vault, Settings, and Quit. Use native menu actions; Tauri raw tray mouse events are unsupported on Linux.
- Closing the library hides it. Closing the finder hides it. The app remains alive until explicit Quit. Normal application launch opens the library; `--background` starts hidden. Start at login is opt-in.
- Provide `dev-console vault lock`, `dev-console vault unlock`, and `dev-console vault status` with useful exit codes and reliable acknowledgement. A single-instance callback alone is not a synchronous response channel. Implement an appropriate user-session D-Bus control API or equally well-justified native control mechanism.
- Manual lock must also work when the app is not running without briefly auto-unlocking the vault. Status never prints secret data. Repeated launches never duplicate windows or resident processes.

## Persistence and security requirements

Commands use SQLite through Rust. Secrets use an established encrypted vault library. Evaluate Stronghold's native API first; if it cannot satisfy requirements cleanly, choose another maintained, documented vault/encryption implementation and record the reason. Do not invent a cryptographic scheme. Use atomic writes and versioned migrations, user-only file permissions, and XDG-aware paths.

Generate a random vault credential and store it in the system keyring using the Linux Secret Service API. Ordinary unlocked desktop use should auto-unlock the vault without a separate app master password or inactivity timeout. A missing or locked keyring keeps secret features unavailable and displays a real setup/unlock path. Never use plaintext fallback or a committed default key. Preserve the random credential across restarts; do not regenerate it and lose access to an existing vault.

Manual vault locking persists across finder opens, app restarts, and login until explicit authenticated unlock. Clear frontend secret values/results, backend credentials/decrypted state where the library permits it, pending secret actions, and app-owned clipboard contents. Recheck backend authorization for every secret operation, including stale requests and requests racing with lock. Hide secret names/types while locked; command features remain available.

Manual unlock must require real system authentication. Merely retrieving a credential from an already-unlocked keyring is not authentication. Research supported native mechanisms and choose a practical Linux implementation, such as an isolated native authentication adapter with a correctly configured PAM flow or a dedicated keyring collection with a real unlock prompt. Do not lock the user's shared login collection, install a privileged helper silently, change login PAM files, or build a password-check stub that always succeeds. Avoid retaining or logging authentication passwords. If installation requires an explicit privileged system component, ship it with documented installation steps and leave it inactive until installed.

Be honest about the threat model: app-level locking and a keyring available to the logged-in user do not protect against malicious software already executing as that user or someone able to modify the app/configuration. Recommend desktop locking for leaving the computer unattended. Do not claim same-user attacker resistance merely because a local lock flag persists.

Follow desktop lock/suspend events where supported and restore normal access after authenticated desktop return unless manually locked. Include testable OS adapters and documented desktop validation for these hooks.

Copy secrets in the native backend with a short configurable expiry. Clear only when the clipboard still contains the app's copied value. Implement the available Wayland sensitive-content/clipboard-manager exclusion protocol and document exactly which managers support it. A timer cannot erase copies already archived by a clipboard manager. Do not claim universal history exclusion or turn off the entire user's clipboard manager. If exclusion is unavailable, expose the limitation and provide a safe supported path rather than silently asserting safety.

Keep secret values out of notifications, logs, generic error messages, search DTOs, change events, command arguments, plaintext exports, and Git artifacts. Narrow frontend capabilities; verify calling-window access in Rust. The finder cannot edit/delete/reveal/run through hidden backend APIs. Do not give frontend code unrestricted shell, filesystem, or keyring access. Use local bundled content and a restrictive CSP; render custom text as text, not arbitrary HTML.

Include command JSON import/export and encrypted vault backup/restore. A backup must be recoverable on a machine without the original keyring using a separately held recovery credential or password-protected export. Wrong recovery credentials, malformed imports, and damaged backups must not destroy existing data.

If the cloud environment cannot provide real keyring authentication, clipboard history, or Hyprland, implement and test the native adapter code as far as possible, use clearly isolated test doubles only in tests, and identify the remaining desktop checks. Never ship mock services as production fallbacks. Do not label an unresolved security requirement complete. Continue all independent work rather than stopping the entire run because a desktop-only check is unavailable.

## Git checkpoints and progress preservation

Commit and push after each meaningful tested slice, approximately these checkpoints:

1. Runnable Tauri shell, two windows, tray and single-instance lifecycle.
2. Command persistence, complete library editor, notes/custom fields.
3. Finder ranking, completion, keyboard behavior, copy, details handoff.
4. Encrypted vault, keyring integration, authenticated locking, CLI.
5. Native integrations, execution, backups, settings, installation scripts.
6. Full verification, CI, release artifacts, documentation, final report.

Use descriptive commits such as `feat: add tray lifecycle and library window`. Update PROGRESS.md in each checkpoint with what changed, exact checks run, outcomes, and the next step. Push the branch immediately after committing; local commits alone do not preserve work in an ephemeral cloud environment. Do not wait until the end for one large commit.

If a session/time limit approaches or a genuine external blocker prevents verification, save a clearly labeled WIP checkpoint with the known failure and next steps, push it, and continue when possible. Never describe failing WIP as a passing checkpoint. Do not force-push, reset away changes, squash away checkpoint history, merge into main, or delete the repository. If GitHub temporarily fails, keep local commits, retry sensibly, and report the last confirmed pushed SHA.

Open a draft PR early if helpful and update it at checkpoints. At the end, create or update one PR against main. Mark it ready only when the product checks pass and outstanding desktop verification is accurately disclosed; leave it draft if required implementation/security behavior remains incomplete. Upload build artifacts to GitHub Actions artifacts. No public release, merge, or deployment is required.

## Required verification and compilation

Establish reproducible scripts and commit both bun.lock and Cargo.lock. Provide a single documented check command or script that runs the relevant checks and fails on the first failure. The exact package scripts may be selected by you, but these capabilities are mandatory:

```sh
bun install --frozen-lockfile
bun run check
bun run lint
bun run test
bun run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
bunx tauri build --no-bundle
```

Also run frontend E2E tests through a documented script and produce a Linux release bundle. Confirm actual CLI options with the installed Tauri version (`bunx tauri build --help`). For AppImage or another chosen bundle, run the applicable Tauri bundling command and verify that it actually produces a usable binary/artifact. Do not claim success based on a development server or `bun run build` alone. If packaging dependencies are missing, install them in the disposable build environment and retry. A successful Rust test suite does not substitute for a native release build.

Add GitHub Actions CI that installs documented system dependencies, runs checks, compiles the native app, and uploads the release output plus a concise build manifest with commit SHA, OS/architecture, compiler versions, and checksums. Include an Arch Linux build job or reproducible Arch container build in addition to the primary supported Linux build environment. Build as a normal user when tooling rejects root. Document the artifact's actual glibc/system-library requirements. Do not label an Ubuntu-built executable an Arch-tested binary without evidence. Do not waste the run setting up cross-platform matrices.

Use unit/integration tests for actual failure modes, not just snapshots of implementation details:

- CRUD persists across restart; duplicate names and custom-field ordering survive; stale revisions and failed writes preserve existing data.
- Search ranking, type/tag filters, pin/recent ties, selection, Tab completion, no results, duplicate IDs, old async responses, IME, and locked-vault metadata exclusion.
- Vault round trips, wrong credentials, corrupted files, startup auto-unlock, missing keyring, persistent manual lock, cancellation/failed authentication, lock/reveal/copy races, and unauthorized finder IPC calls.
- Encrypted files and persisted metadata do not contain test secret plaintext. Use disposable synthetic secrets only. Do not leak those values through CI logs while testing for leakage.
- Clipboard expiry does not overwrite newer content; unsupported exclusion is reported; terminal arguments and working directory stay correct; execution occurs only after explicit saved-command approval.
- Encrypted backup/restore round trip on a fresh profile without the original keyring; wrong recovery data and malicious/invalid imports leave existing records intact.
- CLI lock/status acknowledgements, single-instance routing, library close vs Quit, event synchronization, and dirty-draft handling.

Use Playwright or equivalent for browser-based Svelte interaction tests with explicit mocked IPC. Those tests do not prove native IPC works. Separately exercise real Tauri IPC/lifecycle with the supported native test driver or a controlled graphical session. For Linux headless native tests, use a disposable display server and session D-Bus where feasible, such as Xvfb with dbus-run-session, and document that this does not validate Wayland/Hyprland placement. Capture library/finder screenshots with synthetic entries and inspect layout at normal and narrow sizes. Fix clipping, unreadable text, missing focus, broken scrolling, and keyboard regressions.

Run the packaged app's launch/CLI smoke tests in a disposable profile. Test at least the real command CRUD path through the native backend. Do not substitute production storage with a mock to make the packaged smoke test pass. Set isolated XDG directories and never use the builder's real keyring or command database for destructive tests.

After all corrections, rerun the complete relevant checks on the final commit. Observe the PR's CI and fix failures. Do not disable security checks or rewrite tests solely to get green results. If native display access or CI permissions are unavailable, record the exact failure, complete all feasible checks, and leave the required remaining validation explicit.

## Deliverables and stopping condition

Deliver source, lockfiles, migrations, tests, CI, a working Linux desktop release build, installation/uninstallation instructions, a desktop entry, optional autostart setup, and a repeatable Super+F integration script/snippet. The installer must preserve the user's config, back it up before edits, avoid duplicate bindings on rerun, and support restoring only its own changes. Leave Omarchy-managed source files untouched. Do not apply the installer on the cloud worker's desktop.

Update README.md with actual development, check, build, install, first-run keyring setup, backup/recovery, lock/unlock, and uninstall instructions. Update the plan/specification only to reflect real justified implementation choices; do not delete unmet requirements. Create TEST_REPORT.md with passed/failed/unavailable checks, exact commands, environment, artifact paths/checksums, and a remaining Arch/Hyprland smoke-test checklist. Create a short HANDOFF.md with the PR, branch, last pushed SHA, how to obtain and launch the build, known limitations, and next actions.

Evaluate all C01-C24 outcomes in COMPLETION_CHECKLIST.md and record test evidence for each in TEST_REPORT.md. Report "Complete and verified" only when all required outcomes and tests pass, including real desktop integrations. If all implementation and cloud-executable verification are finished but genuine desktop-only tests remain unavailable, report "Cloud implementation finished; desktop verification pending" and leave the PR draft. Missing implementation, failing checks, or simulated security behavior means "Partial or blocked". Never count skipped/unavailable tests as passing or use a successful frontend build as proof the native app is complete.

If you reach an external blocker that cannot be resolved in this environment, finish unaffected features, preserve all progress in pushed commits, document the smallest concrete next step, and return an honest partial result. Do not keep retrying the same unavailable service indefinitely and do not claim the entire task completed merely because time ran out.

Final response: repository/PR URL, branch and last pushed SHA, implemented capabilities, verification results, release artifact links, installation command, and any remaining blockers or desktop checks. Keep it factual and concise.
