# AGENTS.md

Instructions for coding agents working in this repo. Read this before you commit, push, or open a PR.

## Branches

| Branch | Purpose |
| --- | --- |
| `dev` | Integration branch. All feature work lands here first. |
| `main` | Release branch. Every push to `main` that passes `verify` publishes a new AppImage release. |
| `feat/*`, `fix/*` | Short-lived branches cut from `dev`. Merge them back into `dev`. |

Never commit feature work straight to `main`. Always start a feature branch from an up-to-date `dev`:

```sh
git fetch origin
git switch -c feat/<name> origin/dev
```

## Workflow

1. **Build on a branch from `dev`.** Run the local checks (below) before pushing. Add a line for every user-visible change under `## Unreleased` in `CHANGELOG.md`.
2. **Merge into `dev`.** Use a PR or a direct push. The `verify` workflow runs on every push to `dev` and on every PR. A red `verify` on `dev` blocks promotion. Fix it on `dev` first.
3. **Promote `dev` to `main` in batches.** Promote when:
   - two or three features are waiting in `dev`, or
   - one large feature is waiting in `dev`.

   Small fixes wait in `dev` for the next batch. The exception is an urgent fix the owner asks to ship now.
4. **Promote with a PR from `dev` into `main`.** Start it with a merge commit that brings `main` into `dev`, so the two branches never drift. List the features in the batch in the PR description. In the same PR, rename `## Unreleased` in `CHANGELOG.md` to the version this merge will publish (the latest `v0.0.N` tag plus one, e.g. `## 0.0.5 — 2026-11-04`) and start a fresh, empty `## Unreleased` above it. Merge only once `verify` is green on the PR.
5. **Confirm the release.** Merging into `main` runs `verify` on `main`. Its `build` job compiles and signs the AppImage alongside the other checks, and its `publish` job releases it once every job has passed. A promotion counts as done only after you check:
   - `gh run list -w verify -L 3` shows `success` for the merge commit on `main`, including its `publish` job (`gh run view <id>`).
   - `gh release list -L 1` shows a new `vX.Y.Z` tagged **Latest**. It must carry `Dev-Console.AppImage`, `Dev-Console.AppImage.sig` and `latest.json`. Check them with `gh release view vX.Y.Z`.

   If any job fails, report it with the failing job's log (`gh run view <id> --log-failed`) and fix it through `dev`. Never claim a release went out without seeing both checks.
6. **Merge `main` back into `dev`** after the release, so `dev` contains everything that shipped.

## Versioning

You don't edit version numbers by hand. The `build` job picks the next `v0.0.N` tag and stamps it into `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `Cargo.lock`, and the root workspace's `Cargo.toml` (`[workspace.package] version`, which the `crates/*` members inherit) and root `Cargo.lock`, at build time (`scripts/set-release-version.py`). The repo files stay at their committed value. `dev-console-core` stamps that version into the sealed-backup header, so both trees must carry the same number.

## Release build

A release ships the native Rune shell in `crates/dev-console-desktop`, not the Tauri app. The Svelte frontend and `src-tauri` stay in the tree, with their checks, until the last milestone of `docs/plans/rune-migration.md` removes them. A change the owner should see in a release goes in `crates/`.

The `build` job runs the same steps on every push: `cargo build --release --locked -p dev-console-desktop`, `scripts/build-appimage.sh`, then `scripts/smoke-native-cli.sh` against the image. On `main` it also signs the image with `cargo run -p dev-console-release` and verifies the signature against `crates/dev-console-desktop/update-key.pub`. Installed clients pin that key, so never change the key, the `latest.json` shape, the asset names or the signature's `version:` field without a migration for them. `docs/packaging/packaging.md` has the details.

## Local checks

Run these before pushing to `dev`:

