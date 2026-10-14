<script lang="ts">
  import { tick } from 'svelte';
  import { onMount } from 'svelte';

  let {
    title,
    label,
    labelledBy,
    oncancel,
    children,
    footer
  }: {
    title?: string;
    /** Accessible name when it should differ from the visible title. */
    label?: string;
    labelledBy?: string;
    oncancel?: () => void;
    children?: import('svelte').Snippet;
    footer?: import('svelte').Snippet;
  } = $props();

  let panel = $state<HTMLDivElement | null>(null);

  onMount(() => {
    void tick().then(() => {
      const focusable = panel?.querySelector<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      (focusable ?? panel)?.focus();
    });
  });

  function onkey(event: KeyboardEvent): void {
    if (event.key === 'Escape' && oncancel) {
      event.stopPropagation();
      oncancel();
    }
  }
</script>

<div class="backdrop" role="presentation" onclick={(event) => oncancel && event.target === event.currentTarget && oncancel()}
  onkeydown={onkey}>
  <div
    class="dialog"
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
    width: min(520px, 100%);
    background: var(--bg-panel);
    border: var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-dialog);
    padding: var(--sp-4);
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    max-height: 90vh;
    overflow: auto;
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
  }
</style>
