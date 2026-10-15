# TEST REPORT — Dev Console

Acceptance contract: `COMPLETION_CHECKLIST.md` (C01–C24). Statuses are `PASS` / `FAIL` /
`UNVERIFIED` and are only assigned from tests that were actually executed. A `PASS` here is
always scoped to the environment named in the row; where a criterion also needs the real
desktop (Hyprland/Waybar/login keyring/polkit agent/logind) that part is listed separately and
those C-numbers stay in the unverified set at the bottom.

Tested commit: HEAD of `codex/overnight-mvp` containing this file (hash in `PROGRESS.md` →
"UTC checkpoints"). The clean-checkout statements below were executed in a **fresh `git clone` of
the pushed branch** (`/tmp/cora/clean-checkout`). The PR #1 review fixes (section "PR #1 review
fixes" at the bottom) are committed there as `27e580c` and were re-verified end-to-end in a
**fresh clone of `36736ef`** - `bash scripts/verify.sh` "all checks passed", log
`/tmp/cora/verify-clean2.log` - including the release compile, the native smoke and the deb
stage; the deb was then uploaded as draft release `v0.1.0-rc3` and its downloaded copy was
smoked from inside the package (`/tmp/cora/smoke-rc3.log`).
Cloud environment, re-verified
2026-10-15: Ubuntu 22.04.5 LTS container, x86_64, 4 CPUs, 3 GiB RAM + 6 GiB swapfile,
rustc 1.99.0, bun 1.4.3 (node v24.21.0 exists on the image but the project never invokes
npm/npx), webkit2gtk 2.50.4. No Wayland compositor, no Waybar, no polkit authentication agent,
no systemd/logind session.

## Clean-checkout full run (C01)

`git clone -b codex/overnight-mvp` → `bash scripts/verify.sh` reached **"all checks passed"**.
Executed stages, all green unless stated:

| Stage | Result |
| --- | --- |
| `bun install --frozen-lockfile` | ok, no lockfile drift |
| `bun run check` / `bun run lint` | 0 errors, 0 warnings / clean |
| `bun run test` (vitest) | 13/13 at this table's first run; 18/18 since the argv suite (checkpoint 18) |
| `bun run build:frontend` | ok |
| Playwright E2E (`bunx playwright test`, 4 projects) | **43/43** at this table's first run; **51/51** after the Settings/review suites (`tests/e2e/settings.spec.ts`, 6 cases); **56/56** on the review-fix commits, re-run from a fresh clone of `36736ef` (settings suite now 11 cases) |
| `cargo fmt --check` | ok |
| `cargo clippy --all-targets --locked -- -D warnings` | clean |
| `cargo test --locked` | 36 library tests + 1 ignored + native suites at this table's first run; **52 library tests + the provider/abort-ignored tests** on the review-fix commits (fresh clone of `36736ef`) — and it no longer needs a Secret Service provider nor touches the user profile (provider tests are `#[ignore]`d and run under their own harnesses) |
| `scripts/test-keyring.sh` | exit 0: real `gnome-keyring-daemon` on a private bus, `real_keyring_round_trip` ok |
| `scripts/test-clipboard.sh` | 4/4 incl. the 7 s expiry case, real X11 selection via xclip |
| `scripts/test-packaging.sh` | "PACKAGING CHECKS PASSED" |
| `scripts/test-arch-package.sh` | static half green; exit 77 → `UNVERIFIED: makepkg not available here` |
| `scripts/test-native-ipc.sh` | exit 0 in both phases (default and `--include-ignored`) |
| `bunx tauri build --no-bundle` | 8.0 MB release binary |
| `scripts/smoke-native.sh <that binary>` | "ALL NATIVE SMOKE CHECKS PASSED", exit 0, including step 6b (see C08) |
| `scripts/build-deb.sh` (now a final `verify.sh` stage; ran standalone at this checkpoint) | deb + SHA-256 built, extracted and validated; the packaged binary is the one that passed the native smoke |
| full `scripts/verify.sh` re-run at `2ffe0e9`/`d964397` (log `/tmp/cora/verify2.log`) | **exit 0 end-to-end**: release compile, native smoke incl. 6b, deb stage; e2e count is now **51/51** (settings suite added) |
| rc2 artifact round trip | downloaded deb is a valid Debian package, `sha256sum -c` OK against the uploaded `.sha256` (server digest `3c23ba67…`), extracted binary passes the full native smoke |

