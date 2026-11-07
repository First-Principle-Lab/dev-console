# Project environment history

Entering a project folder in zsh automatically discovers it and syncs its environment.
No command entry, manual project addition or Browse step is needed. The existing
Dev Console shell hook reports directory changes, a terminal's initial working folder,
and prompts to retry visits made while the app was stopped or the vault locked.
Existing hook installations update when Dev Console starts. Open a new terminal once
after updating to load the directory-change hook. Older open terminals also discover
the project when the next command is entered there. Command-history recommendations
can be disabled independently; environment capture still works.

Open **Environment** in Dev Console to see the automatically discovered folders and
copy older versions. Expanding a project group or choosing a folder with **Browse**
also syncs immediately. Environment capture requires the app to be running and the
vault to be unlocked. A paused project stays paused when visited.

Dev Console reads regular UTF-8 `.env` and `.env.*` files directly inside the selected
folder. It does not recursively scan subfolders or follow file symlinks. Entering a monorepo
subfolder with its own environment files automatically tracks that folder as well as
the repository root. A folder containing environment files also works without Git. Each file keeps its exact text, including
comments and line endings. No dotenv evaluation or variable expansion runs.

Each changed file gets the next version number and a timestamp. Unchanged content
creates no version. Versions belong to the canonical folder path and filename, so
projects with identical names remain separate. Moving a project to another path
starts a separate history. Missing files and folders never delete saved versions.
Select **Copy** on any version to recover the complete file through the native secret
clipboard. Values are never returned to the webview.

Automatic sync checks registered folders every 30 seconds while Dev Console runs
and the vault is unlocked. Opening the Environment section or selecting **Sync now**
also scans immediately. **Pause sync** stops automatic capture and keeps history.
**Sync now** still works while paused. Reopening the section does not resume a pause.
Changes made and lost between scans cannot be recovered.

Snapshots, project paths, filenames and dates live in the encrypted vault and travel
with encrypted Dev Console backups and exports. No plaintext history database or
mirror is written. Vault locking hides metadata and blocks scanning and copying.
The vault format is now version 2; existing version 1 vaults and backups load with
empty environment history. Older Dev Console versions cannot open version 2 vaults.

Reads are limited to 64 files per project, 1 MiB per file, and 32 MiB of total saved
contents. A failed scan or exceeded limit keeps the last complete saved state. The UI
shows errors from manual sync. Background scans retry on their next interval; the
latest saved timestamp shows what was actually captured. Versions are not pruned
automatically. No remote or cloud sync is performed.

Validation covers encrypted persistence, lock/unlock reload, archived history,
per-file versions, deduplication, missing source recovery, paused sync, symlink and
subfolder exclusion, bounded reads, browser navigation and ID-based clipboard requests.
