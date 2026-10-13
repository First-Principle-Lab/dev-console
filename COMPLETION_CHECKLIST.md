# Completion checklist

This checklist is the acceptance contract for the overnight agent. Creating files, a tray icon, a mockup, or a successful frontend build does not complete the project. Each required outcome below needs recorded evidence.

Use these statuses in TEST_REPORT.md:

- PASS: the required behavior was exercised and its assertions succeeded. Record command/test name, environment, tested application commit, and log/artifact reference.
- FAIL: behavior is missing or a confirming test failed.
- UNVERIFIED: implemented behavior could not be exercised in the available environment. Record the reason and exact remaining test.

Do not replace required tests with SKIP or N/A to count them as passed. Test doubles can confirm internal behavior, but cannot establish that a real keyring, tray, authentication prompt, or Hyprland shortcut works.

## Outcomes and confirming tests

### C01. Reproducible source and build

Outcome: a clean checkout produces the actual desktop application with committed dependency lockfiles and documented dependencies.

Test: in a fresh build directory, run `npm ci`, TypeScript/Svelte checking, lint, frontend tests, frontend production build, `cargo fmt --check`, Clippy with warnings denied, Rust tests, native Tauri release compilation, and the selected packaging build. All must exit zero. No uncommitted generated code may be necessary to make the build work. Record versions and artifact checksums.

### C02. A usable native release artifact

Outcome: a remotely preserved Linux binary/package launches as a Tauri desktop app.

Test: download the actual uploaded artifact, verify its checksum, launch it in an isolated profile, and exercise a real library add/read/update/delete operation through native IPC. Close and relaunch to confirm persistence. Browser-only tests or build-file existence are insufficient. Publish the supported runtime/system-library requirements.

### C03. Tray lifecycle

Outcome: a resident app exposes every planned tray action and survives closing its GUI.

Test: open library and finder from the native tray menu, create an entry from each Add action, toggle vault state, open Settings, close the library, and reopen it from the tray. Confirm the same resident process remains and Quit terminates it. Repeat with an unsaved draft and confirm Save/Discard/Cancel behavior. Test Linux menu actions rather than unsupported raw tray mouse events.

### C04. Complete command management

Outcome: commands support browse, view, create, edit, delete, copy, pin, names, custom types, tags, notes, and optional custom fields.

Test: create two commands with the same name and different types; add and reorder text/URL/multiline fields; edit one; cancel another edit; pin it; restart; inspect all persisted values/order; copy the intended saved command; cancel deletion once and confirm deletion once. Assert other records remain unchanged.

### C05. Complete secret management

Outcome: unlocked secrets support the same metadata/editor features with deliberate reveal and masked normal inspection.

Test: create synthetic secrets, inspect without revealing, explicitly reveal/edit, copy, pin, cancel edits, and delete. Restart and verify encrypted persistence. Normal details, finder results, events, and notifications must omit secret values. A locked vault must reject all secret operations in the backend.

### C06. Combined search and ranking

Outcome: finder searches both kinds by name/type/tags/description, supports type/tag filters, and uses deterministic recommendations.

Test: seed a mixed dataset containing exact, prefix, token, fuzzy, type-only, tag-only, and description-only matches. Assert ranking and filters, combined results, pinned/recent suggestions, stable ties, and a maximum result count. Assert searching a secret value or an unindexed notes/custom-field value does not expose it. Lock the vault and assert all secret metadata disappears.

### C07. Keyboard selection and completion

Outcome: keyboard-only finder use matches the agreed interaction.

Test: first result is selected; repeated Down/Up moves selection and scrolls the active result into view; Tab completes its name; Enter copies exactly the selected ID's saved value and hides only after success; Ctrl+O opens the correct library tab/entry; Escape clears and hides. Assert Tab never copies/runs. Cover no results, duplicate names after completion, IME composition, and stale asynchronous responses. Simulate copy failure and assert the finder remains open with a safe error.

### C08. Immediate synchronization

Outcome: both GUI windows use the same records and vault state.

Test: keep both windows open, add/edit/delete/pin an entry in the library, and assert finder results refresh without restart. Lock from CLI and assert both GUIs clear secrets. Delay an old search response and assert it cannot restore stale secret results after lock. Verify events contain metadata only.

### C09. Safe persistence and concurrency

Outcome: failure, stale edits, and interruption do not silently destroy data.

Test: inject a failed database/vault write and assert the previous record remains and the draft is preserved. Save two competing revisions and assert the stale one is rejected. Exercise migrations from a prior schema fixture. Test malformed imports and interruption around atomic vault writes with disposable profiles. Assert failures do not create an empty replacement vault.

