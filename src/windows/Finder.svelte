<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Icon from '../components/Icon.svelte';
  import Keycap from '../components/Keycap.svelte';
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
    vault && !vault.unlocked
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
    // Tauri rejects with the backend's message as a plain string (AppError serializes via
    // Display); without this branch every real failure would show the fallback instead.
    if (typeof problem === 'string' && problem.length > 0) return problem;
    return problem instanceof Error && problem.message.length > 0 ? problem.message : fallback;
  }

  async function runSearch(next: string): Promise<void> {
    const id = ++requestId;
    loading = true;
    error = null;
    // A "Copied" confirmation belongs to the query it happened for; leaving it on screen under a
    // different query reads as a statement about the new one.
    status = null;
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
      // The value itself never appears here: the state says what happened, not what it was.
      status = entry.kind === 'secret' ? 'Copied the secret to the clipboard.' : 'Copied the command to the clipboard.';
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
  <div class="palette">
    <div class="search">
      <span class="search-icon" aria-hidden="true"><Icon name="search" size={16} /></span>
      <div class="field">
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
          aria-autocomplete="both"
          aria-activedescendant={selectedId ? `row-${selectedId}` : undefined}
          aria-label="Search commands and secrets"
          placeholder="Search commands and secrets…"
          spellcheck="false"
          autocapitalize="off"
          autocomplete="off"
          data-testid="finder-input"
        />
      </div>
      {#if vault}
        <span class="vaultchip" data-tone={vault.unlocked ? 'ok' : 'warn'} data-testid="finder-vault">
          <Icon name={vault.unlocked ? 'unlock' : 'lock'} size={12} />
          {vault.unlocked ? 'Vault unlocked' : 'Vault locked'}
        </span>
      {/if}
    </div>

    <ul id="finder-results" class="results" role="listbox" aria-label="Results" bind:this={list}>
      {#each rows as row (row.id)}
        <li
          id={`row-${row.id}`}
          data-row-id={row.id}
          role="option"
          aria-selected={row.id === selectedId}
          class="row"
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
          <span class="glyph" class:secret={row.kind === 'secret'} aria-hidden="true">
            <Icon name={row.id === UNLOCK_ID ? 'lock' : row.kind === 'secret' ? 'secret' : 'command'} size={14} />
          </span>
          <span class="body">
            <span class="line">
              <span class="name truncate">{row.name}</span>
              {#if row.tags.length > 0}
                <span class="tags truncate">{#each row.tags.slice(0, 3) as tag (tag)}<span class="tag">#{tag}</span>{/each}</span>
              {/if}
            </span>
            {#if row.id === UNLOCK_ID}
              <span class="sub">Secret metadata stays encrypted until you unlock the vault.</span>
            {:else if row.description}
              <span class="sub truncate">{row.description}</span>
            {/if}
          </span>
          <span class="trail">
            {#if row.id === UNLOCK_ID}
              <span class="chip" data-tone="status">Unlock</span>
            {:else}
              <span class="chip" data-tone={row.kind === 'secret' ? 'secret' : 'command'}>
                <Icon name={row.kind === 'secret' ? 'secret' : 'command'} size={10} />
                {row.kind === 'secret' ? 'Secret' : 'Command'}
              </span>
            {/if}
            {#if row.type}<span class="type">{row.type}</span>{/if}
            {#if row.id === selectedId}
              <span class="hintkeys" aria-hidden="true"><Keycap keys={['Enter']} /></span>
            {/if}
          </span>
        </li>
      {:else}
        {#if loading}
          <li class="state" role="presentation"><Icon name="search" size={14} /> Searching…</li>
        {:else if query}
          <li class="state" role="presentation" data-testid="finder-empty">
            <Icon name="search" size={14} />
            <span>
              No match for <strong>{query}</strong>
              <span class="sub">— try a tag, a type, or part of the name</span>
            </span>
          </li>
        {:else}
          <li class="state" role="presentation">Nothing saved yet</li>
        {/if}
      {/each}
    </ul>

    {#if error}
      <p class="error-banner" role="alert" data-testid="finder-error"><Icon name="alert" size={12} /> {error}</p>
    {:else if status}
      <p class="copied" role="status" data-testid="finder-status"><Icon name="check" size={12} /> {status}</p>
    {/if}

    <footer>
      <span class="hint"><Keycap keys={['Up', 'Down']} /> select</span>
      <span class="hint"><Keycap keys={['Tab']} /> complete</span>
      <span class="hint"><Keycap keys={['Enter']} /> copy</span>
      <span class="hint"><Keycap keys={['Ctrl', 'O']} /> details</span>
      <span class="hint"><Keycap keys={['Esc']} /> close</span>
      <span class="spacer"></span>
      <span class="count muted">{rows.length}</span>
    </footer>
  </div>
</div>

<style>
  /* The palette frame. In Tauri the window is 680x420 and borderless; in the browser
     preview the same frame is centred, and on narrow screens it fills the viewport. */
  .finder {
    height: 100%;
    background: var(--bg);
    display: grid;
    place-items: center;
    padding: var(--sp-3);
  }
  .palette {
    width: min(680px, 100%);
    height: min(420px, 100%);
    display: flex;
    flex-direction: column;
    background: var(--bg-panel);
    border: var(--border-strong);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-3);
    border-bottom: var(--border);
    background: var(--bg-raised);
  }
  .search-icon {
    color: var(--accent);
    display: grid;
    place-items: center;
  }
  .field {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .field input {
    background: transparent;
    border: none;
    border-radius: 0;
    padding: 0;
    min-height: 28px;
    font-size: var(--text-lg);
  }
  .field input:focus {
    background: transparent;
    border: none;
    outline: none;
  }
  .search:focus-within {
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .mirror {
    position: absolute;
    inset: 0;
    font-size: var(--text-lg);
    line-height: 28px;
    white-space: pre;
    pointer-events: none;
    overflow: hidden;
    color: var(--fg);
  }
  .measure {
    color: transparent;
  }
  .tail {
    color: var(--fg-dim);
  }
  .vaultchip {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-1);
    font-size: var(--text-xs);
    padding: 2px var(--sp-2);
    border: var(--border);
    border-radius: 999px;
    color: var(--fg-muted);
    background: var(--bg-panel);
    white-space: nowrap;
  }
  .vaultchip[data-tone='ok'] {
    color: var(--ok);
    border-color: var(--line);
  }
  .vaultchip[data-tone='warn'] {
    color: var(--warn);
    border-color: var(--line-strong);
  }
  .results {
    list-style: none;
    margin: 0;
    padding: var(--sp-2);
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow: auto;
    flex: 1;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-2);
    border-radius: var(--radius);
    border: 1px solid transparent;
    cursor: pointer;
    min-height: 40px;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.active {
    background: var(--selected);
    border-color: var(--accent-line);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .glyph {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    flex: none;
    border-radius: var(--radius-sm);
    color: var(--command-accent);
    background: var(--accent-soft);
  }
  .glyph.secret {
    color: var(--secret-accent);
    background: var(--secret-soft);
  }
  .row.system .glyph {
    color: var(--warn);
    background: var(--warn-soft);
  }
  .body {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
    gap: 1px;
  }
  .line {
    display: flex;
    align-items: baseline;
    gap: var(--sp-2);
    min-width: 0;
  }
  .name {
    color: var(--fg);
    font-size: var(--text);
    font-weight: 550;
  }
  .tags {
    display: inline-flex;
    gap: 5px;
    color: var(--fg-dim);
    font-size: var(--text-xs);
  }
  .sub {
    color: var(--fg-muted);
    font-size: var(--text-xs);
  }
  .trail {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    flex: none;
  }
  .type {
    color: var(--fg-muted);
    font-size: var(--text-xs);
  }
  .row.system .name {
    color: var(--warn);
  }
  .state {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-4) var(--sp-3);
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .state strong {
    color: var(--fg);
    font-weight: 600;
  }
  .copied {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin: 0;
    padding: var(--sp-2) var(--sp-3);
    color: var(--ok);
    background: var(--ok-soft);
    border-top: var(--border);
    font-size: var(--text-sm);
  }
  .error-banner {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin: 0;
    border-radius: 0;
    border-left: 0;
    border-right: 0;
  }
  footer {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    padding: var(--sp-2) var(--sp-3);
    border-top: var(--border);
    background: var(--bg-raised);
    color: var(--fg-muted);
    font-size: var(--text-xs);
    flex-wrap: wrap;
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }
  .hintkeys {
    opacity: 0.8;
  }
  .count {
    font-variant-numeric: tabular-nums;
  }
  @media (max-width: 720px) {
    .finder {
      padding: 0;
    }
    .palette {
      width: 100%;
      height: 100%;
      border-radius: 0;
      border: none;
    }
    .trail .type,
    .hint:nth-child(n + 4) {
      display: none;
    }
  }
</style>
