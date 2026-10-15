<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import type {
    ArchiveSummary,
    BackupInfo,
    ClipboardExclusion,
    RecoveryKeyInfo,
    Settings
  } from '../lib/models';
  import { absoluteTime } from '../lib/time';
  import { formatArgv, parseArgv } from '../lib/argv';
  import { applyTheme } from '../lib/state.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let settings = $state<Settings | null>(null);
  let exclusion = $state<ClipboardExclusion | null>(null);
  let backupDir = $state('');
  let currentCommands = $state(0);
  let currentSecrets = $state(0);
  let error = $state<string | null>(null);
  let saved = $state(false);

  let backups = $state<BackupInfo[]>([]);
  let backupBusy = $state(false);
  let backupMessage = $state<string | null>(null);
  let backupError = $state<string | null>(null);

  let exportPath = $state('');
  let exportConfirmPath = $state<string | null>(null);
  let importPath = $state('');

  let review = $state<null | { kind: 'backup' | 'file'; summary: ArchiveSummary }>(null);
  let preBackup = $state(true);

  let terminalText = $state('');
  let terminalTokens = $state<string[]>([]);
  let terminalError = $state<string | null>(null);

  let recovery = $state<RecoveryKeyInfo | null>(null);
  let recoveryError = $state<string | null>(null);
  let recoveryCopied = $state(false);

  async function refreshBackups(): Promise<void> {
    try {
      backups = await api.listBackups();
    } catch (problem: unknown) {
      backupError = String(problem);
    }
  }

  async function runBackup(action: () => Promise<string>, done: string): Promise<void> {
    backupBusy = true;
    backupError = null;
    backupMessage = null;
    try {
      await action();
      backupMessage = done;
      await refreshBackups();
    } catch (problem: unknown) {
      backupError = String(problem);
    } finally {
      backupBusy = false;
    }
  }

  function backUpNow(): void {
    void runBackup(() => api.createBackup(), 'Encrypted backup written.');
  }

  async function exportArchive(overwrite = false): Promise<void> {
    await runBackup(
      async () => {
        try {
          return String(await api.exportArchive(exportPath, overwrite));
        } catch (problem: unknown) {
          if (String(problem).includes('already exists')) {
            exportConfirmPath = exportPath;
            revealCard('export-overwrite-card');
          }
          throw problem;
        }
      },
      `Archive exported to ${exportPath}.`
    );
    if (backupMessage) exportConfirmPath = null;
  }

  function revealCard(testId: string): void {
    // Confirmation cards render at the end of the scrollable body; make sure the user
    // actually sees (and can immediately operate with the keyboard) the card they asked for.
    requestAnimationFrame(() => {
      const card = document.querySelector<HTMLElement>(`[data-testid="${testId}"]`);
      card?.scrollIntoView({ block: 'center' });
      card?.querySelector<HTMLElement>('button')?.focus();
    });
  }

  async function reviewRestore(name: string): Promise<void> {
    backupError = null;
    try {
      review = { kind: 'backup', summary: await api.inspectBackup(name) };
      revealCard('review-card');
    } catch (problem: unknown) {
      backupError = String(problem);
    }
  }

  async function reviewImport(): Promise<void> {
    backupError = null;
    try {
      review = { kind: 'file', summary: await api.inspectArchive(importPath) };
      revealCard('review-card');
    } catch (problem: unknown) {
      backupError = String(problem);
    }
  }

  async function confirmApply(): Promise<void> {
    if (!review) return;
    const target = review;
    review = null;
    backupBusy = true;
    backupError = null;
    try {
      if (preBackup) await api.createBackup();
      const outcome =
        target.kind === 'backup'
          ? await api.restoreBackup(target.summary.source)
          : await api.importArchive(target.summary.source);
      backupMessage = `Replaced the library with ${target.summary.source} (${outcome.commands} commands, ${outcome.secrets} secrets)${preBackup ? ' - your previous data was backed up first' : ''}.`;
      await refreshBackups();
    } catch (problem: unknown) {
      backupError = String(problem);
    } finally {
      backupBusy = false;
    }
  }

  async function revealRecovery(): Promise<void> {
    recoveryError = null;
    try {
      recovery = await api.recoveryKey();
    } catch (problem: unknown) {
      recoveryError = String(problem);
    }
  }

  async function copyRecovery(): Promise<void> {
    if (!recovery) return;
    await navigator.clipboard.writeText(recovery.secret);
    recoveryCopied = true;
    setTimeout(() => (recoveryCopied = false), 4000);
  }

  function setTerminal(value: string): void {
    terminalText = value;
    terminalError = null;
    try {
      terminalTokens = parseArgv(value);
      if (settings) settings = { ...settings, terminal: terminalTokens };
    } catch (problem: unknown) {
      terminalTokens = [];
      terminalError = String(problem).replace('Error: ', '');
    }
  }

  onMount(() => {
    void Promise.all([api.settings(), api.appState(), api.list({ kind: 'command' }), api.list({ kind: 'secret' })])
      .then(([loaded, state, commands, secrets]) => {
        settings = loaded;
        exclusion = state.clipboardExclusion;
        backupDir = state.backupDir;
        currentCommands = commands.length;
        currentSecrets = secrets.length;
        terminalText = formatArgv(loaded.terminal);
        terminalTokens = loaded.terminal;
        void refreshBackups();
      })
      .catch((problem: unknown) => (error = String(problem)));
  });

  async function save(): Promise<void> {
    if (!settings || terminalError) return;
    error = null;
    try {
      settings = await api.saveSettings(settings);
      applyTheme(settings.theme);
      saved = true;
    } catch (problem: unknown) {
      error = String(problem);
    }
  }

  const reviewVerb = $derived(review?.kind === 'backup' ? 'Restore' : 'Import');