### C10. Encrypted secrets at rest

Outcome: secret values and all secret metadata remain encrypted outside an unlocked process.

Test: inspect the production storage API and configuration for a maintained encryption implementation and keyring-held random credential. Persist distinctive synthetic names/types/tags/notes/field values/secrets, shut down, and scan all profile files, SQLite content, exports, logs, and artifact output for those markers without printing the markers. None may appear outside an intentional encrypted test fixture. Wrong credentials and tampered/truncated ciphertext must be rejected without overwriting the original. Plaintext-marker absence alone is not proof of correct encryption; also test the real vault library's rejection and authenticated round trip.

### C11. Real keyring auto-unlock

Outcome: normal unlocked keyring use opens the existing vault automatically without an app master password.

Test: use an isolated real Secret Service provider, initialize once, quit/restart, and verify the same credential opens the same vault. With the provider locked or unavailable, confirm secrets stay unavailable and command features remain usable. Assert no plaintext/default-key fallback and no credential regeneration over existing data. On the target desktop, verify login/session unlock behavior with its actual keyring.

### C12. Persistent manual lock

Outcome: manual lock blocks secrets and cannot be undone by reopening the app.

Test: run `dev-console vault lock` with the app running, query status, open the finder, restart the app, and repeat in a fresh login session. Assert the vault remains manually locked and secret operations fail at each step. Repeat lock with no running process; starting the app afterward must not auto-unlock. Assert lock cancels queued secret actions and clears app-owned clipboard content.

### C13. Authenticated manual unlock

Outcome: explicit unlock requires real system authentication and preserves unrelated keyring users.

Test: after manual lock, attempt GUI and CLI unlock with failed/cancelled authentication and confirm the lock flag remains. Successfully authenticate through the actual native mechanism and confirm access returns. An already-unlocked keyring alone must not satisfy this test. Verify unrelated login-keyring items remain available. Test adapters may cover errors, but successful mock authentication does not count as real integration verification. Document that this is an app access control, not protection against malicious software running as the logged-in user.

### C14. Lock enforcement and IPC permissions

Outcome: every secret API enforces current vault access, and finder IPC lacks privileged editor/execution actions.

Test: call reveal/copy/save/delete directly while locked and assert rejection. Race reveal/copy/save against locking using controlled synchronization and assert no action finishes with unauthorized secret access after lock acknowledgement. Attempt edit/delete/reveal/run from the finder calling context and assert rejection. Successful library operations while unlocked confirm permissions are not merely disabled globally.

### C15. Clipboard behavior

Outcome: copy works natively, expires safely, and reports actual clipboard-history support.

Test: copy a synthetic value and read it through the native clipboard; let expiry clear it; copy again, replace clipboard content from another app, and assert expiry preserves the replacement. Verify the supported sensitive-content mechanism with the actual clipboard manager and confirm the copied value is absent from its history. Test the unsupported-manager path and assert the limitation is explicit. Do not claim universal exclusion. If an unsupported manager needs a user-selected safer flow, document and test that flow.

### C16. Separate, intentional command execution

Outcome: command execution only occurs through the deliberate Run action on a saved record.

Test: use a harmless fixture command that writes a marker inside a temporary directory. Assert search, Tab, Enter-copy, and editing never create the marker. Run after approval and confirm marker location/content. Change command text/working directory and assert approval is invalidated. Test spaces/quotes in working directories and metadata without injection. Do not run user-supplied commands against the builder's host.

### C17. Reliable CLI and single instance

Outcome: CLI operations report actual results and repeated launches route correctly.

Test: start background mode; launch library/finder multiple times; assert one resident backend and one window of each label. Run lock/status/unlock from a second process and assert the exit status/output reflects completion or authentication failure, not just dispatch. Test unavailable service/timeouts and no-process lock/status behavior. No CLI output may contain secret data.

### C18. Backup, recovery, and import/export

Outcome: commands can be exported/imported and secrets restored without the original keyring.

Test: export command JSON and round-trip it into a fresh profile. Export an encrypted vault backup, remove access to the original keyring in the test profile, restore with the recovery credential, and verify records. Wrong recovery credentials, corrupt backups, invalid URLs, oversized/malformed inputs, and stale schema data must fail without destroying existing records. Scan backup output for plaintext secret metadata and values.

### C19. Installer, desktop entry, and autostart

Outcome: installation is documented, repeatable, reversible, and does not damage unrelated settings/data.

