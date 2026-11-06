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

You don't edit version numbers by hand. The `build` job picks the next `v0.0.N` tag and stamps it into `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `Cargo.lock` at build time (`scripts/set-release-version.py`). The repo files stay at their committed value.

## Local checks

Run these before pushing to `dev`:

```sh
bun install --frozen-lockfile
bun run check        # svelte-check
bun run lint         # eslint
bun run test         # vitest unit tests
bun run test:e2e     # builds the frontend, then runs Playwright
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
