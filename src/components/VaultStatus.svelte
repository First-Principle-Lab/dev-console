<script lang="ts">
  import type { VaultState } from '../lib/models';
  import { api } from '../lib/api';

  let { vault, onunlock }: { vault: VaultState; onunlock?: () => void } = $props();

  let busy = $state(false);
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

<div class="vault" data-testid="vault-status">
  <span class="dot" data-unlocked={vault.unlocked} aria-hidden="true"></span>
  <span>{label}</span>
  <button onclick={toggle} disabled={busy} title={vault.reason ?? ''}>
    {vault.unlocked ? 'Lock' : 'Unlock'}
  </button>
</div>

<style>
  .vault {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    color: var(--fg-muted);
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--danger);
  }
  .dot[data-unlocked='true'] {
    background: var(--ok);
  }
</style>