The only non-green lines in the run are the two environment-boundary exits (makepkg absent)
described at the bottom; nothing was silently skipped.

## Verified so far

| C | Status | Evidence actually executed | Remaining test for full closure |
| --- | --- | --- | --- |
| C01 | PASS (cloud) | Full clean-checkout chain above, run from a fresh clone of the pushed branch | CI workflow run on a clean runner (needs a `workflow`-scoped token) |
| C06 | PASS (cloud) | `cargo test search::` → 6/6: exact > prefix > token > contains > field > fuzzy, type/tag filters, pin-then-usage ordering for the empty query, description-only match ranks below name match, usage-only breaks ties, id is the last tie-break. Deterministic (no timing/RNG input) | none for the ranking itself |
| C07 | PASS (cloud) | `bun run test` → 13/13 (`src/lib/finder-state.ts`: selection wrap-around, tab completion only extends a prefix, IME composition is not treated as commit); `bun run test:e2e` keyboard cases in `tests/e2e/finder.spec.ts` + the redesign keycap/activedescendant cases | real-keyboard run in Hyprland (C20) |
| C09 | PASS (storage layer) | `cargo test vault::` → 8/8 and `cargo test repository::` → 7/7 on a real file database: reopen round trip, revision conflict (`Conflict`, winner intact), delete guard, newer-schema refusal, 0700 directory / 0600 database with no side file holding content after commit; schema v2 added the `apply_state` marker table alongside the replacement journal (see C18) | an upgrade migration from a real earlier-schema fixture (the v2 schema now exists, so the fixture is the next step) |
| C04 | PASS (native backend + real IPC halves) | `scripts/test-native-ipc.sh` runs both suites in both phases against a real SQLite file, a real age vault opened through a real Secret Service provider and a real X11 selection: `native-crud.rs` (restart round trip, revisions/usage/custom fields, pinned sort, ranking, stale-writer refusal) and `native-two-window.rs`, which drives the **production `ipc.rs` handlers** through `tauri::test::get_ipc_response` across two windows labelled `library` and `finder` — the handlers are now generic over `tauri::Runtime`, so the mock runtime exercises the real command layer, not a copy. The `#[ignore]`d vault variant also runs in the harness's ignored phase over the real provider | launching the compiled binary and driving the same flows over Wry (partially covered by `smoke-native.sh` step 6b), plus the desktop run |
| C10 | PASS (storage layer) | Journaling is delete-mode rather than WAL (a WAL side file holds committed rows without inheriting the 0600 mode). Armored `age` round trip, `nothing_sensitive_appears_in_the_vault_file` (synthetic markers scanned out of the vault file, never printed), wrong key / tampered / truncated rejected without replacing the vault, **and the owed whole-profile sweep**: after a real secret lifecycle (create, copy, backup, lock, restart) the native test now reads back **every file the run wrote** - database, vault, sealed backup, flag/autostart files - and asserts the secret-only markers are absent from all of them. The first sweep round earned its keep: it flagged `commands.sqlite3`, which turned out to be the *command's* plaintext notes (plaintext by design), so the secret draft now carries its own marker and the assertion is exact | none cloud-side; the desktop profile sweep rides along with the desktop run |
| C11 | PASS (isolated real provider) | `scripts/test-keyring.sh` → exit 0: private `dbus-run-session`, real `gnome-keyring-daemon` registers `org.freedesktop.secrets`, `scripts/keyring-default-alias.py` performs the desktop's `SetAlias("default", …)` call, `secret-tool` sanity check, then `keyring::tests::real_keyring_round_trip` stores and re-reads the vault credential over real D-Bus through the app's own adapter. Exit 77 (UNVERIFIED) rather than a silent pass when no provider can start | the same flow against the owner's login keyring at graphical session start on Arch/Hyprland |
| C11b | PASS (provider present but unusable) | Earlier smoke run: a Secret Service name was activatable but could not serve the request; the app reported `unavailable: vault storage is unavailable: …` with exit 20 while commands stayed usable — no plaintext fallback, no credential regeneration | the same with the collection explicitly locked |
| C16 | PASS (approval + argv construction half) | `cargo test execution::` → 3/3: the secret never reaches `argv`, empty/whitespace command refused, secret-bearing commands refuse to run without approval, missing working directory is an error, spaces survive. `repository::tests::approval_of_a_command_is_voided_by_editing_it` binds approval to revision + text + cwd | launching in the configured real terminal on the desktop, approval prompt UX |
| C17 | PASS (single instance + control channel half) | Smoke run on the clean-checkout binary: second `dev-console --library` handed off and exited while the first stayed resident and the control socket kept answering; live exit codes 0 / 10 / 20 / 30 all observed; `cargo test cli::` covers every documented invocation | `--finder` toggling and window focus on real Hyprland |
| C12 | PASS (running-instance half) | `smoke-native.sh`: `vault lock` executed by the live instance (exit 10, flag 0600), status `locked (manual)` exit 10, **still locked after the process was killed and restarted**; `vault::tests::manual_lock_survives_a_restart_and_blocks_every_secret_operation` + `a_failed_unlock_keeps_the_manual_lock_and_the_flag_file` cover the state machine (unlock authenticates → opens → only then clears the flag) | fresh graphical login session, real finder refusing secret search, clipboard purge assertion on lock |
| C15 | PASS (real X11 selection, cloud half) | `scripts/test-clipboard.sh` → 4/4 under Xvfb with a genuine selection owner: pasteable + cleared on lock, never clears content it does not own, expiry ticker retracts only after its window, exclusion honestly reported `supported: false / mechanism: none`. (The harness found a real defect: publishing only under `x-kde-passwordManagerHint` made secrets unpasteable; now ordinary text.) This suite previously *printed* "skipping" and still reported ok — it now hard-fails via `require_display()` so a missing display can never look like a pass. The Settings recovery-credential copy went through this same backend path in the review fixes (frontend no longer holds a `navigator.clipboard` shortcut; expiry label reflects the real ticker) | whether a clipboard history manager on the real desktop keeps a copy; Wayland `wl-copy` behaviour with the session manager in use |
| C18 | PASS (cloud half) | Recovery credential: every archive is sealed to TWO age recipients (vault identity + a per-profile `recovery.agekey`, 0600, revealed only via explicit Settings action). Tests on real files: `a_recovery_credential_restores_an_archive_into_a_fresh_profile` (brand-new vault identity + recovery key opens the archive, records intact), `wrong_or_missing_recovery_credentials_are_rejected`, `a_recovery_key_is_created_once_kept_private_and_reusable`; plaintext marker scans still cover names/metadata/values. Atomicity: `apply_archive` = vault snapshot -> vault rename -> command table in one transaction, with rollback; fault-injected at EVERY stage - `a_command_table_fault_rolls_the_vault_back_on_disk_and_in_memory` proves both stores keep their previous data, and the failure path that cannot roll back now reports divergence honestly (the old false "previous data is intact" string is gone and a test forbids its return). Command JSON export/import round-trips inside the archive; Settings gained a review step (date + counts + replace warning + pre-restore backup, e2e) and non-overwriting export (unique O_EXCL staging file; the publish step is now an atomic `hard_link` that fails with EEXIST rather than clobbering a destination that appeared mid-export - test `a_destination_that_appears_mid_export_is_never_overwritten`). **Crash-recoverable replacement** (review fix): `recovery.rs` writes a journal file plus a `pending_apply` marker inside the `apply_state` table *in one SQLite transaction* before either store is touched; at startup the pending operation is completed or rolled back deterministically (journal absent/unmarked ⇒ completed; marked + journal present ⇒ roll back to the snapshot). `src-tauri/tests/interrupted-apply.rs` kills a real child process with SIGABRT at both boundaries and asserts exactly one complete version remains; an unreadable journal is quarantined (never deleted) and retried after the next unlock. Six new state.rs unit tests cover the decision table. | restore/backup rehearsal on the real desktop with the real keyring, incl. a real-device interrupted-import rehearsal |
| C19 | PASS (packaging half) | `scripts/test-packaging.sh`: `desktop-file-validate` clean (source and installed), staged `DESTDIR`/`PREFIX` install places binary/launcher/0755 helper/polkit policy/4 icon sizes with expected modes, policy XML re-validated with `exec.path` rewritten to the prefix, uninstall removes **exactly** the manifest (foreign file survives, second uninstall refuses without a manifest). Two real defects were found by this harness (malformed policy XML; wrong rewrite target). `cargo test autostart::` covers the runtime autostart entry | logging into a real desktop: launcher appears and starts the app, icon cache, polkit actually prompts for the helper |
| C02 | PASS (cloud .deb half) | `scripts/build-deb.sh`: staged install packed into `dev-console_0.1.0_amd64.deb` (2,549,908 B) + `.sha256`; extracted into a clean filesystem root (binary 0755, `desktop-file-validate` clean, polkit policy present, package metadata valid); **uploaded to draft release `v0.1.0-rc2` (rc1 was superseded by the Settings/recovery branch), downloaded back, `sha256sum -c` OK, downloaded hash identical to the build that was uploaded and to the API-reported asset digest**, and the downloaded binary extracted and run through the full `smoke-native.sh` (live control channel, single-instance handoff, 0600 data files, real window render) - exit 0. **rc3 round (current artifact):** rebuilt from a fresh clone of `36736ef` (review fixes) - `dev-console_0.1.0_amd64.deb` 2,566,894 B, SHA-256 `216323da9e8ee22e8f978a5ecd8b360d453b4d7298bc84cd0d337250a655bfc7`, uploaded to **draft** `v0.1.0-rc3` (tag at the tested commit), server asset digest matches, downloaded back, `sha256sum -c` OK, and the binary extracted from the downloaded package passed the full `smoke-native.sh`. (Re-running `build-deb.sh` produced a differing hash: `dpkg-deb` embeds file mtimes, so byte-reproducible packing is an open improvement, not a claim.) | Arch `.pkg.tar.zst` via real `makepkg` (still owed, C02 Arch half); publishing the draft after the desktop checks; a real distro package-manager install with dependency resolution; reproducible deb packing |
| C23 | PASS (push + artifact preservation half) | branch pushed with checkpoints; lockfiles committed (`src-tauri/Cargo.lock`, `bun.lock`); draft release `v0.1.0-rc1` created with the `.deb` + `.sha256` assets and a server-reported asset digest matching the local build | CI workflow green on a clean runner (token lacks `workflow` scope - re-verified this session, push to `.github/workflows/` still rejected); publishing the release after desktop checks |

