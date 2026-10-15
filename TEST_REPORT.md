# TEST REPORT — Dev Console

Acceptance contract: `COMPLETION_CHECKLIST.md` (C01–C24). Statuses are `PASS` / `FAIL` /
`UNVERIFIED` and are only assigned from tests that were actually executed. A `PASS` here is
always scoped to the environment named in the row; where a criterion also needs the real
desktop (Hyprland/Waybar/login keyring/polkit agent/logind) that part is listed separately and
those C-numbers stay in the unverified set at the bottom.

Tested commit: HEAD of `codex/overnight-mvp` containing this file (hash in `PROGRESS.md` →
"UTC checkpoints"). Every gate below was re-executed in a **fresh `git clone` of the pushed
branch** (`/tmp/cora/clean-checkout`), not in the working copy. Cloud environment, re-verified
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
| `bun run test` (vitest) | 13/13 |
| `bun run build:frontend` | ok |
| Playwright E2E (`bunx playwright test`, 4 projects) | **43/43 passed (38.0s)** |
| `cargo fmt --check` | ok |
| `cargo clippy --all-targets --locked -- -D warnings` | clean |
| `cargo test --locked` | 36 library tests + 1 ignored + native suites, all ok — and it no longer needs a Secret Service provider nor touches the user profile (provider tests are `#[ignore]`d and run under their own harnesses) |
| `scripts/test-keyring.sh` | exit 0: real `gnome-keyring-daemon` on a private bus, `real_keyring_round_trip` ok |
| `scripts/test-clipboard.sh` | 4/4 incl. the 7 s expiry case, real X11 selection via xclip |
| `scripts/test-packaging.sh` | "PACKAGING CHECKS PASSED" |
| `scripts/test-arch-package.sh` | static half green; exit 77 → `UNVERIFIED: makepkg not available here` |
| `scripts/test-native-ipc.sh` | exit 0 in both phases (default and `--include-ignored`) |
| `bunx tauri build --no-bundle` | 8.0 MB release binary |
| `scripts/smoke-native.sh <that binary>` | "ALL NATIVE SMOKE CHECKS PASSED", exit 0, including step 6b (see C08) |

The only non-green lines in the run are the two environment-boundary exits (makepkg absent)
described at the bottom; nothing was silently skipped.

## Verified so far

