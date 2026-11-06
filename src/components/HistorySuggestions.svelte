<script lang="ts">
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

  function onRowKeydown(event: KeyboardEvent, suggestion: HistorySuggestion): void {
    if (event.key === 'Enter') {
      event.preventDefault();
      onSave(suggestion);
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault();
      onDismiss(suggestion);
    }
  }
</script>

{#if suggestions.length > 0}
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
      <span class="head-glyph" aria-hidden="true"><Icon name="sparkle" size={13} /></span>
      <span class="head-title" id="history-suggestions-title">From your shell history</span>
      <span class="chip count">{suggestions.length}</span>
      <span class="spacer"></span>
      <span class="chevron" class:closed={!open} aria-hidden="true"><Icon name="chevron" size={13} /></span>
    </button>

    {#if open}
      <ul class="rows" id="history-suggestion-list" data-testid="history-suggestion-list">
        {#each suggestions as suggestion (suggestion.key)}
          <li class="row" data-testid="history-suggestion-row">
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

      <div class="foot">
        <span class="hint muted">
          {busy
            ? 'Reading your history…'
            : 'Read on this machine only. Enter saves, Delete dismisses; a dismissal is remembered as a hash.'}
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
{/if}

<style>
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
  .head-glyph {
    display: grid;
    place-items: center;
    color: var(--accent);
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
  .row {
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
    padding: var(--sp-1) var(--sp-2);
    border-radius: var(--radius-row);
    border: 1px solid transparent;
    transition: background var(--transition);
  }
  .row:hover {
    background: var(--hover-fill);
  }
  .row:focus-visible {
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
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--gap-sm);
    min-width: 0;
  }
  .name {
    min-width: 0;
    font-size: var(--text-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .uses {
    font-size: var(--text-xs);
    white-space: nowrap;
  }
  .actions {
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
