import type {
  AppStateInfo,
  CopyResult,
  EntryDetails,
  EntryDraft,
  EntryMeta,
  HistorySuggestion,
  Kind,
  SavedEntry,
  SearchResponse,
  ArchiveSummary,
  BackupInfo,
  BackupOutcome,
  ProjectImportOutcome,
  ProjectScan,
  RecoveryKeyInfo,
  RunResult,
  Settings,
  TerminalOption,
  VaultState
} from './models';

export type IpcCall = (command: string, payload?: Record<string, unknown>) => Promise<unknown>;

declare global {
  interface Window {
    __devConsoleIpc?: IpcCall;
    __devConsoleEmit?: (event: string, payload: unknown) => void;
  }
}

export function inTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function call<T>(command: string, payload?: Record<string, unknown>): Promise<T> {
  if (typeof window !== 'undefined' && window.__devConsoleIpc) {
    return (await window.__devConsoleIpc(command, payload)) as T;
  }
  if (inTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    return (await invoke(command, payload ?? {})) as T;
  }
  throw new Error(`ipc unavailable: ${command}`);
}

export interface ListRequest {
  kind: Kind;
  query?: string;
  type?: string;
  tag?: string;
}

export const api = {
  createBackup: (): Promise<string> => call('create_backup'),
  listBackups: (): Promise<BackupInfo[]> => call('list_backups'),
  restoreBackup: (name: string): Promise<BackupOutcome> => call('restore_backup', { name }),
  inspectBackup: (name: string): Promise<ArchiveSummary> => call('inspect_backup', { name }),
  inspectArchive: (path: string): Promise<ArchiveSummary> => call('inspect_archive', { path }),
  recoveryKey: (): Promise<RecoveryKeyInfo> => call('recovery_key'),
  copyRecoveryKey: (): Promise<CopyResult> => call('copy_recovery_key'),
  exportArchive: (path: string, overwrite = false): Promise<number> =>
    call('export_archive', { path, overwrite }),
  importArchive: (path: string): Promise<BackupOutcome> => call('import_archive', { path }),
  appState: (): Promise<AppStateInfo> => call('app_state'),
  search: (query: string, requestId: number): Promise<SearchResponse> =>
    call('search_entries', { request: { query, requestId } }),
  /** Forgets the detected project for as long as this finder stays open. */
  clearFinderContext: (): Promise<void> => call('clear_finder_context'),
  list: (req: ListRequest): Promise<EntryMeta[]> => call('list_entries', { request: req }),
  types: (kind: Kind): Promise<string[]> => call('types_for', { request: { kind } }),
  details: (kind: Kind, id: string): Promise<EntryDetails> =>
    call('get_entry', { request: { kind, id } }),
  reveal: (kind: Kind, id: string): Promise<EntryDetails> =>
    call('reveal_secret', { request: { kind, id } }),
  save: (draft: EntryDraft): Promise<SavedEntry> => call('save_entry', { request: { draft } }),
  setPinned: (kind: Kind, id: string, expectedRevision: number, pinned: boolean): Promise<SavedEntry> =>
    call('set_pinned', { request: { kind, id, expectedRevision, pinned } }),
  openUrl: (url: string): Promise<void> => call('open_url', { url }),
  restartApp: (): Promise<void> => call('restart_app'),
  remove: (kind: Kind, id: string, expectedRevision: number): Promise<void> =>
    call('delete_entry', { request: { kind, id, expectedRevision } }),
  copy: (kind: Kind, id: string, expectedRevision: number): Promise<CopyResult> =>
    call('copy_entry', { request: { kind, id, expectedRevision } }),
  copyCommandOutput: (text: string): Promise<CopyResult> => call('copy_command_output', { text }),
  run: (id: string, expectedRevision: number, approved: boolean): Promise<RunResult> =>
    call('run_command', { request: { id, expectedRevision, approved } }),
  lockVault: (): Promise<VaultState> => call('lock_vault'),
  unlockVault: (): Promise<VaultState> => call('unlock_vault'),
  openLibrary: (kind: Kind, entryId?: string): Promise<void> =>
    call('open_library', { request: { kind, entryId } }),
  hideSelf: (): Promise<void> => call('hide_self'),
  allowClose: (): Promise<void> => call('allow_close'),
  quit: (): Promise<void> => call('quit_app'),
  settings: (): Promise<Settings> => call('get_settings'),
  terminals: (): Promise<TerminalOption[]> => call('list_terminals'),
  saveSettings: (settings: Settings): Promise<Settings> => call('save_settings', { settings }),
  /** Ranked suggestions from the shell's history. The backend reads the file per call. */
  installHistoryTracking: (): Promise<string> => call('install_history_tracking'),
  historySuggestions: (limit?: number): Promise<HistorySuggestion[]> =>
    call('history_suggestions', { limit: limit ?? null }),
  dismissSuggestion: (key: string): Promise<void> => call('dismiss_suggestion', { key }),
  /** How many suggestions are dismissed, for the Settings row that brings them back. */
  dismissedSuggestionCount: (): Promise<number> => call('dismissed_suggestion_count'),
  /** Forgets every dismissal; returns how many there were. */
  clearDismissedSuggestions: (): Promise<number> => call('clear_dismissed_suggestions'),
  pickProjectFolder: async (defaultPath?: string): Promise<string | null> => {
    if (typeof window !== 'undefined' && window.__devConsoleIpc) {
      return call('pick_project_folder', { defaultPath });
    }
    const { open } = await import('@tauri-apps/plugin-dialog');
    return open({
      title: 'Choose a project folder',
      directory: true,
      multiple: false,
      defaultPath: defaultPath?.startsWith('/') ? defaultPath : undefined
    });
  },
  scanProject: (path: string): Promise<ProjectScan> => call('scan_project', { request: { path } }),
  importProject: (path: string, names: string[]): Promise<ProjectImportOutcome> =>
    call('import_project', { request: { path, names } })
};
