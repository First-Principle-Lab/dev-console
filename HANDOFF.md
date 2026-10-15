# HANDOFF — Dev Console

Exact source version: branch `codex/overnight-mvp` of
https://github.com/parththirwani/dev-console. The binary in the release artifact was built
from `673ed9c` (no code changed afterwards, only docs). App version 0.1.0.

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

Pre-built artifact (draft release `v0.1.0-rc2`, supersedes rc1):
`dev-console_0.1.0_amd64.deb` (2,557,002 bytes),
SHA-256 `3c23ba673b1b9b45ea53f2a7becc14cbd7595ef015b7076310dc1615a629c535`. The upload →
download round trip was verified: the downloaded file passes `sha256sum -c` against the
published `.sha256`, matches the build that was uploaded (note: `dpkg-deb` embeds file mtimes,
so a rebuild is not byte-identical - reproducible packing is still owed), and is the exact
binary that passed
`scripts/smoke-native.sh` after extraction. The release stays a **draft** until the desktop
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
   for C03, C11, C12, C13, C15, C17, C20, C21, C22; then publish the draft release.
3. Produce the Arch artifact with real `makepkg` + SHA-256 and attach it to the release (C02).
   Re-verified this session: `makepkg` does not exist on this host (no Arch container available).
4. Settings UX (review-before-replace, recovery reveal, non-overwriting export, quoted terminal
   argv) is cloud-tested (6 new e2e cases + screenshots); a human pass on the real desktop is
   still part of checklist item 1.
4. Until 2 finishes, keep the PR draft: cloud implementation is finished and green, desktop
   verification is pending.
