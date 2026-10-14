import type { Page } from '@playwright/test';
import path from 'node:path';

const MOCK = path.join(import.meta.dirname, 'ipc-mock.js');

export async function boot(page: Page, seed: Record<string, unknown>, view = 'library'): Promise<void> {
  await page.addInitScript((data) => {
    (window as unknown as Record<string, unknown>).__devConsoleSeed = data;
  }, seed);
  await page.addInitScript({ path: MOCK });
  await page.goto(`/${view === 'finder' ? 'index.html#view=finder' : 'index.html'}`);
  await page.waitForFunction(() => '__devConsoleIpc' in window);
}

export function mockState(page: Page): Promise<{
  commands: { id: string; name: string; type: string; pinned: boolean; revision: number; fields: unknown[] }[];
  secrets: { id: string; name: string; value: string; revision: number }[];
  copied: { id: string; kind: string; value: string }[];
  hidden: string[];
  opened: unknown[];
  runs: unknown[];
  manualLock: boolean;
}> {
  return page.evaluate(() => (window as never as { __mock: { state: never } }).__mock.state);
}
