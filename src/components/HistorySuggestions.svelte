<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity';
  import { api } from '../lib/api';
  import Button from './Button.svelte';
  import Icon from './Icon.svelte';
  import type { HistorySuggestion } from '../lib/models';

  /**
   * The most-used commands from the user's own shell history, offered as starting points for a new
   * entry. Nothing here writes anything: saving opens the normal editor with the command prefilled
   * and waits for confirmation, and dismissing only reports the suggestion's digest.
   */
  let {
    suggestions,
    busy = false,
    onSave,
    onDismiss,
    onRescan
  }: {
    suggestions: HistorySuggestion[];
    busy?: boolean;
    onSave: (suggestion: HistorySuggestion) => void;
    onDismiss: (suggestion: HistorySuggestion) => void;
    onRescan: () => void;
  } = $props();

  let open = $state(true);
  let trackingMessage = $state('');
  let installing = $state(false);
  const projects = $derived.by(() => {
    const groups = new SvelteMap<string, { name: string; root: string; count: number; suggestions: HistorySuggestion[] }>();
    for (const suggestion of suggestions) {
      const root = suggestion.projectRoot ?? '';
      const group = groups.get(root) ?? { name: suggestion.project ?? 'Project', root,
        count: suggestion.projectCount ?? 0, suggestions: [] };
      group.suggestions.push(suggestion);
      groups.set(root, group);
    }
    return [...groups.values()];
  });
  async function installTracking(): Promise<void> {
    installing = true;
    try { trackingMessage = await api.installHistoryTracking(); }
    catch (error) { trackingMessage = typeof error === 'string' ? error : 'Could not install project tracking.'; }
    finally { installing = false; }
  }


  function onRowKeydown(event: KeyboardEvent, suggestion: HistorySuggestion): void {
    if (event.key === 'Enter') {
      event.preventDefault();
      onSave(suggestion);
    } else if (event.key === 'Delete') {
      event.preventDefault();
      onDismiss(suggestion);
    }
  }