## Not yet exercised (no status claimed)

| C | Status | Why, and the exact test that is still owed |
| --- | --- | --- |
| C03 | UNVERIFIED | Tray code path exists, tray failure is deliberately non-fatal, but no `org.kde.StatusNotifierWatcher` has been driven: Show Library / Finder / Lock / Quit via a real tray menu |
| C05 | PASS for the native half as of this run (secret create/reveal/copy through the real provider incl. the two-window IPC vault test, **and the recovery credential now behaves like a secret: `recovery_key` refuses while the vault is locked or manually locked after a restart, and its bytes never cross IPC in those states** - `the_recovery_credential_stays_behind_the_vault_and_copies_natively`), UNVERIFIED for the desktop half | re-run the same flows on the real desktop; the mocked suite additionally covers the reveal/masked UI states |
| C08 | PASS (cloud half) | Smoke step 6b executed on the clean-checkout binary: `xwininfo` sees the top-level windows of one resident process serving both intents, a cross-process lock is handled and observed by the running instance; `native-two-window.rs` proves refresh events reach the other window through real IPC. Remaining: the same two-window sync on the real Hyprland session |
| C13 | UNVERIFIED (fail-closed half covered) | With no authentication mechanism installed `vault unlock` exits 30 with an explanatory error and **keeps** the manual-lock flag; `auth::mechanism_available()` is `false` and the UI says so. Owed: cancelled polkit keeps the lock, successful `pkexec` unlock restores access, unrelated keyring items stay readable |
| C14 | UNVERIFIED | `ipc.rs` re-checks `window.label()` and vault state per call (now also asserted through real IPC dispatch in `native-two-window.rs`: the finder context cannot call library commands, and the recovery-credential surface - `recovery_key` plus the new library-only `copy_recovery_key` - is gated by both label **and** vault state), `capabilities/finder.json` restricts the finder set. Owed: a lock/reveal race with controlled synchronisation on the real binary |
| C20 | UNVERIFIED | No Wayland compositor. Owed on target: window rules, focus, finder shortcut via Waybar/hyprland keybind, no focus stealing |
| C21 | UNVERIFIED | No logind/session manager. Owed: lock/suspend the real session, confirm the vault is locked on resume and nothing can be revealed |
| C22 | UNVERIFIED | Layouts render in the e2e suite incl. 390 px single-pane and the 680×420 palette, and the review grid was inspected at 1440/768/390 (Settings re-inspected at 1040×700 and the 760×520 minimum after the review fixes: token-based type scale, footer pinned and reachable, body scrolls independently). Owed: a keyboard-only walkthrough, accessible-name audit and startup/search latency numbers on the release artifact |
| C24 | PASS (cloud half) | `HANDOFF.md` names the exact source version, install/launch/test/uninstall commands, artifact checksum, limitations and remaining actions; README commands followed from the clean-checkout run. A README claim bug was found and fixed on the way: it advertised `install.sh --uninstall`, which the script rejects with a usage error - the verified form `install.sh uninstall` is now documented and was executed for real | re-follow the HANDOFF on the target desktop before publishing the release |

