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
  await page.getByTestId('field-name').fill('prune-volumes');
  await page.getByTestId('field-command').fill('docker volume prune -f');
  await page.getByRole('textbox', { name: 'Notes' }).fill('Run only after the backup.');
  await page.getByTestId('save').click();
  await expect(page.getByTestId('entry-details')).toContainText('docker volume prune -f');
  const state = await mockState(page);
  expect(state.commands).toHaveLength(3);
  expect(state.commands[2].name).toBe('prune-volumes');
  expect(state.commands[2]).toMatchObject({
    type: 'Command', notes: 'Run only after the backup.'
  });
});

test('a newly saved ordinary command shows stdout, stderr and its exit code in the app', async ({ page }) => {
  const command = "printf 'files'; printf 'warning' >&2; exit 7";
  await boot(page, { ...SEED, runResults: {
    [command]: { stdout: 'files', stderr: 'warning', exitCode: 7 }
  } });
  await page.getByTestId('add-entry').click();
  await page.getByTestId('field-name').fill('list-files');
  await page.getByTestId('field-command').fill(command);
  await page.getByTestId('save').click();
  await page.getByTestId('run').click();
  await expect(page.getByRole('dialog', { name: 'Confirm run' })).toBeVisible();
  await page.getByTestId('confirm-run').click();
  await expect(page.getByRole('dialog', { name: 'Command output' })).toBeVisible();
  await expect(page.getByTestId('command-stdout')).toHaveText('files');
  await expect(page.getByTestId('command-stderr')).toHaveText('warning');
  await expect(page.getByTestId('command-exit')).toContainText('Exit code 7');
  await page.getByTestId('copy-command-stdout').click();
  await expect(page.getByTestId('copy-command-stdout')).toContainText('Copied');
  await page.getByTestId('copy-command-stderr').click();
  await expect(page.getByTestId('copy-command-stderr')).toContainText('Copied');
  expect((await mockState(page)).copiedOutputs).toEqual(['files', 'warning']);
  await page.screenshot({ path: 'artifacts/ui-review/command-result.png' });
  await page.setViewportSize({ width: 390, height: 780 });
  await expect(page.getByTestId('close-command-output')).toBeInViewport();
  expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
  await page.screenshot({ path: 'artifacts/ui-review/command-result-narrow.png' });
  await page.getByTestId('close-command-output').click();
  await expect(page.getByRole('dialog', { name: 'Command output' })).toHaveCount(0);
});

test('error-only command output can be copied and clipboard failures stay visible', async ({ page }) => {
  await boot(page, { ...SEED, runResults: { 'echo one': { stdout: '', stderr: 'permission denied', exitCode: 1 } } });
  await page.getByTestId('entry-row').nth(1).click();
  await page.getByTestId('run').click();
  await page.getByTestId('confirm-run').click();
  await expect(page.getByTestId('copy-command-stdout')).toHaveCount(0);
  await expect(page.getByTestId('command-stderr')).toHaveText('permission denied');
  await page.evaluate(() => ((window as never as { __mock: { state: { failCopyOutput: boolean } } }).__mock.state.failCopyOutput = true));
  await page.getByTestId('copy-command-stderr').click();
  await expect(page.getByTestId('output-copy-error')).toContainText('clipboard provider is unavailable');
  await page.evaluate(() => ((window as never as { __mock: { state: { failCopyOutput: boolean } } }).__mock.state.failCopyOutput = false));
  await page.getByTestId('copy-command-stderr').click();
  await expect(page.getByTestId('copy-command-stderr')).toContainText('Copied');
  await expect(page.getByTestId('output-copy-error')).toHaveCount(0);
  expect((await mockState(page)).copiedOutputs).toEqual(['permission denied']);
});

test('Escape closes confirmation, output, and unsaved changes dialogs', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('run').click();
  await expect(page.getByRole('dialog', { name: 'Confirm run' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'Confirm run' })).toHaveCount(0);
  expect((await mockState(page)).runs).toHaveLength(0);

  await page.getByTestId('run').click();
  await page.getByTestId('confirm-run').click();
  await expect(page.getByRole('dialog', { name: 'Command output' })).toBeVisible();
  await page.evaluate(() => (document.querySelector('[data-testid="run"]') as HTMLElement)?.focus());
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'Command output' })).toHaveCount(0);

  await page.getByTestId('delete').click();
  await expect(page.getByRole('dialog', { name: 'Confirm delete' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'Confirm delete' })).toHaveCount(0);

  await page.getByTestId('add-entry').click();
  await page.getByTestId('field-name').fill('Keep draft');
  await page.evaluate(() => (window as never as { __mock: { emit: (event: string, payload: unknown) => void } }).__mock.emit('close-requested', { reason: 'window' }));
  await expect(page.getByRole('dialog', { name: 'Unsaved changes' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'Unsaved changes' })).toHaveCount(0);
  await expect(page.getByTestId('field-name')).toHaveValue('Keep draft');
});

