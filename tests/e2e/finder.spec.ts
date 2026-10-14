import { expect, test } from '@playwright/test';
import { boot, mockState } from './helpers';

const SEED = {
  commands: [
    { id: 'c1', revision: 1, name: 'docker', type: 'Docker', tags: [], description: '', notes: '', fields: [], pinned: false, command: 'docker ps', workingDirectory: '', usageCount: 9 },
    { id: 'c2', revision: 1, name: 'docker compose up', type: 'Docker', tags: [], description: '', notes: '', fields: [], pinned: false, command: 'docker compose up', workingDirectory: '', usageCount: 2 },
    { id: 'c3', revision: 1, name: 'git revert HEAD', type: 'Git', tags: [], description: '', notes: '', fields: [], pinned: false, command: 'git revert HEAD', workingDirectory: '', usageCount: 1 },
    { id: 'c4', revision: 1, name: 'Restart containers', type: 'Docker', tags: ['work'], description: 'restarts everything', notes: '', fields: [], pinned: false, command: 'docker compose restart', workingDirectory: '', usageCount: 0 }
  ],
  secrets: [
    { id: 's1', revision: 1, name: 'Docker registry token', type: 'API key', tags: ['work'], description: '', notes: '', fields: [], pinned: false, value: 'synthetic-secret-value' }
  ]
};

test('the first result is selected and labelled for assistive tech', async ({ page }) => {
  await boot(page, SEED, 'finder');
  const rows = page.getByTestId('result-row');
  await expect(rows).toHaveCount(5);
  await expect(rows.first()).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByTestId('finder-input')).toHaveAttribute('aria-activedescendant', 'row-c1');
});

test('Up and Down move the selection and clamp at both ends', async ({ page }) => {
  await boot(page, { ...SEED, commands: SEED.commands.slice(0, 3) }, 'finder');
  const input = page.getByTestId('finder-input');
  await expect(input).toHaveAttribute('aria-activedescendant', /row-c1/);
  await page.keyboard.press('ArrowDown');
  await expect(input).toHaveAttribute('aria-activedescendant', /row-c2/);
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('ArrowUp');
  await expect(input).toHaveAttribute('aria-activedescendant', /row-c1/);
});

test('Tab completes the selected name without copying anything', async ({ page }) => {
  await boot(page, SEED, 'finder');
  const input = page.getByTestId('finder-input');
  await input.fill('docker com');
  await expect(page.getByTestId('result-row').first()).toContainText('docker compose up');
  await page.keyboard.press('Tab');
  await expect(input).toHaveValue('docker compose up');
  const state = await mockState(page);
  expect(state.copied).toHaveLength(0);
  expect(state.hidden).toHaveLength(0);
});

test('Enter copies exactly the selected record and hides only after success', async ({ page }) => {
  await boot(page, SEED, 'finder');
  const input = page.getByTestId('finder-input');
  await input.fill('docker compose');
  await expect(page.getByTestId('result-row').first()).toContainText('docker compose up');
  await page.keyboard.press('Enter');
  const state = await mockState(page);
  expect(state.copied).toEqual([{ id: 'c2', kind: 'command', value: 'docker compose up' }]);
  expect(state.hidden).toContain('self');
});

test('a copy failure keeps the finder open with a safe message', async ({ page }) => {
  await boot(page, SEED, 'finder');
  await page.evaluate(() => ((window as never as { __mock: { state: { failCopy: boolean } } }).__mock.state.failCopy = true));
  await page.getByTestId('finder-input').fill('docker ps');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('finder-error')).toBeVisible();
  await expect(page.getByTestId('finder-error')).not.toContainText('synthetic');
  const state = await mockState(page);
  expect(state.hidden).toHaveLength(0);
});

test('Ctrl+O hands the selected entry to the library', async ({ page }) => {
  await boot(page, SEED, 'finder');
  await page.getByTestId('finder-input').fill('registry');
  await page.keyboard.press('Control+o');
  const state = await mockState(page);
  expect(state.opened).toEqual([{ kind: 'secret', entryId: 's1' }]);
  expect(state.copied).toHaveLength(0);
});

test('Escape clears the query and hides the finder', async ({ page }) => {
  await boot(page, SEED, 'finder');
  await page.getByTestId('finder-input').fill('docker');
  await page.keyboard.press('Escape');
  const state = await mockState(page);
  expect(state.hidden).toContain('self');
  await expect(page.getByTestId('finder-input')).toHaveValue('');
});

test('with no matches Tab and Enter do nothing', async ({ page }) => {
  await boot(page, SEED, 'finder');
  await page.getByTestId('finder-input').fill('zzzznomatch');
  await expect(page.getByTestId('result-row')).toHaveCount(0);
  await page.keyboard.press('Tab');
  await page.keyboard.press('Enter');
  const state = await mockState(page);
  expect(state.copied).toHaveLength(0);
  expect(state.hidden).toHaveLength(0);
});

test('a stale asynchronous response cannot replace newer results', async ({ page }) => {
  await boot(page, { ...SEED, searchDelayMs: 700, delayedRequestId: 1 }, 'finder');
  const input = page.getByTestId('finder-input');
  // The initial empty-query search is request 1 and is deliberately slow; typing follows immediately.
  await input.fill('docker');
  await input.fill('revert');
  await expect(page.getByTestId('result-row').first()).toContainText('git revert HEAD');
  await page.waitForTimeout(900);
  // The slow empty-query response arrived last and must not have replaced anything.
  await expect(page.getByTestId('result-row').first()).toContainText('git revert HEAD');
  await expect(page.getByTestId('result-row')).toHaveCount(1);
});

test('a locked vault removes all secret metadata from the results', async ({ page }) => {
  await boot(page, SEED, 'finder');
  await page.getByTestId('finder-input').fill('docker');
  await expect(page.getByTestId('result-row')).toHaveCount(3);
  await expect(page.getByTestId('result-row').nth(2)).toContainText('Docker registry token');
  await page.evaluate(() => (window as never as { __mock: { lockVault: () => void } }).__mock.lockVault());
  await expect(page.getByTestId('result-row')).toHaveCount(3);
  await expect(page.getByTestId('result-row').nth(2)).toContainText('Unlock vault to search secrets');
  await expect(page.getByRole('option', { name: /Docker registry token/ })).toHaveCount(0);
  await expect(page.getByTestId('finder-vault')).toContainText('Vault locked');
});

test('reopening starts with a fresh query instead of the previous search', async ({ page }) => {
  await boot(page, SEED, 'finder');
  await page.getByTestId('finder-input').fill('registry');
  await page.evaluate(() => (window as never as { __mock: { emit: (e: string, p: unknown) => void } }).__mock.emit('finder-shown', null));
  await expect(page.getByTestId('finder-input')).toHaveValue('');
});
