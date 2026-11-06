<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { on } from '../lib/events';
  import type {
    ArchiveSummary,
    BackupInfo,
    ClipboardExclusion,
    RecoveryKeyInfo,
    Settings,
    TerminalOption,
    Theme,
    VaultState
  } from '../lib/models';
  import { absoluteTime } from '../lib/time';
  import { formatArgv, parseArgv } from '../lib/argv';
  import { applyTheme } from '../lib/state.svelte';
  import { OMARCHY_THEMES } from '../lib/models';
  import { checkForUpdates, installUpdate, updates, updatesSupported } from '../lib/updates.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let settings = $state<Settings | null>(null);
  let exclusion = $state<ClipboardExclusion | null>(null);
  let backupDir = $state('');
  let currentCommands = $state(0);
  // null means "not readable": a locked vault reports no secret count at all rather than 0.
  let currentSecrets = $state<number | null>(null);
  let error = $state<string | null>(null);
  let saved = $state(false);
  let savedTheme: Theme | null = null;

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
  let terminals = $state<TerminalOption[]>([]);
  let terminalChoice = $state('custom');

  let recovery = $state<RecoveryKeyInfo | null>(null);
  let recoveryError = $state<string | null>(null);
  let recoveryCopied = $state(false);
  let recoveryCopyExpiresAt = $state<number | null>(null);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  // Bumped by every lock while this panel is open, so a reveal that was already in flight
  // when the vault shut cannot paint its answer afterwards.
  let lockEpoch = 0;

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
    const epoch = lockEpoch;
    try {
      const revealed = await api.recoveryKey();
      if (epoch !== lockEpoch) return;
      recovery = revealed;
    } catch (problem: unknown) {
      recovery = null;
      recoveryError = String(problem);
    }
  }

  /// Forgets the revealed credential outright. Called the moment the vault locks: what the
  /// vault authorised cannot outlive the vault that authorised it.
  function forgetRecovery(): void {
    recovery = null;
    recoveryCopied = false;
    recoveryCopyExpiresAt = null;
    if (copyTimer) {
      clearTimeout(copyTimer);
      copyTimer = undefined;
    }
  }

  /// Copies through the backend, so the configured expiry and the lock-time clipboard wipe
  /// apply exactly as they do for a saved secret. The value itself never reaches this
  /// component, so it cannot be logged or shown in a message.
  async function copyRecovery(): Promise<void> {
    if (!recovery) return;
    recoveryError = null;
    try {
      const result = await api.copyRecoveryKey();
      recoveryCopied = true;
      recoveryCopyExpiresAt = result.expiresAtMs;
      if (copyTimer) clearTimeout(copyTimer);
      const delay = Math.min(Math.max(result.expiresAtMs - Date.now(), 0), 2_147_483_647);
      copyTimer = setTimeout(() => {
        recoveryCopied = false;
        recoveryCopyExpiresAt = null;
      }, delay);
    } catch (problem: unknown) {
      recoveryError = String(problem);
    }
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

  function chooseTerminal(id: string): void {
    terminalChoice = id;
    const terminal = terminals.find((option) => option.id === id);
    if (terminal) setTerminal(formatArgv(terminal.command));
    saved = false;
  }

  function matchesTerminal(command: string[], option: TerminalOption): boolean {
    return command.length === option.command.length
      && command[0]?.split('/').pop() === option.id
      && command.slice(1).every((part, index) => part === option.command[index + 1]);
  }

  /// Counts are a separate, best-effort load: the secret list legitimately fails while the
  /// vault is locked and must never take the General controls down with it.
  async function refreshCounts(): Promise<void> {
    try {
      currentCommands = (await api.list({ kind: 'command' })).length;
    } catch {
      currentCommands = 0;
    }
    try {
      currentSecrets = (await api.list({ kind: 'secret' })).length;
    } catch {
      currentSecrets = null;
    }
  }

  async function loadGeneral(): Promise<void> {
    error = null;
    try {
      const [loaded, appState, foundTerminals] = await Promise.all([api.settings(), api.appState(), api.terminals()]);
      savedTheme = loaded.theme;
      settings = loaded;
      exclusion = appState.clipboardExclusion;
      backupDir = appState.backupDir;
      terminalText = formatArgv(loaded.terminal);
      terminalTokens = loaded.terminal;
      terminals = foundTerminals;
      terminalChoice = foundTerminals.find((option) => matchesTerminal(loaded.terminal, option))?.id ?? 'custom';
      await refreshBackups();
      await refreshCounts();
    } catch (problem: unknown) {
      error = String(problem);
    }
  }

  onMount(() => {
    // Settings stays in step with the vault while it is open: an unlock refreshes the counts,
    // a lock drops both the count and anything this panel is currently showing the user.
    const unsubscribe = on<VaultState>('vault-state-changed', (state: VaultState) => {
      if (state.unlocked) {
        void refreshCounts();
        return;
      }
      lockEpoch += 1;
      forgetRecovery();
      currentSecrets = null;
    });
    void loadGeneral();
    return () => {
      unsubscribe();
      if (copyTimer) clearTimeout(copyTimer);
      if (savedTheme) applyTheme(savedTheme);
    };
  });

  function previewTheme(theme: Theme): void {
    if (!settings) return;
    settings.theme = theme;
    applyTheme(theme);
    saved = false;
  }

  async function save(): Promise<void> {
    if (!settings || terminalError) return;
    error = null;
    try {
      settings = await api.saveSettings(settings);
      savedTheme = settings.theme;
      applyTheme(settings.theme);
      saved = true;
    } catch (problem: unknown) {
      error = String(problem);
    }
  }

  const reviewVerb = $derived(review?.kind === 'backup' ? 'Restore' : 'Import');
  const copyExpiryLabel = $derived(
    recoveryCopyExpiresAt === null ? '' : new Date(recoveryCopyExpiresAt).toLocaleTimeString()
  );
</script>

<div class="backdrop" role="dialog" aria-modal="true" aria-label="Settings">
  <div class="sheet surface-shell" data-testid="settings-panel">
    <header class="head">
      <h3>Settings</h3>
      <button onclick={onClose} data-testid="close-settings">Close</button>
    </header>

    <div class="body">
      <section aria-label="Updates" data-testid="updates">
        <h4>Updates</h4>
        {#if updatesSupported()}
          <div class="row">
            <span data-testid="current-version">Dev Console {updates.current ?? '…'}</span>
            <span class="spacer"></span>
            {#if updates.available}
              <button
                type="button"
                data-variant="primary"
                onclick={() => void installUpdate()}
                disabled={updates.status === 'installing'}
                data-testid="settings-install-update"
              >
                {updates.status === 'installing' ? 'Installing…' : `Update to ${updates.available.version} and restart`}
              </button>
            {:else}
              <button
                type="button"
                onclick={() => void checkForUpdates()}
                disabled={updates.status === 'checking'}
                data-testid="check-updates"
              >
                {updates.status === 'checking' ? 'Checking…' : 'Check for updates'}
              </button>
            {/if}
          </div>
          <p class="muted detail" role="status" data-testid="update-status">
            {#if updates.status === 'checking'}
              Checking for a newer release…
            {:else if updates.status === 'installing'}
              Downloading and verifying {updates.available?.version}. Dev Console restarts when it is done.
            {:else if updates.status === 'available'}
              Version {updates.available?.version} is available. Updating replaces the AppImage and restarts the app.
            {:else if updates.status === 'current'}
              You're on the latest version.
            {:else if updates.status !== 'error'}
              Dev Console checks for updates when it starts and every 15 minutes.
            {/if}
          </p>
          {#if updates.status === 'error'}
            <p class="error-banner" role="alert" data-testid="update-error">{updates.error}</p>
          {/if}
        {:else}
          <p class="muted detail" data-testid="update-status">Updates are only available in the installed app.</p>
        {/if}
      </section>

      {#if settings}
        <section aria-label="Appearance and startup">
          <h4>General</h4>
          <div class="grid">
            <label class="field">
              <span class="label-text">Theme</span>
              <select bind:value={settings.theme} onchange={(event) => previewTheme(event.currentTarget.value as Theme)}>
                <option value="system">Follow system</option>
                <option value="light">Light</option>
                <option value="dark">Dark</option>
                {#each OMARCHY_THEMES as [slug, name] (slug)}
                  <option value={`omarchy-${slug}`}>{name}</option>
                {/each}
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
            <label class="label-text" for="terminal-choice">Terminal</label>
            <select id="terminal-choice" value={terminalChoice} onchange={(event) => chooseTerminal(event.currentTarget.value)} data-testid="terminal-choice">
              {#each terminals as terminal (terminal.id)}
                <option value={terminal.id}>{terminal.name}</option>
              {/each}
              <option value="custom">Custom command</option>
            </select>
            {#if terminals.length === 0}
              <p class="muted detail">No supported terminal was found in PATH. Enter a custom command below.</p>
            {/if}
            {#if terminalChoice === 'custom'}
              <label class="label-text" for="terminal-command">Terminal command (quotes kept)</label>
              <input
                id="terminal-command"
                class="mono"
                value={terminalText}
                oninput={(e) => setTerminal(e.currentTarget.value)}
                aria-describedby="terminal-preview"
                data-testid="terminal"
              />
            {/if}
            {#if terminalError}
              <p class="field-error" role="alert" data-testid="terminal-error">{terminalError}</p>
            {:else if terminalTokens.length}
              <p class="muted detail tokens" id="terminal-preview" data-testid="terminal-tokens">
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
            <span class="spacer"></span>
            <span class="muted detail" data-testid="recovery-copy-note">
              {#if recoveryCopied}
                Copied through the app: it clears at {copyExpiryLabel} with the clipboard
                expiry, and as soon as the vault locks.
              {:else}
                Copies through the app, with the same expiry as a saved secret.
              {/if}
            </span>
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
            <dt>Currently</dt>
            <dd data-testid="review-current">
              {currentCommands} commands ·
              {#if currentSecrets === null}
                secret count unavailable while the vault is locked
              {:else}
                {currentSecrets} secrets
              {/if}
            </dd>
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
    background: var(--backdrop);
    display: grid;
    place-items: center;
    padding: var(--sp-4);
  }
  /* Settings is a floating surface, so it takes the one shell definition the delete
     dialog and the palette use (fill, edge, radius, elevation) from tokens.css.
     Nothing is painted by `sheet` itself: the shared frame does that, and repeating
     the fill here would win on specificity and freeze a second frame. */
  .sheet {
    width: min(780px, 100%);
    max-height: min(100%, 92vh);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .head,
  .foot {
    display: flex;
    align-items: center;
    gap: var(--gap);
    padding: var(--sp-3) var(--sp-4);
    flex: none;
  }
  /* hairlines, not frames: the shell edge already draws the outline, so a second
     1px line under the header and over the footer would just thicken it */
  .head {
    border-bottom: var(--border-hair);
  }
  .foot {
    border-top: var(--border-hair);
  }
  .head h3 {
    margin: 0;
    flex: 1;
  }
  .body {
    overflow-y: auto;
    /* min-height: 0 keeps the body scrollable inside the flex column, so the footer and every
       confirmation action stay reachable in a short window. */
    min-height: 0;
    padding: var(--sp-1) var(--sp-4) var(--sp-4);
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    padding-top: var(--sp-2);
  }
  /* the shared group-header recipe, identical to the editor's fieldset legends */
  h4 {
    margin: 0;
    font-size: var(--text-sm);
    letter-spacing: var(--label-tracking);
    text-transform: uppercase;
    color: var(--fg-muted);
  }
  h5 {
    margin: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--gap);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
  }
  .label-text {
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .detail {
    font-size: var(--text);
    line-height: 1.5;
    margin: 0;
    max-width: 74ch;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
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
  /* no local .chip rule: the argv tokens render through the shared chip pill in
     tokens.css, the same one the empty library uses for its shortcut hint. Inline
     pills need the same inter-item rhythm the entry-list metadata line uses. */
  .tokens code + code {
    margin-left: var(--gap-tags);
  }
  .field-error {
    color: var(--danger);
    font-size: var(--text-sm);
    margin: 0;
  }
  .path-line code {
    background: var(--bg-sunken);
    padding: var(--gap-xs) var(--gap-tags);
    border-radius: var(--radius);
  }
  .backup-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    max-height: 150px;
    overflow: auto;
  }
  .backup-list li {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    font-size: var(--text-sm);
  }
  /* the recovery block and every confirm card are inset surfaces: same fill and
     hairline as the palette's locked note, one warn frame token */
  .recovery {
    margin: 0;
    padding: var(--sp-2);
    border: var(--border-hair);
    border-radius: var(--radius);
    background: var(--inset-fill);
    font-size: var(--text);
    line-height: 1.5;
    user-select: all;
  }
  .confirm {
    border: var(--border-warn);
    border-radius: var(--radius);
    padding: var(--sp-3);
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .summary {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--sp-1) var(--sp-3);
    margin: 0;
    font-size: var(--text);
  }
  .summary dt {
    color: var(--fg-muted);
  }
  .summary dd {
    margin: 0;
  }
  .warning {
    margin: 0;
    font-size: var(--text);
    line-height: 1.5;
    color: var(--warn);
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
