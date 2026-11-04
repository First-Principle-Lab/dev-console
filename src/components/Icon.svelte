<script lang="ts">
  import { ICON_PATHS, type IconName } from '../lib/icons';

  let { name, size = 14 }: { name: IconName; size?: number } = $props();
  const outlined = $derived(name === 'command' || name === 'collection' || name === 'inbox');
</script>

<svg
  class="icon"
  width={size}
  height={size}
  viewBox="0 0 20 20"
  fill={outlined ? 'none' : 'currentColor'}
  stroke={outlined ? 'currentColor' : 'none'}
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  <path d={ICON_PATHS[name]}></path>
</svg>

<style>
  .icon {
    flex: none;
    display: block;
  }
</style>
