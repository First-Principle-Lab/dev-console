<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import type { ClipboardExclusion, Settings } from '../lib/models';
  import { applyTheme } from '../lib/state.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let settings = $state<Settings | null>(null);
  let exclusion = $state<ClipboardExclusion | null>(null);
  let error = $state<string | null>(null);
  let saved = $state(false);

  onMount(() => {
    void Promise.all([api.settings(), api.appState()])
      .then(([loaded, state]) => {
        settings = loaded;
        exclusion = state.clipboardExclusion;
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
</style>