| C | Status | Evidence actually executed | Remaining test for full closure |
| --- | --- | --- | --- |
| C01 | PASS (cloud) | Full clean-checkout chain above, run from a fresh clone of the pushed branch | CI workflow run on a clean runner (needs a `workflow`-scoped token) |
| C06 | PASS (cloud) | `cargo test search::` → 6/6: exact > prefix > token > contains > field > fuzzy, type/tag filters, pin-then-usage ordering for the empty query, description-only match ranks below name match, usage-only breaks ties, id is the last tie-break. Deterministic (no timing/RNG input) | none for the ranking itself |
| C07 | PASS (cloud) | `bun run test` → 13/13 (`src/lib/finder-state.ts`: selection wrap-around, tab completion only extends a prefix, IME composition is not treated as commit); `bun run test:e2e` keyboard cases in `tests/e2e/finder.spec.ts` + the redesign keycap/activedescendant cases | real-keyboard run in Hyprland (C20) |
| C09 | PASS (storage layer) | `cargo test vault::` → 8/8 and `cargo test repository::` → 7/7 on a real file database: reopen round trip, revision conflict (`Conflict`, winner intact), delete guard, newer-schema refusal, 0700 directory / 0600 database with no side file holding content after commit | an upgrade migration from a real earlier-schema fixture once a v2 schema exists, plus interrupted-import fixtures |
| C04 | PASS (native backend + real IPC halves) | `scripts/test-native-ipc.sh` runs both suites in both phases against a real SQLite file, a real age vault opened through a real Secret Service provider and a real X11 selection: `native-crud.rs` (restart round trip, revisions/usage/custom fields, pinned sort, ranking, stale-writer refusal) and `native-two-window.rs`, which drives the **production `ipc.rs` handlers** through `tauri::test::get_ipc_response` across two windows labelled `library` and `finder` — the handlers are now generic over `tauri::Runtime`, so the mock runtime exercises the real command layer, not a copy. The `#[ignore]`d vault variant also runs in the harness's ignored phase over the real provider | launching the compiled binary and driving the same flows over Wry (partially covered by `smoke-native.sh` step 6b), plus the desktop run |
| C10 | PASS (storage layer) | Journaling is delete-mode rather than WAL (a WAL side file holds committed rows without inheriting the 0600 mode). Armored `age` round trip, `nothing_sensitive_appears_in_the_vault_file` (synthetic markers scanned out of the vault file, never printed), wrong key / tampered / truncated rejected without replacing the vault | whole-profile marker sweep across SQLite, export files, logs and build artifacts (S6 script) |
| C11 | PASS (isolated real provider) | `scripts/test-keyring.sh` → exit 0: private `dbus-run-session`, real `gnome-keyring-daemon` registers `org.freedesktop.secrets`, `scripts/keyring-default-alias.py` performs the desktop's `SetAlias("default", …)` call, `secret-tool` sanity check, then `keyring::tests::real_keyring_round_trip` stores and re-reads the vault credential over real D-Bus through the app's own adapter. Exit 77 (UNVERIFIED) rather than a silent pass when no provider can start | the same flow against the owner's login keyring at graphical session start on Arch/Hyprland |
| C11b | PASS (provider present but unusable) | Earlier smoke run: a Secret Service name was activatable but could not serve the request; the app reported `unavailable: vault storage is unavailable: …` with exit 20 while commands stayed usable — no plaintext fallback, no credential regeneration | the same with the collection explicitly locked |
| C16 | PASS (approval + argv construction half) | `cargo test execution::` → 3/3: the secret never reaches `argv`, empty/whitespace command refused, secret-bearing commands refuse to run without approval, missing working directory is an error, spaces survive. `repository::tests::approval_of_a_command_is_voided_by_editing_it` binds approval to revision + text + cwd | launching in the configured real terminal on the desktop, approval prompt UX |
| C17 | PASS (single instance + control channel half) | Smoke run on the clean-checkout binary: second `dev-console --library` handed off and exited while the first stayed resident and the control socket kept answering; live exit codes 0 / 10 / 20 / 30 all observed; `cargo test cli::` covers every documented invocation | `--finder` toggling and window focus on real Hyprland |
| C12 | PASS (running-instance half) | `smoke-native.sh`: `vault lock` executed by the live instance (exit 10, flag 0600), status `locked (manual)` exit 10, **still locked after the process was killed and restarted**; `vault::tests::manual_lock_survives_a_restart_and_blocks_every_secret_operation` + `a_failed_unlock_keeps_the_manual_lock_and_the_flag_file` cover the state machine (unlock authenticates → opens → only then clears the flag) | fresh graphical login session, real finder refusing secret search, clipboard purge assertion on lock |
| C15 | PASS (real X11 selection, cloud half) | `scripts/test-clipboard.sh` → 4/4 under Xvfb with a genuine selection owner: pasteable + cleared on lock, never clears content it does not own, expiry ticker retracts only after its window, exclusion honestly reported `supported: false / mechanism: none`. (The harness found a real defect: publishing only under `x-kde-passwordManagerHint` made secrets unpasteable; now ordinary text.) This suite previously *printed* "skipping" and still reported ok — it now hard-fails via `require_display()` so a missing display can never look like a pass | whether a clipboard history manager on the real desktop keeps a copy; Wayland `wl-copy` behaviour with the session manager in use |
| C18 | PASS (cloud half) | `cargo test backup::` → 6/6 on real age-sealed files: round trip, no plaintext in archives, rotation keeps newest N and never touches foreign files, foreign key refused, corrupt/truncated/newer archives refused without changing live data, inconsistent archives (dup ids, `../escape.age`, absolute names) rejected. Import is two-phase (validate, then apply only with an unlocked vault). 2 UI cases in the e2e suite; startup rotation runs in the app | restoring into a real desktop session with the real keyring; a from-scratch recovery rehearsal on the target machine |
| C19 | PASS (packaging half) | `scripts/test-packaging.sh`: `desktop-file-validate` clean (source and installed), staged `DESTDIR`/`PREFIX` install places binary/launcher/0755 helper/polkit policy/4 icon sizes with expected modes, policy XML re-validated with `exec.path` rewritten to the prefix, uninstall removes **exactly** the manifest (foreign file survives, second uninstall refuses without a manifest). Two real defects were found by this harness (malformed policy XML; wrong rewrite target). `cargo test autostart::` covers the runtime autostart entry | logging into a real desktop: launcher appears and starts the app, icon cache, polkit actually prompts for the helper |
| C23 | PASS (push half) | branch `codex/overnight-mvp` pushed with checkpoints; lockfiles committed (`src-tauri/Cargo.lock`, `bun.lock`) | CI workflow green on a clean runner, tagged release artifacts + SHA-256 |

