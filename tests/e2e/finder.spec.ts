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

test('keys still work after a click moved focus onto a row', async ({ page }) => {
  await boot(page, SEED, 'finder');
  const rows = page.getByTestId('result-row');
  await expect(rows).toHaveCount(5);
  const ids = await rows.evaluateAll((els) => els.map((el) => (el as HTMLElement).dataset.rowId ?? ''));

  await rows.nth(2).click();
  // Enter acts on the clicked row exactly once (no double activation).
  await page.keyboard.press('Enter');
  let state = await mockState(page);
  expect(state.copied).toHaveLength(1);
  expect(state.copied[0]?.id).toBe(ids[2]);
  expect(state.hidden).toContain('self');

  await boot(page, SEED, 'finder');
  const rows2 = page.getByTestId('result-row');
  await expect(rows2).toHaveCount(5);
  const ids2 = await rows2.evaluateAll((els) => els.map((el) => (el as HTMLElement).dataset.rowId ?? ''));
  await rows2.nth(2).click();
  // Arrow keys keep moving the selection from the row focus.
  await page.keyboard.press('ArrowDown');
  await expect(page.getByTestId('finder-input')).toHaveAttribute('aria-activedescendant', `row-${ids2[3]}`);
  // And Escape hides the finder even when the input does not hold focus.
  await page.keyboard.press('Escape');
  state = await mockState(page);
  expect(state.hidden).toContain('self');
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

test('typing shows the matching saved entries with their tags', async ({ page }) => {
  await boot(page, SEED, 'finder');
  await page.getByTestId('finder-input').fill('restart');
  const rows = page.getByTestId('result-row');
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText('Restart containers');
  await expect(rows.first()).toContainText('#work');
  await expect(page.getByText('Searching…')).toHaveCount(0);
});

test('an empty query lists the most recently used entries first', async ({ page }) => {
  const commands = SEED.commands.map((c) =>
    c.id === 'c3' ? { ...c, lastUsedAt: '2026-11-03T09:00:00Z' } : c.id === 'c1' ? { ...c, lastUsedAt: '2026-10-01T09:00:00Z' } : c
  );
  await boot(page, { ...SEED, commands, secrets: [] }, 'finder');
  const rows = page.getByTestId('result-row');
  await expect(rows).toHaveCount(4);
  await expect(rows.nth(0)).toContainText('git revert HEAD');
  await expect(rows.nth(1)).toContainText('docker');
});

// Project context: ranking biasing, the chip, and both dismissal paths. The browser double
// cannot detect anything natively, so the context is seeded; the mock's in_context logic
// mirrors the Rust one and the Rust unit tests pin the real semantics.
const CTX_SEED = {
  commands: [
    { id: 'a', revision: 1, name: 'migrate db', type: 'DB', tags: [], description: '', notes: '', fields: [], pinned: false, command: 'rake db:migrate', workingDirectory: '/home/mock/projects/api/db', usageCount: 1 },
    { id: 'b', revision: 1, name: 'deploy api', type: 'Deploy', tags: ['project:API'], description: '', notes: '', fields: [], pinned: false, command: 'bin/deploy', workingDirectory: '', usageCount: 5 },
    { id: 'c', revision: 1, name: 'wipe disks', type: 'Danger', tags: [], description: '', notes: '', fields: [], pinned: false, command: 'shred -va 1 /*', workingDirectory: '/home/mock/projects/other', usageCount: 999 }
  ],
  secrets: [],
  context: { root: '/home/mock/projects/api', name: 'api' }
};

test('project context promotes its commands, names itself in a chip and marks its rows', async ({ page }) => {
  await boot(page, CTX_SEED, 'finder');
  const chip = page.getByTestId('finder-context-chip');
  await expect(chip).toBeVisible();
  await expect(chip).toContainText('api');
  const rows = page.getByTestId('result-row');
  // In-context first (usage inside the group still orders b before a), then the outside winner.
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(0)).toContainText('deploy api');
  await expect(rows.nth(1)).toContainText('migrate db');
  await expect(rows.nth(2)).toContainText('wipe disks');
  const markers = page.getByTestId('row-in-context');
  await expect(markers).toHaveCount(2);
  await expect(markers.first()).toHaveAttribute('aria-label', 'in api');
});

test('clicking the context chip clears it for the session and restores the default order', async ({ page }) => {
  await boot(page, CTX_SEED, 'finder');
  await page.getByTestId('finder-context-chip').click();
  await expect(page.getByTestId('finder-context-chip')).toHaveCount(0);
  const rows = page.getByTestId('result-row');
  await expect(rows.nth(0)).toContainText('wipe disks');
  await expect(page.getByTestId('row-in-context')).toHaveCount(0);
});

test('Ctrl+P clears the project context just like clicking the chip', async ({ page }) => {
  await boot(page, CTX_SEED, 'finder');
  await page.getByTestId('finder-input').press('Control+p');
  await expect(page.getByTestId('finder-context-chip')).toHaveCount(0);
  await expect(page.getByTestId('result-row').nth(0)).toContainText('wipe disks');
});

test('without context there is no chip, and a late finder-context event re-ranks the list', async ({ page }) => {
  await boot(page, { ...CTX_SEED, context: undefined }, 'finder');
  await expect(page.getByTestId('finder-context-chip')).toHaveCount(0);
  await expect(page.getByTestId('result-row').nth(0)).toContainText('wipe disks');
  // The native side finishes detection after the first search; the Finder must re-rank.
  // Like the real backend, the store is updated before the event fires.
  await page.evaluate(() => {
    const mock = (window as unknown as { __mock: { state: { context: unknown }; emit: (e: string, p: unknown) => void } }).__mock;
    mock.state.context = { root: '/home/mock/projects/api', name: 'api' };
    mock.emit('finder-context', mock.state.context);
  });
  await expect(page.getByTestId('finder-context-chip')).toContainText('api');
  await expect(page.getByTestId('result-row').nth(0)).toContainText('deploy api');
});
