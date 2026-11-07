import type { Page } from '@playwright/test';
import path from 'node:path';

const MOCK = path.join(import.meta.dirname, 'ipc-mock.js');

/// Storage the IPC double keeps across documents, so "stays gone after a reload" tests a real
/// reload instead of whatever the previous test left behind.
const PERSISTED_KEYS = ['dev-console-test-dismissed-suggestions'];

export async function boot(page: Page, seed: Record<string, unknown>, view = 'library'): Promise<void> {
  await page.addInitScript((data) => {
    (window as unknown as Record<string, unknown>).__devConsoleSeed = data;
  }, seed);
  await page.addInitScript({ path: MOCK });
  const url = `/${view === 'finder' ? 'index.html#view=finder' : 'index.html'}`;
  await page.goto(url);
  await page.evaluate((keys) => {
    for (const key of keys) window.localStorage.removeItem(key);
  }, PERSISTED_KEYS);
  // A goto() to the URL the page already has - same document, same fragment - does not reload
  // it, so a second boot() on the same page would inherit the previous component state (a stale
  // "Copied" banner, an open editor). Every boot must start from a fresh document, or a
  // screenshot shows the previous test's state rather than the state under review.
  await page.reload();
  await page.waitForFunction(() => '__devConsoleIpc' in window);
}

export function mockState(page: Page): Promise<{
  commands: { id: string; name: string; type: string; pinned: boolean; revision: number; fields: unknown[] }[];
  secrets: { id: string; name: string; value: string; revision: number }[];
  copied: { id: string; kind: string; value: string }[];
  copiedOutputs: string[];
  hidden: string[];
  opened: unknown[];
  runs: unknown[];
  manualLock: boolean;
  /** The double's stand-in for the detected project: `null` once it has been dismissed. */
  context: { root: string; name: string } | null;
  settings: { projectContextEnabled?: boolean } & Record<string, unknown>;
}> {
  return page.evaluate(() => (window as never as { __mock: { state: never } }).__mock.state);
}
