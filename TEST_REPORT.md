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
| C09 | PASS (storage layer) | `cargo test vault::` → 8/8 (`stale_revisions_are_rejected_and_reorder_survives`, `a_failed_write_keeps_the_previous_vault_and_memory`, `truncated_ciphertext_is_rejected_without_replacing_the_vault`: atomic temp+rename, no empty replacement vault) **and** `cargo test repository::` → 7/7 on a real file database: `a_saved_command_survives_closing_and_reopening_the_database`, `two_editors_cannot_both_save_the_same_record` (stale revision → `Conflict`, winner intact), `deleting_with_a_stale_revision_is_refused_and_keeps_the_record`, `a_database_from_a_newer_version_is_refused_rather_than_downgraded`, `app_data_never_lands_in_a_world_readable_file` (0700 directory, 0600 database, no side file with content left after commit) | an upgrade migration from a real earlier-schema fixture once a v2 schema exists, plus interrupted-import fixtures and the native-process restart run |
| C04 | PASS (native backend half) | `cargo test repository::` → 7/7 on a real file database, plus `bash scripts/test-native-ipc.sh` → 2/2 in `src-tauri/tests/native-crud.rs`, which drives the same public operations the IPC layer calls against a real SQLite file, a real age vault opened through a real Secret Service provider and a real X11 selection, including a genuine restart: revisions, usage counts and custom fields all survive it, pinned rows sort first, and the ranking finds the command by prefix | The WebKit↔Rust `invoke()` plumbing per command (the handlers are still typed for `Wry`, so `tauri::test::mock_app()` cannot call them; making `ipc.rs` generic over the runtime is the listed remaining step) |
| C10 | PASS (storage layer) | Journaling is delete-mode rather than WAL because a WAL side file holds committed rows without inheriting the database's 0600 mode (measured alternative: truncate mode leaves a persistent but empty 0600 file). `cargo test vault::` → armored `age` ciphertext round trip through the real library, `nothing_sensitive_appears_in_the_vault_file` (distinctive synthetic name/type/tag/note/field/secret markers scanned out of the vault file, never printed), `a_wrong_key_is_rejected_without_touching_the_file`, tampered/truncated ciphertext rejected without overwriting | whole-profile marker sweep across SQLite, export files, logs and build artifacts (S6 script) |
| C11 | PASS (isolated real provider) | `bash scripts/test-keyring.sh` → exit 0: private `dbus-run-session`, real `gnome-keyring-daemon` registers `org.freedesktop.secrets`, `scripts/keyring-default-alias.py` performs the desktop's `SetAlias("default", …)` call, `secret-tool` store/lookup sanity check, then the previously `#[ignore]`d `keyring::tests::real_keyring_round_trip` writes and re-reads the vault credential over the real D-Bus protocol through the app's own adapter (`src-tauri/src/keyring.rs`). The script exits 77 (UNVERIFIED) instead of passing silently when no provider can start | the same flow against the owner's login keyring at graphical session start on Arch/Hyprland; plus a restart test through the full app process (native smoke) |
| C11b | PASS (provider present but unusable) | Same native smoke run: a Secret Service name was activatable on the bus but could not serve the request (its prompter needs a display), and the app reported `unavailable: vault storage is unavailable: ...` with exit 20 while the command library, finder and tray stayed fully usable — no plaintext fallback, no credential regeneration over existing data | The same with the collection explicitly locked rather than half-registered |
| C16 | PASS (approval + argv construction half) | `cargo test execution::` → 3/3: the secret never reaches `argv`, empty/whitespace command refused, secret-bearing commands refuse to run without approval, missing working directory is an error and not a silent fallback, spaces in the working directory survive. `repository::tests::approval_of_a_command_is_voided_by_editing_it` proves the stored approval is bound to revision + command text + working directory, so an edited command cannot inherit approval | launching in the configured real terminal on the desktop, and the approval prompt UX |
| C17 | PASS (single instance + control channel half) | Same smoke run: a second `dev-console --library` exited after handing off while the first process stayed resident and its control socket kept answering; `cargo test cli::` covers every documented invocation; observed live exit codes 0 ok / 10 locked / 20 unavailable / 30 no running instance | `--finder` toggling and window focus on the real Hyprland session |
| C12 | PASS (running-instance half) | `bash scripts/smoke-native.sh` against the release binary: `dev-console vault lock` while the app was running was executed by the live instance (exit 10, flag file 0600), the live status then reported `locked (manual)` exit 10, and after the process was killed and started again it was still `locked (manual)` exit 10. `vault::tests::manual_lock_survives_a_restart_and_blocks_every_secret_operation` covers the state machine | A fresh graphical login session, the real finder refusing secret search, and asserting app-owned clipboard content is cleared on lock |
| C15 | PASS (real X11 selection, cloud half) | `bash scripts/test-clipboard.sh` → 4/4 under Xvfb with a genuine selection owner: `a_copied_secret_is_pasteable_and_is_cleared_when_the_vault_locks` (the value comes back out of the X selection and a lock clears it), `the_app_never_clears_content_it_does_not_own` (content copied by the user afterwards survives a lock), `the_expiry_ticker_retracts_the_copy_and_only_then` (nothing before the window, retraction after it), and `an_x11_session_reports_that_history_exclusion_is_unavailable` (`supported: false`, mechanism `none`). This run also found and fixed a real defect: the copy used to be published only under `x-kde-passwordManagerHint`, which a single-type client cannot combine with the ordinary text targets, so the secret was not pasteable at all. It is now written as normal text and the UI is told exclusion is unavailable | Whether a clipboard history manager on the real desktop keeps a copy, and Wayland `wl-copy` behaviour with the session manager in use |
| C18 | PASS (cloud half) | `cargo test backup::` → 6/6 on real age-sealed files in disposable directories: `a_backup_round_trips_commands_secrets_and_usage`, `backup_files_never_contain_plaintext` (no synthetic name/note/command/secret markers in the file), `rotation_keeps_the_newest_and_touches_foreign_files` (keeps exactly `keep`, a foreign file survives, backup directory is 0700), `a_foreign_key_cannot_open_a_backup_or_an_export`, `corrupt_truncated_and_newer_archives_are_refused_without_changing_anything` (truncated, bit-damaged and newer-version archives all fail and the one live backup is untouched and still opens), and `an_archive_with_inconsistent_entries_is_rejected` (duplicate ids, nameless secret, and `../escape.age` / absolute path names refused). Backups are sealed before they reach the filesystem - there is no unsealed staging file. Import is two-phase: validation returns the archive, and only the caller holding an unlocked vault applies it. `bun run test:e2e` adds 2 UI cases (create/list/restore/export through settings, and a refused backup while locked); startup rotation runs in the app itself | Restoring into a real desktop session with the real keyring, and a restored-from-scratch recovery rehearsal on the target machine |
| C19 | PASS (packaging half) | `bash scripts/test-packaging.sh` → "PACKAGING CHECKS PASSED": `desktop-file-validate` clean on `packaging/dev-console.desktop` (and on the installed copy), a staged `DESTDIR`/`PREFIX` install places the binary, launcher entry, 0755 helper, polkit policy and four hicolor icon sizes with the expected modes, the policy is re-validated as XML and its `org.freedesktop.policykit.exec.path` is rewritten to the helper of the chosen prefix, and uninstall removes **exactly** the manifest entries - a foreign `.desktop` file in the same directory survives, and a second uninstall refuses to guess without a manifest. `cargo test autostart::` covers the runtime autostart entry (0600, own file only, foreign entries untouched, `Exec=… --background`). Two real defects were found by this harness: the shipped policy comment contained a `--` sequence, which makes the XML malformed so polkit would reject the file, and the helper-path rewrite targeted a `<value>` element that polkit policies do not use | Logging into a real desktop and confirming the launcher appears and starts the app, the icon theme cache, and that polkit actually prompts for the helper |
| C23 | UNVERIFIED | branch `codex/overnight-mvp` pushed with checkpoints and lockfiles committed (`src-tauri/Cargo.lock`, `package-lock.json`) | CI workflow green on a clean runner, tagged release artifacts + SHA-256 |

