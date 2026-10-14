<script lang="ts">
  import Icon from './Icon.svelte';
  import type { IconName } from '../lib/icons';

  let {
    icon = 'search',
    title,
    hint,
    actions
  }: {
    icon?: IconName;
    title: string;
    hint?: string;
    actions?: import('svelte').Snippet;
  } = $props();
</script>

<div class="empty" data-testid="empty-state">
  <span class="glyph" aria-hidden="true"><Icon name={icon} size={20} /></span>
  <p class="title">{title}</p>
  {#if hint}<p class="hint">{hint}</p>{/if}
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-6) var(--sp-4);
    text-align: center;
  }
  .glyph {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    color: var(--accent);
    background: var(--accent-soft);
    border: var(--border);
  }
  .title {
    margin: 0;
    color: var(--fg);
    font-size: var(--text);
    font-weight: 600;
  }
  .hint {
    margin: 0;
    color: var(--fg-muted);
    font-size: var(--text-sm);
    max-width: 46ch;
  }
  .actions {
    display: flex;
    gap: var(--gap-sm);
    margin-top: var(--sp-2);
  }
</style>
