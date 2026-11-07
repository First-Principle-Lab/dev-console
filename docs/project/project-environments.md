# Project environment history

Open a project's command group and select **Environment**, or use the toolbar's
**Environment** button and choose a folder with **Browse**. Expanding a project group also syncs its environment when the vault is unlocked.
The first sync opts that folder into automatic local snapshots. No existing project is scanned until then.

Dev Console reads regular UTF-8 `.env` and `.env.*` files directly inside the selected
folder. It does not scan subfolders or follow file symlinks. For a monorepo, choose
additional app folders individually. Each file keeps its exact text, including
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
