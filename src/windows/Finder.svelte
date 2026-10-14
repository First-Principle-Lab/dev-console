<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api } from '../lib/api';
  import { on } from '../lib/events';
  import { displayedQuery, nextSelection, selected, shouldCommit, tabCompletion } from '../lib/finder-state';
  import type { Kind, SearchResult, VaultState } from '../lib/models';

  const UNLOCK_ID = '__unlock_vault__';

  let input = $state<HTMLInputElement | null>(null);
  let list = $state<HTMLElement | null>(null);
  let query = $state('');
  let results = $state<SearchResult[]>([]);
  let selectedId = $state<string | null>(null);
  let vault = $state<VaultState | null>(null);
  let error = $state<string | null>(null);
  let status = $state<string | null>(null);
  let loading = $state(false);
  let requestId = 0;

  const rows = $derived<SearchResult[]>(
    vault && !vault.unlocked && vault.vaultAvailable
      ? [
          ...results,
          {
            id: UNLOCK_ID,
            kind: 'command',
            revision: 0,
            name: 'Unlock vault to search secrets',
            type: 'System',
            tags: [],
            description: '',
            pinned: false,
            updatedAt: '',
            usageCount: 0,
            lastUsedAt: null
          }
        ]
      : results
  );
  const current = $derived(selected(rows, selectedId));
  const completion = $derived(displayedQuery(query, current));
  const tail = $derived(completion && completion.length > query.length ? completion.slice(query.length) : '');

  function message(problem: unknown, fallback: string): string {
    return problem instanceof Error && problem.message.length > 0 ? problem.message : fallback;
  }

  async function runSearch(next: string): Promise<void> {
    const id = ++requestId;
    loading = true;
    error = null;
    try {
      const response = await api.search(next, id);
      // Ignore any response for a query the user has already replaced.
      if (id !== requestId) return;
      const generation = vault?.generation ?? response.vaultGeneration;
      if (response.vaultGeneration < generation && vault?.unlocked === false) return;
      results = response.results;
      selectedId = nextSelection(results, selectedId, 0);
    } catch (problem: unknown) {
      if (id === requestId) {
        error = message(problem, 'Search failed.');
        results = [];
        selectedId = null;
      }
    } finally {
      if (id === requestId) loading = false;
    }
  }

  function reset(): void {
    requestId += 1;
    query = '';
    results = [];
    selectedId = null;
    error = null;
    status = null;
    void runSearch('').catch(() => undefined);
    input?.focus();
  }

  async function scrollSelectedIntoView(): Promise<void> {
    await tick();
    if (!list || !selectedId) return;
    list.querySelector(`[data-row-id="${CSS.escape(selectedId)}"]`)?.scrollIntoView({ block: 'nearest' });
  }

  async function acceptCompletion(): Promise<void> {
    const completed = tabCompletion(query, current);
    if (!completed) return;
    const keep = current?.id ?? null;
    query = completed;
    await runSearch(completed);
    if (results.some((r) => r.id === keep)) selectedId = keep;
    await scrollSelectedIntoView();
  }

  async function activate(): Promise<void> {
    const entry = current;
    if (!entry) return;
    if (entry.id === UNLOCK_ID) {
      await unlock();
      return;
    }
    try {
      await api.copy(entry.kind, entry.id, 0);
      status = null;
      error = null;
      await api.hideSelf();
    } catch (problem: unknown) {
      // Stay open with a safe message; never include the value in the error.
      error = message(problem, 'Copy failed. Nothing was copied.');
    }
  }

  async function openDetails(): Promise<void> {
    const entry = current;
    if (!entry || entry.id === UNLOCK_ID) return;
    await api.openLibrary(entry.kind as Kind, entry.id);
    await api.hideSelf();
  }

  async function hide(): Promise<void> {
    requestId += 1;
    query = '';
    results = [];
    selectedId = null;
    error = null;
    await api.hideSelf();
  }

  async function unlock(): Promise<void> {
    try {
      vault = await api.unlockVault();
      await runSearch(query);
    } catch (problem: unknown) {
      error = message(problem, 'Unlock was cancelled.');
    }
  }

  async function onKeydown(event: KeyboardEvent): Promise<void> {
    if (!shouldCommit({ isComposing: event.isComposing, keyCode: event.keyCode })) return;
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        selectedId = nextSelection(rows, selectedId, 1);
        await scrollSelectedIntoView();
        break;
      case 'ArrowUp':
        event.preventDefault();
        selectedId = nextSelection(rows, selectedId, -1);
        await scrollSelectedIntoView();
        break;
      case 'Tab':
        event.preventDefault();
        await acceptCompletion();
        break;
      case 'Enter':
        event.preventDefault();
        await activate();
        break;
      case 'Escape':
        event.preventDefault();
        await hide();
        break;
      default:
        if (event.ctrlKey && event.key.toLowerCase() === 'o') {
          event.preventDefault();
          await openDetails();
        }
    }
  }

  onMount(() => {
    const unsubscribers = [
      on('finder-shown', () => reset()),
      on<VaultState>('vault-state-changed', (state) => {
        const wasLocked = vault ? !vault.unlocked : false;
        vault = state;
        if (wasLocked && state.unlocked) {
          void runSearch(query).catch(() => undefined);
        } else if (!state.unlocked) {
          results = results.filter((r) => r.kind !== 'secret');
          if (current?.kind === 'secret') selectedId = rows[0]?.id ?? null;
        }
      }),
      on('entries-changed', () => {
        void runSearch(query).catch(() => undefined);
      })
    ];
    void api
      .appState()
      .then((state) => {
        vault = state.vault;
        return runSearch('');
      })
      .catch((problem: unknown) => (error = message(problem, 'Backend unavailable.')))
      .finally(() => input?.focus());
    return () => {
      for (const unsubscribe of unsubscribers) unsubscribe();
    };
  });
