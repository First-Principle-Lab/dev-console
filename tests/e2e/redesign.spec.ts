import { expect, test } from '@playwright/test';

import { boot, mockState } from './helpers';

const SEED = {
  commands: [
    {
      id: 'c1', revision: 2, name: 'Deploy staging', type: 'Shell', tags: ['work'],
      description: 'Ships staging', notes: '', fields: [], pinned: true,
      command: 'make deploy', workingDirectory: '', usageCount: 5
    },
    {
      id: 'c2', revision: 1, name: 'Rebase main', type: 'Git', tags: ['git'],
      description: '', notes: '', fields: [], pinned: false,
      command: 'git rebase origin/main', workingDirectory: '', usageCount: 0
    }
  ],
  secrets: [
    {
      id: 's1', revision: 1, name: 'Registry token', type: 'API key', tags: ['infra'],
      description: '', notes: '', fields: [], pinned: false, value: 'synthetic-value-1'
    }
  ]
};

test('tabs carry entry counts and expose one selected tab', async ({ page }) => {
  await boot(page, SEED);
  await expect(page.getByTestId('tab-commands')).toContainText('Commands');
  await expect(page.getByTestId('tab-commands')).toContainText('2');
  await expect(page.getByTestId('tab-secrets')).toContainText('1');
  await expect(page.getByTestId('tab-commands')).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByTestId('tab-secrets')).toHaveAttribute('aria-selected', 'false');

  await page.getByTestId('tab-secrets').click();
  await expect(page.getByTestId('tab-secrets')).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByTestId('tab-commands')).toHaveAttribute('aria-selected', 'false');
});

test('secret rows stay masked and are visibly a different kind', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('tab-secrets').click();
  const row = page.getByTestId('entry-row').first();
  await expect(row).toContainText('Registry token');
  await expect(row.getByLabel('Value hidden')).toBeVisible();
  const body = await page.content();
  expect(body).not.toContain('synthetic-value-1');
});

test('the editor reports unsaved changes and clears them on cancel', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('add-entry').click();
  await expect(page.getByTestId('dirty-indicator')).toHaveCount(0);
  await page.getByTestId('field-name').fill('Half typed');
  await expect(page.getByTestId('dirty-indicator')).toBeVisible();
  await page.getByTestId('cancel-edit').click();
  await expect(page.getByTestId('dirty-indicator')).toHaveCount(0);
});

test('locked vault hides the list and offers one clear unlock action', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('tab-secrets').click();
  await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
  await expect(page.getByTestId('unlock-vault')).toBeVisible();
  await expect(page.getByTestId('entry-row')).toHaveCount(0);
  await expect(await page.content()).not.toContain('Registry token');
});

test('finder keeps combobox/listbox semantics while typing', async ({ page }) => {
  await boot(page, SEED, 'finder');
  const input = page.getByTestId('finder-input');
  await expect(input).toBeFocused();
  await expect(input).toHaveAttribute('role', 'combobox');
  await expect(input).toHaveAttribute('aria-expanded', 'true');
  await expect(input).toHaveAttribute('aria-autocomplete', 'both');
  await expect(input).toHaveAttribute('aria-controls', 'finder-results');

  await input.fill('Deploy');
  const list = page.getByRole('listbox', { name: 'Results' });
  await expect(list).toBeVisible();
  const active = await input.getAttribute('aria-activedescendant');
  expect(active).toBeTruthy();
  await expect(page.locator(`#${active as string}`)).toHaveAttribute('aria-selected', 'true');
});

test('copying from the finder reports a copied state without the value', async ({ page }) => {
  await boot(page, SEED, 'finder');
  await page.getByTestId('finder-input').fill('Deploy');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('finder-status')).toContainText('Copied');
  expect(await page.content()).not.toContain('make deploy');
  const copied = await mockState(page);
  expect(copied.copied).toHaveLength(1);
});

test('a locked vault turns the finder into one keyboard-reachable unlock row', async ({ page }) => {
  await boot(page, SEED, 'finder');
  await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
  await page.getByTestId('finder-input').fill('token');
  const system = page.locator('.row.system');
  await expect(system).toContainText('Secret metadata stays encrypted');
  await expect(page.getByTestId('result-row')).toHaveCount(1);
  expect(await page.content()).not.toContain('Registry token');
});

test('the finder footer presents keyboard hints as keycaps', async ({ page }) => {
  await boot(page, SEED, 'finder');
  const keys = page.locator('footer kbd');
  await expect(keys.filter({ hasText: 'Tab' })).toBeVisible();
  await expect(keys.filter({ hasText: 'Esc' })).toBeVisible();
  await expect(keys.filter({ hasText: 'Ctrl' })).toBeVisible();
});

test('narrow windows keep one pane with an explicit way back to the list', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 780 });
  await boot(page, SEED);
  // The list is the landing pane on a phone-width window: nothing is auto-selected.
  await expect(page.getByTestId('entry-row')).toHaveCount(2);
  await expect(page.getByTestId('entry-details')).toHaveCount(0);
  await page.getByTestId('entry-row').first().click();
  await expect(page.getByTestId('entry-details')).toBeVisible();
  await expect(page.getByTestId('entry-row')).toHaveCount(0);
  await page.getByTestId('back-to-list').click();
  await expect(page.getByTestId('entry-details')).toHaveCount(0);
  await expect(page.getByTestId('entry-row')).toHaveCount(2);
});

