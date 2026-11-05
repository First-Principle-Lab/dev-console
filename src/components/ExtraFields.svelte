<script lang="ts">
  import { api } from '../lib/api';
  import type { ExtraField, FieldDisplay } from '../lib/models';
  import Button from './Button.svelte';

  let {
    fields,
    editable = false,
    onChange
  }: {
    fields: ExtraField[];
    editable?: boolean;
    onChange?: (fields: ExtraField[]) => void;
  } = $props();

  const newId = (): string =>
    `f-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

  function update(index: number, patch: Partial<ExtraField>): void {
    const next = fields.map((f, i) => (i === index ? { ...f, ...patch } : f));
    onChange?.(next);
  }

  function move(index: number, delta: number): void {
    const target = index + delta;
    if (target < 0 || target >= fields.length) return;
    const next = [...fields];
    const [item] = next.splice(index, 1);
    next.splice(target, 0, item);
    onChange?.(next);
  }

  function remove(index: number): void {
    onChange?.(fields.filter((_, i) => i !== index));
  }

  function add(): void {
    onChange?.([...fields, { id: newId(), label: '', value: '', display: 'text' }]);
  }
</script>

<div class="fields" data-testid="extra-fields">
  {#each fields as field, index (field.id)}
    <div class="field" class:editing={editable}>
      {#if editable}
        <input
          class="label-input"
          placeholder="Label"
          value={field.label}
          oninput={(e) => update(index, { label: e.currentTarget.value })}
          aria-label="Field label"
        />
        <select
          value={field.display}
          onchange={(e) => update(index, { display: e.currentTarget.value as FieldDisplay })}
          aria-label="Field kind"
        >
          <option value="text">Text</option>
          <option value="multiline">Multiline</option>
          <option value="url">URL</option>
        </select>
        {#if field.display === 'multiline'}
          <textarea
            value={field.value}
            oninput={(e) => update(index, { value: e.currentTarget.value })}
            aria-label="Field value"
          ></textarea>
        {:else}
          <input
            class="value"
            class:url={field.display === 'url'}
            value={field.value}
            placeholder={field.display === 'url' ? 'https://example.test' : 'Value'}
            oninput={(e) => update(index, { value: e.currentTarget.value })}
            aria-label="Field value"
          />
        {/if}
        <div class="row">
          <button type="button" onclick={() => move(index, -1)} disabled={index === 0} title="Move up">↑</button>
          <button
            type="button"
            onclick={() => move(index, 1)}
            disabled={index === fields.length - 1}
            title="Move down"
          >
            ↓
          </button>
          <button type="button" onclick={() => remove(index)} data-variant="danger" title="Remove">Remove</button>
        </div>
      {:else}
        <div class="viewed">
          <span class="label muted">{field.label || 'Field'}</span>
          {#if field.display === 'url' && /^https?:\/\//i.test(field.value)}
            <button
              type="button"
              class="value url link"
              data-testid="field-url"
              title="Open in browser"
              onclick={() => void api.openUrl(field.value.trim())}>{field.value}</button
            >
          {:else if field.display === 'multiline'}
            <pre class="value multiline">{field.value}</pre>
          {:else}
            <code class="value">{field.value}</code>
          {/if}
        </div>
      {/if}
    </div>
  {/each}
  {#if editable}
    <div class="add">
      <Button type="button" size="sm" variant="ghost" icon="plus" onclick={add} data-testid="add-field"
        >Add field</Button
      >
    </div>
  {/if}
</div>

<style>
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: var(--gap-sm);
  }
  .field {
    border: var(--border-hair);
    border-radius: var(--radius);
    padding: var(--sp-2);
    background: var(--bg-sunken);
    min-width: 0;
  }
  .add {
    grid-column: 1 / -1;
  }
  .field:hover {
    border-color: var(--line-strong);
  }
  .field.editing {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
  }
  .row {
    display: flex;
    gap: var(--sp-1);
  }
  .value {
    word-break: break-word;
  }
  .value.url {
    color: var(--accent);
  }
  button.link {
    background: none;
    border: 0;
    padding: 0;
    font: inherit;
    text-align: left;
    cursor: pointer;
    text-decoration: underline;
  }
  .value.multiline {
    white-space: pre-wrap;
    margin: var(--sp-1) 0 0;
  }
  .viewed {
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
  }
  .viewed .label {
    flex: none;
    color: var(--fg-muted);
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .value {
    color: var(--fg);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }
</style>