</script>

<div class="backdrop" role="dialog" aria-modal="true" aria-label="Settings">
  <div class="panel" data-testid="settings-panel">
    <header class="head">
      <h3>Settings</h3>
      <button onclick={onClose} data-testid="close-settings">Close</button>
    </header>

    <div class="body">
      {#if settings}
        <section aria-label="Appearance and startup">
          <h4>General</h4>
          <div class="grid">
            <label class="field">
              <span class="label-text">Theme</span>
              <select bind:value={settings.theme}>
                <option value="system">Follow system</option>
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </select>
            </label>
            <label class="field">
              <span class="label-text">Clipboard expiry (seconds)</span>
              <input
                type="number"
                min="5"
                max="600"
                bind:value={settings.clipboardExpirySeconds}
                data-testid="clipboard-expiry"
              />
            </label>
          </div>
          <label class="row">
            <input type="checkbox" bind:checked={settings.autostart} data-testid="autostart" />
            <span>Start at login (hidden in the tray)</span>
          </label>
          <div class="field">
            <span class="label-text">Terminal command (quotes kept)</span>
            <input
              class="mono"
              value={terminalText}
              oninput={(e) => setTerminal(e.currentTarget.value)}
              aria-describedby="terminal-preview"
              data-testid="terminal"
            />
            {#if terminalError}
              <p class="field-error" role="alert" data-testid="terminal-error">{terminalError}</p>
            {:else if terminalTokens.length}
              <p class="muted detail" id="terminal-preview" data-testid="terminal-tokens">
                Runs: {#each terminalTokens as token, i (i)}<code class="chip mono">{token}</code>{/each}
              </p>
            {/if}
          </div>
          <p class="muted detail" data-testid="clipboard-exclusion">
            {#if exclusion}
              Clipboard history exclusion: {exclusion.mechanism} - {exclusion.detail}
            {:else}
              Checking clipboard history support…
            {/if}
          </p>
        </section>
      {:else}
        <p class="muted">Loading…</p>
      {/if}

      <section aria-label="Backups and archive files">
        <h4>Backups</h4>
        <p class="muted detail">
          Archives are encrypted to <strong>two</strong> credentials: this machine's vault key
          and your recovery credential below. They open with either one, so a lost keyring
          never loses a backup. The newest 8 backups are kept; restoring or importing replaces
          everything and is always confirmed against the archive's own contents first.
        </p>
        <p class="detail path-line" data-testid="backup-dir">
          Backup folder: <code class="mono wrap">{backupDir || '…'}</code>
        </p>
        <div class="row">
          <button onclick={backUpNow} disabled={backupBusy} data-testid="backup-now">Back up now</button>
          <span class="spacer"></span>
          <span class="muted detail" data-testid="backup-count">{backups.length} of 8 kept</span>
        </div>
        {#if backups.length}
          <ul class="backup-list" data-testid="backup-list">
            {#each backups as item (item.name)}
              <li>
                <span class="mono ellip" title={item.name}>{item.name}</span>
                <span class="muted nowrap">{absoluteTime(new Date(item.modifiedAtMs).toISOString())}</span>
                <span class="muted nowrap">{Math.round(item.size / 1024)} KB</span>
                <button onclick={() => void reviewRestore(item.name)} disabled={backupBusy} data-testid="restore-backup">
                  Review &amp; restore
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted detail" data-testid="backup-empty">No backups yet.</p>
        {/if}
      </section>

      <section aria-label="Export">
        <h4>Export a portable archive</h4>
        <p class="muted detail">
          Writes <strong>one new encrypted file</strong> containing commands and secrets. An
          existing file is never overwritten without asking first.
        </p>
        <div class="row">
          <input
            class="mono grow"
            placeholder="/path/to/dev-console-export.age"
            bind:value={exportPath}
            aria-label="Export destination file"
            data-testid="archive-path"
          />
          <button onclick={() => void exportArchive(false)} disabled={backupBusy || !exportPath} data-testid="export-archive">
            Export
          </button>
        </div>
        {#if exportConfirmPath}
          <div class="confirm" role="alertdialog" aria-label="Confirm replacing the existing export" data-testid="export-overwrite-card">
            <p><code class="mono wrap">{exportConfirmPath}</code> already exists. Nothing has been changed.</p>
            <div class="row">
              <span class="spacer"></span>
              <button onclick={() => (exportConfirmPath = null)} data-testid="export-overwrite-cancel">Keep both</button>
              <button data-variant="danger" onclick={() => void exportArchive(true)} data-testid="export-overwrite-confirm">
                Replace it
              </button>
            </div>
          </div>
        {/if}
      </section>

      <section aria-label="Import">
        <h4>Import an archive file</h4>
        <p class="muted detail">
          Restore a file you exported or copied from another machine (its recovery credential
          must match). You will see what it contains and confirm before anything is replaced.
        </p>
        <div class="row">
          <input
            class="mono grow"
            placeholder="/path/to/dev-console-export.age"
            bind:value={importPath}
            aria-label="Archive file to import"
            data-testid="import-path"
          />
          <button onclick={() => void reviewImport()} disabled={backupBusy || !importPath} data-testid="import-archive">
            Review &amp; import
          </button>
        </div>
      </section>

      <section aria-label="Recovery credential">
        <h4>Recovery credential</h4>
        <p class="muted detail">
          If this machine's keyring is lost or you move to a new computer, this credential is
          the only way to open existing backups and exports. Store a copy somewhere separate
          from this machine. Anyone who holds it can read every archive you create from now on.
        </p>
        {#if recovery}
          <p class="mono recovery wrap" data-testid="recovery-secret" title="Recovery credential">{recovery.secret}</p>
          <p class="muted detail">Saved at <code class="mono wrap">{recovery.path}</code></p>
          <div class="row">
            <button onclick={() => void copyRecovery()} data-testid="recovery-copy">{recoveryCopied ? 'Copied' : 'Copy to clipboard'}</button>
          </div>
          <p class="muted detail" data-testid="recovery-steps">
            On a fresh install: unlock the (empty) vault once, put this credential in
            <code class="mono">~/.config/dev-console/recovery.agekey</code> (mode 0600), restart
            Dev Console, then use Import above.
          </p>
        {:else}
          <div class="row">
            <button onclick={() => void revealRecovery()} data-testid="recovery-reveal">Show recovery credential</button>
            <span class="muted detail">Hidden by default - it decrypts every archive.</span>
          </div>
        {/if}
        {#if recoveryError}<p class="error-banner" role="alert" data-testid="recovery-error">{recoveryError}</p>{/if}
      </section>

      {#if review}
        <div class="confirm review" role="alertdialog" aria-label="Confirm replacing the library" data-testid="review-card">
          <h5 data-testid="review-title">{reviewVerb} this archive?</h5>
          <dl class="summary">
            <dt>Created</dt><dd>{absoluteTime(review.summary.createdAt)}</dd>
            <dt>Contains</dt><dd>{review.summary.commands} commands · {review.summary.secrets} secrets</dd>
            <dt>Currently</dt><dd data-testid="review-current">{currentCommands} commands · {currentSecrets} secrets</dd>
            <dt>File</dt><dd class="mono wrap">{review.summary.source}</dd>
          </dl>
          <p class="warning" data-testid="review-warning">
            This <strong>replaces your entire library</strong>: every command and secret above is
            swapped for the archive's contents. Entries that only exist here are gone afterwards.
          </p>
          <label class="row">
            <input type="checkbox" bind:checked={preBackup} data-testid="prebackup" />
            <span>Back up the current data first (recommended)</span>
          </label>
          <div class="row">
            <span class="spacer"></span>
            <button onclick={() => (review = null)} data-testid="review-cancel">Cancel</button>
            <button data-variant="danger" onclick={() => void confirmApply()} data-testid="review-confirm">
              {reviewVerb} and replace
            </button>
          </div>
        </div>
      {/if}

      {#if backupBusy}<p class="muted" role="status">Working…</p>{/if}
      {#if backupMessage}<p class="muted" role="status" data-testid="backup-message">{backupMessage}</p>{/if}
      {#if backupError}<p class="error-banner" role="alert" data-testid="backup-error">{backupError}</p>{/if}
      {#if error}<p class="error-banner" role="alert">{error}</p>{/if}
      {#if saved}<p class="muted" role="status">Saved.</p>{/if}
    </div>

    <footer class="foot">
      <span class="spacer"></span>
      <button onclick={() => void save()} data-variant="primary" data-testid="save-settings" disabled={Boolean(terminalError)}>
        Save
      </button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 45%);
    display: grid;
    place-items: center;
    padding: 16px;
  }
  .panel {
    width: min(780px, 100%);
    max-height: min(100%, 92vh);
    background: var(--bg-panel);
    border: var(--border);
    border-radius: var(--radius);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .head,
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    flex: none;
  }
  .head {
    border-bottom: var(--border);
  }
  .foot {
    border-top: var(--border);
  }
  .head h3 {
    margin: 0;
    flex: 1;
  }
  .body {
    overflow-y: auto;
    padding: 4px 16px 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding-top: 8px;
  }
  h4 {
    margin: 0;
    font-size: 13px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--fg-muted);
  }
  h5 {
    margin: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .label-text {
    color: var(--fg-muted);
    font-size: 12px;
  }
  .detail {
    font-size: 12px;
    margin: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .spacer {
    flex: 1;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .nowrap {
    white-space: nowrap;
  }
  .ellip {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .wrap {
    overflow-wrap: anywhere;
  }
  .chip {
    border: var(--border);
    border-radius: var(--radius);
    padding: 0 5px;
    margin-right: 4px;
    font-size: 11px;
  }
  .field-error {
    color: var(--danger);
    font-size: 12px;
    margin: 0;
  }
  .path-line code {
    background: var(--bg-sunken, rgb(0 0 0 / 18%));
    padding: 1px 6px;
    border-radius: var(--radius);
  }
  .backup-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 150px;
    overflow: auto;
  }
  .backup-list li {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .recovery {
    margin: 0;
    padding: 8px;
    border: var(--border);
    border-radius: var(--radius);
    background: rgb(0 0 0 / 22%);
    font-size: 12px;
    user-select: all;
  }
  .confirm {
    border: 1px solid var(--warning, #b8860b);
    border-radius: var(--radius);
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .summary {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 12px;
    margin: 0;
    font-size: 12px;
  }
  .summary dt {
    color: var(--fg-muted);
  }
  .summary dd {
    margin: 0;
  }
  .warning {
    margin: 0;
    font-size: 12px;
    color: var(--warning, #d9a441);
  }
  @media (max-width: 720px) {
    .grid {
      grid-template-columns: 1fr;
    }
    .backup-list li {
      flex-wrap: wrap;
    }
  }
</style>
