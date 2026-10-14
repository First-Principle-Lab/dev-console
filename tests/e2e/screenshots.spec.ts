import { test, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';

import { boot } from './helpers';

/**
 * Visual review captures. These are UI-shell checks: the browser preview cannot reach native
 * IPC, so every page here runs against the documented IPC test double (`ipc-mock.js`) and the
 * native binary is exercised separately by `scripts/smoke-native.sh`.
 *
 *   SHOT_SET=before bun run test:e2e -- screenshots   # captures artifacts/ui-review/before/
 *   SHOT_SET=after  bun run test:e2e -- screenshots   # captures artifacts/ui-review/after/
 *
 * Secrets use obviously synthetic values and are captured masked unless the state under
 * review is explicitly a reveal state (there is none).
 */

const SET = process.env.SHOT_SET ?? 'after';
const OUT_DIR = path.join(import.meta.dirname, '..', '..', 'artifacts', 'ui-review', SET);

const ago = (hours: number): string => new Date(Date.now() - hours * 3600_000).toISOString();

const SEED = {
  commands: [
    {
      id: 'c1', revision: 4, name: 'Deploy staging', type: 'Shell', tags: ['work', 'ci'],
      description: 'Builds and ships the staging image', notes: 'Needs `make db-migrate` first.',
      fields: [
        { id: 'f1', label: 'Project', value: 'local-api', display: 'text' },
        { id: 'f2', label: 'URL', value: 'https://staging.example.test', display: 'link' }
      ],
      pinned: true, command: 'docker compose up --build api worker', workingDirectory: '~/src/api', usageCount: 27, updatedAt: ago(2)
    },
    {
      id: 'c2', revision: 2, name: 'Rebase main', type: 'Git', tags: ['git'],
      description: '', notes: '', fields: [], pinned: false,
      command: 'git fetch origin && git rebase origin/main', workingDirectory: '', usageCount: 3, updatedAt: ago(26)
    },
    {
      id: 'c3', revision: 1, name: 'Tunnel to staging db', type: 'Shell', tags: ['db', 'work'],
      description: 'Opens an SSH tunnel to the staging Postgres', notes: '',
      fields: [{ id: 'f3', label: 'Environment', value: 'staging', display: 'text' }],
      pinned: false, command: 'ssh -N -L 5433:db.staging:5432 bastion', workingDirectory: '', usageCount: 11, updatedAt: ago(96)
    }
  ],
  secrets: [
    {
      id: 's1', revision: 3, name: 'Registry token', type: 'API key', tags: ['work', 'infra'],
      description: 'Used by the publish job', notes: 'Rotated quarterly.',
      fields: [{ id: 'f1', label: 'Account', value: 'svc-build', display: 'text' }],
      pinned: false, value: 'synthetic-masked-value', updatedAt: ago(5)
    },
    {
      id: 's2', revision: 1, name: 'Staging Postgres', type: 'Database', tags: ['db'],
      description: '', notes: '', fields: [], pinned: true, value: 'synthetic-db-password',
      updatedAt: ago(1720)
    }
  ]
};

const EMPTY = { commands: [], secrets: [] };

const VIEWPORTS = [
  { name: 'desktop-1440', width: 1440, height: 900 },
  { name: 'tablet-768', width: 768, height: 900 },
  { name: 'mobile-390', width: 390, height: 780 }
];

async function libraryShell(page: Page, seed: typeof SEED | typeof EMPTY): Promise<void> {
  await boot(page, seed);
  await page.getByTestId('entry-row').first().waitFor({ timeout: 5000 }).catch(() => undefined);
}

async function capture(page: Page, name: string, width: number, height: number): Promise<void> {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  await page.setViewportSize({ width, height });
  await page.waitForTimeout(150);
  await page.screenshot({ path: path.join(OUT_DIR, `${name}__${width}.png`) });
}

for (const viewport of VIEWPORTS) {
  test(`library review set @ ${viewport.name}`, async ({ page }) => {
    const { width, height } = viewport;

    await libraryShell(page, SEED);
    await page.getByTestId('entry-row').first().click();
    await capture(page, '01-library-commands', width, height);

    await page.getByTestId('tab-secrets').click();
    await page.getByTestId('entry-row').first().click();
    await capture(page, '02-library-secrets-masked', width, height);

    await libraryShell(page, EMPTY);
    await capture(page, '03-library-empty', width, height);

    await libraryShell(page, SEED);
    await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
    await page.getByTestId('tab-secrets').click();
    await capture(page, '04-vault-locked', width, height);

    await libraryShell(page, SEED);
    await page.getByTestId('add-entry').click();
    await page.getByTestId('field-name').fill('New nightly job');
    await page.getByTestId('field-command').fill('bun run verify');
    await capture(page, '05-editor-command-dirty', width, height);

    await libraryShell(page, SEED);
    await page.getByTestId('tab-secrets').click();
    await page.getByTestId('entry-row').first().click();
    await page.getByTestId('edit').click();
    await capture(page, '06-editor-secret-masked', width, height);

    await libraryShell(page, SEED);
    await page.getByTestId('entry-row').first().click();
    await page.getByTestId('delete').click();
    await capture(page, '07-delete-confirmation', width, height);
  });

  test(`finder review set @ ${viewport.name}`, async ({ page }) => {
    const { width, height } = viewport;

    await boot(page, SEED, 'finder');
    await capture(page, '08-finder-results', width, height);

    await boot(page, SEED, 'finder');
    await page.getByTestId('finder-input').fill('Reg');
    await page.waitForTimeout(200);
    await capture(page, '09-finder-inline-completion', width, height);

    await boot(page, SEED, 'finder');
    await page.getByTestId('finder-input').fill('nothing matches this');
    await page.waitForTimeout(200);
    await capture(page, '10-finder-no-matches', width, height);

    await boot(page, SEED, 'finder');
    await page.getByTestId('finder-input').fill('Deploy');
    await page.keyboard.press('Enter');
    await page.waitForTimeout(200);
    await capture(page, '11-finder-copied', width, height);

    await boot(page, SEED, 'finder');
    await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
    await page.getByTestId('finder-input').fill('token');
    await page.waitForTimeout(200);
    await capture(page, '12-finder-locked-row', width, height);
  });
}