## Not yet exercised (no status claimed)

| C | Status | Why, and the exact test that is still owed |
| --- | --- | --- |
| C02 | UNVERIFIED | The release compile succeeds (`bunx tauri build --no-bundle` → 8.0 MB binary, which is what `smoke-native.sh` drives) and the staged install/uninstall is proven, but **no installable package has been produced**: `.deb` and `.pkg.tar.zst` with SHA-256, installed into a clean prefix and launched from there |
| C03 | UNVERIFIED | Tray code path exists, tray failure is deliberately non-fatal, but no `org.kde.StatusNotifierWatcher` has been driven: Show Library / Finder / Lock / Quit via a real tray menu |
| C05 | PASS for the native half as of this run (secret create/reveal/copy through the real provider incl. the two-window IPC vault test), UNVERIFIED for the desktop half | re-run the same flows on the real desktop; the mocked suite additionally covers the reveal/masked UI states |
| C08 | PASS (cloud half) | Smoke step 6b executed on the clean-checkout binary: `xwininfo` sees the top-level windows of one resident process serving both intents, a cross-process lock is handled and observed by the running instance; `native-two-window.rs` proves refresh events reach the other window through real IPC. Remaining: the same two-window sync on the real Hyprland session |
| C13 | UNVERIFIED (fail-closed half covered) | With no authentication mechanism installed `vault unlock` exits 30 with an explanatory error and **keeps** the manual-lock flag; `auth::mechanism_available()` is `false` and the UI says so. Owed: cancelled polkit keeps the lock, successful `pkexec` unlock restores access, unrelated keyring items stay readable |
| C14 | UNVERIFIED | `ipc.rs` re-checks `window.label()` and vault state per call (now also asserted through real IPC dispatch in `native-two-window.rs`: the finder context cannot call library commands), `capabilities/finder.json` restricts the finder set. Owed: a lock/reveal race with controlled synchronisation on the real binary |
| C20 | UNVERIFIED | No Wayland compositor. Owed on target: window rules, focus, finder shortcut via Waybar/hyprland keybind, no focus stealing |
| C21 | UNVERIFIED | No logind/session manager. Owed: lock/suspend the real session, confirm the vault is locked on resume and nothing can be revealed |
| C22 | UNVERIFIED | Layouts render in the e2e suite incl. 390 px single-pane and the 680×420 palette, and the review grid was inspected at 1440/768/390, but a keyboard-only walkthrough, accessible-name audit and startup/search latency numbers on the release artifact have not been measured |
| C24 | UNVERIFIED | README, packaging docs, session-lock notes exist; final handoff summary still owed (S6) |

## Unverified because the environment cannot host the test

C02 (package production), C03, C11 (desktop login-keyring half), C12 (fresh-login + real
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

## Arch packaging

`bash scripts/test-arch-package.sh` → static half passes (PKGBUILD fields, `bun install
--frozen-lockfile` + `cargo build --release --locked`, absolute `$pkgdir` destinations, policy
`exec.path` + launcher `Exec=` rewrites, policy XML parses, `desktop-file-validate` clean);
the real build reports `UNVERIFIED: makepkg is not available on this host` (exit 77, recorded
in the clean-checkout run above). **`makepkg` has not been run anywhere yet** — the AUR path is
documented and statically validated, not built.
