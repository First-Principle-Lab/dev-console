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
  const RECOVERY_SECRET = 'AGE-SECRET-KEY-1SYNTHETICSYNTHETICSYNTHETICSYNTHETICSYNTHETICSYNTH1';
  const RECOVERY_COPY_ID = 'recovery-credential';

  const state = {
    backups: [],
    exportedPaths: [],
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
    terminals: seed.terminals || [
      { id: 'ghostty', name: 'Ghostty', command: ['/usr/bin/ghostty', '-e'] },
      { id: 'kitty', name: 'Kitty', command: ['/usr/bin/kitty', '--'] }
    ],
    copied: [],
    copiedOutputs: [],
    hidden: [],
    opened: [],
    closes: 0,
    quits: 0,
    runs: [],
    failCopy: false,
    failCopyOutput: false,
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
        return (
          (!b.entry.pinned) - (!a.entry.pinned) ||
          (b.entry.lastUsedAt || '').localeCompare(a.entry.lastUsedAt || '') ||
          b.entry.usageCount - a.entry.usageCount ||
          a.entry.name.localeCompare(b.entry.name)
        );
      }
      return a.best - b.best || b.entry.usageCount - a.entry.usageCount || a.entry.name.localeCompare(b.entry.name);
    });
    const trimmed = query.trim();
    // Only the fields models.rs SearchResult serializes: a field the finder reads that the real
    // backend never sends must fail here too, not just in the shipped app.
    return scored.slice(0, limit).map(({ entry }) => ({
      id: entry.id,
      kind: entry.kind,
      name: entry.name,
      type: entry.type,
      tags: entry.tags,
      description: entry.description,
      pinned: entry.pinned,
      usageCount: entry.usageCount,
      lastUsedAt: entry.lastUsedAt,
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

  /// Mirrors the backend's clipboard.clear_owned(): an app-owned secret copy is dropped when
  /// the vault locks; anything the user copied afterwards is untouched.
  function clearOwnedCopies() {
    state.copied = state.copied.filter((c) => !c.owned);
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
        clipboardExclusion: { supported: false, mechanism: 'none', detail: 'mocked clipboard provider' },
        backupDir: '/home/mock/.local/share/dev-console/backups'
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
    if (command === 'open_url') {
      (window.__openedUrls = window.__openedUrls || []).push(payload.url);
      return null;
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
    if (command === 'set_pinned') {
      if (request.kind === 'secret') requireVault();
      const pool = request.kind === 'command' ? state.commands : state.secrets;
      const record = pool.find((r) => r.id === request.id);
      if (!record) throw new Error('entry not found');
      if (record.revision !== request.expectedRevision) throw new Error('this entry was changed elsewhere');
      record.pinned = request.pinned;
      record.revision += 1;
      record.updatedAt = now;
      emit('entries-changed', { kind: request.kind, id: record.id, revision: record.revision, operation: 'updated', vaultGeneration: state.generation });
      return { id: record.id, revision: record.revision, kind: request.kind };
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
    if (command === 'copy_command_output') {
      if (state.failCopyOutput) throw new Error('clipboard provider is unavailable');
      if (!payload.text || payload.text.length > 64 * 1024) throw new Error('command output is empty or too large to copy');
      state.copiedOutputs.push(payload.text);
      return { kind: 'command', id: 'command-output', expiresAtMs: Date.now() + 30000, excluded: false, exclusion: { supported: false, mechanism: 'none', detail: 'mocked clipboard provider' } };
    }
    if (command === 'run_command') {
      if (!request.approved) throw new Error('running this command needs explicit approval');
      const record = state.commands.find((r) => r.id === request.id);
      if (!record) throw new Error('entry not found');
      state.runs.push({ id: record.id, revision: record.revision });
      const terminal = /(^|[^a-zA-Z0-9_])sudo(?=$|[^a-zA-Z0-9_])/.test(record.command);
      if (terminal) return { mode: 'terminal', stdout: '', stderr: '', exitCode: null, truncated: false, timedOut: false };
      return { mode: 'inline', stdout: '', stderr: '', exitCode: 0, truncated: false, timedOut: false,
        ...(seed.runResults?.[record.command] || {}) };
    }
    if (command === 'run_command_captured') {
      if (!request.approved) throw new Error('running this command needs explicit approval');
      const record = state.commands.find((r) => r.id === request.id);
      if (!record) throw new Error('entry not found');
      state.runs.push({ id: record.id, revision: record.revision, captured: true });
      const fails = record.command.includes('fail');
      return {
        exitCode: fails ? 2 : 0,
        stdout: fails ? '' : 'mock stdout for ' + record.command + '\n',
        stderr: fails ? 'mock stderr: it broke\n' : '',
        truncated: false,
        timedOut: false,
        durationMs: 12
      };
    }
    if (command === 'lock_vault') {
      state.manualLock = true;
      clearOwnedCopies();
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
    if (command === 'create_backup') {
      if (!state.vaultUnlocked || state.manualLock) throw new Error('the vault is locked');
      const name = `dev-console-backup-${state.backups.length + 1}.age`;
      state.backups.push({ name, size: 20480, modifiedAtMs: Date.now() });
      return name;
    }
    if (command === 'list_backups') return clone(state.backups);
    if (command === 'restore_backup') {
      const found = state.backups.find((b) => b.name === payload.name);
      if (!found) throw new Error('that backup file could not be read');
      emit('entries-changed', { kind: 'command', id: '', revision: 0, operation: 'reloaded', vaultGeneration: state.generation });
      return { source: payload.name, commands: 2, secrets: 1 };
    }
    if (command === 'inspect_backup') {
      const found = state.backups.find((b) => b.name === payload.name);
      if (!found) throw new Error('that backup file could not be read');
      return { source: payload.name, createdAt: now, commands: state.commands.length, secrets: state.secrets.length, sizeBytes: found.size };
    }
    if (command === 'inspect_archive') {
      if (!payload.path || !payload.path.endsWith('.age')) throw new Error('the archive is not a valid Dev Console backup');
      return { source: payload.path, createdAt: now, commands: state.commands.length, secrets: state.secrets.length, sizeBytes: 20480 };
    }
    if (command === 'recovery_key') {
      // Same gate as the Rust handler: window check *and* an unlocked vault.
      requireVault();
      return { secret: RECOVERY_SECRET, path: '/home/mock/.config/dev-console/recovery.agekey' };
    }
    if (command === 'copy_recovery_key') {
      // The protected native copy path: vault-gated, expiry-bound, and the value is never
      // handed back to the renderer.
      requireVault();
      const seconds = state.settings.clipboardExpirySeconds || 30;
      state.copied.push({ id: RECOVERY_COPY_ID, kind: 'secret', value: RECOVERY_SECRET, owned: true });
      return {
        kind: 'secret',
        id: RECOVERY_COPY_ID,
        expiresAtMs: Date.now() + seconds * 1000,
        excluded: false,
        exclusion: { supported: false, mechanism: 'none', detail: 'mocked clipboard provider' }
      };
    }
    if (command === 'export_archive') {
      if (!payload.path) throw new Error('choose a destination file first');
      if (state.exportedPaths.includes(payload.path) && !payload.overwrite) {
        throw new Error('that file already exists - nothing was overwritten. choose another name, or confirm that the existing file may be replaced');
      }
      if (!state.exportedPaths.includes(payload.path)) state.exportedPaths.push(payload.path);
      return 20480;
    }
    if (command === 'import_archive') {
      if (!payload.path.endsWith('.age')) throw new Error('the archive is not a valid Dev Console backup');
      return { source: payload.path, commands: 2, secrets: 1 };
    }
    if (command === 'list_terminals') return clone(state.terminals);
    if (command === 'save_settings') { state.settings = clone(payload.settings); return clone(state.settings); }
    throw new Error(`unmocked command ${command}`);
  }

  // Stands in for the Tauri updater and process plugins. Only installed when a test seeds
  // `updater`, so every other spec still sees a build without self-update.
  if (seed.updater) {
    state.updater = { checks: 0, installs: 0, relaunches: 0, ...seed.updater };
    window.__devConsoleUpdater = {
      currentVersion: async () => state.updater.current,
      check: async () => {
        state.updater.checks += 1;
        if (state.updater.failCheck) throw new Error(state.updater.failCheck);
        if (!state.updater.available) return null;
        return {
          version: state.updater.available,
          install: async () => {
            if (state.updater.failInstall) throw new Error(state.updater.failInstall);
            state.updater.installs += 1;
          }
        };
      },
      relaunch: async () => {
        state.updater.relaunches += 1;
      }
    };
  }

  window.__mock = {
    state,
    emit,
    lockVault() {
      state.manualLock = true;
      clearOwnedCopies();
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
