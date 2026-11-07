<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity';
  import { api } from '../lib/api';
  import type { ProjectCandidate, ProjectScan, ProjectSource } from '../lib/models';
  import Button from './Button.svelte';
  import Dialog from './Dialog.svelte';
  import StatusLine from './StatusLine.svelte';

  /**
   * Review the runnable tasks a project folder defines before any of them becomes a saved command.
   *
   * The backend reads the folder (`scan_project`) and reads it again when the selection is saved
   * (`import_project`), so this dialog only ever shows what the scanner found: command text is never
   * typed or edited here, and nothing the folder contains is executed by scanning or importing.
   */
  let { ondone }: { ondone: (imported: number) => void } = $props();

  /** Group order, and the heading each source gets in the review list. */
  const GROUPS: { source: ProjectSource; label: string }[] = [
    { source: 'npm', label: 'package.json scripts' },
    { source: 'make', label: 'Makefile targets' },
    { source: 'just', label: 'just recipes' },
    { source: 'compose', label: 'Docker Compose services' }
  ];

  let path = $state('');
  let scan = $state<ProjectScan | null>(null);
  /**
   * Candidate names agreed to. `SvelteSet` is reactive per key, so adding or removing one re-renders
   * exactly the rows that changed.
   */
  const selected = new SvelteSet<string>();
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** A non-failure worth saying out loud, e.g. "everything you picked is already saved". */
  let notice = $state<string | null>(null);

  function describe(problem: unknown, fallback: string): string {
    if (typeof problem === 'string' && problem.length > 0) return problem;
    return problem instanceof Error && problem.message.length > 0 ? problem.message : fallback;
  }

  const candidates = $derived(scan?.candidates ?? []);
  const groups = $derived(
    GROUPS.map((group) => ({
      ...group,
      items: candidates.filter((candidate) => candidate.source === group.source)
    })).filter((group) => group.items.length > 0)
  );
  /** What a selection can contain: everything found that is not already in the library. */
  const importable = $derived(candidates.filter((candidate) => !candidate.alreadyImported));
  const chosen = $derived(importable.filter((candidate) => selected.has(candidate.name)));
  const savedCount = $derived(candidates.length - importable.length);
  const everything = $derived(importable.length > 0 && chosen.length === importable.length);

  /** Group rows that are not already saved, i.e. the rows a group toggle can change. */
  function open(group: { items: ProjectCandidate[] }): ProjectCandidate[] {
    return group.items.filter((candidate) => !candidate.alreadyImported);
  }

  async function rescan(): Promise<void> {
    const target = path.trim();
    if (target.length === 0) {
      error = 'Enter a project folder first.';
      return;
    }
    busy = true;
    error = null;
    notice = null;
    try {
      const found = await api.scanProject(target);
      scan = found;
      // Everything new starts selected; a task the library already has is left alone.
      selected.clear();
      for (const candidate of found.candidates) {
        if (!candidate.alreadyImported) selected.add(candidate.name);
      }
    } catch (problem) {
      scan = null;
      selected.clear();
      error = describe(problem, 'That folder could not be read.');
    } finally {
      busy = false;
    }
  }

  function choose(names: string[], want: boolean): void {
    for (const name of names) {
      if (want) selected.add(name);
      else selected.delete(name);
    }
  }

  async function confirm(): Promise<void> {
    if (!scan || chosen.length === 0) return;
    busy = true;
    error = null;
    notice = null;
    try {
      const outcome = await api.importProject(scan.dir, chosen.map((c) => c.name));
      const refused = outcome.skipped.length > 0;
      if (refused || (outcome.imported === 0 && outcome.duplicates > 0)) {
        // Folder and library disagree: a task could not be saved, or every one of them turned out
        // to be already saved (another window wrote them first, or this import was repeated).
        // Re-read the folder so the rows describe the library as it now is, then say what
        // happened — closing quietly would leave the user believing the import had gone through.
        const wording = refused
          ? `${outcome.skipped.length} could not be saved: ${outcome.skipped.join('; ')}`
          : `All ${outcome.duplicates} ${outcome.duplicates === 1 ? 'task is' : 'tasks are'} already saved here.`;
        await rescan();
        if (refused) error = wording;
        else notice = wording;
        return;
      }
      ondone(outcome.imported);
    } catch (problem) {
      error = describe(problem, 'The tasks could not be imported.');
    } finally {
      busy = false;
    }
  }
</script>