</script>

  <section
    class="suggest"
    aria-labelledby="history-suggestions-title"
    data-busy={busy ? 'true' : undefined}
    data-testid="history-suggestions"
  >
    <button
      type="button"
      class="head"
      aria-expanded={open}
      aria-controls="history-suggestion-list"
      data-testid="history-suggestions-toggle"
      onclick={() => (open = !open)}
    >
      <span class="head-glyph" aria-hidden="true"><Icon name="history" size={13} /></span>
      <span class="head-title" id="history-suggestions-title">Project history</span>
      {#if projects.length > 0}<span class="chip count">{projects.length} projects</span>{/if}
      <span class="spacer"></span>
      <span class="chevron" class:closed={!open} aria-hidden="true"><Icon name="chevron" size={13} /></span>
    </button>

    {#if open}
      <div id="history-suggestion-list" data-testid="history-suggestion-list">
      {#if suggestions.length === 0}
        <p class="hint muted">Suggestions appear here as you use projects. Set up tracking, then open a new terminal.</p>
        <Button type="button" size="sm" loading={installing} onclick={() => void installTracking()} data-testid="history-tracking-setup">Enable project tracking for zsh</Button>
        {#if trackingMessage}<p class="hint" role="status">{trackingMessage}</p>{/if}
      {/if}
      {#each projects as project (project.root)}
        <details open={projects.length === 1} class="history-project" data-testid="history-project">
          <summary title={project.root}><span class="history-chevron"><Icon name="chevron" size={13} /></span><span class="head-title">{project.name}</span><span class="chip">{project.count} runs</span></summary>
          <ul class="rows">
        {#each project.suggestions as suggestion (suggestion.key)}
          <li class="suggestion" data-testid="history-suggestion-row">
            <!--
              The command itself is the keyboard target: Enter runs the same action as **Save**,
              Delete dismisses. A real button rather than a focusable list item, so it is in the
              tab order and the accessibility tree for the right reason.
            -->
            <button
              type="button"
              class="command mono"
              data-testid="history-suggestion-command"
              aria-label="{suggestion.command} - used {suggestion.count} times. Enter saves it as “{suggestion.suggestedName}”, Delete dismisses it."
              title="Enter saves it, Delete dismisses it"
              onclick={() => onSave(suggestion)}
              onkeydown={(event) => onRowKeydown(event, suggestion)}
            >
              {suggestion.command}
            </button>
            <div class="meta">
              <span class="name muted" title="Saved as “{suggestion.suggestedName}”">
                {suggestion.suggestedName}
              </span>
              <span class="chip uses" data-testid="history-suggestion-count">used {suggestion.count}×</span>
              <span class="spacer"></span>
              <div class="actions">
                <Button
                  size="sm"
                  variant="primary"
                  icon="plus"
                  data-testid="history-suggestion-save"
                  onclick={() => onSave(suggestion)}
                >
                  Save
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  icon="close"
                  label="Dismiss this suggestion"
                  data-testid="history-suggestion-dismiss"
                  onclick={() => onDismiss(suggestion)}
                />
              </div>
            </div>
          </li>
        {/each}
          </ul>
        </details>
      {/each}
      </div>

      <div class="foot">
        <span class="hint muted">
          {busy
            ? 'Reading project counts…'
            : suggestions.length ? 'Top five projects by recorded runs. Counts stay separate per project and folder.' : ''}
        </span>
        <Button
          size="sm"
          variant="ghost"
          icon="update"
          loading={busy}
          data-testid="history-suggestions-rescan"
          onclick={onRescan}
        >
          Rescan
        </Button>
      </div>
    {/if}
  </section>

<style>
  .history-chevron { display: grid; place-items: center; }
  .history-project:not([open]) .history-chevron { transform: rotate(-90deg); }
  .history-project { margin-top: var(--sp-2); }
  .history-project summary { display: flex; align-items: center; gap: var(--gap-sm); padding: var(--sp-1) 0; cursor: pointer; }
  .history-project summary .head-title { flex: 1; }
  .suggest { max-height: 45vh; overflow-y: auto; flex-shrink: 0; }

  .suggest {
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
    padding: var(--sp-1) var(--sp-2) var(--sp-2);
    margin: var(--sp-2) var(--sp-2) 0;
    border: var(--border-hair);
    border-radius: var(--radius-row);
    background: var(--bg-panel);
  }
  .suggest[data-busy='true'] {
    opacity: 0.72;
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    width: 100%;
    padding: var(--sp-1) 0;
    border: 0;
    background: transparent;
    color: var(--fg-muted);
    font-size: var(--text-sm);
    text-align: left;
  }
  /* This is a section disclosure, not an action button. Keep pointer states unboxed;
     the shared :focus-visible ring still marks keyboard focus. */
  .head:hover:not(:disabled),
  .head:active:not(:disabled) {
    background: transparent;
    border-color: transparent;
  }
  .head-glyph {
    display: grid;
    place-items: center;
    color: var(--fg-muted);
  }
  .head-title {
    font-weight: 600;
    color: var(--fg);
  }
  .spacer {
    flex: 1;
  }
  .chevron {
    display: grid;
    place-items: center;
    transition: transform var(--transition);
  }
  .chevron.closed {
    transform: rotate(-90deg);
  }
  .count {
    font-size: var(--text-xs);
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  /* not `.row`: that is a global layout class in tokens.css (centred, for form rows), and in this
     column it would shrink and centre the meta line instead of letting it span the row */
  .suggestion {
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
    padding: var(--sp-1) var(--sp-2);
    border-radius: var(--radius-row);
    border: 1px solid transparent;
    transition: background var(--transition);
  }
  .suggestion:hover {
    background: var(--hover-fill);
  }
  .suggestion:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .command {
    /* the whole width, so a long command truncates only when it really has to */
    display: block;
    width: 100%;
    min-width: 0;
    padding: 0;
    border: 0;
    background: transparent;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--fg);
    text-align: left;
    text-decoration: none;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .command:hover,
  .command:focus-visible {
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  /* one line in every row, however narrow the list pane: the name gives way (ellipsis) before the
     count or the buttons do, so Save never drops to a line of its own in some rows only */
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: nowrap;
    gap: var(--gap-sm);
    min-width: 0;
  }
  .name {
    flex: 0 1 auto;
    min-width: 0;
    font-size: var(--text-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .uses {
    flex: none;
    font-size: var(--text-xs);
    white-space: nowrap;
  }
  .actions {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--gap-xs);
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-2);
    margin-top: var(--sp-1);
  }
  .foot .hint {
    font-size: var(--text-xs);
    min-width: 0;
  }
</style>
