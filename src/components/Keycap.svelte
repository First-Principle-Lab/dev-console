<script lang="ts">
  let { keys }: { keys: string[] } = $props();
</script>

<span class="keys">
  {#each keys as key, index (key + index)}
    {#if index > 0}<span class="sep" aria-hidden="true">+</span>{/if}
    <kbd>{key}</kbd>
  {/each}
</span>

<style>
  .keys {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .sep {
    color: var(--fg-dim);
    font-size: var(--text-xs);
  }
</style>