## Unverified because the environment cannot host the test

C02 (Arch `.pkg.tar.zst` production), C03, C11 (desktop login-keyring half), C12 (fresh-login + real
clipboard half), C13, C14 (race half), C15 (real clipboard manager), C17 (Hyprland focus half),
C20, C21, plus the desktop halves of C04/C05/C08/C18/C19. `makepkg` is not available here, so
`scripts/test-arch-package.sh` exits 77 with the static half green — reported, not hidden.
Nothing in this report claims those pass.

## UI redesign pass (this checkpoint)

Executed in the workspace itself (the earlier read-only-filesystem problem is gone) and
re-verified in the clean checkout:

- `bun run check` 0/0, `bun run lint` clean, `bun run test` 13/13, `bun run build` ok,
  `bun run test:e2e` **43/43** (library 11, finder 11, redesign 13, screenshot 8).
- `test:e2e` now builds `dist/` before serving (`bun run build && playwright test`); previously
  a stale `dist/` could be served and "pass" against deleted code.
- `SHOT_SET=after` captures 38 review PNGs at 1440/768/390 plus the 680×420 palette and a
  long-content library into `artifacts/ui-review/after/`. These are **UI-shell** checks against
  the documented IPC test double; the native binary is exercised by `scripts/smoke-native.sh`.
