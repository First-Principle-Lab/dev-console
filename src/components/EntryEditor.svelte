<script lang="ts">
  import type { EntryDraft } from '../lib/models';
  import { validateDraft } from '../lib/validation';
  import Button from './Button.svelte';
  import ExtraFields from './ExtraFields.svelte';
  import Icon from './Icon.svelte';

  let {
    draft,
    typeSuggestions,
    saving,
    dirty = false,
    error,
    onSave,
    onCancel
  }: {
    draft: EntryDraft;
    typeSuggestions: string[];
    saving: boolean;
    dirty?: boolean;
    error: string | null;
    onSave: () => void;
    onCancel: () => void;
  } = $props();

  let tagInput = $state('');
  let valueVisible = $state(false);
  // A brand-new entry has no type yet, so flagging that the moment the editor opens reads as a
  // failure rather than as guidance. The hint and the invalid markers appear with the first save
  // attempt; saving stays gated on the same validation either way.
  let attempted = $state(false);
  const isCommand = $derived(draft.kind === 'command');
  const problem = $derived(validateDraft(draft));

  function addTag(): void {
    const tag = tagInput.trim().toLowerCase();
    if (tag && !draft.tags.includes(tag)) draft.tags = [...draft.tags, tag];
    tagInput = '';
  }
</script>

<!-- The app owns validation and its messaging: the native bubbles are unstyled, would be the
     only error UI inside a borderless window, and would bypass the draft-preserving save path.
     The fields keep `required` so the semantics stay in the accessibility tree. -->
<form
  class="editor"
  data-testid="entry-editor"
  novalidate
  onsubmit={(e) => {
    e.preventDefault();
    attempted = true;
    onSave();
  }}
