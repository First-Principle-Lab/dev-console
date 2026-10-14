<script lang="ts">
  import type { EntryDetails as Details, Kind } from '../lib/models';
  import ExtraFields from './ExtraFields.svelte';

  let {
    details,
    kind,
    revealed,
    busy,
    oncopy,
    onreveal,
    onedit,
    onpin,
    onrun,
    ondelete
  }: {
    details: Details;
    kind: Kind;
    revealed: boolean;
    busy: boolean;
    oncopy: () => void;
    onreveal: () => void;
    onedit: () => void;
    onpin: () => void;
    onrun: () => void;
    ondelete: () => void;
  } = $props();

  const meta = $derived(details.meta);
</script>

<div class="details" data-testid="entry-details">
  <h2 class="name">{meta.name}{#if meta.pinned}<span class="pin" title="Pinned"> • pinned</span>{/if}</h2>
  <dl>
    <div class="pair"><dt>Type</dt><dd>{meta.type}</dd></div>
    <div class="pair"><dt>Tags</dt><dd>{meta.tags.join(', ') || '—'}</dd></div>
    <div class="pair wide"><dt>Description</dt><dd>{details.meta.description || '—'}</dd></div>
  </dl>

  {#if kind === 'command'}
    <span class="block">Command</span>
    <pre class="cmd mono" data-testid="command-text">{details.command}</pre>
    {#if details.workingDirectory}
      <span class="block">Working directory</span>
      <pre class="mono muted">{details.workingDirectory}</pre>
    {/if}
  {:else}
    <span class="block">Secret value</span>
    <pre class="cmd mono" data-testid="secret-value">{revealed ? details.value : '••••••••••'}</pre>
  {/if}

  {#if details.notes}
    <span class="block">Notes</span>
    <pre class="notes">{details.notes}</pre>
  {/if}

  {#if details.fields.length > 0}
    <span class="block">Extra information</span>
    <ExtraFields fields={details.fields} editable={false} onChange={() => {}} />
  {/if}

  <div class="actions">
    <button onclick={oncopy} disabled={busy} data-testid="copy">Copy</button>
    {#if kind === 'secret' && !revealed}
      <button onclick={onreveal} disabled={busy} data-testid="reveal">Reveal</button>
    {/if}
    <button onclick={onedit} disabled={busy} data-testid="edit">Edit</button>
    <button onclick={onpin} disabled={busy} data-testid="pin">{meta.pinned ? 'Unpin' : 'Pin'}</button>
    {#if kind === 'command'}
      <button onclick={onrun} disabled={busy} data-testid="run">Run</button>
    {/if}
    <span class="spacer"></span>
    <button onclick={ondelete} disabled={busy} data-variant="danger" data-testid="delete">Delete</button>
  </div>
</div>

<style>
  .details {
    padding: 14px 16px 18px;
    overflow-y: auto;
    height: 100%;
  }
  .name {
    font-size: 17px;
    margin: 0 0 10px;
    word-break: break-word;
  }
  .pin {
    font-size: 12px;
    color: var(--accent);
    font-weight: 400;
  }
  dl {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 6px 18px;
    margin: 0 0 12px;
  }
  .pair.wide {
    grid-column: 1 / -1;
  }
  dt {
    color: var(--fg-muted);
    font-size: 12px;
  }
  dd {
    margin: 0;
    word-break: break-word;
  }
  .block {
    display: block;
    color: var(--fg-muted);
    font-size: 12px;
    margin: 10px 0 3px;
  }
  .cmd,
  .notes {
    background: var(--bg-raised);
    border: var(--border);
    border-radius: var(--radius);
    padding: 8px 10px;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .notes {
    font-family: var(--font-ui);
  }
  .actions {
    display: flex;
    gap: var(--gap-sm);
    margin-top: 16px;
    flex-wrap: wrap;
  }
</style>