test('sudo commands use the configured terminal after confirmation', async ({ page }) => {
  await boot(page, { ...SEED, commands: [{ ...SEED.commands[0], command: 'sudo ls' }] });
  await page.getByTestId('run').click();
  await page.getByTestId('confirm-run').click();
  await expect(page.getByTestId('status')).toContainText('Launched in a terminal for sudo');
  await expect(page.getByRole('dialog', { name: 'Command output' })).toHaveCount(0);
  expect((await mockState(page)).runs).toHaveLength(1);
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

test('sidebar actions target their row without selecting it, including secret pinning', async ({ page }) => {
  await boot(page, SEED);
  await page.screenshot({ path: 'artifacts/ui-review/sidebar-actions-wide.png' });
  await page.setViewportSize({ width: 390, height: 780 });
  await page.getByTestId('back-to-list').click();
  await expect(page.locator('[data-entry-id="c1"]').getByTestId('row-delete')).toBeInViewport();
  expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
  await page.screenshot({ path: 'artifacts/ui-review/sidebar-actions-narrow.png' });
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.locator('[data-entry-id="c2"]').getByTestId('select-entry').click();
  const commandRow = page.locator('[data-entry-id="c1"]');
  await expect(commandRow).toHaveText('Deploy');
  const copyBox = await commandRow.getByTestId('row-copy').boundingBox();
  expect(copyBox?.width).toBeGreaterThanOrEqual(34);
  expect(copyBox?.height).toBeGreaterThanOrEqual(34);
  await commandRow.getByTestId('row-copy').click();
  expect((await mockState(page)).copied).toEqual([{ id: 'c1', kind: 'command', value: 'echo one' }]);
  await expect(page.getByTestId('command-text')).toHaveText('docker compose up');

  await commandRow.getByTestId('row-pin').click();
  await expect(commandRow.getByTestId('row-pin')).toHaveAttribute('aria-label', 'Unpin Deploy');
  expect((await mockState(page)).commands.find((c) => c.id === 'c1')?.pinned).toBe(true);
  await expect(page.getByTestId('command-text')).toHaveText('docker compose up');

  await commandRow.getByTestId('row-delete').click();
  await expect(page.getByRole('dialog', { name: 'Confirm delete' })).toContainText('Deploy');
  await page.getByTestId('confirm-delete').click();
  await expect(commandRow).toHaveCount(0);
  expect((await mockState(page)).commands.map((c) => c.id)).toEqual(['c2']);

  await page.getByTestId('tab-secrets').click();
  const secretRow = page.locator('[data-entry-id="s1"]');
  await secretRow.getByTestId('row-pin').click();
  await expect(secretRow.getByTestId('row-pin')).toHaveAttribute('aria-label', 'Unpin Registry token');
  expect((await mockState(page)).secrets[0]).toMatchObject({ pinned: true, value: 'synthetic-value-1' });
  await secretRow.getByTestId('row-copy').click();
  expect((await mockState(page)).copied.at(-1)).toMatchObject({ id: 's1', value: 'synthetic-value-1' });
});

test('secrets are masked in details and only appear through an explicit reveal', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('tab-secrets').click();
  await expect(page.getByTestId('secret-value')).toHaveText('••••••••••');
  await page.getByTestId('reveal').click();
  await expect(page.getByTestId('secret-value')).toHaveText('synthetic-value-1');
  await page.getByTestId('reveal').click();
  await expect(page.getByTestId('secret-value')).toHaveText('••••••••••');
});

test('a new secret needs only name, value and notes', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('tab-secrets').click();
  await page.getByTestId('add-entry').click();
  await expect(page.getByTestId('entry-editor').locator('input, textarea')).toHaveCount(3);
  await page.getByRole('button', { name: 'Show' }).click();
  await expect(page.getByTestId('field-value')).toHaveAttribute('type', 'text');
  await expect(page.getByTestId('validation-hint')).toHaveCount(0);
  await page.getByTestId('field-name').fill('build-token');
  await page.getByTestId('field-value').fill('synthetic-build-token');
  await page.getByRole('textbox', { name: 'Notes' }).fill('Rotate monthly.');
  await page.getByTestId('save').click();
  const state = await mockState(page);
  expect(state.secrets.at(-1)).toMatchObject({
    name: 'build-token', type: 'Secret', notes: 'Rotate monthly.', value: 'synthetic-build-token'
  });
  await expect(page.getByTestId('secret-value')).toHaveText('••••••••••');
});

