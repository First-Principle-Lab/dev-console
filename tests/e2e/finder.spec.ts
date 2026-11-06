import { expect, test, type Page } from '@playwright/test';
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

const PROJECT = { root: '/home/dev/projects/dev-console', name: 'dev-console' };

const command = (
  id: string,
  name: string,
  workingDirectory: string,
  extra: Record<string, unknown> = {}
) => ({
  id, revision: 1, name, type: 'Shell', tags: [], description: '', notes: '', fields: [],
  pinned: false, command: `echo `, workingDirectory, usageCount: 0, ...extra
});

/**
 * One command per way an entry can belong to the project the palette was opened from, plus the
 * near misses that must not count: a folder whose name merely starts the same (`sibling`), and
 * another project's tag (`other-tag`). `~` is expanded by the double against the seeded `home`.
 */
const contextSeed = (detected: boolean) => ({
  home: '/home/dev',
  context: detected ? PROJECT : undefined,
  commands: [
    command('here', 'just test', PROJECT.root),
    command('nested', 'build release', PROJECT.root + '/src'),
    command('tilde', 'fmt everything', '~/projects/dev-console/docs'),
    command('tagged', 'import scripts', '', { tags: ['project:dev-console'] }),
    command('sibling', 'dev-console-lite notes', '/home/dev/projects/dev-console-lite'),
    command('other-tag', 'unrelated', '', { tags: ['project:other'] }),
    // The most recently used command of all, from somewhere else entirely: project membership is
    // what moves the others past it, nothing else.
    command('elsewhere', 'cargo bench', '/home/dev/scratch', { usageCount: 99, lastUsedAt: '2026-11-05T09:00:00Z' })
  ],
  secrets: []
});

const rowNames = (page: Page) =>
  page.getByTestId('result-row').evaluateAll((rows) =>
    rows.map((row) => row.querySelector('.name')?.textContent?.trim() ?? '')
  );

test('the palette says which project it was opened from, and ranks it first', async ({ page }) => {
  await boot(page, contextSeed(true), 'finder');
  await expect(page.getByTestId('finder-context')).toContainText('dev-console');
  // In the project first (each other by name, none by usage), then everything else with its old
  // most-recent-first ordering.
  expect(await rowNames(page)).toEqual([
    'build release', 'fmt everything', 'import scripts', 'just test',
    'cargo bench', 'dev-console-lite notes', 'unrelated'
  ]);
});

test('a row from the current project is labelled for assistive tech', async ({ page }) => {
  await boot(page, contextSeed(true), 'finder');
  await expect(page.getByTestId('row-in-project')).toHaveCount(4);
  await expect(page.getByRole('img', { name: 'in dev-console' })).toHaveCount(4);
  // The two near misses are not marked, so the marker means "this project" and not "similar".
  await expect(page.getByRole('option', { name: /dev-console-lite notes/ }).getByTestId('row-in-project')).toHaveCount(0);
  await expect(page.getByRole('option', { name: /^unrelated/ }).getByTestId('row-in-project')).toHaveCount(0);
});

test('clicking the project chip ranks everything again, without closing the palette', async ({ page }) => {
  await boot(page, contextSeed(true), 'finder');
  await page.getByTestId('finder-context').click();
  await expect(page.getByTestId('finder-context')).toHaveCount(0);
  await expect(page.getByTestId('row-in-project')).toHaveCount(0);
  expect(await rowNames(page)).toEqual([
    'cargo bench', 'build release', 'dev-console-lite notes', 'fmt everything',
    'import scripts', 'just test', 'unrelated'
  ]);
  expect((await mockState(page)).context).toBeNull();
  expect((await mockState(page)).hidden).toHaveLength(0);
});

test('Ctrl+P ignores the project from the keyboard', async ({ page }) => {
  await boot(page, contextSeed(true), 'finder');
  await page.keyboard.press('Control+p');
  await expect(page.getByTestId('finder-context')).toHaveCount(0);
  await expect(page.getByTestId('result-row')).toHaveCount(7);
  expect((await mockState(page)).context).toBeNull();
});