- The screenshots were opened and inspected; defects found only by looking: flex list rows could
  not ellipsize long names/meta (long content pushed tags out of the row); a stale red error
  banner and a stale "Copied" status rendered for the *expected* locked-vault state; a fresh
  editor showed validation warnings on open (native browser bubbles also intercepted the styled
  save error); a "hidden" Badge added noise to every masked secret row. Fixed with a shared
  `src/lib/time.ts` helper, inline (non-flex) truncating text, `tags.slice(0, 2)` with a
  overflow-safe tail, lock glyphs for locked states, save-attempt-gated validation with
  `aria-invalid`, and status clearing on new searches. Four new regression tests pin these.

## Settings redesign (previous session)

Before/after screenshots at the real 1040x700 window and the 760x520 minimum, plus the import
review card, in `artifacts/ui-review/{before,after}/15-*` and `after/16-*` (all opened and
inspected). The panel gained: sticky header/footer with a scrollable body; separate Export and
Import sections; the literal backup folder path; a recovery-credential section hidden behind an
explicit reveal (locked vault refuses it - tested); the review-before-replace card (archive date,
archive vs current counts, "replaces your entire library" warning, pre-restore backup checked by
default, focus moved to the card); a terminal field that parses quoted argv (unbalanced quotes
block Save) replacing the old `split(' ')`; and export confirm-to-replace. All controls are
native labelled inputs/buttons (keyboard reachable). Library and Finder untouched.

## Arch packaging

`bash scripts/test-arch-package.sh` → static half passes (PKGBUILD fields, `bun install
--frozen-lockfile` + `cargo build --release --locked`, absolute `$pkgdir` destinations, policy
`exec.path` + launcher `Exec=` rewrites, policy XML parses, `desktop-file-validate` clean);
the real build reports `UNVERIFIED: makepkg is not available on this host` (exit 77, recorded
in the clean-checkout run above). **`makepkg` has not been run anywhere yet** — the AUR path is
documented and statically validated, not built.

