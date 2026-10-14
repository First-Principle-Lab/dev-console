<script lang="ts">
  import Icon from './Icon.svelte';
  import type { IconName } from '../lib/icons';

  type Variant = 'default' | 'primary' | 'ghost' | 'danger';

  let {
    variant = 'default',
    size = 'md',
    icon,
    loading = false,
    label,
    children,
    ...rest
  }: {
    variant?: Variant;
    size?: 'sm' | 'md';
    icon?: IconName;
    loading?: boolean;
    /** Accessible name when there is no visible text (icon-only buttons). */
    label?: string;
    children?: import('svelte').Snippet;
    [key: string]: unknown;
  } = $props();

  const isIconOnly = $derived(Boolean(icon) && !children);
</script>

<button
  {...rest}
  data-variant={variant}
  data-size={size}
  data-icon={isIconOnly ? 'true' : undefined}
  data-loading={loading ? 'true' : undefined}
  aria-busy={loading || undefined}
  aria-label={isIconOnly ? label : undefined}
  disabled={Boolean(rest.disabled) || loading}
>
  {#if loading}
    <span class="spinner" aria-hidden="true"></span>
  {:else if icon}
    <Icon name={icon} size={size === 'sm' ? 13 : 14} />
  {/if}
  {#if children}
    {@render children()}
  {/if}
</button>

<style>
  .spinner {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid currentColor;
    border-right-color: transparent;
    animation: spin 700ms linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
