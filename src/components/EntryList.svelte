<script lang="ts">
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import type { EntryMeta } from '../lib/models';
  import { absoluteTime, relativeTime } from '../lib/time';

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
    <EmptyState icon="inbox" title="Nothing here yet" hint={emptyText} />
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
            {#each entry.tags.slice(0, 2) as tag (tag)}
              <span class="tag">#{tag}</span>
            {/each}
            <span class="dot" aria-hidden="true">·</span>
            <span>{usage(entry)}</span>
            {#if entry.updatedAt}
              <span class="dot" aria-hidden="true">·</span>
              <span title={absoluteTime(entry.updatedAt)}>{relativeTime(entry.updatedAt)}</span>
            {/if}
          </span>
        </span>
        {#if entry.kind === 'secret'}
          <!-- A wide "hidden" pill ate enough width that the updated time ellipsised away on
               every secret row at the default pane width. The glyph says the same thing here,
               and the detail pane spells the rule out in full. -->
          <span class="masked" title="Value hidden" role="img" aria-label="Value hidden">
            <Icon name="lock" size={11} />
          </span>
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
    gap: var(--gap-xs);
  }
  .row-wrap {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: var(--row-h);
    padding: var(--sp-2);
    border-radius: var(--radius-row);
    border: 1px solid transparent;
    cursor: pointer;
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
  .kind {
    display: grid;
    place-items: center;
    width: var(--tile-md);
    height: var(--tile-md);
    flex: none;
    border-radius: var(--radius-sm);
    color: var(--kind-command);
    background: var(--accent-soft);
  }
  .secret .kind {
    color: var(--kind-secret);
    background: var(--secret-soft);
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
    /* The ellipsis needs a little air before the trailing badge, otherwise the clipped
       character reads as the end of the word. */
    padding-right: var(--sp-2);
  }
  /* The name and metadata lines clip with a real ellipsis. A flex container cannot ellipsize its
     own overflow, which is why the review screenshots showed rows cut mid-word with a dangling
     separator; these lines are inline text, so the `.truncate` utility only works on them while
     they are laid out inline. */
  .name {
    display: block;
    color: var(--fg);
    font-size: var(--text-row);
    font-weight: 550;
  }
  .pin {
    color: var(--accent);
    display: inline-flex;
    vertical-align: -1px;
    margin-right: var(--gap-keys);
  }
  .meta {
    display: block;
    color: var(--fg-muted);
    font-size: var(--text-row-meta);
  }
  .meta > * + * {
    margin-left: var(--gap-tags);
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
  .masked {
    display: grid;
    place-items: center;
    width: var(--badge-min);
    height: var(--badge-min);
    flex: none;
    border: var(--border);
    border-color: var(--kind-secret);
    border-radius: var(--radius-pill);
    background: var(--secret-soft);
    color: var(--kind-secret);
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
