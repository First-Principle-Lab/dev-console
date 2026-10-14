<script lang="ts">
  import Badge from './Badge.svelte';
  import Button from './Button.svelte';
  import Icon from './Icon.svelte';
  import ExtraFields from './ExtraFields.svelte';
  import type { EntryDetails as Details, Kind } from '../lib/models';

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
  const isSecret = $derived(kind === 'secret');
  // Fixed-width mask: the masked view must not leak the length of the secret.
  const masked = '•'.repeat(10);

  function updated(iso: string): string {
    const then = Date.parse(iso);
    if (Number.isNaN(then)) return iso;
    const minutes = Math.round((Date.now() - then) / 60000);
    if (minutes < 1) return 'just now';
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.round(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.round(hours / 24);
    if (days < 30) return `${days}d ago`;
    return new Date(then).toLocaleDateString();
  }
</script>

<article class="details" data-testid="entry-details">
  <header class="head">
    <span class="glyph" class:secret={isSecret} aria-hidden="true">
      <Icon name={isSecret ? 'secret' : 'command'} size={16} />
    </span>
    <div class="titles">
      <h2 class="name">{meta.name}</h2>
      <div class="chips">
        <Badge tone={isSecret ? 'secret' : 'command'} icon={isSecret ? 'secret' : 'command'}>
          {isSecret ? 'Secret' : 'Command'}
        </Badge>
        <Badge>{meta.type}</Badge>
        {#each meta.tags as tag (tag)}
          <Badge>#{tag}</Badge>
        {/each}
        {#if meta.pinned}
          <Badge tone="status" icon="pin">Pinned</Badge>
        {/if}
        <Badge tone="status">r{meta.revision}</Badge>
        <span class="muted when" title={meta.updatedAt}>updated {updated(meta.updatedAt)}</span>
      </div>
    </div>
  </header>

  {#if meta.description}
    <p class="description">{meta.description}</p>
  {/if}

  {#if isSecret}
    <section class="section">
      <span class="label">Value {#if !revealed}<span class="muted">(masked)</span>{/if}</span>
      <div class="code secret-value" class:revealed>
        <code data-testid="secret-value">{revealed ? details.value : masked}</code>
        <Button size="sm" variant="ghost" icon="reveal" onclick={onreveal}
          >{revealed ? 'Hide' : 'Reveal'}</Button
        >
      </div>
      <p class="hint muted">
        Values are never shown until you ask, never written to logs, and copied to the clipboard
        only for the expiry window you set.
      </p>
    </section>
  {:else}
    <section class="section">
      <span class="label">Command</span>
      <pre class="code block mono" data-testid="command-text">{details.command}</pre>
      {#if details.workingDirectory}
        <p class="hint muted">
          Runs in <code>{details.workingDirectory}</code>
        </p>
      {/if}
    </section>
  {/if}

  {#if details.notes}
    <section class="section">
      <span class="label">Notes</span>
      <p class="notes">{details.notes}</p>
    </section>
  {/if}

  {#if details.fields.length > 0}
    <section class="section">
      <span class="label">Custom fields</span>
      <ExtraFields fields={details.fields} editable={false} />
    </section>
  {/if}

  <footer class="actions">
    <div class="group">
      <Button variant="primary" icon="copy" disabled={busy} onclick={oncopy} data-testid="copy"
        >Copy</Button
      >
      {#if isSecret}
        <Button icon="reveal" onclick={onreveal} data-testid="reveal"
          >{revealed ? 'Hide' : 'Reveal'}</Button
        >
      {:else}
        <Button icon="run" onclick={onrun} data-testid="run">Run</Button>
      {/if}
    </div>
    <div class="group">
      <Button icon="edit" onclick={onedit} data-testid="edit">Edit</Button>
      <Button icon="pin" onclick={onpin} data-testid="pin">{meta.pinned ? 'Unpin' : 'Pin'}</Button>
      <Button variant="danger" icon="trash" onclick={ondelete} data-testid="delete">Delete</Button>
    </div>
  </footer>
</article>

<style>
  .details {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
    padding: var(--sp-4);
    height: 100%;
    overflow: auto;
  }
  .head {
    display: flex;
    gap: var(--sp-3);
    align-items: flex-start;
  }
  .glyph {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: var(--radius);
    color: var(--command-accent);
    background: var(--accent-soft);
    border: var(--border);
  }
  .glyph.secret {
    color: var(--secret-accent);
    background: var(--secret-soft);
  }
  .titles {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .name {
    font-size: var(--text-xl);
    word-break: break-word;
  }
  .chips {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    flex-wrap: wrap;
  }
  .when {
    font-size: var(--text-xs);
  }
  .description {
    margin: 0;
    color: var(--fg-muted);
    font-size: var(--text-sm);
    max-width: 78ch;
  }
  .section {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .code {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    background: var(--bg-sunken);
    border: var(--border);
    border-radius: var(--radius);
    padding: var(--sp-3);
  }
  .code code {
    flex: 1;
    min-width: 0;
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--fg);
  }
  .code.revealed {
    border-color: var(--secret-accent);
  }
  pre.code.block {
    display: block;
    margin: 0;
    line-height: 1.55;
  }
  .notes {
    margin: 0;
    white-space: pre-wrap;
    color: var(--fg);
    font-size: var(--text-sm);
    max-width: 78ch;
  }
  .hint {
    margin: 0;
    font-size: var(--text-xs);
    max-width: 70ch;
  }
  .actions {
    margin-top: auto;
    display: flex;
    gap: var(--sp-3);
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    padding-top: var(--sp-3);
    border-top: var(--border);
  }
  .group {
    display: flex;
    gap: var(--gap-sm);
    flex-wrap: wrap;
  }
</style>
