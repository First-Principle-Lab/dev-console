# Documentation

`README.md`, `AGENTS.md` and `CHANGELOG.md` stay in the repo root. Everything else is here, in a
subfolder. See the *Documentation layout* rule in [`AGENTS.md`](../AGENTS.md).

## plans/

Implementation plans for features that aren't built yet. Each says how to build the feature, how
to test it, and how to tell it is complete.

- [`shell-history-suggestions.md`](plans/shell-history-suggestions.md)
- [`project-scripts.md`](plans/project-scripts.md)
- [`env-file-import.md`](plans/env-file-import.md)
- [`project-aware-ranking.md`](plans/project-aware-ranking.md)
- [`rune-migration.md`](plans/rune-migration.md): Arch Linux migration from Tauri/Svelte to Rune, component rendering, desktop integrations, updates, and acceptance checks

## design/

- [`raycast-ui-redesign.md`](design/raycast-ui-redesign.md): the UI redesign brief
- [`raycast-ui-research-2026-11-02.md`](design/raycast-ui-research-2026-11-02.md): research and
  screenshot capture protocol

## packaging/

- [`packaging.md`](packaging/packaging.md): install, packaging and desktop integration

## project/

- [`development-guide.md`](project/development-guide.md): running, verification, CI, visual review, and README screenshot sources
- [`PLAN.md`](project/PLAN.md): product plan
- [`IMPLEMENTATION.md`](project/IMPLEMENTATION.md): implementation notes
- [`COMPLETION_CHECKLIST.md`](project/COMPLETION_CHECKLIST.md): the acceptance contract (C01 to C24)
- [`TEST_REPORT.md`](project/TEST_REPORT.md): verdict and evidence for each criterion
- [`native-feasibility-ledger.md`](project/native-feasibility-ledger.md): which native-shell checks
  this container decides, which only the owner's Hyprland/NVIDIA session can, and the command that
  settles each one - gated by `cargo test -p dev-console-desktop --lib ledger_tests`
- [`native-capability-matrix.md`](project/native-capability-matrix.md): every user-visible behaviour
  the shipped app has, mapped to the native test that decides it or to the seam that is missing -
  the honest gap list behind plan milestone 4, gated by
  `cargo test -p dev-console-desktop --lib matrix_tests`
- [`PROGRESS.md`](project/PROGRESS.md): progress log
- [`HANDOFF.md`](project/HANDOFF.md): handoff notes
- [`CLOUD_AGENT_PROMPT.md`](project/CLOUD_AGENT_PROMPT.md): the original agent brief

## reviews/

- [`project-import-fixes.md`](reviews/project-import-fixes.md): project import review evidence

The release batch review is in [`release-0.0.9-review.md`](project/release-0.0.9-review.md).

- [`project-command-groups.md`](project/project-command-groups.md): project groups and shell tracking behavior

- [`project-environments.md`](project/project-environments.md): encrypted local environment file versions and recovery

- [`release-0.0.10-review.md`](project/release-0.0.10-review.md): environment sync review and release validation

- [`native-port-validation-2026-11-09.md`](project/native-port-validation-2026-11-09.md): native screen port, regression checks, desktop evidence and remaining acceptance work.

## assets/

Product documentation images. `assets/screenshots/` contains the selected demo screenshots used by the main README.
