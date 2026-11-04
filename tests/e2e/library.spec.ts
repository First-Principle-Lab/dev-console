import { expect, test } from '@playwright/test';
import { boot, mockState } from './helpers';

const SEED = {
  commands: [
    { id: 'c1', revision: 1, name: 'Deploy', type: 'Shell', tags: ['work'], description: 'first', notes: '', fields: [], pinned: false, command: 'echo one', workingDirectory: '', usageCount: 4 },
    { id: 'c2', revision: 3, name: 'Deploy', type: 'Docker', tags: ['home'], description: 'second', notes: '', fields: [{ id: 'f1', label: 'Project', value: 'local-api', display: 'text' }], pinned: true, command: 'docker compose up', workingDirectory: '/tmp', usageCount: 1 }
  ],
  secrets: [
    { id: 's1', revision: 2, name: 'Registry token', type: 'API key', tags: ['work'], description: '', notes: '', fields: [], pinned: false, value: 'synthetic-value-1' }
  ]
};

test('library lists both same-named commands and shows the selected record', async ({ page }) => {
  await boot(page, SEED);
  const rows = page.getByTestId('entry-row');
  await expect(rows).toHaveCount(2);
  // pinned sorts first
  await expect(rows.first()).toContainText('Deploy');
  await expect(page.getByTestId('entry-details')).toContainText('docker compose up');
  await expect(page.getByTestId('command-text')).toHaveText('docker compose up');
});

test('creating a command persists through the backend and refreshes the list', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('add-entry').click();
  await page.getByTestId('field-name').fill('Prune volumes');
  await page.getByTestId('field-command').fill('docker volume prune -f');
  await page.getByRole('textbox', { name: 'Notes' }).fill('Run only after the backup.');
  await page.getByTestId('save').click();
  await expect(page.getByTestId('entry-details')).toContainText('docker volume prune -f');
  const state = await mockState(page);
  expect(state.commands).toHaveLength(3);
  expect(state.commands[2].name).toBe('Prune volumes');
  expect(state.commands[2]).toMatchObject({
    type: 'Command', notes: 'Run only after the backup.'
  });
});

test('a cancelled edit leaves the saved record untouched', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('entry-row').nth(1).click();
  await expect(page.getByTestId('command-text')).toHaveText('echo one');
  await page.getByTestId('edit').click();
  await page.getByTestId('field-name').fill('Renamed by mistake');
  await page.getByTestId('field-command').fill('rm -rf /');
  await page.getByTestId('cancel-edit').click();
  await expect(page.getByTestId('entry-details')).toContainText('Deploy');
  const state = await mockState(page);
  expect(state.commands.find((c) => c.id === 'c1')?.name).toBe('Deploy');
  expect(state.commands.find((c) => c.id === 'c1')?.revision).toBe(1);
});

test('a failed write keeps the draft on screen', async ({ page }) => {
  await boot(page, SEED);
  await page.evaluate(() => ((window as never as { __mock: { state: { saveFails: boolean } } }).__mock.state.saveFails = true));
  await page.getByTestId('add-entry').click();
  await page.getByTestId('field-name').fill('Kept draft');
  await page.getByTestId('field-command').fill('echo keep');
  await page.getByTestId('save').click();
  await expect(page.getByTestId('save-error')).toBeVisible();
  await expect(page.getByTestId('field-name')).toHaveValue('Kept draft');
});

test('pin, copy and delete confirmation all act on the intended record', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('entry-row').nth(1).click();
  await page.getByTestId('copy').click();
  const copied = (await mockState(page)).copied;
  expect(copied).toEqual([{ id: 'c1', kind: 'command', value: 'echo one' }]);

  await page.getByTestId('pin').click();
  expect((await mockState(page)).commands.find((c) => c.id === 'c1')?.pinned).toBe(true);

  await page.getByTestId('delete').click();
  await expect(page.getByRole('dialog', { name: 'Confirm delete' })).toContainText('Deploy');
  await page.getByTestId('cancel-delete').click();
  expect((await mockState(page)).commands).toHaveLength(2);

  await page.getByTestId('delete').click();
  await page.getByTestId('confirm-delete').click();
  await expect(page.getByTestId('status')).toContainText('Deleted');
  expect((await mockState(page)).commands).toHaveLength(1);
});

test('secrets are masked in details and only appear through an explicit reveal', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('tab-secrets').click();
  await expect(page.getByTestId('secret-value')).toHaveText('••••••••••');
  await page.getByTestId('reveal').click();
  await expect(page.getByTestId('secret-value')).toHaveText('synthetic-value-1');
});