test('a locked vault is presented as a state, never as an error', async ({ page }) => {
  // It used to render a red failure banner for an expected condition, on top of the locked
  // notice and the unlock action: three competing messages for one state.
  await boot(page, SEED);
  await page.getByTestId('tab-secrets').click();
  await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
  await expect(page.getByTestId('status-error')).toHaveCount(0);
  await expect(page.getByTestId('list-locked')).toBeVisible();
  await expect(page.getByTestId('unlock-vault')).toBeVisible();
});

test('list rows show relative times and never overflow their own line', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('entry-row').first().click();
  const meta = page.locator('[data-testid="entry-row"]').first().locator('.meta');
  await expect(meta).toContainText(/ago$/);
  const clipped = await page.$$eval('[data-testid="entry-row"] .meta', (nodes) =>
    nodes.map((n) => ({ over: n.scrollWidth > n.clientWidth, text: n.textContent?.trim() }))
  );
  for (const row of clipped) {
    expect(row.over, `metadata line clipped: ${row.text}`).toBe(false);
  }
});

test('a fresh editor gives guidance on save, not a warning on open', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('add-entry').click();
  await expect(page.getByTestId('validation-hint')).toHaveCount(0);
  await expect(page.getByTestId('save-error')).toHaveCount(0);
  await page.getByTestId('field-name').fill('No type yet');
  await page.getByTestId('save').click();
  await expect(page.getByTestId('save-error')).toContainText('Type is required.');
  const after = await mockState(page);
  expect(after.commands).toHaveLength(2);
  // …and the hint follows further typing once the user has tried to save.
  await page.getByTestId('field-type').fill('Shell');
  await page.getByTestId('field-command').fill('make deploy');
  await page.getByTestId('save').click();
  const saved = await mockState(page);
  expect(saved.commands).toHaveLength(3);
});

test('a copied confirmation does not describe the next search', async ({ page }) => {
  await boot(page, SEED, 'finder');
  await page.getByTestId('finder-input').fill('Deploy');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('finder-status')).toContainText('Copied');
  // The window is reopened for an unrelated query; the old confirmation must not still be up.
  await page.getByTestId('finder-input').fill('Reg');
  await expect(page.getByTestId('finder-status')).toHaveCount(0);
});

test('the selected finder row is perceivable without colour alone, and hover is not selection', async ({ page }) => {
  await boot(page, SEED, 'finder');
  const rows = page.getByTestId('result-row');
  await expect(rows).toHaveCount(3);
  // Colour is the weakest cue: the selected row additionally carries the solid inset
  // marker box-shadow (--selected-marker) that no unselected row has, so the selection
  // survives greyscale and any colour-vision difference.
  const marks = await rows.evaluateAll((els) =>
    els.map((el) => {
      const cs = getComputedStyle(el);
      return {
        selected: el.getAttribute('aria-selected') === 'true',
        marker: cs.boxShadow.includes('inset'),
        background: cs.backgroundColor
      };
    })
  );
  const active = marks.filter((m) => m.selected);
  expect(active).toHaveLength(1);
  expect(active[0]?.marker, 'selected row must carry the inset marker').toBe(true);
  for (const m of marks.filter((x) => !x.selected)) {
    expect(m.marker, 'unselected row must not carry the inset marker').toBe(false);
  }
  // Hover must read differently from selection: hovering an unselected row paints the
  // neutral hover fill, never the selected band.
  await rows.nth(1).hover();
  const hovered = await rows.nth(1).evaluate((el) => getComputedStyle(el).backgroundColor);
  const selectedBg = await rows.nth(0).evaluate((el) => getComputedStyle(el).backgroundColor);
  expect(hovered, 'hover fill must differ from the selected band').not.toBe(selectedBg);
});

test('a locked finder keeps secret names, types and tags out of the DOM', async ({ page }) => {
  await boot(page, SEED, 'finder');
  await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
  await page.getByTestId('finder-input').fill('Reg');
  await expect(page.getByTestId('result-row')).toHaveCount(1);
  // Not just the seeded value: the name, the type and the tags of the locked secret
  // must all be absent while the vault is locked.
  const body = await page.content();
  for (const metadata of ['synthetic-value-1', 'Registry token', 'API key', 'infra']) {
    expect(body, `locked vault DOM leaked secret metadata "${metadata}"`).not.toContain(metadata);
  }
});

test('a long name truncates without losing the right-aligned action', async ({ page }) => {
  const longName =
    'Rebuild and republish the staging image for the payments service with verbose logging, then run the full smoke suite';
  await boot(
    page,
    {
      commands: [
        {
          id: 'L1', revision: 1, name: longName, type: 'Docker / compose', tags: ['payments'],
          description: 'Recreates the compose project and smoke-tests the health endpoint.',
          notes: '', fields: [], pinned: true, command: 'docker compose up -d', workingDirectory: '', usageCount: 1
        }
      ],
      secrets: []
    },
    'finder'
  );
  const row = page.getByTestId('result-row').first();
  await expect(row).toContainText(longName);
  // The palette caps at 680px wide, so the name must ellipsis rather than push the
  // action off the row: clipped name, fully visible action inside the row.
  const clipped = await row.locator('.name').evaluate((el) => el.scrollWidth > (el as HTMLElement).clientWidth);
  expect(clipped, 'long name should ellipsize').toBe(true);
  const action = row.getByTestId('row-action');
  await expect(action).toBeVisible();
  const [rowBox, actionBox] = await Promise.all([row.boundingBox(), action.boundingBox()]);
  expect(rowBox).not.toBeNull();
  expect(actionBox).not.toBeNull();
  expect(actionBox!.x + actionBox!.width).toBeLessThanOrEqual(rowBox!.x + rowBox!.width + 1);
});

