<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { on } from '../lib/events';
  import type { EnvironmentProject, VaultState } from '../lib/models';
  import Button from './Button.svelte';
  import Dialog from './Dialog.svelte';
  import StatusLine from './StatusLine.svelte';

  let { initialRoot, onClose }: { initialRoot: string; onClose: () => void } = $props();
  let projects = $state<EnvironmentProject[]>([]);
  let root = $state('');
  let busy = $state(false);
  let error = $state('');
  let notice = $state('');
  let alive = true;
  const project = $derived(projects.find((p) => p.root === root));
  const files = $derived([...new Set(project?.snapshots.map((s) => s.file) ?? [])].sort());

  function describe(problem: unknown): string {
    return problem instanceof Error ? problem.message : String(problem);
  }
  async function refresh(): Promise<void> {
    const next = await api.environmentProjects();
    if (alive) projects = next;
  }
  async function sync(path = root): Promise<void> {
    if (busy || !path) return;
    busy = true; error = ''; notice = '';
    try {
      const next = await api.syncEnvironment(path);
      if (!alive) return;
      projects = next.projects;
      root = next.root;
      notice = 'Synced locally. Unchanged files keep their existing version.';
    } catch (problem) { if (alive) error = describe(problem); }
    finally { busy = false; }
  }
  async function browse(): Promise<void> {
    error = '';
    try {
      const path = await api.pickProjectFolder(root);
      if (path && alive) { root = path; await sync(path); }
    } catch (problem) { if (alive) error = describe(problem); }
  }
  async function toggle(): Promise<void> {
    if (!project || busy) return;
    busy = true; error = ''; notice = '';
    try { await api.setEnvironmentSync(root, !project.enabled); await refresh(); }
    catch (problem) { if (alive) error = describe(problem); }
    finally { busy = false; }
  }
  async function copy(id: string): Promise<void> {
    if (busy) return;
    busy = true; error = ''; notice = '';
    try { await api.copyEnvironment(id); if (alive) notice = 'Copied environment file. The secret clipboard expiry applies.'; }
    catch (problem) { if (alive) error = describe(problem); }
    finally { busy = false; }
  }
  onMount(() => {
    root = initialRoot;
    void refresh().then(() => {
      if (!alive) return;
      if (initialRoot) void sync(initialRoot);
      else root = projects[0]?.root || '';
    }).catch((problem) => { if (alive) error = describe(problem); });
    const timer = window.setInterval(() => {
      if (!busy) void refresh().catch((problem) => { if (alive) error = describe(problem); });
    }, 30_000);
    const unsubscribe = on<VaultState>('vault-state-changed', (vault) => {
      if (!vault.unlocked) { projects = []; onClose(); }
    });
    return () => { alive = false; window.clearInterval(timer); unsubscribe(); };
  });
</script>

<Dialog title="Project environment" wide oncancel={onClose}>
  <p>Local, encrypted versions of <code>.env</code> and <code>.env.*</code> files in the project folder. Opening a project group or this section starts sync for that project. Subfolders are not scanned.</p>
  <div class="controls">
    <label for="environment-project">Project</label>
    <select id="environment-project" bind:value={root} disabled={busy} onchange={() => { error = ''; notice = ''; void sync(root); }} data-testid="environment-project">
      {#if !projects.some((p) => p.root === root)}<option value={root}>{root || 'Choose a project folder'}</option>{/if}
      {#each projects as p (p.root)}<option value={p.root}>{p.root}</option>{/each}
    </select>
    <Button icon="folder" disabled={busy} onclick={() => void browse()} data-testid="environment-browse">Browse</Button>
  </div>
  {#if root}<p class="path mono">{root}</p>{/if}
  {#if project}
    <div class="controls">
      <span class="muted">{project.enabled ? 'Syncs every 30 seconds while Dev Console is running and the vault is unlocked.' : 'Automatic sync paused. Saved versions are kept.'}</span>
      <Button disabled={busy} onclick={() => void toggle()} data-testid="environment-toggle">{project.enabled ? 'Pause sync' : 'Resume sync'}</Button>
      <Button disabled={busy} onclick={() => void sync()} data-testid="environment-sync">Sync now</Button>
    </div>
  {/if}
  {#if error}<StatusLine tone="error" message={error} />{/if}
  {#if notice}<StatusLine tone="ok" message={notice} />{/if}
  {#if project && !files.length}
    <p class="muted" data-testid="environment-empty">No environment files found yet. Sync will pick them up when they appear.</p>
  {/if}
  {#each files as file (file)}
    <section class="file" aria-label={file}>
      <h3 class="mono">{file}</h3>
      {#each project?.snapshots.filter((s) => s.file === file) ?? [] as snapshot (snapshot.id)}
        <div class="version" data-testid="environment-version">
          <span>Version {snapshot.version}</span>
          <time datetime={snapshot.savedAt}>{new Date(snapshot.savedAt).toLocaleString()}</time>
          <span class="muted">{snapshot.bytes} bytes</span>
          <Button size="sm" icon="copy" disabled={busy} onclick={() => void copy(snapshot.id)} data-testid="environment-copy">Copy</Button>
        </div>
      {/each}
    </section>
  {/each}
  <p class="muted">Deleting or losing a project file keeps its saved versions here. Use Copy to recover it. These snapshots stay on this machine and are included in encrypted Dev Console backups.</p>
  {#snippet footer()}<Button onclick={onClose}>Close</Button>{/snippet}
</Dialog>

<style>
  .controls { display: flex; align-items: center; flex-wrap: wrap; gap: var(--sp-2); }
  select { flex: 1; min-width: 0; }
  .path { overflow-wrap: anywhere; color: var(--fg-muted); }
  .file { border-top: var(--border-hair); padding-top: var(--sp-2); }
  h3 { margin: 0 0 var(--sp-2); font-size: var(--text-sm); }
  .version { display: flex; align-items: center; flex-wrap: wrap; gap: var(--sp-3); padding: var(--sp-1) 0; }
  time { flex: 1; }
</style>
