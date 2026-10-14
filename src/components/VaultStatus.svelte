<script lang="ts">
  import { api } from '../lib/api';
  import type { VaultState } from '../lib/models';
  import Button from './Button.svelte';
  import Icon from './Icon.svelte';

  let { vault, onunlock }: { vault: VaultState; onunlock?: () => void } = $props();

  let busy = $state(false);

  const tone = $derived(
    vault.unlocked ? 'ok' : vault.manualLock ? 'warn' : vault.vaultAvailable ? 'warn' : 'error'
  );

  const label = $derived(
    vault.unlocked
      ? 'Vault unlocked'
      : vault.manualLock
        ? 'Vault locked'
        : vault.vaultAvailable
          ? 'Vault unavailable'
          : 'Keyring unavailable'
  );

  async function toggle(): Promise<void> {
    busy = true;
    try {
      if (vault.unlocked) await api.lockVault();
      else onunlock?.();
    } catch (error: unknown) {
      console.error('vault toggle failed', error);
    } finally {
      busy = false;
    }
  }
</script>

<div class="vault" data-tone={tone} data-testid="vault-status" title={vault.reason ?? ''}>
  <span class="glyph" aria-hidden="true">
    <Icon name={vault.unlocked ? 'unlock' : 'lock'} size={13} />
  </span>
  <span class="text">
    <span class="state-text">{label}</span>
    {#if !vault.unlocked && vault.keyringAvailable && !vault.manualLock}
      <span class="sub">keyring present, vault sealed</span>
    {/if}
  </span>
  <Button
    size="sm"
    variant={vault.unlocked ? 'ghost' : 'primary'}
    icon={vault.unlocked ? 'lock' : 'unlock'}
    disabled={busy}
    onclick={toggle}
    data-testid="vault-toggle">{vault.unlocked ? 'Lock' : 'Unlock'}</Button
  >
</div>

<style>
  .vault {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: 3px var(--sp-2) 3px 3px;
    border: var(--border);
    border-radius: 999px;
    background: var(--bg-raised);
    white-space: nowrap;
  }
  .glyph {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--bg-sunken);
  }
  .vault[data-tone='ok'] .glyph {
    color: var(--ok);
  }
  .vault[data-tone='warn'] .glyph {
    color: var(--warn);
  }
  .vault[data-tone='error'] .glyph {
    color: var(--danger);
  }
  .vault[data-tone='ok'] {
    border-color: var(--line);
  }
  .vault[data-tone='warn'],
  .vault[data-tone='error'] {
    border-color: var(--line-strong);
  }
  .text {
    display: flex;
    flex-direction: column;
    line-height: 1.15;
  }
  .state-text {
    font-size: var(--text-sm);
    color: var(--fg);
  }
  .sub {
    font-size: 10px;
    color: var(--fg-muted);
  }
  @media (max-width: 560px) {
    .state-text,
    .sub {
      display: none;
    }
    .vault {
      padding-right: 3px;
    }
  }
</style>
