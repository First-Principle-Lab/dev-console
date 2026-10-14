<script lang="ts">
  import { onMount } from 'svelte';
  import EntryList from '../components/EntryList.svelte';
  import EntryDetails from '../components/EntryDetails.svelte';
  import EntryEditor from '../components/EntryEditor.svelte';
  import VaultStatus from '../components/VaultStatus.svelte';
  import SettingsPanel from '../components/SettingsPanel.svelte';
  import { api } from '../lib/api';
  import { on } from '../lib/events';
  import { EMPTY_DRAFT, draftFromDetails } from '../lib/models';
  import type {
    AppStateInfo,
    EntriesChanged,
    EntryDetails as Details,
    EntryDraft,
    EntryMeta,
    Kind,
    VaultState,
    WindowIntent
  } from '../lib/models';
  import { validateDraft } from '../lib/validation';

  let app = $state<AppStateInfo | null>(null);
  let vault = $state<VaultState | null>(null);
  let tab = $state<Kind>('command');
  let query = $state('');
  let entries = $state<EntryMeta[]>([]);
  let typeSuggestions = $state<string[]>([]);
  let selectedId = $state<string | null>(null);
  let details = $state<Details | null>(null);
  let revealed = $state(false);
  let loading = $state(true);
  let listError = $state<string | null>(null);
  let mode = $state<'view' | 'edit'>('view');
  let draft = $state<EntryDraft>(EMPTY_DRAFT('command'));
  let baseline = $state('');
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let status = $state<string | null>(null);
  let pendingDelete = $state<EntryMeta | null>(null);
  let pendingRun = $state<Details | null>(null);
  let pendingClose = $state(false);
  let showSettings = $state(false);
  let narrow = $state(false);
  let authenticating = $state(false);

  const draftDirty = $derived(JSON.stringify(draft) !== baseline);
  const meta = $derived(entries.find((e) => e.id === selectedId) ?? null);

  function message(error: unknown, fallback: string): string {
    return error instanceof Error && error.message.length > 0 ? error.message : fallback;
  }

  async function refreshVault(): Promise<void> {
    app = await api.appState();
    vault = app.vault;
  }

  async function loadTypes(): Promise<void> {
    typeSuggestions = await api.types(tab);
  }

  async function loadList(): Promise<void> {
    loading = true;
    listError = null;
    try {
      entries = await api.list({ kind: tab, query: query.trim().length > 0 ? query : undefined });
      if (selectedId && !entries.some((e) => e.id === selectedId)) {
        selectedId = entries[0]?.id ?? null;
      } else if (!selectedId && entries.length > 0) {
        selectedId = entries[0].id;
      }
      if (selectedId) await loadDetails(selectedId);
      else details = null;
    } catch (error: unknown) {
      listError = message(error, 'Could not load entries.');
      entries = [];
      details = null;
    } finally {
      loading = false;
    }
  }

  async function loadDetails(id: string): Promise<void> {
    revealed = false;
    try {
      details = await api.details(tab, id);
      baseline = JSON.stringify(draftFromDetails(details, tab));
      if (mode === 'edit') mode = 'view';
    } catch (error: unknown) {
      listError = message(error, 'Could not load the selected entry.');
      details = null;
    }
  }

  function selectTab(next: Kind): void {
    if (tab === next) return;
    if (mode === 'edit' && draftDirty) {
      status = 'Save or discard the open edit first.';
      return;
    }
    tab = next;
    selectedId = null;
    details = null;
    void Promise.all([loadList(), loadTypes()]).catch((error: unknown) =>
      console.error('tab load failed', error)
    );
  }

  async function select(entry: EntryMeta): Promise<void> {
    if (mode === 'edit' && draftDirty) {
      status = 'Save or discard the open edit first.';
      return;
    }
    mode = 'view';
    selectedId = entry.id;
    await loadDetails(entry.id);
  }

  function startNew(): void {
    mode = 'edit';
    selectedId = null;
    details = null;
    draft = EMPTY_DRAFT(tab);
    baseline = JSON.stringify(draft);
    saveError = null;
  }

  async function startEdit(): Promise<void> {
    if (!details || !selectedId) return;
    let base = details;
    if (tab === 'secret') {
      try {
        base = await api.reveal(tab, selectedId);
      } catch (error: unknown) {
        saveError = message(error, 'Unlock the vault to edit this secret.');
        return;
      }
    }
    draft = draftFromDetails(base, tab);
    baseline = JSON.stringify(draft);
    saveError = null;
    status = null;
    revealed = tab === 'secret';
    mode = 'edit';
  }

  function cancelEdit(): void {
    draft.value = '';
    revealed = false;
    mode = 'view';
    saveError = null;
    if (details) baseline = JSON.stringify(draftFromDetails(details, tab));
  }

  async function saveDraft(): Promise<void> {
    const problem = validateDraft(draft);
    if (problem) {
      saveError = problem;
      return;
    }
    saving = true;
    saveError = null;
    try {
      const saved = await api.save(draft);
      draft.value = '';
      revealed = false;
      mode = 'view';
      selectedId = saved.id;
      status = 'Saved.';
      await loadList();
      await loadTypes();
    } catch (error: unknown) {
      // The draft is intentionally preserved so nothing typed is lost on a failed write.
      saveError = message(error, 'Save failed. Your changes were kept.');
    } finally {
      saving = false;
    }
  }

  async function togglePin(): Promise<void> {
    if (!details) return;
    const current = draftFromDetails(details, tab);
    current.pinned = !details.meta.pinned;
    try {
      await api.save(current);
      await loadList();
    } catch (error: unknown) {
      status = message(error, 'Pin failed.');
    }
  }

  async function copyEntry(): Promise<void> {
    if (!selectedId || !details) return;
    try {
      const result = await api.copy(tab, selectedId, details.meta.revision);
      const seconds = Math.max(1, Math.round((result.expiresAtMs - Date.now()) / 1000));
      status = result.excluded
        ? `Copied for about ${seconds}s (excluded from clipboard history).`
        : `Copied for about ${seconds}s. ${result.exclusion.detail}`;
    } catch (error: unknown) {
      status = message(error, 'Copy failed.');
    }
  }

  async function revealEntry(): Promise<void> {
    if (!selectedId) return;
    try {
      details = await api.reveal(tab, selectedId);
      revealed = true;
    } catch (error: unknown) {
      status = message(error, 'Reveal requires an unlocked vault.');
    }
  }

  async function confirmDelete(): Promise<void> {
    if (!pendingDelete) return;
    const target = pendingDelete;
    pendingDelete = null;
    try {
      await api.remove(target.kind, target.id, target.revision);
      if (selectedId === target.id) {
        selectedId = null;
        details = null;
      }
      status = `Deleted “${target.name}”.`;
      await loadList();
    } catch (error: unknown) {
      status = message(error, 'Delete failed.');
    }
  }

  async function confirmRun(): Promise<void> {
    if (!pendingRun) return;
    const target = pendingRun;
    pendingRun = null;
    try {
      await api.run(target.meta.id, target.meta.revision, true);
      status = 'Launched in a terminal.';
    } catch (error: unknown) {
      status = message(error, 'Execution failed.');
    }
  }

  async function unlock(): Promise<void> {
    authenticating = true;
    status = null;
    try {
      vault = await api.unlockVault();
      await loadList();
    } catch (error: unknown) {
      vault = await api.appState().then((a) => a.vault);
      status = message(error, 'Unlock was cancelled.');
    } finally {
      authenticating = false;
    }
  }

  async function allowClose(): Promise<void> {
    revealed = false;
    draft.value = '';
    await api.allowClose();
  }

  onMount(() => {
    const media = window.matchMedia('(max-width: 900px)');
    narrow = media.matches;
    const onResize = (event: MediaQueryListEvent): void => {
      narrow = event.matches;
    };
    media.addEventListener('change', onResize);

    const unsubscribers = [
      on<EntriesChanged>('entries-changed', (payload) => {
        if (payload.kind === tab) void loadList().catch(() => undefined);
      }),
      on<VaultState>('vault-state-changed', (state) => {
        vault = state;
        if (!state.unlocked && tab === 'secret') {
          entries = [];
          details = null;
          selectedId = null;
          revealed = false;
          if (mode === 'edit' && draft.kind === 'secret') cancelEdit();
        }
      }),
      on<WindowIntent>('window-intent', (intent) => {
        tab = intent.kind;
        void Promise.all([loadList(), loadTypes()]).catch(() => undefined);
        if (intent.action === 'new-entry') startNew();
        else if (intent.entryId) {
          selectedId = intent.entryId;
          void loadDetails(intent.entryId).catch(() => undefined);
        }
      }),
      on<{ reason: string }>('close-requested', () => {
        if (mode === 'edit' && draftDirty) pendingClose = true;
        else void allowClose().catch(() => undefined);
      })
    ];

    void refreshVault()
      .then(() => Promise.all([loadList(), loadTypes()]))
      .catch((error: unknown) => {
        listError = message(error, 'Backend unavailable.');
      })
      .finally(() => (loading = false));

    return () => {
      media.removeEventListener('change', onResize);
      for (const unsubscribe of unsubscribers) unsubscribe();
    };
  });

  function onGlobalKeydown(event: KeyboardEvent): void {
    if (event.ctrlKey && event.key.toLowerCase() === 'n') {
      event.preventDefault();
      startNew();
    } else if (event.ctrlKey && event.key.toLowerCase() === 's') {
      event.preventDefault();
      if (mode === 'edit') void saveDraft();
    } else if (event.ctrlKey && event.key === '1') {
      event.preventDefault();
      selectTab('command');
    } else if (event.ctrlKey && event.key === '2') {
      event.preventDefault();
      selectTab('secret');
    } else if (event.key === 'Escape' && pendingDelete) {
      pendingDelete = null;
    } else if (event.key === 'Escape' && pendingRun) {
      pendingRun = null;
    }
  }
