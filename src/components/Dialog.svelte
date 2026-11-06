<script lang="ts">
  import { tick } from 'svelte';
  import { onMount } from 'svelte';

  let {
    title,
    label,
    labelledBy,
    wide = false,
    oncancel,
    children,
    footer
  }: {
    title?: string;
    /** Accessible name when it should differ from the visible title. */
    label?: string;
    labelledBy?: string;
    wide?: boolean;
    oncancel?: () => void;
    children?: import('svelte').Snippet;
    footer?: import('svelte').Snippet;
  } = $props();

  let panel = $state<HTMLDivElement | null>(null);

  onMount(() => {
    window.addEventListener('keydown', onkey, true);
    void tick().then(() => {
      const focusable = panel?.querySelector<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      (focusable ?? panel)?.focus();
    });
    return () => window.removeEventListener('keydown', onkey, true);
  });

  function onkey(event: KeyboardEvent): void {
    if (event.key === 'Escape' && oncancel) {
      event.preventDefault();
      event.stopImmediatePropagation();
      oncancel();
    }
  }
</script>

<div class="backdrop" role="presentation" onclick={(event) => oncancel && event.target === event.currentTarget && oncancel()}
  >
  <div
    class="dialog surface-shell" class:wide
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    aria-label={label ?? title}
    aria-labelledby={labelledBy}
    bind:this={panel}
  >
    {#if title}<h3 class="title">{title}</h3>{/if}
    <div class="body">{@render children?.()}</div>
    {#if footer}
      <div class="foot">{@render footer()}</div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--backdrop);
    display: grid;
    place-items: center;
    padding: var(--sp-4);
    z-index: 40;
  }
  .dialog {
    /* The frame itself (fill, edge, radius, elevation) is .surface-shell from
       tokens.css so every floating surface in the app shares one definition. */
    width: min(520px, 100%);
    padding: var(--sp-4);
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    max-height: 90vh;
    overflow: auto;
  }
  .dialog.wide {
    width: min(780px, 100%);
  }
  .title {
    font-size: var(--text-lg);
    word-break: break-word;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--gap-sm);
    flex-wrap: wrap;
    margin-top: var(--sp-1);
    padding-top: var(--sp-3);
    border-top: var(--border-hair);
  }
</style>
