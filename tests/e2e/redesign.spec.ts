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
  await expect(row).toContainText('hidden');
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
