<script lang="ts">
  import Icon from './Icon.svelte';
  import type { IconName } from '../lib/icons';

  let {
    tone = 'info',
    message,
    icon
  }: {
    tone?: 'info' | 'ok' | 'warn' | 'error';
    message: string;
    icon?: IconName;
  } = $props();

  const defaults: Record<typeof tone, IconName> = {
    info: 'check',
    ok: 'check',
    warn: 'alert',
    error: 'alert'
  };

  const iconName = $derived(icon ?? defaults[tone]);
</script>

{#if tone === 'error'}
  <p class="error-banner" role="alert" data-testid="status-error">
    <Icon name={iconName} size={12} /> {message}
  </p>
{:else}
  <p class="status" data-tone={tone} role="status" data-testid="status">
    <Icon name={iconName} size={12} /> <span class="truncate">{message}</span>
  </p>
{/if}

<style>
  .status {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    margin: 0;
    padding: var(--sp-1) var(--sp-3);
    font-size: var(--text-sm);
    color: var(--fg-muted);
    border-bottom: var(--border);
    background: var(--bg-panel);
  }
  .status[data-tone='ok'] {
    color: var(--ok);
    background: var(--ok-soft);
  }
  .status[data-tone='warn'] {
    color: var(--warn);
    background: var(--warn-soft);
  }
  .error-banner {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    margin: 0;
    border-radius: 0;
    border-left: 0;
    border-right: 0;
  }
</style>
