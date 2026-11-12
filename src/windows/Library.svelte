<script lang="ts">
  import { onMount } from 'svelte';
  import ProjectEnvironment from '../components/ProjectEnvironment.svelte';
  import EntryList from '../components/EntryList.svelte';
  import HistorySuggestions from '../components/HistorySuggestions.svelte';
  import EntryDetails from '../components/EntryDetails.svelte';
  import EntryEditor from '../components/EntryEditor.svelte';
  import VaultStatus from '../components/VaultStatus.svelte';
  import SettingsPanel from '../components/SettingsPanel.svelte';
  import Button from '../components/Button.svelte';
  import Dialog from '../components/Dialog.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import ProjectImport from '../components/ProjectImport.svelte';
  import CliProxyUsage from '../components/cliproxy/CliProxyUsage.svelte';
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
    HistorySuggestion,
    Kind,
    RunResult,
    VaultState,
    WindowIntent
  } from '../lib/models';
  import { validateDraft } from '../lib/validation';
  import { checkForUpdates, installUpdate, updates } from '../lib/updates.svelte';

  let environmentRoot = $state<string | null>(null);
  let app = $state<AppStateInfo | null>(null);
  let vault = $state<VaultState | null>(null);
  let tab = $state<Kind>('command');
  /** The CLI Proxy tab is a usage dashboard, not an entry list, so it sits beside
   * the Kind tabs without extending the backend enum. */
  let cliproxyActive = $state(false);
  let query = $state('');
  let entries = $state<EntryMeta[]>([]);
  let selectedId = $state<string | null>(null);
  let details = $state<Details | null>(null);
  let revealed = $state(false);
  let loading = $state(true);
  let listError = $state<string | null>(null);
  // Commands from the shell's own history, offered as starting points for a new entry. The
  // backend is the authority on whether they are read at all (the setting is checked there).
  let suggestions = $state<HistorySuggestion[]>([]);
  let suggestionsBusy = $state(false);
  let historyEnabled = $state(false);
  let mode = $state<'view' | 'edit'>('view');
  let draft = $state<EntryDraft>(EMPTY_DRAFT('command'));
  let baseline = $state('');
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveErrorDraft = $state('');
  let status = $state<string | null>(null);
  let pendingDelete = $state<EntryMeta | null>(null);
  let pendingRun = $state<Details | null>(null);
  let runOutput = $state<{ name: string; phase: 'running' | 'done' | 'error'; result?: RunResult; error?: string } | null>(null);
  let runSerial = 0;
  let copyingStream = $state<'stdout' | 'stderr' | null>(null);
  let copiedStream = $state<'stdout' | 'stderr' | null>(null);
  let outputCopyError = $state<string | null>(null);
  let pendingClose = $state(false);
  let showSettings = $state(false);
  /** The project-script import dialog; `null` closes it. */
  let importingProject = $state(false);

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

  function selectTab(next: Kind | 'cliproxy'): void {
    const current: Kind | 'cliproxy' = cliproxyActive ? 'cliproxy' : tab;
    if (next !== current && mode === 'edit' && draftDirty) {
      status = 'Save or discard the open edit first.';
      return;
    }
    if (next === 'cliproxy') {
      cliproxyActive = true;
      return;
    }
    cliproxyActive = false;
    if (tab === next) return;
    tab = next;
    selectedId = null;
    details = null;
    void loadList().catch((error: unknown) =>
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

  async function loadSuggestions(): Promise<void> {
    suggestionsBusy = true;
    try {
      historyEnabled = (await api.settings()).historySuggestionsEnabled;
      suggestions = historyEnabled ? await api.historySuggestions() : [];
    } catch {
      // A missing or unreadable history file is not a failure the user needs to hear about:
      // there is simply nothing to offer.
      suggestions = [];
    } finally {
      suggestionsBusy = false;
    }
  }

  /** Opens the normal editor with the suggestion prefilled; nothing is saved until it is saved. */
  function saveSuggestion(suggestion: HistorySuggestion): void {
    if (mode === 'edit' && draftDirty) {
      status = 'Save or discard the open edit first.';
      return;
    }
    tab = 'command';
    mode = 'edit';
    selectedId = null;
    details = null;
    draft = {
      ...EMPTY_DRAFT('command'),
      name: suggestion.suggestedName || suggestion.command.slice(0, 60),
      command: suggestion.command,
      workingDirectory: suggestion.workingDirectory ?? '',
      tags: suggestion.project ? [`project:${suggestion.project.toLowerCase().replace(/\s+/g, '-').slice(0, 32)}`] : []
    };
    baseline = JSON.stringify(draft);
    saveError = null;
    status = null;
  }

  async function dismissSuggestion(suggestion: HistorySuggestion): Promise<void> {
    // Gone immediately; the backend only ever learns the digest.
    suggestions = suggestions.filter((other) => other.key !== suggestion.key);
    try {
      await api.dismissSuggestion(suggestion.key);
      status = 'Dismissed - only its hash is remembered.';
    } catch (error: unknown) {
      status = message(error, 'Could not dismiss that suggestion.');
      void loadSuggestions();
    }
  }

  /** The dialog only ever creates commands, so it opens on the tab that shows them. */
  function startProjectImport(): void {
    tab = 'command';
    importingProject = true;
  }

  /**
   * Close the import dialog and refresh the command list. The backend already wrote the rows, so
   * this only re-reads them; `imported` is 0 when the dialog was cancelled.
   */
  function finishProjectImport(imported: number): void {
    importingProject = false;
    if (imported > 0) {
      status = `Imported ${imported} ${imported === 1 ? 'task' : 'tasks'}.`;
      void loadList().catch(() => undefined);
    }
  }

  function startNew(): void {
    const projectDirectory = tab === 'command' ? meta?.project?.root || meta?.workingDirectory || '' : '';
    mode = 'edit';
    selectedId = null;
    details = null;
    draft = { ...EMPTY_DRAFT(tab), workingDirectory: projectDirectory };
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
      // A command that is now in the library must stop being offered.
      void loadSuggestions();
    } catch (error: unknown) {
      // The draft is intentionally preserved so nothing typed is lost on a failed write.
      saveError = message(error, 'Save failed. Your changes were kept.');
    } finally {
      saving = false;
    }
  }

  async function togglePin(target: EntryMeta | null = meta): Promise<void> {
    if (!target || (mode === 'edit' && draftDirty)) return;
    try {
      await api.setPinned(target.kind, target.id, target.revision, !target.pinned);
      await loadList();
    } catch (error: unknown) {
      status = message(error, 'Pin failed.');
    }
  }

  async function copyEntry(target: EntryMeta | null = meta): Promise<void> {
    if (!target) return;
    try {
      const result = await api.copy(target.kind, target.id, target.revision);
      const seconds = Math.max(1, Math.round((result.expiresAtMs - Date.now()) / 1000));
      status = result.excluded
        ? `Copied for about ${seconds}s (excluded from clipboard history).`
        : `Copied for about ${seconds}s. ${result.exclusion.detail}`;
    } catch (error: unknown) {
      status = message(error, 'Copy failed.');
    }
  }

  async function revealEntry(): Promise<void> {
    if (!selectedId || !details) return;
    if (revealed) {
      // Hiding must also drop the plaintext, not just mask it.
      details = { ...details, value: '' };
      revealed = false;
      return;
    }
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
    status = null;
    const serial = ++runSerial;
    copiedStream = null;
    outputCopyError = null;
    runOutput = { name: target.meta.name, phase: 'running' };
    try {
      const result = await api.run(target.meta.id, target.meta.revision, true);
      if (serial !== runSerial) return;
      if (result.mode === 'terminal') {
        runOutput = null;
        status = 'Launched in a terminal for sudo.';
      } else {
        runOutput = { name: target.meta.name, phase: 'done', result };
      }
    } catch (error: unknown) {
      if (serial === runSerial) runOutput = { name: target.meta.name, phase: 'error', error: message(error, 'Execution failed.') };
    }
  }

  function closeRunOutput(): void {
    runSerial += 1;
    runOutput = null;
    copiedStream = null;
    outputCopyError = null;
  }

  async function copyRunStream(stream: 'stdout' | 'stderr'): Promise<void> {
    if (runOutput?.phase !== 'done' || !runOutput.result || copyingStream) return;
    const value = runOutput.result[stream];
    if (!value) return;
    const serial = runSerial;
    copyingStream = stream;
    outputCopyError = null;
    try {
      await api.copyCommandOutput(value);
      if (serial === runSerial) copiedStream = stream;
    } catch (error: unknown) {
      if (serial === runSerial) outputCopyError = message(error, 'Could not copy command output.');
    } finally {
      copyingStream = null;
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
    void checkForUpdates();
    const updateTimer = window.setInterval(() => void checkForUpdates(), 15 * 60 * 1000);
    const checkOnFocus = (): void => { void checkForUpdates(); };
    window.addEventListener('focus', checkOnFocus);
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
        void loadList().catch(() => undefined);
        if (intent.action === 'new-entry') startNew();
        else if (intent.entryId) {
          selectedId = intent.entryId;
          void loadDetails(intent.entryId).catch(() => undefined);
        }
      }),
      on<{ historySuggestionsEnabled: boolean }>('settings-saved', (saved) => {
        // Switching the setting off must hide the panel without a reload; switching it back on
        // re-reads, because the file is never opened while the setting is off.
        historyEnabled = saved.historySuggestionsEnabled;
        suggestions = saved.historySuggestionsEnabled ? suggestions : [];
        if (saved.historySuggestionsEnabled) void loadSuggestions();
      }),
      // Settings forgot every dismissal: those commands are candidates again.
      on('suggestions-restored', () => void loadSuggestions()),
      on<{ reason: string }>('close-requested', () => {
        if (mode === 'edit' && draftDirty) pendingClose = true;
        else void allowClose().catch(() => undefined);
      })
    ];

    void loadSuggestions();
    void refreshVault()
      .then(() => loadList())
      .catch((error: unknown) => {
        listError = message(error, 'Backend unavailable.');
      })
      .finally(() => (loading = false));

    return () => {
      window.clearInterval(updateTimer);
      window.removeEventListener('focus', checkOnFocus);
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
    } else if (event.ctrlKey && event.key === '3') {
      event.preventDefault();
      selectTab('cliproxy');
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
      <img class="brand-glyph" src="/logo.svg" alt="" />
      <strong>Dev Console</strong>
    </span>
    <span class="spacer"></span>
    {#if authenticating}<span class="muted waiting" role="status">Waiting for authentication…</span>{/if}
    {#if updates.available}
      <Button variant="primary" size="sm" icon="update" loading={updates.status === 'installing'} onclick={() => void installUpdate()} data-testid="install-update">
        Update to {updates.available.version}
      </Button>
    {/if}
    {#if updates.status === 'error'}
      <Button variant="ghost" size="sm" icon="alert" onclick={() => (showSettings = true)} data-testid="update-failed">Update check failed</Button>
    {/if}
    {#if vault}<VaultStatus {vault} onunlock={unlock} />{/if}
    <Button variant="ghost" size="sm" icon="gear" onclick={() => (showSettings = true)} data-testid="settings"
      >Settings</Button
    >
  </header>

  <div class="toolbar">
    <Tabs
      items={[
        { id: 'command', label: 'Commands', count: counts.command, icon: 'collection', testId: 'tab-commands' },
        {
          id: 'secret',
          label: 'Secrets',
          count: secretMetadataHidden ? null : counts.secret,
          icon: 'secret',
          testId: 'tab-secrets'
        },
        { id: 'cliproxy', label: 'CLI Proxy', count: null, badge: null, icon: 'history', testId: 'tab-cliproxy' }
      ]}
      active={cliproxyActive ? 'cliproxy' : tab}
      onSelect={(id) => selectTab(id === 'cliproxy' ? 'cliproxy' : id === 'secret' ? 'secret' : 'command')}
    />
    <div class="tools">
      {#if cliproxyActive}
        <!-- The dashboard refreshes itself; list tools do not apply here. -->
      {:else}
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
      <Button icon="lock" onclick={() => (environmentRoot = meta?.project?.root || '')} data-testid="open-environment">Environment</Button>
      <!-- Reading a project's own task definitions is the only way a repository's build steps
           become library entries, so it sits beside New rather than hidden in Settings. -->
      <Button
        icon="folder"
        onclick={startProjectImport}
        title="Import the tasks a project folder defines as commands"
        data-testid="import-project"
        >Import from project</Button
      >
      {/if}
    </div>
  </div>

  {#if !cliproxyActive && listError && !(secretMetadataHidden && /locked/i.test(listError))}
    <StatusLine tone="error" message={listError} />
  {:else if !cliproxyActive && status}
    <StatusLine tone={status.startsWith('Copied') || status.startsWith('Saved') || status.startsWith('Deleted') || status.startsWith('Launched') ? 'ok' : 'info'} message={status} />
  {/if}

  {#if cliproxyActive}
    <main class="panes" id="entry-pane">
      <section id="cliproxy-pane" class="cliproxy-pane" aria-label="CLI Proxy usage">
        <CliProxyUsage />
      </section>
    </main>
  {:else}
  <main class="panes" class:stacked={narrow} id="entry-pane">
    {#if !narrow || (mode === 'view' && !selectedId)}
      <section class="list-pane" aria-label="Entry list">
        {#if tab === 'command' && historyEnabled}
          <HistorySuggestions
            {suggestions}
            busy={suggestionsBusy}
            onSave={saveSuggestion}
            onDismiss={(suggestion) => void dismissSuggestion(suggestion)}
            onRescan={() => void loadSuggestions()}
          />
        {/if}
        <div class="list-scroll">
        <EntryList
          {entries}
          filterActive={query.trim().length > 0}
          {selectedId}
          {loading}
          kind={tab}
          locked={secretMetadataHidden}
          emptyText={query
            ? 'Nothing matches this filter. Clear the search or add a new entry.'
            : tab === 'secret'
              ? 'Add a secret and it will be encrypted with the vault key.'
              : 'Add a command and it will be searchable from the Finder.'}
          onProjectOpen={(root) => {
            if (vault?.unlocked) void api.syncEnvironment(root).catch(() => {
              status = 'Environment sync failed. Open the project’s Environment section to retry and see the error.';
            });
          }}
          onEnvironment={(root) => (environmentRoot = root)}
          onSelect={(entry) => void select(entry)}
          onCopy={(entry) => void copyEntry(entry)}
          onPin={(entry) => void togglePin(entry)}
          onDelete={(entry) => (pendingDelete = entry)}
          actionsDisabled={mode === 'edit' && draftDirty}
        />
        </div>
      </section>
    {/if}

    {#if !narrow || selectedId || mode === 'edit'}
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
    {/if}
  </main>
  {/if}
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
    <p>Commands using sudo open your terminal for authentication. Other commands show their output here.</p>
    <pre class="block mono">{pendingRun.command}</pre>
    {#if pendingRun.workingDirectory}
      <p>Working directory: <code>{pendingRun.workingDirectory}</code></p>
    {/if}
    {#snippet footer()}
      <span class="spacer"></span>
      <Button onclick={() => (pendingRun = null)}>Cancel</Button>
      <Button variant="primary" icon="run" onclick={() => void confirmRun()} data-testid="confirm-run"
        >Run</Button
      >
    {/snippet}
  </Dialog>
{/if}

{#if runOutput}
  <Dialog title={runOutput.phase === 'running' ? `Running “${runOutput.name}”…` : `Result: ${runOutput.name}`} label="Command output" wide oncancel={closeRunOutput}>
    {#if runOutput.phase === 'running'}
      <p role="status" data-testid="command-running">Running command. Output appears when it finishes. Closing this view does not stop it.</p>
    {:else if runOutput.phase === 'error'}
      <p class="error-banner" role="alert" data-testid="command-error">{runOutput.error}</p>
    {:else if runOutput.result}
      <p class="run-summary" class:failed={runOutput.result.timedOut || runOutput.result.exitCode !== 0} data-testid="command-exit">
        {#if runOutput.result.timedOut}Timed out after 120 seconds.
        {:else if runOutput.result.exitCode === null}Stopped by a signal.
        {:else}Exit code {runOutput.result.exitCode}.{/if}
        {#if runOutput.result.truncated} Output was shortened to 64 KiB per stream.{/if}
      </p>
      {#if runOutput.result.stdout}
        <section class="run-stream">
          <div class="run-stream-heading"><h4>Output</h4><Button size="sm" icon="copy" loading={copyingStream === 'stdout'} onclick={() => void copyRunStream('stdout')} data-testid="copy-command-stdout">{copiedStream === 'stdout' ? 'Copied' : 'Copy output'}</Button></div>
          <pre data-testid="command-stdout">{runOutput.result.stdout}</pre>
        </section>
      {/if}
      {#if runOutput.result.stderr}
        <section class="run-stream">
          <div class="run-stream-heading"><h4>Errors</h4><Button size="sm" icon="copy" loading={copyingStream === 'stderr'} onclick={() => void copyRunStream('stderr')} data-testid="copy-command-stderr">{copiedStream === 'stderr' ? 'Copied' : 'Copy errors'}</Button></div>
          <pre data-testid="command-stderr">{runOutput.result.stderr}</pre>
        </section>
      {/if}
      {#if outputCopyError}<p class="error-banner" role="alert" data-testid="output-copy-error">{outputCopyError}</p>{/if}
      {#if !runOutput.result.stdout && !runOutput.result.stderr}
        <p class="muted">The command produced no output.</p>
      {/if}
    {/if}
    {#snippet footer()}
      <Button onclick={closeRunOutput} data-testid="close-command-output">Close</Button>
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

{#if importingProject}
  <ProjectImport ondone={finishProjectImport} />
{/if}

{#if environmentRoot !== null}
  <ProjectEnvironment initialRoot={environmentRoot} onClose={() => (environmentRoot = null)} />
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
  .brand-glyph {
    width: var(--tile-sm);
    height: var(--tile-sm);
    flex: none;
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
    flex-wrap: wrap;
    align-items: center;
    gap: var(--gap-sm);
  }
  .search {
    position: relative;
    flex: 1 1 180px;
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
  /* the CLI Proxy dashboard takes the whole pane instead of the list/detail split */
  .cliproxy-pane {
    grid-column: 1 / -1;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
    background: var(--bg-panel);
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
    display: flex;
    flex-direction: column;
    border-right: var(--border-shell);
    background: var(--bg-panel);
  }
  /* the entry list scrolls on its own; the suggestion panel above it stays put */
  .list-scroll {
    flex: 1;
    min-height: 0;
    overflow: hidden;
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
  .run-summary {
    margin: 0;
    color: var(--ok);
  }
  .run-summary.failed {
    color: var(--danger);
  }
  .run-stream {
    min-width: 0;
  }
  .run-stream-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-2);
    margin-bottom: var(--sp-1);
  }
  .run-stream h4 {
    color: var(--fg-muted);
    font-size: var(--text-sm);
    margin: 0;
  }
  .run-stream pre {
    margin: 0;
    padding: var(--sp-3);
    max-height: 35vh;
    overflow: auto;
    background: var(--bg-sunken);
    color: var(--fg);
    border: var(--border-hair);
    border-radius: var(--radius);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }
  @media (max-width: 900px) {
    .panes {
      grid-template-columns: 1fr;
    }
  }
</style>
