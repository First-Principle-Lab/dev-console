# Plan: Import project scripts

Pick a project folder. Dev Console finds the runnable tasks in it and lets you import them as
commands, grouped by project.

## Goal

- From the library, **Import from project…** asks for a folder and shows every task it found in a
  review list with a checkbox for each.
- Imported commands get:
  - the project's folder as their working directory;
  - a `project:<name>` tag;
  - a type that names the source (`npm`, `make`, `just`, `compose`).
- Running the import again on the same folder adds new tasks and skips the ones already imported,
  with no duplicates.

## Sources

| Source | File | Command created | Notes |
| --- | --- | --- | --- |
| npm/bun/pnpm/yarn | `package.json` → `scripts` | `<pm> run <name>` | Package manager detected from the lockfile: `bun.lock`/`bun.lockb` → bun, `pnpm-lock.yaml` → pnpm, `yarn.lock` → yarn, else npm. Skip `pre*`/`post*` hooks. |
| Make | `Makefile`, `makefile`, `GNUmakefile` | `make <target>` | Parse `^target:` rules. Skip pattern rules (`%`), special targets (`.PHONY`, `.DEFAULT`), and targets containing `$`. Use the `## comment` on the same line as the description when present. |
| just | `justfile`, `.justfile` | `just <recipe>` | Prefer `just --dump --dump-format json` when `just` is in `PATH`; otherwise use a simple regex over recipe headers. Skip `_private` recipes and `[private]`. |
| Docker Compose | `compose.y(a)ml`, `docker-compose.y(a)ml` | `docker compose up -d <svc>`, `docker compose logs -f <svc>` | One `up` and one `logs` per service, plus a project-wide `docker compose down`. |

Scan only the chosen folder, not subfolders. A later version could add a depth-2 option for
monorepos (`packages/*/package.json`).

## Design

### Backend (`src-tauri/src/projects.rs`, new)

- `pub fn scan(dir: &Path) -> AppResult<ProjectScan>`:
  - Canonicalises the folder.
  - Refuses anything that isn't a directory.
  - Reads only the files listed above, each capped at 1 MiB.
- `ProjectScan { project: String, dir: String, candidates: Vec<Candidate> }`. `project` is the git
  repo name (the folder containing `.git`) if there is one, else the folder name.
- `Candidate { source, name, command, description, already_imported: bool }`.
  - `name` follows `<project> · <task>`, e.g. `dev-console · test:e2e`.
  - `already_imported` is true when a saved command has the same `command` and
    `working_directory`. This is the dedupe key.
- `pub fn import(state, dir, selected: Vec<String>) -> AppResult<ImportOutcome>`:
  - Rescans on the backend rather than trusting command text from the frontend.
  - Saves the selected candidates through `Repository::save_command`.
  - Returns counts in the same shape as Bitwarden's `Outcome` (imported, duplicates, skipped).
- Parsers are pure functions over `&str` so they can be unit-tested without the filesystem:
  - `serde_json` for `package.json`;
  - a hand-written line parser for Make;
  - a YAML parser for compose. The project has none today, and `serde_yaml` is archived, so
    evaluate a maintained one (`serde_norway`, `saphyr`). The parser only needs the top-level
    keys under `services:`, so a small line-based reader is an acceptable fallback.

### IPC

- `scan_project(dir) -> ProjectScan`.
- `import_project(dir, names) -> ImportOutcome`.
- Both are library-only. Mirror them in `ipc-mock.js` with a fixture project.
- Folder choice: the project has no dialog plugin, and archive import uses a path text field.
  Use the same kind of field for v1. Adding `tauri-plugin-dialog` for a native folder picker is a
  separate decision, because it adds a plugin and capability permissions.

### CLI

- `dev-console import project <dir> [--all]` in `cli.rs` and `control.rs`, alongside
  `import bitwarden`. Without `--all` it prints the candidates and exits; with `--all` it imports
  every one. Update `USAGE` and its parse tests.

### Frontend

- `src/components/ProjectImport.svelte`: a dialog using the shared `Dialog.svelte`, so Escape and
  focus trapping behave like every other dialog.
  - Candidates grouped by source.
  - Per-group and global "select all".
  - Rows already imported show as disabled and checked, labelled "Already saved".
  - The **Import N** button submits.
- `Library.svelte`: add an **Import from project…** action next to New. The list's tag filter
  already works on `project:<name>` tags, so grouping needs no new list mode.

## Security

- Nothing runs during a scan. The one exception is `just --dump`, which is read-only; call it
  with a timeout and an argument array, never through a shell string.
- Imported commands still go through the existing execution approval, so the first Run asks for
  confirmation.
- Show the exact command text in the review list so nothing hidden gets imported.

## Steps to complete

1. Write the parsers in `projects.rs` and their unit tests, with fixture files under
   `src-tauri/tests/fixtures/projects/`.
2. Write `scan` and `import` with their dedupe logic.
3. Add the IPC commands, the mock, and the CLI subcommand.
4. Build `ProjectImport.svelte` and the library action.
5. Add a CHANGELOG line and a README section.
6. Take screenshots of the dialog at 1440 and 390 in four themes, including a project with 40+
   tasks to check scrolling.

## Tests to add

| Kind | Where | What it proves |
| --- | --- | --- |
| Rust unit | `projects.rs` | package.json scripts with `pre`/`post` hooks skipped; lockfile → package manager detection for all four |
| Rust unit | `projects.rs` | Makefile: plain targets, `## help` comments, pattern/special/variable targets skipped, multiple targets on one line |
| Rust unit | `projects.rs` | justfile regex fallback: parameters, `_private`, `[private]` attribute |
| Rust unit | `projects.rs` | compose: services from `compose.yaml` and `docker-compose.yml`; malformed YAML returns a validation error, not a panic |
| Rust unit | `projects.rs` | `already_imported` matches on command and working directory; project name comes from the git root |
| Rust unit | `cli.rs` | `import project <dir>` and `--all` parse; missing dir → usage error |
| Rust integration | `native-crud.rs` | scan a temp project, import two, rescan: both flagged as imported; import all again → 0 new |
| Playwright | `tests/e2e/projects.spec.ts` (new) | dialog lists grouped candidates; select all / none; import creates tagged commands with the right working directory; second import shows them as saved; Escape closes; keyboard-only flow |
| Playwright | `screenshots.spec.ts` | dialog captures at 1440 and 390, long list scrolling |

## How to verify it's complete

- [ ] All local checks in `AGENTS.md` pass.
- [ ] Importing this repo (`package.json`) creates `dev-console · check`, `· lint`, `· test` and
      so on, using `bun run`.
- [ ] A Makefile project and a compose project each import correctly.
- [ ] Running an imported `test` command shows its output in the app from the right folder.
- [ ] Re-importing the same folder reports 0 new and N already saved.
- [ ] `dev-console import project .` lists the candidates without importing anything.
- [ ] Screenshots reviewed.
