# TEST REPORT — Dev Console

Acceptance contract: `COMPLETION_CHECKLIST.md` (C01–C24). Statuses are `PASS` / `FAIL` /
`UNVERIFIED` and are only assigned from tests that were actually executed. A `PASS` here is
always scoped to the environment named in the row; where a criterion also needs the real
desktop (Hyprland/Waybar/login keyring/polkit agent/logind) that part is listed separately and
those C-numbers stay in the unverified set at the bottom.

Tested commit: the HEAD of `codex/overnight-mvp` that contains this file; the hash is recorded in
`PROGRESS.md` → "UTC checkpoints". Cloud environment: Ubuntu 22.04.5 container, x86_64, 2 CPU,
985 MiB RAM + 6 GiB swapfile, rust 1.99.0, node v24.21.0, webkit2gtk 2.50.4. No Wayland
compositor, no Waybar, no polkit authentication agent, no systemd/logind session.

## Verified so far

| C | Status | Evidence actually executed | Remaining test for full closure |
| --- | --- | --- | --- |
| C06 | PASS (cloud) | `cargo test search::` → 6/6: exact > prefix > token > contains > field > fuzzy, type/tag filters, pin-then-usage ordering for the empty query, description-only match ranks below name match, usage-only breaks ties, id is the last tie-break. Deterministic (no timing/RNG input) | none for the ranking itself |
| C07 | PASS (cloud) | `npx vitest run` → 13/13 (`src/lib/finder-state.ts`: selection wrap-around, tab completion only extends a prefix, IME composition is not treated as commit); `npx playwright test` keyboard cases in `tests/e2e/finder.spec.ts` | real-keyboard run in Hyprland (C20) |
| C09 | PASS (vault half) | `cargo test vault::` → 8/8 including `stale_revisions_are_rejected_and_reorder_survives`, `a_failed_write_keeps_the_previous_vault_and_memory`, `truncated_ciphertext_is_rejected_without_replacing_the_vault` (atomic temp+rename, no empty replacement vault) | SQLite command-store concurrency, schema migration from a prior-schema fixture, interrupted-import fixtures (needs the repository-level tests + native run) |
| C10 | PASS (storage layer) | `cargo test vault::` → armored `age` ciphertext round trip through the real library, `nothing_sensitive_appears_in_the_vault_file` (distinctive synthetic name/type/tag/note/field/secret markers scanned out of the vault file, never printed), `a_wrong_key_is_rejected_without_touching_the_file`, tampered/truncated ciphertext rejected without overwriting | whole-profile marker sweep across SQLite, export files, logs and build artifacts (S6 script) |
| C11 | PASS (isolated real provider) | `bash scripts/test-keyring.sh` → exit 0: private `dbus-run-session`, real `gnome-keyring-daemon` registers `org.freedesktop.secrets`, `scripts/keyring-default-alias.py` performs the desktop's `SetAlias("default", …)` call, `secret-tool` store/lookup sanity check, then the previously `#[ignore]`d `keyring::tests::real_keyring_round_trip` writes and re-reads the vault credential over the real D-Bus protocol through the app's own adapter (`src-tauri/src/keyring.rs`). The script exits 77 (UNVERIFIED) instead of passing silently when no provider can start | the same flow against the owner's login keyring at graphical session start on Arch/Hyprland; plus a restart test through the full app process (native smoke) |
| C16 | PASS (argv construction half) | `cargo test execution::` → 3/3: the secret never reaches `argv`, empty/whitespace command refused, secret-bearing commands refuse to run without approval, missing working directory is an error and not a silent fallback, spaces in the working directory survive | launching in the configured real terminal on the desktop, and the approval prompt UX |
| C17 | PASS (parsing half) | `cargo test cli::` → `parses_every_documented_invocation`; `dev-console vault status/lock` in isolated `DEV_CONSOLE_DATA_DIR`/`DEV_CONSOLE_CONFIG_DIR` → `unavailable` exit 20, `lock` exit 0, `status` → `locked (manual)` exit 10, flag file 0600 inside a 0700 directory | the control channel against a **running** instance (release binary + `npx tauri build --no-bundle` smoke run), single-instance handoff with an existing instance |
| C23 | PARTIAL → UNVERIFIED | branch `codex/overnight-mvp` pushed with checkpoints and lockfiles committed (`src-tauri/Cargo.lock`, `package-lock.json`) | CI workflow green on a clean runner, tagged release artifacts + SHA-256 |

## Not yet exercised (no status claimed)

