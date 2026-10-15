# HANDOFF — Dev Console

Exact source version: branch `codex/overnight-mvp` of
https://github.com/parththirwani/dev-console. The binary in the release artifact (rc3) was built
from tag `v0.1.0-rc3` = commit `36736ef4e8eac0099424f4c205f179fb5c80194b`, which contains the
PR #1 review fixes (`27e580c` code + `36736ef` docs). App version 0.1.0.

## PR #1 review fixes (shipped in rc3)

- The recovery credential is now a real secret end to end: `recovery_key` refuses while the
  vault is locked (also asserted over real two-window IPC), copying goes through the new
  library-only `copy_recovery_key` command (backend clipboard + expiry, no
  `navigator.clipboard` in the UI), and any reveal is cleared when the vault state changes.
- Whole-library replacement (import/restore) is crash-recoverable: a journal file plus a
  `pending_apply` marker are written in one SQLite transaction before either store is touched;
  startup reconciles to exactly one complete version, and an unreadable journal is quarantined
  and retried after unlock rather than guessed at. A new test kills a real process mid-write at
  both boundaries and proves one complete version survives.
- Export publish is now atomic (`hard_link`): a destination that appears mid-export fails with
  EEXIST instead of being clobbered.
- Settings panel: no stale counts/secrets across vault or library changes, token-based type
  scale, footer reachable at the 760x520 minimum. Five new e2e cases (56 total), each
  mutation-checked against its fix.

How this was verified (done at `36736ef`): fresh `git clone` of the branch, `bash scripts/verify.sh`
→ "all checks passed" (18 vitest, 56 Playwright, 52 library tests, all harnesses green including
native-IPC in both phases with the new mid-write SIGABRT test), then `bunx tauri build --no-bundle`
and `bash scripts/smoke-native.sh` on the release binary → "ALL NATIVE SMOKE CHECKS PASSED"
(log `/tmp/cora/verify-clean2.log`).

## Get, build, install, launch, test, uninstall

```bash
git clone -b codex/overnight-mvp https://github.com/parththirwani/dev-console.git
cd dev-console
bun install --frozen-lockfile          # Bun is the package manager (bun.lock is committed)
bash scripts/verify.sh                 # the single documented gate; exits non-zero on real failure
                                       # (exit-77 lines = UNVERIFIED-for-environment, printed loudly)
bunx tauri build --no-bundle           # release binary -> src-tauri/target/release/dev-console
bash scripts/build-deb.sh              # dist/dev-console_0.1.0_amd64.deb + .sha256, then
                                       # extracts and smoke-tests the packaged binary
sudo PREFIX=/usr bash packaging/install.sh install     # any prefix; records an uninstall manifest
sudo PREFIX=/usr bash packaging/install.sh uninstall   # removes exactly the manifest entries
dev-console --library                  # launch; `--finder` toggles the palette
```

System requirements (runtime): Linux x86_64, GTK 3, webkit2gtk-4.1, libsoup-3, X11 or Wayland,
a Secret Service provider (`gnome-keyring` / KWallet over D-Bus) for vault storage; `xclip`
(X11) or `wl-clipboard` (Wayland) for copies. Arch: `packaging/arch/PKGBUILD` (statically
validated; `makepkg` has **not** been run anywhere yet).

Pre-built artifact (draft release `v0.1.0-rc3`, supersedes rc2/rc1):
`dev-console_0.1.0_amd64.deb` (2,566,894 bytes),
SHA-256 `216323da9e8ee22e8f978a5ecd8b360d453b4d7298bc84cd0d337250a655bfc7`. The upload →
download round trip was verified: the downloaded file passes `sha256sum -c` against the
published `.sha256`, matches the build that was uploaded and the API-reported asset digest
(note: `dpkg-deb` embeds file mtimes, so a rebuild is not byte-identical - reproducible packing
is still owed), and the binary extracted from the downloaded package passed the full
`scripts/smoke-native.sh` (live control channel, single-instance handoff, 0600 data files, real
window render) - exit 0. The release stays a **draft** until the desktop
checks below pass.

## Verified vs. not verified — read TEST_REPORT.md for the full table

Verified in a clean checkout (cloud container): the full `verify.sh` chain, real Secret
Service keyring round trip, real X11 clipboard behaviour incl. expiry, two-window IPC against
the production handlers, whole-profile plaintext marker sweep, packaging install/uninstall
exactness, release build, and the packaged-binary smoke above.

Explicitly NOT verified (environment could not host the test; each row in TEST_REPORT.md
names the exact steps): real Hyprland/Waybar behaviour and shortcuts (C17/C20), tray menu
under a real StatusNotifierWatcher (C03), login keyring at graphical session start (C11),
polkit prompt for the auth helper (C13), suspend/resume lock (C21), clipboard history
managers (C15), `makepkg`/AUR and `.pkg.tar.zst` production (C02 Arch half), and
accessibility/latency numbers (C22).

Known honest limitations (by design, also surfaced in-app): X11 offers no clipboard
history-exclusion API, so the app copies ordinary text and reports `exclusion unavailable`;
session lock is not observable without logind, so the vault stays unlocked across it unless
you lock manually; the polkit policy is shipped but inactive until the privileged install
step of `docs/packaging.md` runs.

## Remaining actions, in order

1. CI: move `packaging/ci/github-actions.yml` to `.github/workflows/ci.yml` with a
   `workflow`-scoped token and get one green run (C01/C23).
2. Desktop session on the target Arch/Hyprland machine: execute the per-row remaining tests
   for C03, C11, C12, C13, C15, C17, C20, C21, C22, plus the review-fix rehearsals
   (interrupted import, recovery reveal/copy with the login keyring); then publish the draft.
3. Produce the Arch artifact with real `makepkg` + SHA-256 and attach it to the release (C02).
   Re-verified this session: `makepkg` does not exist on this host (no Arch container available).
4. Settings UX (review-before-replace, recovery reveal, non-overwriting export, quoted terminal
   argv) is cloud-tested (11 e2e cases + screenshots); a human pass on the real desktop is
   still part of the desktop session above.
5. Until 2 finishes, keep the PR draft: cloud implementation is finished and green, desktop
   verification is pending.
