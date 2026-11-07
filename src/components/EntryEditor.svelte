<script lang="ts">
  import { api } from '../lib/api';
  import type { EntryDraft } from '../lib/models';
  import { NAME_SPACES_MESSAGE, nameHasSpaces, validateDraft } from '../lib/validation';
  import Button from './Button.svelte';
  import ExtraFields from './ExtraFields.svelte';
  import Icon from './Icon.svelte';

  let {
    draft,
    saving,
    dirty = false,
    error,
    onSave,
    onCancel
  }: {
    draft: EntryDraft;
    saving: boolean;
    dirty?: boolean;
    error: string | null;
    onSave: () => void;
    onCancel: () => void;
  } = $props();

  let valueVisible = $state(false);
  let pickingProject = $state(false);
  let projectError = $state('');
  function changeProject(folder: string): void {
    draft.workingDirectory = folder;
    draft.tags = draft.tags.filter((tag) => !tag.startsWith('project:'));
  }
  async function browseProject(): Promise<void> {
    pickingProject = true; projectError = '';
    try {
      const folder = await api.pickProjectFolder(draft.workingDirectory);
      if (folder) changeProject(folder);
    } catch (error) { projectError = typeof error === 'string' ? error : 'Could not open the folder picker.'; }
    finally { pickingProject = false; }
  }
  let attempted = $state(false);
  const isCommand = $derived(draft.kind === 'command');
  const problem = $derived(validateDraft(draft));
  const spacedName = $derived(nameHasSpaces(draft.name));
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
  {:else if problem && attempted && !spacedName}
    <p class="hint warn" role="alert" data-testid="validation-hint"><Icon name="alert" size={12} /> {problem}</p>
  {/if}

  <div class="fields">
    <label class="field">
      <span class="label">Name</span>
      <input bind:value={draft.name} data-testid="field-name" required autocomplete="off" spellcheck="false"
        aria-describedby={spacedName ? 'name-spaces-hint' : undefined}
        aria-invalid={spacedName || (attempted && draft.name.trim().length === 0)} />
      <!-- Shown as soon as a space is typed, not on save: the user should rename before moving on. -->
      {#if spacedName}
        <span id="name-spaces-hint" class="hint warn" role="alert" data-testid="name-spaces-hint">
          <Icon name="alert" size={12} /> {NAME_SPACES_MESSAGE}
        </span>
      {/if}
    </label>
    {#if isCommand}
      <div class="field">
        <label class="label" for="command-project-folder">Project folder</label>
        <div class="project-folder-row">
          <input id="command-project-folder" class="mono" value={draft.workingDirectory}
            oninput={(event) => changeProject(event.currentTarget.value)}
            placeholder="General, or choose a project folder" data-testid="field-working-directory" />
          <Button type="button" icon="folder" loading={pickingProject} onclick={() => void browseProject()} data-testid="editor-project-browse">Browse…</Button>
        </div>
        {#if projectError}<p class="hint warn" role="alert">{projectError}</p>{/if}
      </div>
      <label class="field">
        <span class="label">Command</span>
        <textarea
          class="mono"
          rows="5"
          spellcheck="false"
          required
          bind:value={draft.command}
          data-testid="field-command"></textarea>
      </label>
    {:else}
      <div class="field">
        <label class="label" for="secret-value-input">Secret value</label>
        <div class="value-row">
          <input
            id="secret-value-input"
            class="mono"
            type={valueVisible ? 'text' : 'password'}
            spellcheck="false"
            autocomplete="off"
            required
            bind:value={draft.value}
            data-testid="field-value"
          />
          <Button type="button" size="sm" variant="ghost" icon="reveal" onclick={() => (valueVisible = !valueVisible)}>
            {valueVisible ? 'Hide' : 'Show'}
          </Button>
        </div>
        <p class="muted hint">Hidden while you type. It is written to the vault encrypted, never to logs.</p>
      </div>
    {/if}
    <label class="field">
      <span class="label">Notes</span>
      <textarea rows="4" bind:value={draft.notes}></textarea>
    </label>
    {#if !isCommand}
      <div class="field">
        <span class="label">Custom fields</span>
        <ExtraFields fields={draft.fields} editable={true} onChange={(f) => (draft.fields = f)} />
      </div>
    {/if}
  </div>
</form>

<style>
  .project-folder-row { display: flex; gap: var(--gap-sm); flex-wrap: wrap; }
  .project-folder-row input { flex: 1 1 180px; min-width: 0; }
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
    padding: var(--sp-4);
    height: 100%;
    overflow: auto;
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding-bottom: var(--sp-3);
    border-bottom: var(--border-hair);
    position: sticky;
    top: 0;
    background: var(--bg);
    z-index: 2;
  }
  /* the same kind tile as the palette row, the list row and the inspector */
  .glyph {
    display: grid;
    place-items: center;
    width: var(--tile-lg);
    height: var(--tile-lg);
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
    gap: var(--gap-xs);
  }
  .titles h3 {
    font-size: var(--text-lg);
  }
  .sub {
    font-size: var(--text-xs);
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    min-width: 0;
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
    .head {
      flex-wrap: wrap;
    }
  }
</style>
