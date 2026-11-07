import { expect, test } from '@playwright/test';
import { boot, mockState } from './helpers';

const root = '/projects/sample';
const environments = [{ root, enabled: true, snapshots: [
  { id: 'env-2', file: '.env', version: 2, savedAt: '2026-11-07T10:00:00Z', bytes: 25 },
  { id: 'env-1', file: '.env', version: 1, savedAt: '2026-11-06T10:00:00Z', bytes: 20 },
  { id: 'local-1', file: '.env.local', version: 1, savedAt: '2026-11-06T11:00:00Z', bytes: 18 }
] }];

test('project Environment shows dated versions, copies old files and pauses sync', async ({ page }) => {
  await boot(page, { environments, commands: [{ id: 'sample', revision: 1, name: 'build', command: 'make build',
    workingDirectory: root, project: { root, name: 'sample' }, tags: [], fields: [], type: 'Shell' }] });
  await page.getByTestId('command-project-toggle').click();
  await page.getByTestId('project-environment').click();
  const dialog = page.getByRole('dialog', { name: 'Project environment' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByTestId('environment-version')).toHaveCount(3);
  await expect(dialog.locator('time').first()).toHaveAttribute('datetime', '2026-11-07T10:00:00Z');
  await dialog.getByTestId('environment-copy').nth(1).click();
  await expect(dialog).toContainText('Copied environment file');
  expect((await mockState(page)).copied.at(-1)).toMatchObject({ kind: 'secret', id: 'env-1' });
  await dialog.getByTestId('environment-toggle').click();
  await expect(dialog).toContainText('Automatic sync paused');
  await expect(dialog.getByTestId('environment-version')).toHaveCount(3);
  await dialog.getByTestId('environment-toggle').click();
  await expect(dialog).toContainText('Syncs every 30 seconds');
  await page.setViewportSize({ width: 390, height: 780 });
  await expect(dialog.getByRole('button', { name: 'Close' })).toBeInViewport();
  expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
});

test('a missing project folder still allows copying saved versions', async ({ page }) => {
  await boot(page, { environments, environmentSyncError: true });
  await page.getByTestId('open-environment').click();
  const dialog = page.getByRole('dialog', { name: 'Project environment' });
  await dialog.getByTestId('environment-sync').click();
  await expect(dialog).toContainText('Project folder is missing');
  await dialog.getByTestId('environment-copy').first().click();
  expect((await mockState(page)).copied.at(-1)?.id).toBe('env-2');
});

test('Browse starts an empty project and a locked vault rejects access', async ({ page }) => {
  await boot(page, { environments: [], projectFolder: root });
  await page.getByTestId('open-environment').click();
  await page.getByTestId('environment-browse').click();
  await expect(page.getByTestId('environment-project')).toHaveValue(root);
  await expect(page.getByTestId('environment-empty')).toBeVisible();
  await boot(page, { vaultUnlocked: false, manualLock: true, environments });
  await page.getByTestId('open-environment').click();
  const dialog = page.getByRole('dialog', { name: 'Project environment' });
  await expect(dialog).toContainText('vault is locked');
  await expect(dialog.getByTestId('environment-version')).toHaveCount(0);
});

test('locking the vault closes the environment view and clears its metadata', async ({ page }) => {
  await boot(page, { environments });
  await page.getByTestId('open-environment').click();
  await expect(page.getByTestId('environment-version')).toHaveCount(3);
  await page.evaluate(() => window.__devConsoleEmit?.('vault-state-changed', { unlocked: false }));
  await expect(page.getByRole('dialog', { name: 'Project environment' })).toHaveCount(0);
});
