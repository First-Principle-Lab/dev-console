<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import type { BackupInfo, ClipboardExclusion, Settings } from '../lib/models';
  import { applyTheme } from '../lib/state.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let settings = $state<Settings | null>(null);
  let exclusion = $state<ClipboardExclusion | null>(null);
  let error = $state<string | null>(null);
  let saved = $state(false);
  let backups = $state<BackupInfo[]>([]);
  let backupPath = $state('');
  let backupBusy = $state(false);
  let backupMessage = $state<string | null>(null);
  let backupError = $state<string | null>(null);

  async function refreshBackups(): Promise<void> {
    try {
      backups = await api.listBackups();
    } catch (problem: unknown) {
      backupError = String(problem);
    }
  }

  async function runBackup(action: () => Promise<string>, done: string): Promise<void> {
    if (!settings) return;
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
    void runBackup(() => api.createBackup().then((name) => name), 'Encrypted backup written.');
  }

  function exportArchive(): void {
    void runBackup(
      () => api.exportArchive(backupPath).then((bytes) => String(bytes)),
      `Archive exported to ${backupPath}.`
    );
  }

  function restore(name: string): void {
    void runBackup(
      () => api.restoreBackup(name).then((out) => `${out.commands} commands, ${out.secrets} secrets`),
      `Restored ${name}.`
    );
  }

  function importArchive(): void {
    void runBackup(
      () => api.importArchive(backupPath).then((out) => `${out.commands} commands, ${out.secrets} secrets`),
      `Imported ${backupPath}.`
    );
  }

  onMount(() => {
    void Promise.all([api.settings(), api.appState()])
      .then(([loaded, state]) => {
        settings = loaded;
        exclusion = state.clipboardExclusion;
        void refreshBackups();
      })
      .catch((problem: unknown) => (error = String(problem)));
  });

  function setTerminal(value: string): void {
    if (!settings) return;
    settings = { ...settings, terminal: value.split(' ').filter((part) => part.length > 0) };
  }

  async function save(): Promise<void> {
    if (!settings) return;
    error = null;
    try {
      settings = await api.saveSettings(settings);
      applyTheme(settings.theme);
      saved = true;
    } catch (problem: unknown) {
      error = String(problem);
    }
  }
</script>

<div class="backdrop" role="dialog" aria-modal="true" aria-label="Settings">
  <div class="panel" data-testid="settings-panel">
    <h3>Settings</h3>
    {#if settings}
      <label class="field">
        <span>Theme</span>
        <select bind:value={settings.theme}>
          <option value="system">Follow system</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </label>
      <label class="row">
        <input type="checkbox" bind:checked={settings.autostart} data-testid="autostart" />
        <span>Start at login (hidden in the tray)</span>
      </label>
      <label class="field">
        <span>Clipboard expiry (seconds)</span>
        <input
          type="number"
          min="5"
          max="600"
          bind:value={settings.clipboardExpirySeconds}
          data-testid="clipboard-expiry"
        />
      </label>
      <label class="field">
        <span>Terminal command (space separated)</span>
        <input
          class="mono"
          value={settings.terminal.join(' ')}
          oninput={(e) => setTerminal(e.currentTarget.value)}
          data-testid="terminal"
        />
      </label>
    {:else}
      <p class="muted">Loading…</p>
    {/if}

    <section class="backups" aria-label="Backups">
      <h4>Backups</h4>
      <p class="muted detail">
        Backups and exports are encrypted with this machine's vault key. The newest
        {backups.length ? backups.length : 0} are kept.
      </p>
      <div class="row">
        <button onclick={backUpNow} disabled={backupBusy} data-testid="backup-now">Back up now</button>
        <input
          class="mono"
          placeholder="/path/to/dev-console.age"
          bind:value={backupPath}
          aria-label="Archive path"
          data-testid="archive-path"
        />
        <button onclick={exportArchive} disabled={backupBusy || !backupPath} data-testid="export-archive">
          Export
        </button>
        <button onclick={importArchive} disabled={backupBusy || !backupPath} data-testid="import-archive">
          Import
        </button>
      </div>
      {#if backups.length}
        <ul class="backup-list" data-testid="backup-list">
          {#each backups as item (item.name)}
            <li>
              <span class="mono">{item.name}</span>
              <span class="muted">{Math.round(item.size / 1024)} KB</span>
              <button onclick={() => restore(item.name)} disabled={backupBusy} data-testid="restore-backup">
                Restore
              </button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="muted detail" data-testid="backup-empty">No backups yet.</p>
      {/if}
      {#if backupBusy}<p class="muted" role="status">Working…</p>{/if}
      {#if backupMessage}<p class="muted" role="status" data-testid="backup-message">{backupMessage}</p>{/if}
      {#if backupError}<p class="error-banner" role="alert" data-testid="backup-error">{backupError}</p>{/if}
    </section>

    <p class="muted detail" data-testid="clipboard-exclusion">
      {#if exclusion}
        Clipboard history exclusion: {exclusion.mechanism} — {exclusion.detail}
      {:else}
        Checking clipboard history support…
      {/if}
    </p>
    {#if error}<p class="error-banner" role="alert">{error}</p>{/if}
    {#if saved}<p class="muted" role="status">Saved.</p>{/if}

    <div class="row">
      <span class="spacer"></span>
      <button onclick={() => void save()} data-variant="primary" data-testid="save-settings">Save</button>
      <button onclick={onClose}>Close</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 35%);
    display: grid;
    place-items: center;
  }
  .panel {
    width: 420px;
    background: var(--bg-panel);
    border: var(--border);
    border-radius: var(--radius);
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h3 {
    margin: 0;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .field > span {
    color: var(--fg-muted);
    font-size: 12px;
  }
  .detail {
    font-size: 12px;
  }
  h4 {
    margin: 8px 0 0;
  }
  .backups {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .backup-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 140px;
    overflow: auto;
  }
  .backup-list li {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .backup-list span:first-child {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
