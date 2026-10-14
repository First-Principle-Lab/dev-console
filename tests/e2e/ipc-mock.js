/* Browser-side IPC double used only by the Playwright suite.
 *
 * It implements the same command names and DTO shapes as the Rust backend so the Svelte
 * components can be driven without a webview. It proves UI behaviour only; native IPC,
 * real storage and the real vault are covered by the Rust test suite and the native smoke
 * test. Configure a case with window.__devConsoleSeed before the page loads.
 */
(() => {
  const seed = window.__devConsoleSeed || {};
  const now = '2026-01-01T00:00:00Z';

  const state = {
    commands: (seed.commands || []).map((c) => ({ value: '', ...c })),
    secrets: (seed.secrets || []).map((s) => ({ command: '', workingDirectory: '', ...s })),
    vaultUnlocked: seed.vaultUnlocked !== false,
    manualLock: !!seed.manualLock,
    generation: 1,
    settings: seed.settings || {
      theme: 'dark',
      autostart: false,
      clipboardExpirySeconds: 30,
      terminal: ['x-terminal-emulator', '-e']
    },
    copied: [],
    hidden: [],
    opened: [],
    closes: 0,
    quits: 0,
    runs: [],
    failCopy: false,
    searchDelayMs: 0,
    delayedRequestId: 0,
    revisionMismatch: false,
    saveFails: false
  };

  const clone = (value) => JSON.parse(JSON.stringify(value));

  function meta(record, kind) {
    return {
      id: record.id,
      kind,
      revision: record.revision || 1,
      name: record.name,
      type: record.type,
      tags: record.tags || [],
      description: record.description || '',
      pinned: !!record.pinned,
      updatedAt: record.updatedAt || now,
      usageCount: record.usageCount || 0,
      lastUsedAt: record.lastUsedAt || null
    };
  }

  function allMeta() {
    const out = state.commands.map((c) => meta(c, 'command'));
    if (state.vaultUnlocked && !state.manualLock) {
      for (const s of state.secrets) out.push(meta(s, 'secret'));
    }
    return out;
  }

  function tier(entry, term) {
    const name = entry.name.toLowerCase();
    if (term.startsWith('type:')) {
      const value = term.slice(5);
      if (entry.type.toLowerCase() === value) return 0;
      return entry.type.toLowerCase().includes(value) ? 4 : -1;
    }
    if (term.startsWith('tag:')) {
      const value = term.slice(4);
      return entry.tags.some((t) => t.toLowerCase().startsWith(value)) ? 1 : -1;
    }
    if (name === term) return 0;
    if (name.startsWith(term)) return 1;
    if (name.split(/\W+/).some((t) => t.startsWith(term))) return 2;
    if (name.includes(term)) return 3;
    if (entry.description.toLowerCase().includes(term) || entry.tags.some((t) => t.includes(term))) return 4;
    let i = 0;
    for (const ch of name) if (ch === term[i]) i += 1;
    return i === term.length ? 5 : -1;
  }

  function rank(query) {
    const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const limit = 50;
    let scored = allMeta().map((entry) => {
      if (terms.length === 0) return { entry, best: 0 };
      let best = 0;
      for (const term of terms) {
        const t = tier(entry, term);
        if (t < 0) return null;
        best = Math.max(best, t);
      }
      return { entry, best };
    }).filter(Boolean);
    scored.sort((a, b) => {
      if (terms.length === 0) {
        return (!b.entry.pinned) - (!a.entry.pinned) || b.entry.usageCount - a.entry.usageCount || a.entry.name.localeCompare(b.entry.name);
      }
      return a.best - b.best || b.entry.usageCount - a.entry.usageCount || a.entry.name.localeCompare(b.entry.name);
    });
    const trimmed = query.trim();
    return scored.slice(0, limit).map(({ entry }) => ({
      ...entry,
      completion: trimmed && entry.name.toLowerCase().startsWith(trimmed.toLowerCase()) && entry.name !== trimmed ? entry.name : undefined
    }));
  }

  function emit(event, payload) {
    if (typeof window.__devConsoleEmit === 'function') window.__devConsoleEmit(event, payload);
  }

  function requireVault() {
    if (!state.vaultUnlocked || state.manualLock) {
      throw new Error(state.manualLock ? 'the vault is locked' : 'vault storage is unavailable: mocked');
    }
  }

  function details(record, kind, includeValue) {
    return {
      meta: meta(record, kind),
      command: kind === 'command' ? record.command : undefined,
      workingDirectory: record.workingDirectory || '',
      notes: record.notes || '',
      fields: clone(record.fields || []),
      value: includeValue ? record.value : undefined,
      executionApproved: !!record.executionApproved
    };
  }

  async function handle(command, payload) {
    const request = (payload && payload.request) || {};
    if (command === 'app_state') {
      return {
        vault: {
          unlocked: state.vaultUnlocked && !state.manualLock,
          manualLock: state.manualLock,
          keyringAvailable: true,
          keyringUnlocked: true,
          vaultAvailable: true,
          generation: state.generation,
          authenticationRequired: state.manualLock,
          reason: state.vaultUnlocked ? undefined : 'vault is locked'
        },
        version: 'mock',
        authenticatedUnlockAvailable: false,
        clipboardExclusion: { supported: false, mechanism: 'none', detail: 'mocked clipboard provider' }
      };
    }
    if (command === 'search_entries') {
      if (state.searchDelayMs && (!state.delayedRequestId || state.delayedRequestId === request.requestId)) {
        await new Promise((r) => setTimeout(r, state.searchDelayMs));
      }
      return { requestId: request.requestId, vaultGeneration: state.generation, results: rank(request.query || '') };
    }
    if (command === 'list_entries') {
      if (request.kind === 'secret') requireVault();
      const query = (request.query || '').toLowerCase();
      const pool = request.kind === 'command' ? state.commands : state.secrets;
      return pool
        .filter((r) => !query || r.name.toLowerCase().includes(query) || (r.type || '').toLowerCase().includes(query))
        // Same ordering as the backend: pinned first, then name, case-insensitively.
        .sort((a, b) => Number(!!b.pinned) - Number(!!a.pinned) || String(a.name).localeCompare(String(b.name)))
        .map((r) => meta(r, request.kind));
    }
    if (command === 'types_for') {
      const pool = request.kind === 'command' ? state.commands : state.secrets;
      return Array.from(new Set(pool.map((r) => r.type)));
    }
    if (command === 'get_entry') {
      if (request.kind === 'secret') requireVault();
      const pool = request.kind === 'command' ? state.commands : state.secrets;
      const record = pool.find((r) => r.id === request.id);
      if (!record) throw new Error('entry not found');
      return details(record, request.kind, false);
    }
    if (command === 'reveal_secret') {
      requireVault();
      const record = state.secrets.find((r) => r.id === request.id);
      if (!record) throw new Error('entry not found');
      return details(record, 'secret', true);
    }
    if (command === 'save_entry') {
      if (state.saveFails) throw new Error('write failed (mocked)');
      const draft = request.draft;
      if (!draft.name.trim()) throw new Error('name is required');
      if (draft.kind === 'secret') requireVault();
      const pool = draft.kind === 'command' ? state.commands : state.secrets;
      if (draft.id) {
        const record = pool.find((r) => r.id === draft.id);
        if (!record) throw new Error('entry not found');
        if (record.revision !== draft.expectedRevision) throw new Error('this entry was changed elsewhere');
        Object.assign(record, clone(draft), { revision: record.revision + 1, updatedAt: now });
        emit('entries-changed', { kind: draft.kind, id: record.id, revision: record.revision, operation: 'updated', vaultGeneration: state.generation });
        return { id: record.id, revision: record.revision, kind: draft.kind };
      }
      const id = `${draft.kind}-${pool.length + 1}-${Math.random().toString(36).slice(2, 7)}`;
      pool.push({ ...clone(draft), id, revision: 1, createdAt: now, updatedAt: now, usageCount: 0 });
      emit('entries-changed', { kind: draft.kind, id, revision: 1, operation: 'created', vaultGeneration: state.generation });
      return { id, revision: 1, kind: draft.kind };
    }
    if (command === 'delete_entry') {
      if (request.kind === 'secret') requireVault();
      const pool = request.kind === 'command' ? state.commands : state.secrets;
      const index = pool.findIndex((r) => r.id === request.id);
      if (index < 0) throw new Error('entry not found');
      pool.splice(index, 1);
      emit('entries-changed', { kind: request.kind, id: request.id, revision: 0, operation: 'deleted', vaultGeneration: state.generation });
      return null;
    }
    if (command === 'copy_entry') {
      if (state.failCopy) throw new Error('clipboard provider is unavailable');
      const pool = request.kind === 'command' ? state.commands : state.secrets;
      const record = pool.find((r) => r.id === request.id);
      if (!record) throw new Error('entry not found');
      if (request.kind === 'secret') requireVault();
      if (request.expectedRevision && request.expectedRevision !== record.revision) throw new Error('this entry was changed elsewhere');
      state.copied.push({ id: record.id, kind: request.kind, value: request.kind === 'command' ? record.command : record.value });
      return { kind: request.kind, id: record.id, expiresAtMs: Date.now() + 30000, excluded: false, exclusion: { supported: false, mechanism: 'none', detail: 'mocked clipboard provider' } };
    }
    if (command === 'run_command') {
      if (!request.approved) throw new Error('running this command needs explicit approval');
      const record = state.commands.find((r) => r.id === request.id);
      if (!record) throw new Error('entry not found');
      state.runs.push({ id: record.id, revision: record.revision });
      return null;
    }
    if (command === 'lock_vault') {
      state.manualLock = true;
      state.generation += 1;
      emit('vault-state-changed', (await handle('app_state', {})).vault);
      return (await handle('app_state', {})).vault;
    }
    if (command === 'unlock_vault') {
      if (!state.vaultUnlocked) throw new Error('vault storage is unavailable: mocked');
      state.manualLock = false;
      state.generation += 1;
      return (await handle('app_state', {})).vault;
    }
    if (command === 'open_library') {
      state.opened.push({ kind: request.kind, entryId: request.entryId });
      return null;
    }
    if (command === 'hide_self') { state.hidden.push('self'); return null; }
    if (command === 'allow_close') { state.closes += 1; state.hidden.push('close'); return null; }
    if (command === 'cancel_quit') { return null; }
    if (command === 'quit_app') { state.quits += 1; return null; }
    if (command === 'get_settings') return clone(state.settings);
    if (command === 'save_settings') { state.settings = clone(payload.settings); return clone(state.settings); }
    throw new Error(`unmocked command ${command}`);
  }

  window.__mock = {
    state,
    emit,
    lockVault() {
      state.manualLock = true;
      state.generation += 1;
      emit('vault-state-changed', {
        unlocked: false, manualLock: true, keyringAvailable: true, keyringUnlocked: false,
        vaultAvailable: true, generation: state.generation, authenticationRequired: true, reason: 'manually locked'
      });
    },
    unlockVault() {
      state.manualLock = false;
      state.generation += 1;
      emit('vault-state-changed', {
        unlocked: true, manualLock: false, keyringAvailable: true, keyringUnlocked: true,
        vaultAvailable: true, generation: state.generation, authenticationRequired: false
      });
    }
  };

  window.__devConsoleIpc = async (command, payload) => handle(command, payload || {});
})();
