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

  let panel = $state<HTMLDialogElement | null>(null);

  onMount(() => {
    const trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    window.addEventListener('keydown', onkey, true);
    void tick().then(() => {
      if (!panel?.isConnected) return;
      panel.showModal();
      const focusable = panel.querySelector<HTMLElement>(
        'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])'
      );
      (focusable ?? panel).focus();
    });
    return () => {
      window.removeEventListener('keydown', onkey, true);
      panel?.close();
      if (trigger?.isConnected) trigger.focus();
    };
  });

  function onkey(event: KeyboardEvent): void {
    // Keep Tab cycling through the controls rather than through browser chrome.
    if (event.key === 'Tab' && panel?.open) {
      const controls = [...panel.querySelectorAll<HTMLElement>(
        'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])'
      )].filter((element) => element.tabIndex >= 0 && element.getClientRects().length > 0);
      const first = controls[0];
      const last = controls.at(-1);
      if (!first) { event.preventDefault(); panel.focus(); }
      else if (event.shiftKey && (document.activeElement === first || document.activeElement === panel)) {
        event.preventDefault(); last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault(); first.focus();
      }
    }
    if (event.key === 'Escape' && oncancel) {
      event.preventDefault();
      event.stopImmediatePropagation();
      oncancel();
    }
  }
</script>

<dialog
  class="dialog surface-shell" class:wide
  aria-label={label ?? title}
  aria-labelledby={labelledBy}
  bind:this={panel}
  oncancel={(event) => { event.preventDefault(); oncancel?.(); }}
  onclick={(event) => {
    if (event.target !== panel || !oncancel || !panel) return;
    const bounds = panel.getBoundingClientRect();
    if (event.clientX < bounds.left || event.clientX > bounds.right ||
        event.clientY < bounds.top || event.clientY > bounds.bottom) oncancel();
  }}
>
  {#if title}<h3 class="title">{title}</h3>{/if}
  <div class="body">{@render children?.()}</div>
  {#if footer}
    <div class="foot">{@render footer()}</div>
  {/if}
</dialog>

<style>
  .dialog::backdrop {
    background: var(--backdrop);
  }
  .dialog:not([open]) {
    display: none;
  }
  .dialog {
    /* The frame itself (fill, edge, radius, elevation) is .surface-shell from
       tokens.css so every floating surface in the app shares one definition. */
    width: min(520px, 100%);
    color: var(--fg);
    padding: var(--sp-4);
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    max-height: 90vh;
    max-width: calc(100vw - 2 * var(--sp-4));
    overflow: hidden;
  }
  .dialog.wide {
    width: min(780px, 100%);
  }
  .title {
    flex-shrink: 0;
    font-size: var(--text-lg);
    word-break: break-word;
  }
  .body {
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .foot {
    flex-shrink: 0;
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
