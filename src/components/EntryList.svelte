<script lang="ts">
  import type { EntryMeta } from '../lib/models';

  let {
    entries,
    selectedId,
    loading,
    locked,
    emptyText,
    onSelect
  }: {
    entries: EntryMeta[];
    selectedId: string | null;
    loading: boolean;
    locked: boolean;
    emptyText: string;
    onSelect: (entry: EntryMeta) => void;
  } = $props();

  function keydown(event: KeyboardEvent, entry: EntryMeta): void {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onSelect(entry);
    }
  }
</script>

<div class="list" role="listbox" aria-label="Entries" aria-busy={loading}>
  {#if locked}
    <p class="state muted">Vault locked. Unlock to browse secrets.</p>
  {:else if loading}
    <p class="state muted">Loading…</p>
  {:else if entries.length === 0}
    <p class="state muted">{emptyText}</p>
  {:else}
    <ul>
      {#each entries as entry (entry.id)}
        <li
          role="option"
          aria-selected={entry.id === selectedId}
          tabindex={entry.id === selectedId ? 0 : -1}
          class:selected={entry.id === selectedId}
          onclick={() => onSelect(entry)}
          onkeydown={(event) => keydown(event, entry)}
          data-testid="entry-row"
        >
          <span class="name" title={entry.name}>
            {#if entry.pinned}<span class="pin" aria-label="pinned">•</span>{/if}{entry.name}</span
          >
          <span class="type muted">{entry.type}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .list {
    height: 100%;
    overflow-y: auto;
    border-right: var(--border);
    background: var(--bg-panel);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    gap: var(--gap-sm);
    align-items: baseline;
    padding: 7px 10px;
    cursor: default;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 45%, transparent);
  }
  li.selected {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pin {
    color: var(--accent);
    margin-right: 4px;
  }
  .type {
    font-size: 12px;
    white-space: nowrap;
  }
  .state {
    padding: 14px 12px;
  }
</style>
