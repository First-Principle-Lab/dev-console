import type { Page } from '@playwright/test';
import path from 'node:path';

const MOCK = path.join(import.meta.dirname, 'ipc-mock.js');

export async function boot(page: Page, seed: Record<string, unknown>, view = 'library'): Promise<void> {
  await page.addInitScript((data) => {
    (window as unknown as Record<string, unknown>).__devConsoleSeed = data;
  }, seed);
  await page.addInitScript({ path: MOCK });
  const url = `/${view === 'finder' ? 'index.html#view=finder' : 'index.html'}`;
  await page.goto(url);
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
}> {
  return page.evaluate(() => (window as never as { __mock: { state: never } }).__mock.state);
}
