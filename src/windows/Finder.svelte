<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Icon from '../components/Icon.svelte';
  import Keycap from '../components/Keycap.svelte';
  import { api } from '../lib/api';
  import { on } from '../lib/events';
  import { displayedQuery, nextSelection, selected, shouldCommit, tabCompletion } from '../lib/finder-state';
  import type { FinderContext, Kind, SearchResult, VaultState } from '../lib/models';

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
  /**
   * The project the terminal the palette was opened from is in, once the backend has worked it
   * out. `null` means no chip and the ranking every version of this app had.
   */
  let context = $state<FinderContext | null>(null);

  const rows = $derived<SearchResult[]>(
    vault && !vault.unlocked
      ? [
          ...results,
          {
            id: UNLOCK_ID,
            kind: 'command',
            name: 'Unlock vault to search secrets',
            type: 'System',
            tags: [],
            description: '',
            pinned: false,
            usageCount: 0,
            lastUsedAt: null,
            // The unlock prompt is not a command; it belongs to no project.
            inContext: false
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
    // Keep the last list on screen until the fresh one arrives: blanking it here showed
    // "Searching…" on every open.
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

  /**
   * Ignores the detected project for as long as this palette stays open: the chip goes, and the
   * next search is ranked the way it would have been with no project at all. The next open looks
   * again, so this is never a setting - it is "not this time".
   */
  async function ignoreContext(): Promise<void> {
    context = null;
    // A new order, so a new first row: keeping the old selection would leave it mid-list.
    selectedId = null;
    try {
      await api.clearFinderContext();
    } catch {
      // The chip is already gone locally; the backend simply still has the context for this open.
    }
    await runSearch(query);
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
      case ' ':
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
        } else if (event.ctrlKey && event.key.toLowerCase() === 'p' && context) {
          // Ctrl+P: "rank everything, not just this project's".
          event.preventDefault();
          await ignoreContext();
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
      }),
      // Detection runs beside the palette, so it usually lands a frame after the first search.
      // Re-searching when it does is what puts the rows in the order the chip promises.
      on<FinderContext | null>('finder-context', (payload) => {
        const next = payload ?? null;
        if (next?.root === context?.root && next?.name === context?.name) return;
        context = next;
        // The first search usually ran before this arrived and selected its own top row. The
        // project's commands now lead, so the selection starts again at the top: Enter straight
        // after Super+F must copy the first project command, not whatever led a moment ago.
        selectedId = null;
        void runSearch(query).catch(() => undefined);
      })
    ];
    void api
      .appState()
      .then((state) => {
        vault = state.vault;
        // A palette created after detection answered (a slow webview, a re-shown window) still
        // needs to know what it is ranking by.
        context = state.finderContext ?? null;
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
  <div class="palette surface-shell">
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
      {#if context}
        <button
          type="button"
          class="project-chip"
          onclick={() => void ignoreContext()}
          title={`${context.name} — its commands are ranked first. Click, or press Ctrl+P, to ignore that.`}
          aria-label={`Ranking the project ${context.name} first. Activate to ignore it.`}
          data-testid="finder-context"
        >
          <Icon name="project" size={12} />
          <span class="project-name">{context.name}</span>
        </button>
      {/if}
      {#if vault}
        <span class="vaultchip" data-tone={vault.unlocked ? 'ok' : 'warn'} data-testid="finder-vault">
          <Icon name={vault.unlocked ? 'unlock' : 'lock'} size={12} />
          {vault.unlocked ? 'Vault unlocked' : 'Vault locked'}
        </span>
      {/if}
    </div>

    <ul id="finder-results" class="results" role="listbox" aria-label="Results" bind:this={list}>
      {#each rows as row (row.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- keyboard activation is handled at window level (listbox pattern: the input owns
             the key focus, rows follow aria-activedescendant), see the <svelte:window> handler -->
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
          tabindex={row.id === selectedId ? 0 : -1}
          data-testid="result-row"
        >
          <span class="glyph" class:secret={row.kind === 'secret'} aria-hidden="true">
            <Icon name={row.id === UNLOCK_ID ? 'lock' : row.kind === 'secret' ? 'secret' : 'command'} size={14} />
          </span>
          <span class="body">
            <span class="name truncate">{row.name}</span>
            {#if row.id === UNLOCK_ID}
              <span class="ctx truncate">Secret metadata stays encrypted until you unlock the vault.</span>
            {:else if row.type || row.description || row.tags.length > 0}
              <span class="ctx truncate">
                {#if row.type}<span class="ctx-part">{row.type}</span>{/if}
                {#if row.description}<span class="ctx-part">{row.description}</span>{/if}
                {#each row.tags.slice(0, 2) as tag (tag)}<span class="ctx-part">#{tag}</span>{/each}
              </span>
            {/if}
          </span>
          <span class="trail">
            {#if row.inContext}
              <span
                class="in-project"
                role="img"
                aria-label={`in ${context?.name ?? 'the current project'}`}
                title={`in ${context?.name ?? 'the current project'}`}
                data-testid="row-in-project"
              >
                <Icon name="project" size={11} />
              </span>
            {/if}
            {#if row.id === selectedId}
              <span data-testid="row-action" aria-hidden="true"><Keycap keys={['Enter']} /></span>
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
      <span class="hint"><Keycap keys={['Esc']} /> dismiss</span>
      <span class="spacer"></span>
      <span class="count muted">{rows.length}</span>
    </footer>
  </div>
</div>

<svelte:window on:keydown={onKeydown} />

<style>
  /* The palette frame. In Tauri the window is 680x420 and borderless; in the browser
     preview the same frame is centred, and on narrow screens it fills the viewport.
     The fill, edge, radius and elevation come from the shared .surface-shell utility;
     nothing is painted here so the shell is defined in exactly one place. */
  .finder {
    height: 100%;
    background: transparent;
    display: grid;
    place-items: center;
    padding: 0;
  }
  .palette {
    width: min(680px, 100%);
    height: min(420px, 100%);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  /* The search line: the tallest thing in the palette, no box of its own, no bright
     rule. Focus reads as a soft wash across the line plus the accent caret. */
  .search {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: var(--search-h);
    padding: 0 var(--sp-3);
    border-bottom: var(--border-hair);
    transition: background var(--transition);
  }
  .search:focus-within {
    background: var(--search-focus-bg);
  }
  .search-icon {
    color: var(--fg-dim);
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
    min-height: var(--search-h);
    font-size: var(--text-search);
    font-weight: 550;
    color: var(--search-fg);
  }
  .field input:focus {
    background: transparent;
    border: none;
    outline: none;
  }
  .field input::placeholder {
    color: var(--search-placeholder);
    font-weight: 450;
  }
  .mirror {
    position: absolute;
    inset: 0;
    font-size: var(--text-search);
    font-weight: 550;
    line-height: var(--search-h);
    white-space: pre;
    pointer-events: none;
    overflow: hidden;
    color: var(--search-fg);
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
    font-size: var(--text-row-meta);
    padding: var(--gap-xs) var(--sp-2);
    border: var(--border-hair);
    border-radius: var(--radius-pill);
    color: var(--fg-muted);
    white-space: nowrap;
  }
  .vaultchip[data-tone='ok'] {
    color: var(--ok);
  }
  .vaultchip[data-tone='warn'] {
    color: var(--warn);
  }
  /* The project chip is a button because dismissing it is an action, but it reads as part of the
     search line: same height and type as the vault chip beside it, no button chrome. */
  .project-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--gap-tags);
    min-width: 0;
    max-width: 40%;
    padding: var(--gap-xs) var(--sp-2);
    border: var(--border-hair);
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--fg-dim);
    font-size: var(--text-row-meta);
    font-family: var(--font-ui);
    white-space: nowrap;
    cursor: pointer;
    transition:
      color var(--transition),
      border-color var(--transition);
  }
  .project-chip:hover,
  .project-chip:focus-visible {
    color: var(--fg-muted);
    border-color: var(--line-strong);
  }
  .project-name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .in-project {
    display: grid;
    place-items: center;
    color: var(--fg-dim);
  }
  .results {
    list-style: none;
    margin: 0;
    padding: var(--sp-2);
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
    overflow: auto;
    flex: 1;
  }
  /* One result pattern for every row: kind glyph, name first, one quieter context
     line, right-aligned action. Kind lives in the glyph tint, not in a pill per row. */
  .row {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: var(--row-h);
    padding: var(--sp-1) var(--sp-2);
    border-radius: var(--radius-row);
    border: 1px solid transparent;
    cursor: pointer;
    transition:
      background var(--transition),
      border-color var(--transition),
      box-shadow var(--transition);
  }
  .row:hover {
    background: var(--hover-fill);
  }
  /* Selection is a soft band, a quiet edge and the solid inset marker: the marker is
     a shape change, so selection survives greyscale and colour-blind viewing. */
  .row.active {
    background: var(--selected);
    color: var(--selected-fg);
    border-color: var(--selected-edge);
    box-shadow: var(--selected-marker);
  }
  .row.active:hover {
    background: var(--selected);
  }
  .glyph {
    display: grid;
    place-items: center;
    width: var(--tile-md);
    height: var(--tile-md);
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
    justify-content: center;
    min-width: 0;
    flex: 1;
    gap: var(--gap-xs);
  }
  .name {
    display: block;
    color: var(--fg);
    font-size: var(--text-row);
    font-weight: 550;
    line-height: var(--leading-tight);
  }
  .ctx {
    display: block;
    color: var(--fg-muted);
    font-size: var(--text-row-meta);
    line-height: var(--leading-tight);
  }
  .ctx-part + .ctx-part::before {
    content: '·';
    margin: 0 var(--gap-tags);
    color: var(--fg-dim);
  }
  .row.system .name {
    color: var(--warn);
  }
  .trail {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    flex: none;
  }
  .state {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-3) var(--sp-2);
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .state strong {
    color: var(--fg);
    font-weight: 600;
  }
  .sub {
    color: var(--fg-dim);
  }
  /* Copied / copy-failed read as one line attached to the shell, same hierarchy as
     the rows above them: hairline on top, quiet fill, no second box. */
  .copied {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin: 0;
    padding: var(--sp-2) var(--sp-3);
    color: var(--ok);
    background: var(--ok-soft);
    border-top: var(--border-hair);
    font-size: var(--text-sm);
  }
  .error-banner {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin: 0;
    padding: var(--sp-2) var(--sp-3);
    font-size: var(--text-sm);
    color: var(--danger);
    background: var(--danger-soft);
    border: 0;
    border-top: var(--border-hair);
    border-radius: 0;
  }
  footer {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-height: var(--footer-h);
    padding: 0 var(--sp-3);
    border-top: var(--border-hair);
    color: var(--footer-fg);
    font-size: var(--text-row-meta);
    flex-wrap: wrap;
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: var(--gap-tags);
    white-space: nowrap;
  }
  .count {
    font-variant-numeric: tabular-nums;
  }
  /* The one real palette window is 680px wide, so the narrow-screen fallback has to
     trigger below that, not at 720 - otherwise the shipped window loses its
     details/dismiss hints. */
  @media (max-width: 520px) {
    .finder {
      padding: 0;
    }
    .palette {
      width: 100%;
      height: 100%;
      border: none;
      border-radius: 0;
      box-shadow: none;
    }
    .hint:nth-child(n + 4) {
      display: none;
    }
  }
</style>
