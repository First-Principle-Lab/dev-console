<script lang="ts">
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import { Copy, Pin, PinOff, Trash2 } from '@lucide/svelte';
  import type { EntryMeta } from '../lib/models';

  let {
    entries,
    selectedId,
    loading,
    locked,
    emptyText,
    kind,
    onSelect,
    onCopy,
    onPin,
    onDelete,
    actionsDisabled = false
  }: {
    entries: EntryMeta[];
    selectedId: string | null;
    loading: boolean;
    locked: boolean;
    emptyText: string;
    kind: 'command' | 'secret';
    onSelect: (entry: EntryMeta) => void;
    onCopy: (entry: EntryMeta) => void;
    onPin: (entry: EntryMeta) => void;
    onDelete: (entry: EntryMeta) => void;
    actionsDisabled?: boolean;
  } = $props();

</script>

<div class="list" id="entry-list" role="listbox" aria-label="{kind === 'secret' ? 'Secrets' : 'Commands'}" aria-busy={loading}
  data-testid="entry-list">
  {#if loading}
    <ul class="skeleton" aria-hidden="true">
      {#each [0, 1, 2] as row (row)}
        <li></li>
      {/each}
    </ul>
  {:else if locked}
    <p class="locked-note" data-testid="list-locked">
      <Icon name="lock" size={13} />
      <span>Secret metadata is hidden while the vault is locked.</span>
    </p>
  {:else if entries.length === 0}
    <EmptyState icon="inbox" title="Nothing here yet" hint={emptyText} />
  {:else}
    {#each entries as entry (entry.id)}
      <div
        class="row-wrap"
        role="presentation"
        class:selected={entry.id === selectedId}
        data-entry-id={entry.id}
        data-testid="entry-row"
      >
        <button class="row-select" type="button" role="option" aria-selected={entry.id === selectedId}
          onclick={() => onSelect(entry)} data-testid="select-entry">
          <span class="name truncate">{entry.name}</span>
        </button>
        <div class="row-actions">
          <button class="icon-action" type="button" aria-label={`Copy ${entry.name}`} title={`Copy ${entry.name}`}
            onclick={() => onCopy(entry)} data-testid="row-copy"><Copy size={14} strokeWidth={1.75} /></button>
          <button class="icon-action" class:active={entry.pinned} type="button" aria-label={`${entry.pinned ? 'Unpin' : 'Pin'} ${entry.name}`}
            aria-pressed={entry.pinned} title={`${entry.pinned ? 'Unpin' : 'Pin'} ${entry.name}`} disabled={actionsDisabled}
            onclick={() => onPin(entry)} data-testid="row-pin">
            {#if entry.pinned}<PinOff size={14} strokeWidth={1.75} />{:else}<Pin size={14} strokeWidth={1.75} />{/if}
          </button>
          <button class="icon-action delete-action" type="button" aria-label={`Delete ${entry.name}`} title={`Delete ${entry.name}`}
            disabled={actionsDisabled} onclick={() => onDelete(entry)} data-testid="row-delete"><Trash2 size={14} strokeWidth={1.75} /></button>
        </div>
      </div>
    {/each}
  {/if}
</div>

<style>
  .list {
    height: 100%;
    overflow: auto;
    padding: var(--sp-2);
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
  }
  .row-wrap {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 44px;
    padding: 4px var(--sp-2);
    border-radius: var(--radius-row);
    border: 1px solid transparent;
    transition:
      background var(--transition),
      border-color var(--transition),
      box-shadow var(--transition);
  }
  .row-wrap:hover {
    background: var(--hover-fill);
  }
  /* Selection is a soft band, a quiet edge and a solid left marker: three cues,
     so the selected row survives greyscale and colour-blind viewing. */
  .row-wrap.selected {
    background: var(--selected);
    color: var(--selected-fg);
    border-color: var(--selected-edge);
    box-shadow: var(--selected-marker);
  }
  .row-wrap.selected:hover {
    background: var(--selected);
  }
  .row-select {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
    align-items: center;
    justify-content: flex-start;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    text-align: left;
    white-space: normal;
  }
  .row-select:hover:not(:disabled) {
    background: transparent;
    border: 0;
  }
  .row-select:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .row-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: none;
  }
  .icon-action {
    width: 34px;
    min-width: 34px;
    height: 34px;
    min-height: 34px;
    padding: 0;
    border: 1px solid transparent;
    background: transparent;
    border-radius: var(--radius-sm);
    color: var(--fg-muted);
  }
  .icon-action.active {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .delete-action {
    color: var(--danger);
  }
  .name {
    display: block;
    min-width: 0;
    color: var(--fg);
    font-size: var(--text-row);
    font-weight: 550;
  }
  .locked-note {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin: var(--sp-2);
    padding: var(--sp-2);
    border: var(--border-hair);
    border-radius: var(--radius-row);
    background: var(--warn-soft);
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .skeleton {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .skeleton li {
    height: var(--row-h);
    border-radius: var(--radius-row);
    background: linear-gradient(90deg, var(--bg-panel), var(--bg-raised), var(--bg-panel));
    opacity: 0.6;
  }
</style>
