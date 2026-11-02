<script lang="ts">
  import { onMount } from 'svelte';
  import EntryList from '../components/EntryList.svelte';
  import EntryDetails from '../components/EntryDetails.svelte';
  import EntryEditor from '../components/EntryEditor.svelte';
  import VaultStatus from '../components/VaultStatus.svelte';
  import SettingsPanel from '../components/SettingsPanel.svelte';
  import Button from '../components/Button.svelte';
  import Dialog from '../components/Dialog.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import Icon from '../components/Icon.svelte';
  import Keycap from '../components/Keycap.svelte';
  import StatusLine from '../components/StatusLine.svelte';
  import Tabs from '../components/Tabs.svelte';
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
  let saveErrorDraft = $state('');
  let status = $state<string | null>(null);
  let pendingDelete = $state<EntryMeta | null>(null);
  let pendingRun = $state<Details | null>(null);
  let pendingClose = $state(false);
  let showSettings = $state(false);
  let narrow = $state(false);
  let authenticating = $state(false);
  let counts = $state<{ command: number; secret: number }>({ command: 0, secret: 0 });

  const draftDirty = $derived(JSON.stringify(draft) !== baseline);

  // A validation message describes the draft as it was when saving was attempted. Once the user
  // edits, keeping it on screen reads as if the new text is still wrong.
  $effect(() => {
    if (!saveError) return;
    if (JSON.stringify(draft) !== saveErrorDraft) saveError = null;
  });
  // A locked vault hides secret metadata. That is the designed state, never a failure: the list
  // pane shows the notice and the detail pane the single unlock action, so no banner is added.
  const secretMetadataHidden = $derived(tab === 'secret' && vault !== null && !vault.unlocked);
  const meta = $derived(entries.find((e) => e.id === selectedId) ?? null);

  function message(error: unknown, fallback: string): string {
    // Tauri rejects with the backend's message as a plain string (AppError serializes via
    // Display); without this branch every real failure would show the fallback instead.
    if (typeof error === 'string' && error.length > 0) return error;
    return error instanceof Error && error.message.length > 0 ? error.message : fallback;
  }

  async function refreshVault(): Promise<void> {
    app = await api.appState();
    vault = app.vault;
  }

  async function loadTypes(): Promise<void> {
    typeSuggestions = await api.types(tab);
  }

  /// Counts for the tab badges. Secret counts read metadata only while the vault is open; a
  /// locked vault reports nothing rather than guessing.
  async function loadCounts(): Promise<void> {
    try {
      const commands = await api.list({ kind: 'command' });
      counts.command = commands.length;
    } catch {
      counts.command = 0;
    }
    if (vault?.unlocked) {
      try {
        counts.secret = (await api.list({ kind: 'secret' })).length;
      } catch {
        counts.secret = 0;
      }
    } else {
      counts.secret = 0;
    }
  }

  async function loadList(): Promise<void> {
    loading = true;
    listError = null;
    try {
      entries = await api.list({ kind: tab, query: query.trim().length > 0 ? query : undefined });
      counts[tab] = entries.length;
      void loadCounts().catch(() => undefined);
      if (selectedId && !entries.some((e) => e.id === selectedId)) {
        selectedId = entries[0]?.id ?? null;
      } else if (!selectedId && entries.length > 0 && !narrow) {
        selectedId = entries[0].id;
      } else if (!selectedId && entries.length > 0 && narrow) {
        // A narrow window opens on the list: silently selecting a row would drop the reader
        // straight into the detail pane they did not ask for.
        details = null;
      }
      if (selectedId) await loadDetails(selectedId);
      else details = null;
    } catch (error: unknown) {
      if (secretMetadataHidden && /locked/i.test(message(error, ''))) {
        entries = [];
        details = null;
      } else {
        listError = message(error, 'Could not load entries.');
        entries = [];
        details = null;
      }
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
      if (!secretMetadataHidden || !/locked/i.test(message(error, ''))) {
        listError = message(error, 'Could not load the selected entry.');
      }
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
    saveErrorDraft = JSON.stringify(draft);
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
        void loadCounts().catch(() => undefined);
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
    <span class="brand">
      <span class="brand-glyph" aria-hidden="true"><Icon name="command" size={14} /></span>
      <strong>Dev Console</strong>
    </span>
    <span class="spacer"></span>
    {#if authenticating}<span class="muted waiting" role="status">Waiting for authentication…</span>{/if}
    {#if vault}<VaultStatus {vault} onunlock={unlock} />{/if}
    <Button variant="ghost" size="sm" icon="gear" onclick={() => (showSettings = true)} data-testid="settings"
      >Settings</Button
    >
  </header>

  <div class="toolbar">
    <Tabs
      items={[
        { id: 'command', label: 'Commands', count: counts.command, icon: 'command', testId: 'tab-commands' },
        {
          id: 'secret',
          label: 'Secrets',
          count: secretMetadataHidden ? null : counts.secret,
          icon: 'secret',
          testId: 'tab-secrets'
        }
      ]}
      active={tab}
      onSelect={(id) => selectTab(id === 'secret' ? 'secret' : 'command')}
    />
    <div class="tools">
      <div class="search">
        <span class="search-icon" aria-hidden="true"><Icon name="search" size={13} /></span>
        <input
          class="filter"
          placeholder="Search this tab"
          aria-label="Search this tab"
          bind:value={query}
          oninput={() => void loadList().catch(() => undefined)}
          data-testid="tab-search"
        />
      </div>
      <Button variant="primary" icon="plus" onclick={startNew} data-testid="add-entry">New</Button>
    </div>
  </div>

  {#if listError && !(secretMetadataHidden && /locked/i.test(listError))}
    <StatusLine tone="error" message={listError} />
  {:else if status}
    <StatusLine tone={status.startsWith('Copied') || status.startsWith('Saved') || status.startsWith('Deleted') || status.startsWith('Launched') ? 'ok' : 'info'} message={status} />
  {/if}

  <main class="panes" class:stacked={narrow} id="entry-pane">
    {#if !narrow || (mode === 'view' && !selectedId)}
      <section class="list-pane" aria-label="Entry list">
        <EntryList
          {entries}
          {selectedId}
          {loading}
          kind={tab}
          locked={secretMetadataHidden}
          emptyText={query
            ? 'Nothing matches this filter. Clear the search or add a new entry.'
            : tab === 'secret'
              ? 'Add a secret and it will be encrypted with the vault key.'
              : 'Add a command and it will be searchable from the Finder.'}
          onSelect={(entry) => void select(entry)}
        />
      </section>
    {/if}

    <section class="detail-pane" aria-label="Entry details">
      {#if narrow && mode === 'view' && selectedId}
        <div class="backrow">
          <Button
            variant="ghost"
            size="sm"
            icon="back"
            onclick={() => {
              selectedId = null;
              details = null;
            }}
            data-testid="back-to-list">List</Button
          >
        </div>
      {/if}
      {#if mode === 'edit'}
        <EntryEditor
          {draft}
          {typeSuggestions}
          {saving}
          dirty={draftDirty}
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
          onrun={() => {
            pendingRun = details;
          }}
          ondelete={() => (pendingDelete = meta)}
        />
      {:else if tab === 'secret' && vault && !vault.unlocked}
        <EmptyState
          icon="lock"
          title="Vault locked"
          hint="Secrets stay encrypted on disk. Unlocking asks the system for confirmation; a manual lock is never cleared automatically."
        >
          {#snippet actions()}
            <Button variant="primary" icon="unlock" loading={authenticating} onclick={() => void unlock()}
              data-testid="unlock-vault">Unlock vault</Button
            >
          {/snippet}
        </EmptyState>
      {:else if entries.length === 0 && !loading}
        <EmptyState
          icon="plus"
          title="Nothing selected"
          hint="Create an entry, or press the shortcut below to search everything at once."
        >
          {#snippet actions()}
            <Button variant="primary" icon="plus" onclick={startNew}>New {tab === 'secret' ? 'secret' : 'command'}</Button>
            <span class="chip shortcut" data-testid="shortcut-hint"><Keycap keys={['Super', 'F']} /> Finder</span>
          {/snippet}
        </EmptyState>
      {:else}
        <EmptyState icon="search" title="Select an entry" hint="Pick a row on the left to see it here." />
      {/if}
    </section>
  </main>
</div>

{#if pendingDelete}
  <Dialog
    title={`Delete “${pendingDelete.name}”?`}
    label="Confirm delete"
    oncancel={() => (pendingDelete = null)}
  >
    <p>This removes the saved {pendingDelete.kind} permanently. Encrypted backups you made earlier still hold it.</p>
    {#snippet footer()}
      <span class="spacer"></span>
      <Button onclick={() => (pendingDelete = null)} data-testid="cancel-delete">Cancel</Button>
      <Button variant="danger" icon="trash" onclick={() => void confirmDelete()} data-testid="confirm-delete"
        >Delete</Button
      >
    {/snippet}
  </Dialog>
{/if}

{#if pendingRun}
  <Dialog title="Run this command?" label="Confirm run" oncancel={() => (pendingRun = null)}>
    <p>It will be launched in your configured terminal after this confirmation.</p>
    <pre class="block mono">{pendingRun.command}</pre>
    {#if pendingRun.workingDirectory}
      <p>Working directory: <code>{pendingRun.workingDirectory}</code></p>
    {/if}
    {#snippet footer()}
      <span class="spacer"></span>
      <Button onclick={() => (pendingRun = null)}>Cancel</Button>
      <Button variant="primary" icon="run" onclick={() => void confirmRun()} data-testid="confirm-run"
        >Run in terminal</Button
      >
    {/snippet}
  </Dialog>
{/if}

{#if pendingClose}
  <Dialog title="Unsaved changes" oncancel={() => (pendingClose = false)}>
    <p>This entry has edits that are not saved.</p>
    {#snippet footer()}
      <span class="spacer"></span>
      <Button onclick={() => (pendingClose = false)} data-testid="close-cancel">Keep editing</Button>
      <Button
        onclick={() => {
          pendingClose = false;
          cancelEdit();
          void allowClose();
        }}
        data-testid="close-discard">Discard</Button
      >
      <Button
        variant="primary"
        onclick={() => {
          pendingClose = false;
          void saveDraft().then(() => allowClose());
        }}
        data-testid="close-save">Save</Button
      >
    {/snippet}
  </Dialog>
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
  /* The header is the quietest band in the window: it carries identity and the
     vault/state controls and nothing else. No border of its own - the toolbar
     under it is the band that separates from the content, so the top of the
     window reads as one chrome area instead of three stacked rules. */
  header {
    display: flex;
    align-items: center;
    gap: var(--gap);
    min-width: 0;
    padding: var(--sp-2) var(--sp-3);
    background: var(--bg);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    font-size: var(--text-sm);
    letter-spacing: 0.01em;
    color: var(--fg-muted);
  }
  .brand strong {
    color: var(--fg-muted);
    font-weight: 600;
  }
  /* the same kind tile as the finder row and the entry list: one size scale,
     no border, no second accent fill in the header */
  .brand-glyph {
    display: grid;
    place-items: center;
    width: var(--tile-sm);
    height: var(--tile-sm);
    border-radius: var(--radius-sm);
    color: var(--kind-command);
    background: var(--accent-soft);
  }
  .waiting {
    font-size: var(--text-sm);
  }
  @media (max-width: 560px) {
    .brand strong {
      display: none;
    }
    header {
      gap: var(--sp-2);
    }
  }
  /* one hairline under the whole chrome area; the tab strip keeps its own track
     line, which is what the active-tab underline sits on */
  .toolbar {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    border-bottom: var(--border-hair);
    background: var(--bg-panel);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
  }
  .search {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .search-icon {
    position: absolute;
    left: var(--sp-2);
    top: 50%;
    transform: translateY(-50%);
    color: var(--fg-dim);
    pointer-events: none;
  }
  .search input {
    padding-left: var(--field-icon-indent);
  }
  .panes {
    flex: 1;
    display: grid;
    grid-template-columns: var(--pane-list) minmax(var(--pane-detail-min), 1fr);
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
  /* the list/detail split is a shell-level separation, not a control border: it is
     the same line colour the floating shell uses, and it is the only divider
     between the two panes */
  .list-pane {
    border-right: var(--border-shell);
    background: var(--bg-panel);
  }
  .detail-pane {
    background: var(--bg);
    overflow: auto;
    display: flex;
    flex-direction: column;
  }
  .backrow {
    padding: var(--sp-2) var(--sp-3);
    border-bottom: var(--border-hair);
    background: var(--bg-panel);
  }
  /* the shortcut hint is a well for keycaps, not another raised pill: keycaps are
     --bg-raised, so the fill behind them has to be the inset token or they disappear */
  .shortcut {
    background: var(--inset-fill);
  }
  @media (max-width: 900px) {
    .panes {
      grid-template-columns: 1fr;
    }
  }
</style>
