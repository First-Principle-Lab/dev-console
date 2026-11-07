# Project import review fixes

Follow-up to PR #25 and `docs/plans/project-scripts.md`.

- Bound both stdout and process exit for `just --dump` with one three-second deadline.
  Nonblocking reads avoid abandoned reader threads; timeout and failure stop the process group
  and reap the child. Failed, oversized, and invalid UTF-8 dumps fall back to the line parser.
- Follow GNU Make's file precedence. Skip nested `define` blocks and continued lines.
- Validate the complete Compose YAML document with `serde_norway` before extracting service
  names. Invalid nested YAML and invalid service mappings produce validation errors.
- Expand `~` and `~/` in the folder field. Report every selected task that disappeared during
  the backend rescan, including when other selected tasks were imported successfully.
- Use a native modal dialog, cycle Tab and Shift+Tab through enabled controls, keep the
  background inert, and restore focus on dismissal. The body scrolls while the footer remains
  visible. Explicit theme text colors override the browser's native-dialog defaults.
- Forward the CLI's `--all` flag to the resident app. A real control-socket test exposed this
  additional bug during the review fixes.

## Regression coverage

Parser fixtures now live in `src-tauri/tests/fixtures/projects/`, including the actual JSON
emitted by the official `just 1.59.0` Linux binary for the supplied justfile. Process tests cover
closed stdout before exit, a descendant retaining stdout, timeout, nonzero exit, and oversized
output. Native tests cover persistent scan/import/rescan, Tauri IPC authorization, execution
approval and working directory, and CLI requests over a real Unix control socket. The Tauri
runtime is mocked; storage, scanner, command execution, and socket transport are real.

The new browser checks cover keyboard containment and focus restoration, partially stale
selections, and full footer containment at 1440×900, 760×520, and 390×780 in dark, light,
Catppuccin, and Gruvbox. Updated project and delete-dialog captures are in
`artifacts/ui-review/raycast-after/`. Captures numbered 20 and 21 cover normal and long lists at
1440 and 390 in all four themes. Text entry and single-folder scanning are planned v1 scope.

## Verification

- `bun install --frozen-lockfile`: passed.
- `bun run check`, `bun run lint`, production frontend build: passed.
- `bun run test`: 24 passed.
- Full Playwright suite: 142 passed, including 28 project-import tests and four new theme capture
  tests. Browser tests use the IPC mock.
- `cargo fmt --check` and Clippy across all targets with warnings denied: passed.
- Full standard Rust suite: 101 library tests and eight integration tests passed; ten tests
  ignored by default.
- The ignored real-just smoke test was also run explicitly against `just 1.59.0`: passed. Run it
  on another host with `cargo test --manifest-path src-tauri/Cargo.toml --lib
  projects::tests::installed_just_dump_imports_only_public_recipes -- --ignored` after putting
  `just` on PATH. No recipe runs during this check.
- The remaining nine ignored tests require a display, clipboard, or Secret Service environment.
  `scripts/test-native-ipc.sh` returned 77 because `xvfb-run` is missing. Those tests and native
  WebKit/desktop rendering remain unverified on this host.
