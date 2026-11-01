<script lang="ts">
  import { onMount } from 'svelte';
  import Library from './windows/Library.svelte';
  import Finder from './windows/Finder.svelte';
  import { loadTheme } from './lib/state.svelte';

  const params = new URLSearchParams(location.hash.replace(/^#/, ''));
  const view = params.get('view') === 'finder' ? 'finder' : 'library';

  onMount(() => {
    void loadTheme().catch((error: unknown) => console.error('settings unavailable', error));
  });
</script>

{#if view === 'finder'}
  <Finder />
{:else}
  <Library />
{/if}