## Not yet exercised (no status claimed)

| C | Status | Why, and the exact test that is still owed |
| --- | --- | --- |
| C01 | UNVERIFIED | The full clean-checkout chain (`npm ci` → check → lint → unit → build → `cargo fmt --check` → clippy `-D warnings` → `cargo test` → release compile → packaging) has not been run end-to-end from a fresh directory. Remaining: `bash scripts/verify.sh` in a fresh clone, recorded in CI |
| C02 | UNVERIFIED | The release compile succeeds (`npx tauri build --no-bundle` → a 7.9 MB `release/dev-console`, and that binary is what `scripts/smoke-native.sh` drives), but no installable package exists yet | `.deb` (plus an AUR PKGBUILD) with SHA-256 checksums, installed into a clean prefix and launched from there |
| C03 | UNVERIFIED | Tray menu code path exists and a tray failure is deliberately non-fatal, but no status-notifier has been driven. Remaining: open tray under a real `org.kde.StatusNotifierWatcher`, exercise Show Library / Finder / Lock / Quit |
| C04 | UNVERIFIED | Command CRUD is proven through Playwright against `tests/e2e/ipc-mock.js`, i.e. a test double, which cannot establish SQLite persistence. Remaining: repository-level Rust tests (create/update/delete/pin/reorder/revision conflict) + native smoke that writes through real IPC, quits, restarts and re-reads |
| C05 | UNVERIFIED | Secrets now have real native coverage - `scripts/test-native-ipc.sh` creates a secret through the keyring-backed vault, reads it back, copies it into a real X selection and asserts the vault file itself contains none of the synthetic markers - but the reveal/masked path as driven by the library UI is only covered by the mocked suite. Remaining: drive reveal/edit/delete through the real IPC surface (same runtime-generic refactor as C04) and re-run it on the desktop |
| C08 | UNVERIFIED | Cross-window refresh events are emitted by `ipc.rs` and both windows react to them in the mocked suite. A native check was added to `scripts/_smoke-session.sh` (step 6b: a second window through the resident instance, verified against the X server with `xwininfo`, plus a cross-process lock observed by the running instance) but the container filesystem went read-only before it could be executed, so it is not claimed as passing. Remaining: run `bunx tauri build --no-bundle` and `bash scripts/smoke-native.sh` after the crate-type change, then the two-window check against a real desktop |
| C13 | UNVERIFIED (fail-closed half now covered) | The smoke run proves the negative side: with no authentication mechanism installed, `vault unlock` exits 30 with an explanatory error and **keeps** the manual-lock flag; `auth::mechanism_available()` is `false` and the UI reports that instead of pretending | The positive side on real hardware: cancelled polkit authentication keeps the lock, a successful `pkexec`/polkit unlock restores access, unrelated keyring items stay readable |
| C14 | UNVERIFIED | `ipc.rs` re-checks `window.label()` and vault state per call and `capabilities/finder.json` grants only the finder command set. Remaining: direct locked-state calls for reveal/copy/save/delete rejected, a lock/reveal race with controlled synchronisation, and editor/run attempts from the finder context rejected |
| C20 | UNVERIFIED | No Wayland compositor in this environment. Remaining: on the target Hyprland session — window rules, focus, the finder shortcut through Waybar/hyprland keybind, no focus stealing |
| C21 | UNVERIFIED | No logind/session manager. Remaining: lock and suspend the real session, confirm the vault is locked on resume and no secret can be revealed |
| C22 | UNVERIFIED | Layouts are rendered in the mocked suite (incl. the narrow single-pane case) but accessibility and performance numbers are not measured. Remaining: keyboard-only walkthrough, accessible-name audit, and startup/search latency measurements on the release artifact |
| C24 | UNVERIFIED | `PROGRESS.md` is current; README/how-to, install and troubleshooting docs and the final handoff summary are still owed (S6) |