</script>

<svelte:window onkeydown={onGlobalKeydown} />

<div class="shell" class:narrow data-testid="library">
  <header>
    <strong>Dev Console</strong>
    <span class="spacer"></span>
    {#if vault}<VaultStatus {vault} onunlock={unlock} />{/if}
    {#if authenticating}<span class="muted">Waiting for authentication…</span>{/if}
    <button onclick={() => (showSettings = true)} data-testid="settings">Settings</button>
  </header>

  <div class="tabs">
    <div class="tabbar" role="tablist">
      <button
        role="tab"
        aria-selected={tab === 'command'}
        class:active={tab === 'command'}
        onclick={() => selectTab('command')}
        data-testid="tab-commands">Commands</button
      >
      <button
        role="tab"
        aria-selected={tab === 'secret'}
        class:active={tab === 'secret'}
        onclick={() => selectTab('secret')}
        data-testid="tab-secrets">Secrets</button
      >
    </div>
    <input
      class="filter"
      placeholder="Search this tab"
      aria-label="Search this tab"
      bind:value={query}
      oninput={() => void loadList().catch(() => undefined)}
      data-testid="tab-search"
    />
    <button onclick={startNew} data-variant="primary" data-testid="add-entry">+ Add</button>
  </div>

  {#if listError}
    <p class="error-banner" role="alert">{listError}</p>
  {:else if status}
    <p class="status muted" role="status" data-testid="status">{status}</p>
  {/if}

  <main class="panes" class:stacked={narrow}>
    {#if !narrow || mode === 'view'}
      <section class="list-pane" hidden={narrow && mode !== 'view' && selectedId !== null}>
        <EntryList
          {entries}
          {selectedId}
          {loading}
          locked={tab === 'secret' && vault ? !vault.unlocked : false}
          emptyText={query ? 'No entries match this filter.' : 'No entries yet. Press + Add.'}
          onSelect={(entry) => void select(entry)}
        />
      </section>
    {/if}

    <section class="detail-pane">
      {#if narrow && mode === 'view' && selectedId}
        <button onclick={() => (selectedId = null)}>← Back</button>
      {/if}
      {#if mode === 'edit'}
        <EntryEditor
          {draft}
          {typeSuggestions}
          {saving}
          error={saveError}
          onSave={() => void saveDraft()}
          onCancel={cancelEdit}
        />
      {:else if details}
        <EntryDetails
          {details}
          kind={tab}
          {revealed}
          busy={saving}
          oncopy={() => void copyEntry()}
          onreveal={() => void revealEntry()}
          onedit={() => void startEdit()}
          onpin={() => void togglePin()}
          onrun={() => { pendingRun = details; }}
          ondelete={() => (pendingDelete = meta)}
        />
      {:else if tab === 'secret' && vault && !vault.unlocked}
        <div class="panel-empty">
          <p class="muted">This vault is locked. Secrets stay encrypted on disk.</p>
          <button onclick={() => void unlock()} data-variant="primary" data-testid="unlock-vault"
            >Unlock vault</button
          >
        </div>
      {:else}
        <p class="muted pad">Select an entry.</p>
      {/if}
    </section>
  </main>
</div>

{#if pendingDelete}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-label="Confirm delete">
    <div class="modal">
      <h3>Delete “{pendingDelete.name}”?</h3>
      <p class="muted">This removes the saved {pendingDelete.kind} permanently.</p>
      <div class="row">
        <span class="spacer"></span>
        <button onclick={() => (pendingDelete = null)} data-testid="cancel-delete">Cancel</button>
        <button
          data-variant="danger"
          onclick={() => void confirmDelete()}
          data-testid="confirm-delete">Delete</button
        >
      </div>
    </div>
  </div>
{/if}

{#if pendingRun}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-label="Confirm run">
    <div class="modal">
      <h3>Run this command?</h3>
      <pre class="mono block">{pendingRun.command}</pre>
      {#if pendingRun.workingDirectory}
        <p class="muted">Working directory: <code>{pendingRun.workingDirectory}</code></p>
      {/if}
      <div class="row">
        <span class="spacer"></span>
        <button onclick={() => (pendingRun = null)}>Cancel</button>
        <button data-variant="primary" onclick={() => void confirmRun()} data-testid="confirm-run"
          >Run in terminal</button
        >
      </div>
    </div>
  </div>
{/if}

{#if pendingClose}
  <div class="modal-backdrop" role="dialog" aria-modal="true" aria-label="Unsaved changes">
    <div class="modal">
      <h3>Unsaved changes</h3>
      <p class="muted">This entry has edits that are not saved.</p>
      <div class="row">
        <span class="spacer"></span>
        <button onclick={() => (pendingClose = false)} data-testid="close-cancel">Cancel</button>
        <button
          onclick={() => {
            pendingClose = false;
            cancelEdit();
            void allowClose();
          }}
          data-testid="close-discard">Discard</button
        >
        <button
          data-variant="primary"
          onclick={() => {
            pendingClose = false;
            void saveDraft().then(() => allowClose());
          }}
          data-testid="close-save">Save</button
        >
      </div>
    </div>
  </div>
{/if}

{#if showSettings}
  <SettingsPanel onClose={() => (showSettings = false)} />
{/if}

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  header {
    display: flex;
    align-items: center;
    gap: var(--gap);
    padding: 9px 14px;
    border-bottom: var(--border);
    background: var(--bg-panel);
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: var(--gap);
    padding: 8px 14px;
    border-bottom: var(--border);
  }
  .tabbar {
    display: flex;
    gap: 4px;
  }
  .tabbar button {
    border-radius: var(--radius) var(--radius) 0 0;
    border-bottom-color: transparent;
  }
  .tabbar button.active {
    background: var(--bg-raised);
    border-bottom-color: var(--bg-raised);
    font-weight: 600;
  }
  .filter {
    flex: 1;
  }
  .status,
  .error-banner {
    margin: 0;
    padding: 6px 14px;
    border-bottom: var(--border);
  }
  .panes {
    flex: 1;
    display: grid;
    grid-template-columns: 320px 1fr;
    min-height: 0;
  }
  .panes.stacked {
    grid-template-columns: 1fr;
  }
  .list-pane,
  .detail-pane {
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }
  .detail-pane {
    background: var(--bg);
  }
  .panel-empty,
  .pad {
    padding: 22px;
  }
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 35%);
    display: grid;
    place-items: center;
  }
  .modal {
    min-width: 340px;
    max-width: 520px;
    background: var(--bg-panel);
    border: var(--border);
    border-radius: var(--radius);
    padding: 16px;
    box-shadow: 0 12px 36px rgb(0 0 0 / 30%);
  }
  .modal h3 {
    margin: 0 0 6px;
    word-break: break-word;
  }
  .block {
    background: var(--bg-raised);
    border: var(--border);
    border-radius: var(--radius);
    padding: 8px 10px;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