| C | Status | Why, and the exact test that is still owed |
| --- | --- | --- |
| C01 | UNVERIFIED | The full clean-checkout chain (`npm ci` → check → lint → unit → build → `cargo fmt --check` → clippy `-D warnings` → `cargo test` → release compile → packaging) has not been run end-to-end from a fresh directory. Remaining: `bash scripts/verify.sh` in a fresh clone, recorded in CI |
| C02 | UNVERIFIED | Release compile of the current tree is running; `.deb`/AUR packaging not produced. Remaining: `npx tauri build` producing an installable artifact + checksum |
| C03 | UNVERIFIED | Tray menu code path exists and a tray failure is deliberately non-fatal, but no status-notifier has been driven. Remaining: open tray under a real `org.kde.StatusNotifierWatcher`, exercise Show Library / Finder / Lock / Quit |
| C04 | UNVERIFIED | Command CRUD is proven through Playwright against `tests/e2e/ipc-mock.js`, i.e. a test double, which cannot establish SQLite persistence. Remaining: repository-level Rust tests (create/update/delete/pin/reorder/revision conflict) + native smoke that writes through real IPC, quits, restarts and re-reads |
| C05 | UNVERIFIED | Vault-side secret CRUD/reveal is unit-tested (`cargo test vault::`) but never driven through the real IPC surface. Remaining: same native smoke covering list → masked detail → explicit reveal → edit → delete while locked and unlocked |
| C08 | UNVERIFIED | Cross-window refresh events are emitted by `ipc.rs` and the UI reacts to them in the mocked E2E suite. Remaining: native two-window run asserting a save in the library is visible in the finder without a manual refresh |
| C12 | UNVERIFIED | Manual lock surviving a restart is proven in `vault::tests::manual_lock_survives_a_restart_and_blocks_every_secret_operation`, and the CLI flag path exits 0/10/20 as specified. Remaining: `dev-console vault lock` while the **app is running** (status must report locked from the live instance, finder must refuse secret search, restart stays locked, fresh login session, clipboard clearing of app-owned copies) |
| C13 | UNVERIFIED | Implemented as polkit + `pkexec /usr/libexec/dev-console-auth-helper`; `auth::mechanism_available()` is `false` in this container (no polkit agent, helper not installed) and the UI reports that instead of faking it. Remaining: cancelled/failed authentication keeps the lock flag, successful real authentication restores access, unrelated keyring items still readable |
| C14 | UNVERIFIED | `ipc.rs` re-checks `window.label()` and vault state per call and `capabilities/finder.json` grants only the finder command set. Remaining: direct locked-state calls for reveal/copy/save/delete rejected, a lock/reveal race with controlled synchronisation, and editor/run attempts from the finder context rejected |
| C15 | UNVERIFIED | `clipboard.rs` sets `x-kde-passwordManagerHint` and reports `supported: false` with the honest reason instead of claiming exclusion. Remaining: under Xvfb with `xclip`, copy a secret, verify the hint target is set, verify expiry wipes/invalidates the app-owned clipboard id, and that no global manager is disabled |
| C18 | FAIL | Backup rotation, import and export are not implemented yet (S5). Remaining: full C18 test with a rotated backup set and a corrupted-backup restore |
| C19 | FAIL | No installer, `.desktop` entry or autostart wiring yet (only the polkit policy + helper files under `packaging/`). Remaining: install into a disposable prefix, log in, confirm the desktop entry, autostart and clean uninstall |
| C20 | UNVERIFIED | No Wayland compositor in this environment. Remaining: on the target Hyprland session — window rules, focus, the finder shortcut through Waybar/hyprland keybind, no focus stealing |
| C21 | UNVERIFIED | No logind/session manager. Remaining: lock and suspend the real session, confirm the vault is locked on resume and no secret can be revealed |
| C22 | UNVERIFIED | Layouts are rendered in the mocked suite (incl. the narrow single-pane case) but accessibility and performance numbers are not measured. Remaining: keyboard-only walkthrough, accessible-name audit, and startup/search latency measurements on the release artifact |
| C24 | UNVERIFIED | `PROGRESS.md` is current; README/how-to, install and troubleshooting docs and the final handoff summary are still owed (S6) |

## Unverified because the environment cannot host the test

C03, C11 (desktop login-keyring half), C12 (running-instance + fresh-login half), C13, C14
(race half), C15 (real clipboard manager), C20, C21. Each row above names the exact remaining
desktop test. Nothing in this report claims those pass.