test('a new secret needs only name, value and notes', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('tab-secrets').click();
  await page.getByTestId('add-entry').click();
  await expect(page.getByTestId('entry-editor').locator('input, textarea')).toHaveCount(3);
  await page.getByRole('button', { name: 'Show' }).click();
  await expect(page.getByTestId('field-value')).toHaveAttribute('type', 'text');
  await expect(page.getByTestId('validation-hint')).toHaveCount(0);
  await page.getByTestId('field-name').fill('Build token');
  await page.getByTestId('field-value').fill('synthetic-build-token');
  await page.getByRole('textbox', { name: 'Notes' }).fill('Rotate monthly.');
  await page.getByTestId('save').click();
  const state = await mockState(page);
  expect(state.secrets.at(-1)).toMatchObject({
    name: 'Build token', type: 'Secret', notes: 'Rotate monthly.', value: 'synthetic-build-token'
  });
  await expect(page.getByTestId('secret-value')).toHaveText('••••••••••');
});

test('a secret can carry optional custom fields added with the plus button', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('tab-secrets').click();
  await page.getByTestId('add-entry').click();
  await page.getByTestId('field-name').fill('Database login');
  await page.getByTestId('field-value').fill('synthetic-db-password');
  await page.getByTestId('add-field').click();
  // adding a field must not submit the form
  await expect(page.getByTestId('entry-editor')).toBeVisible();
  await page.getByRole('textbox', { name: 'Field label' }).fill('Username');
  await page.getByRole('textbox', { name: 'Field value' }).fill('admin');
  await page.getByTestId('save').click();
  const state = await mockState(page);
  expect(state.secrets.at(-1)).toMatchObject({
    name: 'Database login',
    fields: [{ label: 'Username', value: 'admin', display: 'text' }]
  });
  await expect(page.getByTestId('extra-fields')).toContainText('admin');
});

test('a locked vault clears the secret list and blocks secret reads', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('tab-secrets').click();
  await expect(page.getByTestId('entry-row')).toHaveCount(1);
  await page.evaluate(() => (window as never as { __mock: { lockVault: () => void } }).__mock.lockVault());
  await expect(page.getByTestId('unlock-vault')).toBeVisible();
  await expect(page.getByTestId('entry-row')).toHaveCount(0);
});

test('editing the short form preserves older entry metadata', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('entry-row').nth(0).click();
  await page.getByTestId('edit').click();
  await expect(page.getByTestId('entry-editor').locator('input, textarea')).toHaveCount(3);
  await page.getByRole('textbox', { name: 'Notes' }).fill('Still uses /tmp.');
  await page.getByTestId('save').click();
  const state = await mockState(page);
  expect(state.commands.find((c) => c.id === 'c2')).toMatchObject({
    type: 'Docker', tags: ['home'], description: 'second', pinned: true,
    workingDirectory: '/tmp', notes: 'Still uses /tmp.',
    fields: [{ id: 'f1', label: 'Project', value: 'local-api', display: 'text' }]
  });
});

test('narrow layout shows one pane instead of squeezing both', async ({ page }) => {
  await page.setViewportSize({ width: 780, height: 560 });
  await boot(page, SEED);
  await expect(page.getByTestId('library')).toBeVisible();
  await page.getByTestId('entry-row').first().click();
  await expect(page.getByTestId('entry-details')).toBeVisible();
});

test('settings exposes encrypted backups: create, list, reviewed restore, and a rejected import', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('settings').click();
  await expect(page.getByTestId('backup-empty')).toBeVisible();

  await page.getByTestId('backup-now').click();
  await expect(page.getByTestId('backup-message')).toContainText('Encrypted backup written');
  await expect(page.getByTestId('backup-list').getByTestId('restore-backup')).toHaveCount(1);

  await page.getByTestId('export-archive').waitFor();
  await expect(page.getByTestId('export-archive')).toBeDisabled();
  await page.getByTestId('archive-path').fill('/tmp/dev-console.age');
  await page.getByTestId('export-archive').click();
  await expect(page.getByTestId('backup-message')).toContainText('Archive exported');

  // Restore now goes through a review step: nothing is applied until confirmed.
  await page.getByTestId('restore-backup').first().click();
  await expect(page.getByTestId('review-card')).toBeVisible();
  await expect(page.getByTestId('review-warning')).toContainText('replaces your entire library');
  await page.getByTestId('review-cancel').click();
  await expect(page.getByTestId('review-card')).toHaveCount(0);
  await page.getByTestId('restore-backup').first().click();
  await page.getByTestId('review-confirm').click();
  await expect(page.getByTestId('backup-message')).toContainText('dev-console-backup-1.age');

  await page.getByTestId('import-path').fill('/tmp/not-an-archive.txt');
  await page.getByTestId('import-archive').click();
  await expect(page.getByTestId('backup-error')).toContainText('not a valid Dev Console backup');
});

test('backups are refused while the vault is locked', async ({ page }) => {
  await boot(page, SEED);
  await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
  await page.getByTestId('settings').click();
  await page.getByTestId('backup-now').click();
  await expect(page.getByTestId('backup-error')).toContainText('the vault is locked');
});
