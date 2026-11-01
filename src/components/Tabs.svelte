<script lang="ts">
  import Icon from './Icon.svelte';
  import type { IconName } from '../lib/icons';

  interface TabItem {
    id: string;
    label: string;
    count: number | null;
    icon: IconName;
    /** Stable automation id (the tab id itself is a backend `Kind`). */
    testId: string;
  }

  let {
    items,
    active,
    onSelect
  }: {
    items: TabItem[];
    active: string;
    onSelect: (id: string) => void;
  } = $props();

  function onkeydown(event: KeyboardEvent): void {
    const index = items.findIndex((item) => item.id === active);
    if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
      event.preventDefault();
      const step = event.key === 'ArrowRight' ? 1 : -1;
      const next = items[(index + step + items.length) % items.length];
      if (next) onSelect(next.id);
    }
  }
</script>

<div class="tabbar" role="tablist" tabindex="-1" aria-label="Entry kinds" onkeydown={onkeydown}>
  {#each items as item (item.id)}
    <button
      role="tab"
      id="tab-{item.id}"
      aria-selected={active === item.id}
      aria-controls="entry-list"
      tabindex={active === item.id ? 0 : -1}
      class="tab"
      class:active={active === item.id}
      data-testid={item.testId}
      onclick={() => onSelect(item.id)}
    >
      <Icon name={item.icon} size={13} />
      <span>{item.label}</span>
      {#if item.count === null}
        <span class="count" title="Count hidden while the vault is locked" aria-label="Count hidden while the vault is locked">
          <Icon name="lock" size={10} />
        </span>
      {:else}
        <span class="count" aria-label="{item.count} entries">{item.count}</span>
      {/if}
    </button>
  {/each}
</div>

<style>
  .tabbar {
    display: flex;
    gap: var(--sp-1);
    border-bottom: var(--border);
    padding: 0 var(--sp-1);
  }
  .tab {
    position: relative;
    min-height: 34px;
    border: 0;
    border-radius: var(--radius) var(--radius) 0 0;
    background: transparent;
    color: var(--fg-muted);
    padding: 0 var(--sp-3);
    gap: var(--sp-2);
  }
  .tab:hover {
    background: var(--bg-hover);
    color: var(--fg);
    border: 0;
  }
  .tab.active {
    color: var(--fg);
    background: transparent;
  }
  /* the active indicator: an accent underline, not a colour-only cue */
  .tab.active::after {
    content: '';
    position: absolute;
    left: var(--sp-2);
    right: var(--sp-2);
    bottom: -1px;
    height: 2px;
    border-radius: 2px;
    background: var(--accent);
  }
  .count {
    font-size: var(--text-xs);
    color: var(--fg-muted);
    background: var(--bg-raised);
    border: var(--border);
    border-radius: 999px;
    padding: 1px 6px;
    min-width: 20px;
    text-align: center;
  }
  .count :global(svg) {
    display: block;
  }
  .tab.active .count {
    color: var(--accent);
    border-color: var(--accent-line);
    background: var(--accent-soft);
  }
</style>