</script>

<div class="finder" data-testid="finder">
  <div class="search">
    <div class="mirror" aria-hidden="true">
      <span class="measure">{query}</span><span class="tail">{tail}</span>
    </div>
    <input
      bind:this={input}
      bind:value={query}
      oninput={() => void runSearch(query).catch(() => undefined)}
      onkeydown={(event) => void onKeydown(event)}
      role="combobox"
      aria-expanded="true"
      aria-controls="finder-results"
      aria-autocomplete="list"
      aria-activedescendant={selectedId ? `row-${selectedId}` : undefined}
      aria-label="Search commands and secrets"
      placeholder="Search commands and secrets…"
      spellcheck="false"
      data-testid="finder-input"
    />
    <span class="vaultstate muted" data-testid="finder-vault">
      {#if vault}{vault.unlocked ? 'Vault unlocked' : 'Vault locked'}{/if}
    </span>
  </div>

  <ul id="finder-results" class="results" role="listbox" aria-label="Results" bind:this={list}>
    {#each rows as row, index (row.id)}
      <li
        id={`row-${row.id}`}
        data-row-id={row.id}
        role="option"
        aria-selected={row.id === selectedId}
        class:active={row.id === selectedId}
        class:system={row.id === UNLOCK_ID}
        onclick={() => (selectedId = row.id)}
        ondblclick={() => void activate()}
        onkeydown={(event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            selectedId = row.id;
            void activate();
          }
        }}
        tabindex={row.id === selectedId ? 0 : -1}
        data-testid="result-row"
      >
        <span class="marker">{#if row.id === selectedId}> {:else}&nbsp;{/if}</span>
        <span class="name">{row.name}</span>
        <span class="kind">{row.id === UNLOCK_ID ? 'Vault' : row.kind === 'command' ? 'Command' : 'Secret'}</span>
        <span class="type muted">{row.type}</span>
        <button
          class="copy"
          tabindex="-1"
          onclick={(e) => {
            e.stopPropagation();
            selectedId = row.id;
            void activate();
          }}>Copy</button
        >
        <span class="index muted">{index + 1}</span>
      </li>
    {:else}
      <li class="empty muted" class:loading>
        {#if loading}Searching…{:else if query}No matches{:else}Nothing pinned yet{/if}
      </li>
    {/each}
  </ul>

  {#if error}
    <p class="error-banner" role="alert" data-testid="finder-error">{error}</p>
  {:else if status}
    <p class="hint muted" role="status">{status}</p>
  {/if}

  <footer class="muted">
    <span>Tab complete</span><span>Up/Down select</span><span>Enter copy</span>
    <span>Ctrl+O details</span><span>Esc close</span>
  </footer>
</div>

<style>
  .finder {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-panel);
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--gap);
    padding: 12px 14px;
    border-bottom: var(--border);
  }
  .search input {
    flex: 1;
    background: transparent;
    border: none;
    font-size: 16px;
    padding: 4px 0;
  }
  .mirror {
    position: absolute;
    left: 14px;
    right: 150px;
    top: 16px;
    font-size: 16px;
    white-space: pre;
    pointer-events: none;
    overflow: hidden;
  }
  .measure {
    visibility: hidden;
  }
  .tail {
    color: var(--fg-muted);
  }
  .vaultstate {
    font-size: 12px;
    white-space: nowrap;
  }
  .results {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    overflow-y: auto;
    max-height: 372px;
    flex: 1;
  }
  li {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 6px 14px;
    cursor: default;
  }
  li.active {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  li.system .name {
    color: var(--accent);
  }
  .marker {
    width: 12px;
    color: var(--accent);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kind {
    font-size: 12px;
    color: var(--fg-muted);
    min-width: 62px;
  }
  .type {
    font-size: 12px;
    min-width: 74px;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .copy {
    padding: 1px 7px;
    font-size: 12px;
    visibility: hidden;
  }
  li.active .copy,
  li:hover .copy {
    visibility: visible;
  }
  .index {
    font-size: 11px;
    min-width: 14px;
    text-align: right;
  }
  .empty {
    padding: 16px 14px;
  }
  .error-banner {
    margin: 0;
    border-radius: 0;
    border: none;
    border-top: 1px solid var(--danger);
  }
  footer {
    display: flex;
    gap: 12px;
    padding: 7px 14px;
    border-top: var(--border);
    font-size: 12px;
    flex-wrap: wrap;
  }
</style>