## Unverified because the environment cannot host the test

C03, C11 (desktop login-keyring half), C12 (fresh-login + real-clipboard half), C13, C14 (race
half), C15 (real clipboard manager), C17 (Hyprland focus half), C20, C21. Each row above names
the exact remaining desktop test. Nothing in this report claims those pass.


## UI redesign pass (this checkpoint)

Executed in the working copy at `/dev/shm/dev-console` (the container's root filesystem was
remounted read-only mid-session, so the workspace itself could no longer be written; the repository
was copied to tmpfs and every command below was really run there):

- `bun run check` → 0 errors, 0 warnings. `bun run lint` → clean. `bun run test` → 13/13.
  `bun run build` → ok.
- `bun run test:e2e` → **37/37** (library 11, finder 11, redesign 9, screenshot review 6).
  The redesign suite covers: tab counts + single selected tab, masked-and-badged secret rows,
  dirty-state indicator appearing only after an edit, the locked vault's single unlock action,
  Finder combobox/listbox attributes with a live `aria-activedescendant`, the explicit
  "Copied" state that never echoes the value, the locked-vault unlock row, keycap hints, and the
  narrow (390 px) list-first layout with its Back action.
- `SHOT_SET=before` and `SHOT_SET=after` captured 36 PNGs each at 1440/768/390 into
  `artifacts/ui-review/{before,after}/`. These are **UI-shell** checks against the documented IPC
  test double - the browser preview cannot reach native IPC - and the native binary is exercised
  separately by `scripts/smoke-native.sh`.
- The screenshots were opened and inspected, and five real defects found that way were fixed:
  the vault pill rendered sentence-case text as `VAULT UNLOCKED` (it inherited the uppercase
  `.label` token class); the "unlock" icon was actually a briefcase glyph; the Finder's Enter
  keycap rendered as an unreadable box; a locked vault produced **two** full empty states plus a
  red error banner for an expected state; and 390 px headers overflowed horizontally, hiding
  Settings. Review rows and detail timestamps also moved from epoch-looking absolutes to relative
  time with the ISO value in a tooltip.

## Rust verification boundary for this checkpoint

This checkpoint touched only `src/**` (Svelte/TS), `tests/e2e/**`, `scripts/`, `packaging/`,
`.github/`, `docs/`, `README.md` and the docs files; no `.rs` file changed since `8e7b4eb`, where
`cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`
(36 library tests + the 2-test native suite via `scripts/test-native-ipc.sh`, both green), the
keyring, clipboard and packaging harnesses, and the release build all ran to completion. The Rust
toolchain cache lives on the now-read-only filesystem, so those commands were **not** re-executed
here and are not claimed as re-run; they are carried forward with that stated.

## Arch packaging (new in this checkpoint)

`bash scripts/test-arch-package.sh` → the static half passes (PKGBUILD declares every required
field, builds with `bun install --frozen-lockfile` and `cargo build --release --locked`, installs
only absolute `$pkgdir/usr/...` destinations, packages the binary/launcher/policy/helper/icons,
rewrites the policy `exec.path` and the launcher `Exec=` to installed paths, the policy XML parses,
`desktop-file-validate` is clean), and the real build reports `UNVERIFIED: makepkg is not available
on this host` with exit 77. **`makepkg` has not been run anywhere yet** - the AUR path is documented
and statically validated, not built.