test('with no detected project, the palette looks and orders exactly as it used to', async ({ page }) => {
  await boot(page, contextSeed(false), 'finder');
  await expect(page.getByTestId('finder-context')).toHaveCount(0);
  await expect(page.getByTestId('row-in-project')).toHaveCount(0);
  expect(await rowNames(page)).toEqual([
    'cargo bench', 'build release', 'dev-console-lite notes', 'fmt everything',
    'import scripts', 'just test', 'unrelated'
  ]);
});

test('typing narrows the list without dropping the project marker', async ({ page }) => {
  await boot(page, contextSeed(true), 'finder');
  await page.getByTestId('finder-input').fill('just');
  await expect(page.getByTestId('result-row')).toHaveCount(1);
  await expect(page.getByTestId('row-in-project')).toHaveCount(1);
});

test('with project ranking switched off, a detected project changes nothing', async ({ page }) => {
  // The backend never detects while the switch is off; this also covers a project detected just
  // before it was switched off, which neither the chip nor the ranking may still use.
  await boot(page, {
    ...contextSeed(true),
    settings: {
      theme: 'dark', autostart: false, clipboardExpirySeconds: 30,
      terminal: ['x-terminal-emulator', '-e'], projectContextEnabled: false
    }
  }, 'finder');
  await expect(page.getByTestId('result-row')).toHaveCount(7);
  await expect(page.getByTestId('finder-context')).toHaveCount(0);
  await expect(page.getByTestId('row-in-project')).toHaveCount(0);
  expect(await rowNames(page)).toEqual([
    'cargo bench', 'build release', 'dev-console-lite notes', 'fmt everything',
    'import scripts', 'just test', 'unrelated'
  ]);
});

test('a secret tagged for the project is not moved up or marked', async ({ page }) => {
  await boot(page, {
    ...contextSeed(true),
    secrets: [
      { id: 's-here', revision: 1, name: 'deploy token', type: 'API key', tags: ['project:dev-console'], description: '', notes: '', fields: [], pinned: false, value: 'synthetic-secret-value' }
    ]
  }, 'finder');
  await expect(page.getByTestId('finder-context')).toContainText('dev-console');
  await expect(page.getByTestId('row-in-project')).toHaveCount(4);
  await expect(page.getByRole('option', { name: /deploy token/ }).getByTestId('row-in-project')).toHaveCount(0);
});

test('a pinned command still leads the empty query, ahead of the project', async ({ page }) => {
  const seed = contextSeed(true);
  seed.commands.push(command('pinned', 'zz pinned elsewhere', '/home/dev/scratch', { pinned: true }));
  await boot(page, seed, 'finder');
  expect((await rowNames(page)).slice(0, 2)).toEqual(['zz pinned elsewhere', 'build release']);
});

test('a project detected after the first search moves the selection to its first command', async ({ page }) => {
  // The real backend answers beside the palette, so the first search runs with no project and
  // selects the most recently used command; the project then arrives as an event.
  await boot(page, contextSeed(false), 'finder');
  const input = page.getByTestId('finder-input');
  await expect(input).toHaveAttribute('aria-activedescendant', 'row-elsewhere');
  await page.evaluate((project) => {
    const mock = (window as never as {
      __mock: { state: { context: unknown }; emit: (e: string, p: unknown) => void };
    }).__mock;
    mock.state.context = project;
    mock.emit('finder-context', project);
  }, PROJECT);
  await expect(page.getByTestId('finder-context')).toContainText('dev-console');
  await expect(input).toHaveAttribute('aria-activedescendant', 'row-nested');
  // Ignoring the project starts from the top of the restored order, too.
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Control+p');
  await expect(input).toHaveAttribute('aria-activedescendant', 'row-elsewhere');
});