>
  <header class="head">
    <span class="glyph" class:secret={!isCommand} aria-hidden="true">
      <Icon name={isCommand ? 'command' : 'secret'} size={15} />
    </span>
    <div class="titles">
      <h3>{draft.id ? 'Edit' : 'New'} {isCommand ? 'command' : 'secret'}</h3>
      {#if dirty}
        <span class="chip" data-tone="status" data-testid="dirty-indicator">
          <Icon name="alert" size={10} /> unsaved changes
        </span>
      {:else}
        <span class="sub muted">{isCommand ? 'Runs in your terminal after approval' : 'Encrypted with the vault key'}</span>
      {/if}
    </div>
    <span class="spacer"></span>
    <Button type="button" size="sm" onclick={onCancel} data-testid="cancel-edit">Cancel</Button
    >
    <Button type="submit" variant="primary" size="sm" icon="check" loading={saving} data-testid="save"
      >Save</Button
    >
  </header>

  {#if error}
    <p class="error-banner" role="alert" data-testid="save-error">
      <Icon name="alert" size={12} /> {error}
    </p>
  {:else if problem && attempted}
    <p class="hint warn" role="alert" data-testid="validation-hint"><Icon name="alert" size={12} /> {problem}</p>
  {/if}

  <fieldset class="group">
    <legend>Identity</legend>
    <div class="grid">
      <label class="field">
        <span class="label">Name</span>
        <input bind:value={draft.name} data-testid="field-name" required autocomplete="off"
          aria-invalid={attempted && draft.name.trim().length === 0} />
      </label>
      <label class="field">
        <span class="label">Type</span>
        <input bind:value={draft.type} list="type-suggestions" data-testid="field-type" required autocomplete="off"
          aria-invalid={attempted && draft.type.trim().length === 0} />
        <datalist id="type-suggestions">
          {#each typeSuggestions as suggestion (suggestion)}
            <option value={suggestion}></option>
          {/each}
        </datalist>
      </label>
    </div>

    <div class="field">
      <span class="label">Tags</span>
      <div class="row">
        <input
          bind:value={tagInput}
          placeholder="Add a tag and press Enter"
          aria-label="New tag"
          onkeydown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              addTag();
            }
          }}
        />
        <Button size="sm" variant="ghost" icon="plus" onclick={addTag}>Add</Button>
      </div>
      {#if draft.tags.length > 0}
        <div class="tags">
          {#each draft.tags as tag (tag)}
            <button
              type="button"
              class="chip"
              data-tone="status"
              title="Remove tag"
              onclick={() => (draft.tags = draft.tags.filter((t) => t !== tag))}
            >
              #{tag} ×
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <label class="row checkbox">
      <input type="checkbox" bind:checked={draft.pinned} />
      <span>Pin to the top of the list</span>
    </label>
  </fieldset>

  <fieldset class="group">
    <legend>{isCommand ? 'Command' : 'Secret'}</legend>
    {#if isCommand}
      <label class="field">
        <span class="label">Shell command</span>
        <textarea
          class="mono"
          rows="4"
          spellcheck="false"
          aria-label="Command"
          bind:value={draft.command}
          data-testid="field-command"></textarea>
      </label>
      <label class="field">
        <span class="label">Working directory (optional)</span>
        <input class="mono" bind:value={draft.workingDirectory} placeholder="/home/user/project" autocomplete="off" />
      </label>
    {:else}
      <div class="field">
        <span class="label">Secret value</span>
        <div class="value-row">
          <input
            class="mono"
            type={valueVisible ? 'text' : 'password'}
            spellcheck="false"
            autocomplete="off"
            bind:value={draft.value}
            data-testid="field-value"
          />
          <Button size="sm" variant="ghost" icon="reveal" onclick={() => (valueVisible = !valueVisible)}>
            {valueVisible ? 'Hide' : 'Show'}
          </Button>
        </div>
        <p class="muted hint">Hidden while you type. It is written to the vault encrypted, never to logs.</p>
      </div>
    {/if}
  </fieldset>

  <fieldset class="group">
    <legend>Context</legend>
    <label class="field">
      <span class="label">Description</span>
      <input bind:value={draft.description} autocomplete="off" />
    </label>
    <label class="field">
      <span class="label">Notes</span>
      <textarea rows="3" bind:value={draft.notes}></textarea>
    </label>
    <div class="field">
      <span class="label">Custom fields</span>
      <ExtraFields fields={draft.fields} editable={true} onChange={(f) => (draft.fields = f)} />
    </div>
  </fieldset>
</form>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    padding: var(--sp-4);
    height: 100%;
    overflow: auto;
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding-bottom: var(--sp-3);
    border-bottom: var(--border);
    position: sticky;
    top: 0;
    background: var(--bg);
    z-index: 2;
  }
  .glyph {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
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
    gap: var(--gap-xs);
  }
  .titles h3 {
    font-size: var(--text-lg);
  }
  .sub {
    font-size: var(--text-xs);
  }
  .group {
    border: var(--border);
    border-radius: var(--radius-lg);
    padding: var(--sp-3);
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    background: var(--bg-panel);
  }
  .group legend {
    padding: 0 var(--sp-1);
    color: var(--fg-muted);
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 200px;
    gap: var(--sp-3);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    min-width: 0;
  }
  .checkbox {
    justify-content: flex-start;
    gap: var(--sp-2);
    color: var(--fg-muted);
    font-size: var(--text-sm);
    cursor: pointer;
  }
  .tags {
    display: flex;
    gap: var(--sp-1);
    flex-wrap: wrap;
  }
  .tags button {
    min-height: 0;
    padding: 2px var(--sp-2);
  }
  .value-row {
    display: flex;
    gap: var(--sp-2);
    align-items: center;
  }
  .hint {
    margin: 0;
    font-size: var(--text-xs);
    display: flex;
    align-items: center;
    gap: var(--sp-1);
  }
  .hint.warn {
    color: var(--warn);
    background: var(--warn-soft);
    border: var(--border-hair);
    border-radius: var(--radius);
    padding: var(--sp-2);
  }
  @media (max-width: 720px) {
    .grid {
      grid-template-columns: 1fr;
    }
    .head {
      flex-wrap: wrap;
    }
  }
</style>
