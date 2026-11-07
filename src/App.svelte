<script lang="ts">
  import { onMount } from 'svelte';
  import Library from './windows/Library.svelte';
  import Finder from './windows/Finder.svelte';
  import { on } from './lib/events';
  import type { Theme } from './lib/models';
  import { loadTheme, acceptSavedTheme } from './lib/state.svelte';

  const params = new URLSearchParams(location.hash.replace(/^#/, ''));
  const view = params.get('view') === 'finder' ? 'finder' : 'library';
  // Finder stays opaque on Hyprland and shares the selected theme with Library.
  document.documentElement.dataset.view = view;

  onMount(() => {
    const reload = (): void => { void loadTheme().catch((error: unknown) => console.error('theme unavailable', error)); };
    const unsubscribe = on<Theme>('theme-changed', acceptSavedTheme);
    const unsubscribeShown = on('finder-shown', reload);
    reload();
    return () => { unsubscribe(); unsubscribeShown(); };
  });
</script>

{#if view === 'finder'}
  <Finder />
{:else}
  <Library />
{/if}
