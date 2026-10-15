import { expect, test } from '@playwright/test';

import { boot } from './helpers';

const ago = (hours: number): string => new Date(Date.now() - hours * 3600_000).toISOString();

const SEED = {
  commands: [
    {
      id: 'c1', revision: 1, name: 'Deploy staging', type: 'Shell', tags: [], description: '',
      notes: '', fields: [], pinned: false, command: 'make deploy', workingDirectory: '',
      usageCount: 1, updatedAt: ago(1)
    },
    {
      id: 'c2', revision: 1, name: 'Rebase main', type: 'Git', tags: [], description: '',
      notes: '', fields: [], pinned: false, command: 'git rebase', workingDirectory: '',
      usageCount: 0, updatedAt: ago(2)
    }
  ],
  secrets: [
    {
      id: 's1', revision: 1, name: 'Registry token', type: 'API key', tags: [], description: '',
      notes: '', fields: [], pinned: false, value: 'synthetic-value', updatedAt: ago(3)
    }
  ]
};

async function openSettings(page: import('@playwright/test').Page): Promise<void> {
  await boot(page, SEED);
  await page.getByTestId('settings').click();
  await expect(page.getByTestId('settings-panel')).toBeVisible();
}

test('import requires a review that shows the archive date, counts and what gets replaced', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('import-path').fill('/tmp/portable.age');
  await page.getByTestId('import-archive').click();
  await expect(page.getByTestId('review-card')).toBeVisible();
  await expect(page.getByTestId('review-warning')).toContainText('replaces your entire library');
  // Archive side: the mock summary mirrors the seed counts; the current side counts live data.
  await expect(page.getByTestId('review-card')).toContainText('2 commands');
  await expect(page.getByTestId('review-card')).toContainText('1 secrets');
  await expect(page.getByTestId('review-current')).toContainText('2 commands');
  // Cancel applies nothing.
  await page.getByTestId('review-cancel').click();
  await expect(page.getByTestId('review-card')).toHaveCount(0);
  await expect(page.getByTestId('backup-message')).toHaveCount(0);
});

test('the confirm step offers a pre-restore backup and says it took one', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('import-path').fill('/tmp/portable.age');
  await page.getByTestId('import-archive').click();
  await expect(page.getByTestId('prebackup')).toBeChecked();
  await page.getByTestId('review-confirm').click();
  await expect(page.getByTestId('backup-message')).toContainText('backed up first');
  // The safety backup itself shows up in the list.
  await expect(page.getByTestId('backup-list').getByTestId('restore-backup')).toHaveCount(1);
});

test('export refuses an existing file and only replaces after explicit confirmation', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('archive-path').fill('/tmp/once.age');
  await page.getByTestId('export-archive').click();
  await expect(page.getByTestId('backup-message')).toContainText('Archive exported');

  await page.getByTestId('export-archive').click();
  await expect(page.getByTestId('backup-error')).toContainText('already exists');
  await expect(page.getByTestId('export-overwrite-card')).toBeVisible();

  await page.getByTestId('export-overwrite-cancel').click();
  await expect(page.getByTestId('export-overwrite-card')).toHaveCount(0);
  await expect(page.getByTestId('backup-message')).toHaveCount(0);

  await page.getByTestId('export-archive').click();
  await page.getByTestId('export-overwrite-confirm').click();
  await expect(page.getByTestId('backup-message')).toContainText('Archive exported');
});

test('the recovery credential stays hidden until revealed and shows the restore steps', async ({ page }) => {
  await openSettings(page);
  await expect(page.getByTestId('recovery-reveal')).toBeVisible();
  await expect(page.getByTestId('recovery-secret')).toHaveCount(0);
  await page.getByTestId('recovery-reveal').click();
  await expect(page.getByTestId('recovery-secret')).toContainText('AGE-SECRET-KEY-1');
  await expect(page.getByTestId('recovery-steps')).toContainText('recovery.agekey');

  // Locked vault: revealing must fail loudly instead of returning the credential.
  await boot(page, SEED);
  await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
  await page.getByTestId('settings').click();
  await page.getByTestId('recovery-reveal').click();
  await expect(page.getByTestId('recovery-error')).toContainText('vault is locked');
});

test('the terminal field parses quoted arguments instead of splitting on spaces', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('terminal').fill('foot --title "Dev Console -e');
  await expect(page.getByTestId('terminal-error')).toBeVisible();
  await page.getByTestId('terminal').fill('"/usr/bin/my terminal" --title "Dev Console" -e');
  await expect(page.getByTestId('terminal-error')).toHaveCount(0);
  const chips = page.getByTestId('terminal-tokens').locator('code');
  await expect(chips).toHaveCount(4);
  await expect(chips.nth(0)).toHaveText('/usr/bin/my terminal');
  await page.getByTestId('save-settings').click();
  const stored = await page.evaluate(() => (window as unknown as { __mock: { state: { settings: { terminal: string[] } } } }).__mock.state.settings.terminal);
  expect(stored).toEqual(['/usr/bin/my terminal', '--title', 'Dev Console', '-e']);
});

test('an unbalanced quote blocks saving the terminal setting', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('terminal').fill('foot --title "broken');
  await expect(page.getByTestId('terminal-error')).toContainText('unbalanced');
  await expect(page.getByTestId('save-settings')).toBeDisabled();
});
