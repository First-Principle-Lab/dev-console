<script lang="ts">
  import Icon from './Icon.svelte';
  import type { IconName } from '../lib/icons';

  let {
    tone = 'neutral',
    icon,
    children
  }: {
    tone?: 'neutral' | 'command' | 'secret' | 'status';
    icon?: IconName;
    children?: import('svelte').Snippet;
  } = $props();
</script>

<span class="chip" data-tone={tone === 'neutral' ? undefined : tone}>
  {#if icon}<Icon name={icon} size={11} />{/if}
  {#if children}{@render children()}{/if}
</span>