test('a secret can carry optional custom fields added with the plus button', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('tab-secrets').click();
  await page.getByTestId('add-entry').click();
  await page.getByTestId('field-name').fill('database-login');
  await page.getByTestId('field-value').fill('synthetic-db-password');
  await page.getByTestId('add-field').click();
  // adding a field must not submit the form
  await expect(page.getByTestId('entry-editor')).toBeVisible();
  await page.getByRole('textbox', { name: 'Field label' }).fill('Username');
  await page.getByRole('textbox', { name: 'Field value' }).fill('admin');
  await page.getByTestId('save').click();
  const state = await mockState(page);
  expect(state.secrets.at(-1)).toMatchObject({
    name: 'database-login',
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
  await expect(page.getByTestId('entry-editor').locator('input, textarea')).toHaveCount(4);
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

test('a URL field opens through the backend and only for http(s) links', async ({ page }) => {
  await boot(page, {
    ...SEED,
    commands: [
      {
        ...SEED.commands[0],
        fields: [{ id: 'u1', label: 'Docs', value: 'https://example.test/docs', display: 'url' }]
      }
    ]
  });
  await page.getByTestId('field-url').click();
  expect(await page.evaluate(() => (window as unknown as { __openedUrls: string[] }).__openedUrls)).toEqual(['https://example.test/docs']);
});

test('URL hit and focus areas fit short text while long URLs wrap inside the field', async ({ page }) => {
  const shortUrl = 'https://e.co';
  const longUrl = `https://example.test/${'long-path-'.repeat(18)}`;
  await boot(page, {
    ...SEED,
    commands: [{
      ...SEED.commands[0],
      fields: [
        { id: 'short', label: 'Short', value: shortUrl, display: 'url' },
        { id: 'long', label: 'Long', value: longUrl, display: 'url' }
      ]
    }]
  });
  const fields = page.getByTestId('extra-fields').locator('.field');
  const shortButton = fields.first().getByTestId('field-url');
  const shortBox = await shortButton.boundingBox();
  const shortFieldBox = await fields.first().boundingBox();
  expect(shortBox).not.toBeNull();
  expect(shortFieldBox).not.toBeNull();
  expect(shortBox!.width).toBeLessThan(shortFieldBox!.width - 30);
  const idleStyle = await shortButton.evaluate((el) => {
    const css = getComputedStyle(el);
    return { background: css.backgroundColor, color: css.color, decoration: css.textDecorationLine, cursor: css.cursor };
  });
  expect(idleStyle.decoration).toBe('none');
  expect(idleStyle.cursor).toBe('pointer');
  await shortButton.hover();
  const hoverStyle = await shortButton.evaluate((el) => {
    const css = getComputedStyle(el);
    return { background: css.backgroundColor, color: css.color, decoration: css.textDecorationLine };
  });
  expect(hoverStyle).toEqual({ background: idleStyle.background, color: idleStyle.color, decoration: 'none' });
  await page.mouse.click(shortFieldBox!.x + shortFieldBox!.width - 8, shortBox!.y + shortBox!.height / 2);
  expect(await page.evaluate(() => (window as unknown as { __openedUrls?: string[] }).__openedUrls ?? [])).toEqual([]);

  await shortButton.focus();
  await expect(shortButton).toBeFocused();
  expect(await shortButton.evaluate((el) => getComputedStyle(el).outlineWidth)).toBe('2px');
  await page.keyboard.press('Enter');
  expect(await page.evaluate(() => (window as unknown as { __openedUrls: string[] }).__openedUrls)).toEqual([shortUrl]);

  const longButton = fields.nth(1).getByTestId('field-url');
  const longBounds = await longButton.evaluate((el) => ({ width: el.clientWidth, scrollWidth: el.scrollWidth, height: el.clientHeight }));
  expect(longBounds.scrollWidth).toBeLessThanOrEqual(longBounds.width + 1);
  expect(longBounds.height).toBeGreaterThan(35);
  expect(await fields.nth(1).evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  await page.screenshot({ path: 'artifacts/ui-review/url-field-hit-areas.png' });
  await longButton.click();
  expect(await page.evaluate(() => (window as unknown as { __openedUrls: string[] }).__openedUrls)).toEqual([shortUrl, longUrl]);
  await page.setViewportSize({ width: 390, height: 780 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
  expect(await longButton.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  await page.screenshot({ path: 'artifacts/ui-review/url-field-hit-areas-narrow.png' });
});