```sh
bun install --frozen-lockfile
bun run check        # svelte-check
bun run lint         # eslint
bun run test         # vitest unit tests
bun run test:e2e     # builds the frontend, then runs Playwright
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked   # the native shell, its scripted UI tests and the release signer
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

`bun run verify` (`scripts/verify.sh`) runs the full gate, including the native harnesses. A harness that cannot run exits 77 and reports UNVERIFIED. An UNVERIFIED result is not a pass, so don't report it as one.

Playwright reuses a running `vite preview` on port 4173 outside CI. Run `bun run build` first, or set `CI=1`, so the tests run against your current code.

## Documentation layout

Markdown files other than `README.md`, `AGENTS.md` and `CHANGELOG.md` live under `docs/`, each in a subfolder. Don't add `.md` files to the repo root or directly in `docs/`, apart from `docs/README.md`, the index.

| Folder | What goes there |
| --- | --- |
| `docs/plans/` | Implementation plans for features not built yet. |
| `docs/design/` | UI research, design briefs and review protocols. |
| `docs/packaging/` | Packaging, install and desktop-integration notes. |
| `docs/project/` | Project status: plan, progress log, handoff, test report, acceptance checklist. |

Create a new subfolder when none of these fits, and add it to `docs/README.md`. When you move or rename a doc, update every reference to it (README, code comments, tests, scripts) in the same commit, and grep for the old path before pushing.

## Conventions

- Keep UI behaviour covered by the e2e specs in `tests/e2e/`. Add or update a spec for any user-visible change.
- Buttons inside forms need an explicit `type="button"` unless they are meant to submit.
- Use the design tokens in `src/styles/tokens.css`. Don't hard-code colours.
- Never log, print or screenshot real secret values. Tests use synthetic values only.

## Finder on Hyprland

This app is tested on the owner's real Hyprland desktop, including NVIDIA and
WebKitGTK 2.54. Browser tests and Xvfb cannot prove Wayland rendering works.

- Keep the native Finder window **opaque** in `src-tauri/src/windows.rs`.
  `transparent(true)` makes the palette disappear or corrupt when the pointer
  enters it on this setup, even though Hyprland still reports a mapped, visible,
  floating 680×420 window. Reproduced and fixed on 2026-11-07 by switching the
  same build to `transparent(false)`. Do not restore transparency to fix corners.
- Keep the Finder page background opaque and matched to `--shell-bg` in
  `src/styles/tokens.css`. Hyprland rounds the native window.
- Preserve its exact `dev-console-finder` title, 680×420 logical size,
  borderless window, and the size request after mapping. The local Hyprland
  rule matches this title and sets float, size and centering. Check both title
  and class with `hyprctl -j clients` before changing any compositor rule.
  The library must remain tiled; never float every `dev-console` window.
- Preserve the existing `WEBKIT_DISABLE_DMABUF_RENDERER=1` default in `main.rs`
  unless the user explicitly overrides it. It avoids a separate WebKitGTK
  missing-acquire-timeline Wayland protocol error. This alone does not fix the
  transparent-window hover bug.
- When changing Finder rendering or startup, test the native app on Hyprland:
  open/dismiss repeatedly, move the mouse across rows and outside/back inside,
  type a query, check keyboard selection and Escape, and check that the library
  and surrounding tiled windows keep their layout. Confirm Finder remains
  floating at 680×420. Do not declare this covered by Playwright screenshots.
- For compositor changes, read the Omarchy skill and the current official
  Hyprland docs. The owner's current configuration is Lua, not the legacy
  `.conf` syntax. Back up edits and validate with `hyprctl reload` followed by
  `hyprctl configerrors`.

## Checking icons on the owner's desktop

The owner's bar is Ukishima, a Quickshell shell in `~/.local/share/quickshell/ukishima`. Dev Console's icon shows in three places, and they come from two different sources:

| Place | What it is | Icon source |
| --- | --- | --- |
| Dynamic island | The pill at the top centre of each monitor. Clicking it expands it to show the tray icons of running apps. "The island" or "the running apps on top" means this. | The native tray icon, `crates/dev-console-desktop/src/tray.rs` |
| Dock | The auto-hidden bar at the bottom with app icons. | `Icon=dev-console` from the icon theme |
| Launcher | Super+Space. | `Icon=dev-console` from the icon theme |

Capture them without clicking. `ydotool` moves the pointer to the wrong place on this two-monitor setup, and a stray click can land in another app, so don't use it.

- Island: `qs -p ~/.local/share/quickshell/ukishima ipc call ukishima peek <monitor>`, then `grim -g "<x>,0 1920x120" out.png`, then the same `peek` call again to collapse it.
- Launcher: `qs -p ~/.local/share/quickshell/ukishima ipc call ukishima launcher ""`, type the query with `wtype`, `grim`, then `wtype -k Escape`. Type only when the launcher is open.
- Dock: auto-hide only reveals on real pointer hover. Set `dockAutoHide` to `false` in `~/.local/state/ukishima/flags.json` (Ukishima watches it), take the `grim` shot, then restore the file.
- Tray items: `busctl --user get-property org.kde.StatusNotifierWatcher /StatusNotifierWatcher org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems`. Dev Console's item has `Id` `dev-console`.
- Monitors: read them from `hyprctl -j monitors` rather than assuming positions.

Quickshell caches icons in memory. After changing icon files, run `systemctl --user restart ukishima`. If the dock or launcher still shows an old logo, look for every `dev-console.png` under `~/.local/share/icons`, `/usr/local/share/icons` and `/usr/share/icons`: Qt picked a stale `/usr/local` copy over the user's on 2026-11-12.