<Dialog title="Import tasks from a project" label="Import tasks from a project" wide oncancel={() => ondone(0)}>
  <form
    class="picker"
    onsubmit={(event) => {
      event.preventDefault();
      void rescan();
    }}
  >
    <label class="field" for="project-path">Project folder</label>
    <div class="path-row">
      <!-- An absolute path is all this needs: the backend reads the folder, it never runs it. -->
      <input
        id="project-path"
        class="path mono"
        type="text"
        placeholder="~/src/my-project"
        autocomplete="off"
        spellcheck="false"
        bind:value={path}
        data-testid="project-path"
      />
      <Button type="submit" icon="search" loading={busy && scan === null} data-testid="project-scan">
        {scan === null ? 'Scan' : 'Rescan'}
      </Button>
    </div>
  </form>

  {#if error}
    <StatusLine tone="error" message={error} />
  {:else if notice}
    <StatusLine tone="info" message={notice} />
  {/if}

  {#if scan === null}
    <p class="hint">
      Dev Console reads the task definitions a folder already publishes — <code>package.json</code>
      scripts, Makefile targets, <code>justfile</code> recipes and Docker Compose services — and offers
      them as commands that run in that folder. Nothing inside the folder is executed by scanning or
      by importing.
    </p>
  {:else}
    <p class="summary" data-testid="project-scan-summary">
      <strong>{scan.project}</strong>
      <span class="dir mono">{scan.dir}</span>
    </p>

    {#if candidates.length === 0}
      <p class="hint" data-testid="project-empty">
        No tasks found. Dev Console looks for <code>package.json</code> scripts, Makefile targets,
        <code>justfile</code> recipes and Docker Compose services in this folder itself.
      </p>
    {:else}
      <div class="bulk">
        <button
          class="link"
          type="button"
          data-testid="project-select-all"
          disabled={importable.length === 0}
          onclick={() => choose(importable.map((c) => c.name), !everything)}
        >
          {everything ? 'Select none' : 'Select all'}
        </button>
        <span class="count" data-testid="project-import-count">
          {chosen.length} of {importable.length} selected
        </span>
      </div>

      <div class="groups" data-testid="project-groups">
        {#each groups as group (group.source)}
          <section class="group" data-testid="project-group">
            <div class="group-head">
              <h4>{group.label}</h4>
              <button
                class="link"
                type="button"
                data-testid="project-group-all"
                onclick={() => {
                  const rows = open(group).map((c) => c.name);
                  choose(rows, !rows.every((name) => selected.has(name)));
                }}
              >
                {open(group).every((c) => selected.has(c.name)) ? 'None' : 'All'}
              </button>
            </div>
            {#each group.items as candidate (candidate.name)}
              <label class="row" class:disabled={candidate.alreadyImported} data-testid="project-candidate">
                <input
                  type="checkbox"
                  checked={candidate.alreadyImported || selected.has(candidate.name)}
                  disabled={candidate.alreadyImported}
                  onchange={() => choose([candidate.name], !selected.has(candidate.name))}
                />
                <span class="text">
                  <span class="name mono">{candidate.name}</span>
                  <span class="command mono">{candidate.command}</span>
                  {#if candidate.description}
                    <span class="description">{candidate.description}</span>
                  {/if}
                </span>
                {#if candidate.alreadyImported}
                  <span class="badge" data-testid="project-already-saved">Already saved</span>
                {/if}
              </label>
            {/each}
          </section>
        {/each}
      </div>

      {#if savedCount > 0}
        <p class="hint">
          {savedCount} of these already run in this folder, so they are left alone.
        </p>
      {/if}
    {/if}
  {/if}

  {#snippet footer()}
    <span class="spacer"></span>
    <Button onclick={() => ondone(0)} data-testid="project-cancel">Cancel</Button>
    <Button
      variant="primary"
      disabled={scan === null || chosen.length === 0}
      loading={busy && scan !== null}
      onclick={() => void confirm()}
      data-testid="project-import"
    >
      Import{chosen.length > 0 ? ` ${chosen.length}` : ''}
    </Button>
  {/snippet}
</Dialog>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
  }
  .field {
    color: var(--fg-muted);
    font-size: var(--text-sm);
    letter-spacing: var(--label-tracking);
    text-transform: uppercase;
  }
  .path-row {
    display: flex;
    gap: var(--gap-sm);
  }
  .path {
    flex: 1;
    min-width: 0;
  }
  .hint {
    margin: 0;
    color: var(--fg-muted);
    font-size: var(--text-sm);
    line-height: 1.5;
    max-width: 74ch;
  }
  .hint code {
    font-family: var(--mono);
    font-size: 0.92em;
  }
  .summary {
    display: flex;
    align-items: baseline;
    gap: var(--gap-sm);
    margin: 0;
    flex-wrap: wrap;
  }
  .dir {
    color: var(--fg-dim);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }
  .bulk {
    display: flex;
    align-items: center;
    gap: var(--gap);
  }
  .count {
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .link {
    background: none;
    border: 0;
    padding: 0;
    color: var(--accent);
    font: inherit;
    font-size: var(--text-sm);
    text-decoration: underline;
    cursor: pointer;
  }
  .link:disabled {
    color: var(--fg-dim);
    text-decoration: none;
    cursor: default;
  }
  /* A project with dozens of scripts must never push the Import action out of reach, so the review
     list scrolls inside the dialog body instead of growing the sheet. */
  .groups {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    min-height: 0;
    max-height: 46vh;
    overflow-y: auto;
    padding-right: var(--sp-1);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
  }
  .group-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--gap-sm);
  }
  .group-head h4 {
    margin: 0;
    font-size: var(--text-sm);
    letter-spacing: var(--label-tracking);
    text-transform: uppercase;
    color: var(--fg-muted);
  }
  .row {
    display: flex;
    align-items: flex-start;
    gap: var(--gap-sm);
    padding: var(--sp-1) var(--sp-2);
    border-radius: var(--radius);
    cursor: pointer;
  }
  .row:hover {
    background: var(--bg-hover);
  }
  .row.disabled {
    cursor: default;
    color: var(--fg-dim);
  }
  .row input {
    margin-top: 0.25em;
    flex: none;
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;
  }
  .name {
    font-size: var(--text-sm);
    color: var(--fg);
    overflow-wrap: anywhere;
  }
  .command {
    font-size: var(--text-sm);
    color: var(--fg-muted);
    overflow-wrap: anywhere;
  }
  .description {
    font-size: var(--text-sm);
    color: var(--fg-dim);
    overflow-wrap: anywhere;
  }
  .badge {
    margin-left: auto;
    flex: none;
    font-size: var(--text-xs);
    color: var(--fg-dim);
    border: var(--border-hair);
    border-radius: var(--radius);
    padding: 0 var(--sp-1);
  }
  .spacer {
    flex: 1;
  }
</style>
