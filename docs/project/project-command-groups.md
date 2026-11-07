# Project command groups and history

Implemented for the 0.0.9 candidate on 2026-11-07.

## Library

Commands are grouped by the canonical repository root of their working directory.
Subfolders and `~/` paths resolve to the same root. Groups start collapsed and show
the saved-command count. Search opens matching groups. The secret list remains flat.
Project tags provide a fallback for commands without a reachable folder. A tag-only
command joins a known project only when its name identifies one unique root; two
repositories with the same basename stay separate. Commands with no project stay
in General. Assign a folder with Browse in the command editor. Existing command
text, working directories, IDs, approvals and saved records are not migrated or rewritten.

## Project history

Standard zsh/bash history does not record cwd. The app no longer infers projects
from that global history. The zsh integration records the actual working folder
before execution, only while Dev Console is running and history suggestions are enabled.
The record travels through stdin to a Python standard-library helper and the private
Unix socket. Command text is never placed in process arguments or logs. Recording
does not add a saved command or run a recipe.

The backend resolves the project, filters routine editor/navigation/launcher commands
and the existing credential patterns, then increments a private SQLite aggregate.
The store retains at most 10,000 project/folder/command pairs. It is separate from the
saved command database and exports. Suggestions select the five projects with the
highest aggregate recorded run counts, breaking ties by last use and root. Each has
up to five unsaved, undismissed commands ordered by count, last use, command and folder.
Counts and dismissal digests are scoped to project, cwd and exact trimmed command.
Saving a suggestion retains its cwd and a project tag. Dismissing in one project does
not suppress the same command in another. No old unscoped dismissal is applied globally.

`dev-console history setup` or the library setup action installs the zsh hook.
The installer preserves and backs up an existing .zshrc, follows an existing symlink,
and adds one guarded source line. Repeated setup does not add duplicate hooks or
source lines. Hook/helper files are private. Open a new terminal after setup, or
source the installed `shell-history.zsh` manually. Bash/fish automatic integration
is not implemented. Turning the setting off stops storage and suggestion reads.
Filtering cannot establish the meaning of every possible shell command; it reuses
the existing conservative credential detector and adds common launcher exclusions.

## Verification

- Rust tests cover project/folder identity, count persistence, the five-project and
  five-command bounds, ignored commands/credential patterns, absent project context,
  and saved-command exclusion scoped to the actual execution folder.
- A native test sends the shipped Python client through a real Unix socket to the
  production control handler. It proves count increments, no automatic library writes,
  credential filtering and disabled collection without a Secret Service dependency.
- A real local zsh test uses a temporary HOME/config/runtime. It verifies setup is
  idempotent, preserves the existing config and backup, uses mode 0600 files, and
  sends the exact quoted command and cwd over the socket. No real history was read.
- Browser regressions cover one collapsed group for 18 tasks, search expansion,
  same-named repositories, tag-only grouping, editor folder assignment, top-five
  project recommendations and saving the same command independently in two projects.
- Synthetic project screenshots are committed under `artifacts/ui-review/project-*`.
- The owner’s local zsh integration is installed. Counts begin in newly opened terminals;
  prior shell history is not backfilled. Native inspection confirmed existing commands
  grouped by project, including canonical and tilde paths. Finder transparency remains
  disabled as required by AGENTS.md.

Final checks passed: frozen Bun install, Svelte check with zero errors/warnings, ESLint,
24 frontend unit tests, production build, Rust formatting, Clippy with warnings denied,
168 Rust library tests and nine integration tests. Fourteen environment-dependent tests
remain ignored by default. The full browser suite passed all 180 tests, followed by
19 focused history/project tests after the final collapsed-history layout adjustment.
