<script lang="ts">
  import Badge from './Badge.svelte';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import type { EntryMeta } from '../lib/models';

  let {
    entries,
    selectedId,
    loading,
    locked,
    emptyText,
    kind,
    onSelect
  }: {
    entries: EntryMeta[];
    selectedId: string | null;
    loading: boolean;
    locked: boolean;
    emptyText: string;
    kind: 'command' | 'secret';
    onSelect: (entry: EntryMeta) => void;
  } = $props();

  function updated(iso: string): string {
    const then = Date.parse(iso);
    if (Number.isNaN(then)) return '';
    const minutes = Math.max(0, Math.round((Date.now() - then) / 60000));
    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.round(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.round(hours / 24);
    if (days < 30) return `${days}d ago`;
    return new Date(then).toLocaleDateString();
  }

  function usage(entry: EntryMeta): string {
    if (entry.usageCount === 1) return 'used once';
    if (entry.usageCount > 1) return `used ${entry.usageCount}×`;
    return 'unused';
  }

  function onkeydown(event: KeyboardEvent, entry: EntryMeta): void {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onSelect(entry);
    }
  }
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
    <EmptyState icon={kind === 'secret' ? 'secret' : 'command'} title="Nothing here yet" hint={emptyText} />
  {:else}
    {#each entries as entry (entry.id)}
      <div
        class="row-wrap"
        role="option"
        tabindex={entry.id === selectedId ? 0 : -1}
        aria-selected={entry.id === selectedId}
        class:selected={entry.id === selectedId}
        class:secret={entry.kind === 'secret'}
        onclick={() => onSelect(entry)}
        onkeydown={(event) => onkeydown(event, entry)}
        data-testid="entry-row"
      >
        <span class="kind" aria-hidden="true">
          <Icon name={entry.kind === 'secret' ? 'secret' : 'command'} size={14} />
        </span>
        <span class="main">
          <span class="name truncate">
            {#if entry.pinned}<span class="pin" title="Pinned"><Icon name="pin" size={11} /></span>{/if}
            {entry.name}
          </span>
          <span class="meta truncate">
            <span class="type">{entry.type}</span>
            {#each entry.tags.slice(0, 3) as tag (tag)}
              <span class="tag">#{tag}</span>
            {/each}
            <span class="dot" aria-hidden="true">·</span>
            <span>{usage(entry)}</span>
            {#if entry.updatedAt}
              <span class="dot" aria-hidden="true">·</span>
              <span>{updated(entry.updatedAt)}</span>
            {/if}
          </span>
        </span>
        {#if entry.kind === 'secret'}
          <Badge tone="secret">hidden</Badge>
        {/if}
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
    gap: 2px;
  }
  .row-wrap {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: var(--row-h);
    padding: var(--sp-2) var(--sp-2);
    border-radius: var(--radius);
    border: 1px solid transparent;
    cursor: pointer;
    transition: background var(--transition), border-color var(--transition);
  }
  .row-wrap:hover {
    background: var(--bg-hover);
  }
  .row-wrap.selected {
    background: var(--selected);
    border-color: var(--accent-line);
  }
  .kind {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    flex: none;
    border-radius: var(--radius-sm);
    color: var(--command-accent);
    background: var(--accent-soft);
  }
  .secret .kind {
    color: var(--secret-accent);
    background: var(--secret-soft);
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }
  .name {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    color: var(--fg);
    font-size: var(--text);
    font-weight: 550;
  }
  .pin {
    color: var(--accent);
    display: inline-flex;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--fg-muted);
    font-size: var(--text-xs);
  }
  .type {
    color: var(--fg-muted);
  }
  .tag {
    color: var(--fg-dim);
  }
  .dot {
    color: var(--fg-dim);
  }
  .locked-note {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin: var(--sp-2);
    padding: var(--sp-2);
    border: var(--border);
    border-radius: var(--radius);
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
    border-radius: var(--radius);
    background: linear-gradient(90deg, var(--bg-panel), var(--bg-raised), var(--bg-panel));
    opacity: 0.6;
  }
</style>
