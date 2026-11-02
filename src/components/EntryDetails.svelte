<script lang="ts">
  import Badge from './Badge.svelte';
  import Button from './Button.svelte';
  import Icon from './Icon.svelte';
  import ExtraFields from './ExtraFields.svelte';
  import type { EntryDetails as Details, Kind } from '../lib/models';
  import { absoluteTime, relativeTime } from '../lib/time';

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

</script>

<article class="details" data-testid="entry-details">
  <header class="head">
    <span class="glyph" class:secret={isSecret} aria-hidden="true">
      <Icon name={isSecret ? 'secret' : 'command'} size={16} />
    </span>
    <div class="titles">
      <h2 class="name">{meta.name}</h2>
      <!-- One prominent thing (the kind) and one quiet context line, using the same
           separator idiom as a finder row and the entry list, instead of six equally
           prominent chips. Every value that used to be a chip is still on screen. -->
      <div class="headline">
        <Badge tone={isSecret ? 'secret' : 'command'} icon={isSecret ? 'secret' : 'command'}>
          {isSecret ? 'Secret' : 'Command'}
        </Badge>
        <span class="ctx">
          <span class="ctx-part">{meta.type}</span>
          {#each meta.tags as tag (tag)}
            <span class="ctx-part">#{tag}</span>
          {/each}
          {#if meta.pinned}
            <span class="ctx-part"><span class="pin"><Icon name="pin" size={11} /></span>Pinned</span>
          {/if}
          <span class="ctx-part ctx-dim">r{meta.revision}</span>
          <span class="ctx-part ctx-dim" title={absoluteTime(meta.updatedAt)}>
            updated {relativeTime(meta.updatedAt)}
          </span>
        </span>
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
    /* no bottom padding: the sticky action bar supplies it, so the bar can sit flush
       with the bottom edge of the pane instead of floating above a gap */
    padding: var(--sp-4) var(--sp-4) 0;
    height: 100%;
    overflow: auto;
  }
  .head {
    display: flex;
    gap: var(--sp-3);
    align-items: flex-start;
  }
  /* the kind tile at the list/palette size scale, no extra frame of its own */
  .glyph {
    display: grid;
    place-items: center;
    width: var(--tile-lg);
    height: var(--tile-lg);
    flex: none;
    border-radius: var(--radius-row);
    color: var(--kind-command);
    background: var(--accent-soft);
  }
  .glyph.secret {
    color: var(--kind-secret);
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
  .headline {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    flex-wrap: wrap;
    min-width: 0;
  }
  /* the secondary context line: same type, colour and separator as a palette row,
     so one visual language covers the list, the palette and the inspector */
  .ctx {
    display: block;
    color: var(--fg-muted);
    font-size: var(--text-row-meta);
    line-height: var(--leading-tight);
    min-width: 0;
  }
  .ctx-part + .ctx-part::before {
    content: '·';
    margin: 0 var(--gap-tags);
    color: var(--fg-dim);
  }
  .ctx-dim {
    color: var(--fg-dim);
  }
  /* the pinned marker is the same accent glyph the list row uses */
  .pin {
    color: var(--accent);
    display: inline-flex;
    vertical-align: -1px;
    margin-right: var(--gap-keys);
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
  /* one box recipe for every mono surface in the app: --bg-sunken + the hairline
     the shared .block utility uses, so a command block and a run confirmation are
     the same object */
  .code {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    background: var(--bg-sunken);
    border: var(--border-hair);
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
    border-color: var(--kind-secret);
    box-shadow: var(--selected-marker);
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
  /* The action area belongs to the pane, not to the end of the record: it sticks to
     the bottom edge of the scrolling inspector so Copy / Run / Edit / Delete are on
     screen at the 760x520 minimum window and a keyboard user never tabs into a
     control that is off screen. The hairline is the one separator it needs. */
  .actions {
    position: sticky;
    bottom: 0;
    margin: auto calc(-1 * var(--sp-4)) 0;
    padding: var(--sp-3) var(--sp-4);
    display: flex;
    gap: var(--sp-3);
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    background: var(--bg);
    border-top: var(--border-hair);
  }
  .group {
    display: flex;
    gap: var(--gap-sm);
    flex-wrap: wrap;
  }
</style>