Test: install into a disposable HOME/XDG profile; verify executable and desktop entry paths; enable autostart and verify `--background`; disable it. Apply Super+F integration twice to a fixture containing an existing fullscreen binding and unrelated customizations, asserting only one app binding and a backup. Uninstall/rollback must remove only the app's changes, preserve unrelated edits, and preserve user data by default. The installer must also offer clear instructions for an existing user installation.

### C20. Actual Hyprland desktop behavior

Outcome: the requested keybinding and finder behavior work on Arch/Hyprland.

Test: on the target desktop or a representative documented Arch/Hyprland graphical session, validate current window-rule syntax, apply backed-up user config, run `hyprctl reload` and `hyprctl configerrors`, and require no added configuration errors. From another app and another workspace, press Super+F and confirm focused floating finder on the active monitor/workspace. Test two monitors/scaling when available, repeated toggle, Escape, and Ctrl+O handoff. An Xvfb/X11 test cannot pass this Wayland requirement.

### C21. Session lock and suspend

Outcome: ordinary desktop locking prevents vault access, normal authenticated return restores ordinary access, and manual lock persists.

Test: lock the real session and suspend/resume using a controlled graphical test environment; verify backend state/events and no secret operations while locked. Authenticate on return and verify normal access resumes. Repeat with manual lock set first and confirm it remains locked. Do not trigger suspend on a cloud host that cannot safely resume; mark that test UNVERIFIED instead.

### C22. Layout, accessibility, and performance evidence

Outcome: both interfaces are usable without clipping and the lightweight requirement has measured evidence.

Test: inspect native screenshots of the library/finder at supported sizes and scaling, navigate the complete flow by keyboard, verify focus/combobox semantics, and test long names/custom fields. Measure release startup, warm finder appearance, search latency on a declared representative dataset such as 10,000 entries, and idle process-tree memory. Record hardware/environment and results; fix obvious stalls/leaks. No arbitrary memory number is an automatic completion gate because no numerical budget has been agreed. Browser-only memory is not native app memory.

### C23. Remote history, CI, and artifacts

Outcome: work is preserved and independently buildable on GitHub.

Test: verify checkpoint commits exist on the remote working branch; PR targets main without an unauthorized merge; CI succeeds for the final application source, including an Arch build or reproducible Arch-container compilation; and artifacts/download links/checksums remain available. Confirm the downloaded build's manifest matches the application commit tested. Failed, cancelled, or skipped required CI jobs do not count as passing.

### C24. Documentation and truthful handoff

Outcome: a user can obtain, build, install, launch, test, recover, lock/unlock, and uninstall the app without guessing.

Test: follow README commands from a clean checkout/profile. Validate all artifact/PR links. TEST_REPORT.md contains C01-C24 statuses and evidence; PROGRESS.md records pushed checkpoints; HANDOFF.md gives the exact build/source version, installation command, limitations, and remaining actions. No README claim may describe an unimplemented or unverified behavior as working.

## Completion rules

### Complete and verified

Report this only when all C01-C24 outcomes are implemented and all required confirming tests PASS, including real keyring/authentication, clipboard-manager behavior, and Arch/Hyprland desktop checks. No security requirement is left as a placeholder. The branch is pushed, artifacts are preserved, final application checks and CI are green, and the handoff is accurate. The PR may be ready for review; do not merge it automatically.

### Cloud implementation finished; desktop verification pending

Use this exact distinction when the application is implemented, all cloud-executable build/tests and native smoke tests pass, and only genuinely unavailable external desktop integrations remain UNVERIFIED. List each C-number that is unverified and its exact remaining test. Do not claim the whole project is complete. Leave the PR draft until those mandatory checks pass. An Arch container compile establishes build compatibility, not real Hyprland behavior.

### Partial or blocked

Use this when any required implementation is missing, any required test FAILS, native compilation/package creation fails, artifacts/progress are not preserved, or a required security behavior is only simulated. Keep working on unaffected outcomes; push a WIP checkpoint and document the smallest remaining step. Running out of time does not turn missing features into completion.

## Final result format

```text
Result: Complete and verified / Cloud implementation finished; desktop verification pending / Partial or blocked
PR and branch:
Last confirmed pushed commit:
Tested application commit and CI run:
Build download and SHA-256:
Outcomes passed: Cxx, ...
Outcomes failed: Cxx, ...
Outcomes unverified: Cxx, ...
Exact installation command:
Remaining tests/actions:
```

The agent must leave evidence for every C-number, not merely check boxes or provide an overall "tests passed" statement.
