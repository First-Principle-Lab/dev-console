import { inTauri } from './api';

/** A release the updater found, ready to download, verify and install. */
export interface PendingUpdate {
  version: string;
  install: () => Promise<void>;
}

/** What the update state needs from the platform; the e2e suite swaps in a double. */
export interface UpdateBackend {
  currentVersion: () => Promise<string>;
  check: () => Promise<PendingUpdate | null>;
  relaunch: () => Promise<void>;
}

declare global {
  interface Window {
    __devConsoleUpdater?: UpdateBackend;
  }
}

export type UpdateStatus = 'idle' | 'checking' | 'current' | 'available' | 'installing' | 'error';

const tauriBackend: UpdateBackend = {
  currentVersion: async () => (await import('@tauri-apps/api/app')).getVersion(),
  check: async () => {
    const { check } = await import('@tauri-apps/plugin-updater');
    const update = await check();
    return update ? { version: update.version, install: () => update.downloadAndInstall() } : null;
  },
  relaunch: async () => (await import('@tauri-apps/plugin-process')).relaunch()
};

function backend(): UpdateBackend | null {
  if (typeof window !== 'undefined' && window.__devConsoleUpdater) return window.__devConsoleUpdater;
  return inTauri() ? tauriBackend : null;
}

function message(problem: unknown, fallback: string): string {
  if (typeof problem === 'string' && problem.length > 0) return problem;
  return problem instanceof Error && problem.message.length > 0 ? problem.message : fallback;
}

/** One shared state, so the header button and Settings always agree. */
export const updates = $state<{
  status: UpdateStatus;
  current: string | null;
  available: PendingUpdate | null;
  error: string | null;
}>({ status: 'idle', current: null, available: null, error: null });

/** Whether this build can update itself at all (false in a plain browser preview). */
export function updatesSupported(): boolean {
  return backend() !== null;
}

export async function checkForUpdates(): Promise<void> {
  const platform = backend();
  if (!platform || updates.status === 'checking' || updates.status === 'installing') return;
  updates.status = 'checking';
  updates.error = null;
  try {
    updates.current ??= await platform.currentVersion();
    updates.available = await platform.check();
    updates.status = updates.available ? 'available' : 'current';
  } catch (problem: unknown) {
    updates.status = 'error';
    updates.error = message(problem, 'Could not check for updates.');
  }
}

export async function installUpdate(): Promise<void> {
  const platform = backend();
  const pending = updates.available;
  if (!platform || !pending || updates.status === 'installing') return;
  updates.status = 'installing';
  updates.error = null;
  try {
    await pending.install();
    await platform.relaunch();
  } catch (problem: unknown) {
    updates.status = 'error';
    updates.error = message(problem, 'The update could not be installed.');
  }
}