## PR #1 review fixes (committed `27e580c`, shipped in `v0.1.0-rc3`)

The PR #1 review handoff listed seven findings; all seven are addressed in `27e580c` and were
first verified in the working tree there, then re-verified from a fresh clone of `36736ef`.

| # | Finding | Fix | Test added (actually executed) |
| --- | --- | --- | --- |
| 1 | `recovery_key` IPC returned the credential with the vault locked | Handler calls `AppState::recovery_credential()` → `vault.authorise()` first (same gate as secret reveal); the browser test double enforces it identically | `the_recovery_credential_stays_behind_the_vault_and_copies_natively` in `native-two-window.rs`: locked and restarted-into-manual-lock states refuse, no credential bytes in the locked responses; e2e refusal case |
| 2 | Settings kept stale counts/secrets across vault and library changes | Split loaders: `loadGeneral()` (paths/recovery status) vs `refreshCounts()`; secrets are never retained (`currentSecrets = null` after use); every credential cleared on `vault-state-changed` with an in-flight epoch guard | 3 e2e cases in `tests/e2e/settings.spec.ts`; each was mutation-checked to fail when its fix is removed |
| 3 | Frontend copied the recovery key via `navigator.clipboard`, bypassing the backend expiry ticker | New library-only command `copy_recovery_key` routes through the same clipboard module as secrets (`RECOVERY_COPY_ID`, cleared on lock); UI shows the real expiry label | Covered by the native test above (real X11 selection) + e2e expiry-label case |
| 4 | An import/replace killed mid-write could leave vault and database on different versions | New `src-tauri/src/recovery.rs`: journal file + `pending_apply` marker written **in one SQLite transaction** (schema v2 `apply_state` table) before either store changes; startup reconciles to exactly one complete version; an undecryptable journal is quarantined, never deleted, and retried after unlock | `src-tauri/tests/interrupted-apply.rs`: a real child process is SIGABRT-killed at both boundaries (asserted via wait status, so a clean exit cannot fake a pass) and the survivor is verified; 6 state.rs decision-table unit tests; release smoke asserts the `pending-replace` dir exists, 0700, empty at rest |
| 5 | Revealed credential stayed on screen after the vault locked | `on('vault-state-changed')` clears reveal + copied state and refetches status | e2e case (mutation-checked) |
| 6 | Non-overwriting export still used `rename`, which clobbers a destination created mid-export | Publish via `std::fs::hard_link` (atomic EEXIST when the target appeared); staging stays O_EXCL-unique | `a_destination_that_appears_mid_export_is_never_overwritten` (backup.rs, real files) |
| 7 | Settings type scale and minimum window: footer unreachable at 760×520, hardcoded font sizes | Token-based type/spacing (`--text`, `--text-sm`, `--sp-*`), `.body { min-height: 0 }` so the body owns scrolling | e2e scroll/footer cases; screenshots regenerated at 1040×700 and 760×520 and personally inspected (`artifacts/ui-review/after/15-*`) |

Gates re-executed on this working tree (log `/tmp/cora/verify-fixed.log`), then re-executed from a
fresh clone of the pushed result (log `/tmp/cora/verify-clean2.log`, `SKIP_*` flags none):
`bun run check` 0/0,
eslint clean, vitest **18/18**, Playwright **56/56**, `cargo fmt --check` ok, clippy
`-D warnings` clean, `cargo test --locked` **52 library tests** + ignored suites ok, keyring
harness exit 0, clipboard 4/4 on a real X11 selection, packaging green, native-IPC harness green
in **both** phases including the new abort test, then `bunx tauri build --no-bundle` and
`scripts/smoke-native.sh` on that binary → "ALL NATIVE SMOKE CHECKS PASSED", with a real-window
capture at `artifacts/ui-review/native/native-library__xvfb.png`.

Still honestly open for these fixes (environment-bound, not un-executed work): the interrupted-apply
rehearsal on the target desktop and the real-keyring recovery reveal/copy session; the clean-checkout
round and the rc3 artifact round trip were both executed on 2026-10-15/16.
