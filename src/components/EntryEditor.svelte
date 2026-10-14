<script lang="ts">
  import type { EntryDraft } from '../lib/models';
  import { validateDraft } from '../lib/validation';
  import ExtraFields from './ExtraFields.svelte';

  let {
    draft,
    typeSuggestions,
    saving,
    error,
    onSave,
    onCancel
  }: {
    draft: EntryDraft;
    typeSuggestions: string[];
    saving: boolean;
    error: string | null;
    onSave: () => void;
    onCancel: () => void;
  } = $props();

  let tagInput = $state('');
  const isCommand = $derived(draft.kind === 'command');

  const problem = $derived(validateDraft(draft));

  function addTag(): void {
    const tag = tagInput.trim().toLowerCase();
    if (tag && !draft.tags.includes(tag)) draft.tags = [...draft.tags, tag];
    tagInput = '';
  }
</script>

<form
  class="editor"
  data-testid="entry-editor"
  onsubmit={(e) => {
    e.preventDefault();
    onSave();
  }}
>
  <div class="grid">
    <label class="field">
      <span>Name</span>
      <input bind:value={draft.name} data-testid="field-name" required />
    </label>
    <label class="field">
      <span>Type</span>
      <input bind:value={draft.type} list="type-suggestions" data-testid="field-type" required />
      <datalist id="type-suggestions">
        {#each typeSuggestions as suggestion (suggestion)}
          <option value={suggestion}></option>
        {/each}
      </datalist>
    </label>
  </div>

  <label class="field">
    <span>Tags</span>
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
      <button type="button" onclick={addTag}>Add</button>
    </div>
    {#if draft.tags.length > 0}
      <div class="tags">
        {#each draft.tags as tag (tag)}
          <button
            type="button"
            class="tag"
            title="Remove tag"
            onclick={() => (draft.tags = draft.tags.filter((t) => t !== tag))}
          >
            {tag} ×
          </button>
        {/each}
      </div>
    {/if}
  </label>

  <label class="field">
    <span>Description</span>
    <input bind:value={draft.description} />
  </label>

  {#if isCommand}
    <label class="field">
      <span>Command</span>
      <textarea bind:value={draft.command} class="mono" data-testid="field-command"></textarea>
    </label>
    <label class="field">
      <span>Working directory (optional)</span>
      <input bind:value={draft.workingDirectory} class="mono" placeholder="/home/user/project" />
    </label>
  {:else}
    <label class="field">
      <span>Secret value</span>
      <textarea bind:value={draft.value} class="mono secret" data-testid="field-value"></textarea>
    </label>
  {/if}

  <label class="field">
    <span>Notes</span>
    <textarea bind:value={draft.notes}></textarea>
  </label>

  <div class="field">
    <span>Extra information</span>
    <ExtraFields fields={draft.fields} editable={true} onChange={(f) => (draft.fields = f)} />
  </div>

  <label class="row">
    <input type="checkbox" bind:checked={draft.pinned} />
    <span>Suggest on empty finder query</span>
  </label>

  {#if error}
    <p class="error-banner" role="alert" data-testid="save-error">{error}</p>
  {:else if problem && problem.length > 0}
    <p class="muted" data-testid="validation-hint">{problem}</p>
  {/if}

  <div class="actions">
    <button type="submit" data-variant="primary" disabled={saving || problem !== null} data-testid="save">
      {saving ? 'Saving…' : 'Save'}
    </button>
    <button type="button" onclick={onCancel} data-testid="cancel-edit">Cancel</button>
  </div>
</form>

<style>
  .editor {
    padding: 14px 16px 20px;
    overflow-y: auto;
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .field > span {
    color: var(--fg-muted);
    font-size: 12px;
  }
  .tags {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    margin-top: 4px;
  }
  .tag {
    padding: 2px 8px;
    font-size: 12px;
  }
  .mono {
    font-family: var(--font-mono);
  }
  .secret {
    -webkit-text-security: disc;
  }
  .actions {
    display: flex;
    gap: var(--gap-sm);
    margin-top: 6px;
  }
</style>
