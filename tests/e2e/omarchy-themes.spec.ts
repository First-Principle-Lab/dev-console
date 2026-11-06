import { expect, test } from '@playwright/test';
import path from 'node:path';
import { OMARCHY_THEMES } from '../../src/lib/models';
import { boot } from './helpers';

const OUT = path.join(import.meta.dirname, '..', '..', 'artifacts', 'ui-review', 'omarchy-themes');
const SEED = {
  commands: [{ id: 'command', revision: 1, name: 'Deploy staging', type: 'Shell',
    tags: ['work', 'release'], description: 'Build and ship the staging image', notes: 'Review before running.',
    fields: [], pinned: true, command: 'make deploy', workingDirectory: '~/project', usageCount: 4 }],
  secrets: [{ id: 'secret', revision: 1, name: 'Registry token', type: 'API key',
    tags: ['infra'], description: 'Used by the publish job', notes: '', fields: [],
    pinned: false, value: 'synthetic-theme-secret' }]
};

async function checkViewport(page: import('@playwright/test').Page): Promise<void> {
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - innerWidth);
  expect(overflow).toBeLessThanOrEqual(1);
  await expect(page.locator('body')).not.toContainText('undefined');
}

test('theme choices preview immediately and Save keeps the choice', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('settings').click();
  await page.getByLabel('Theme').selectOption('omarchy-tokyo-night');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'omarchy-tokyo-night');
  const previewBackground = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  expect(previewBackground).toBe('rgb(26, 27, 38)');
  const beforeSave = await page.evaluate(() => (window as unknown as { __mock: { state: { settings: { theme: string } } } }).__mock.state.settings.theme);
  expect(beforeSave).toBe('dark');
  await page.getByTestId('save-settings').click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'omarchy-tokyo-night');
  const stored = await page.evaluate(() => (window as unknown as { __mock: { state: { settings: { theme: string } } } }).__mock.state.settings.theme);
  expect(stored).toBe('omarchy-tokyo-night');
  await page.getByTestId('close-settings').click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'omarchy-tokyo-night');
  await page.getByTestId('settings').click();
  await expect(page.getByLabel('Theme')).toHaveValue('omarchy-tokyo-night');
});

test('closing Settings discards a theme preview', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('settings').click();
  await page.getByLabel('Theme').selectOption('omarchy-white');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'omarchy-white');
  await page.getByTestId('close-settings').click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByTestId('settings').click();
  await expect(page.getByLabel('Theme')).toHaveValue('dark');
});

for (const [slug, name] of OMARCHY_THEMES) {
  test(`${name} renders library, secrets, editor, settings and finder`, async ({ page }) => {
    const theme = `omarchy-${slug}`;
    const settings = { theme, autostart: false, clipboardExpirySeconds: 30,
      terminal: ['x-terminal-emulator', '-e'] };
    await page.setViewportSize({ width: 1040, height: 700 });
    await boot(page, { ...SEED, settings });
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    await expect(page.getByTestId('entry-row')).toHaveCount(1);
    await checkViewport(page);
    await page.screenshot({ path: path.join(OUT, `${slug}-library.png`) });

    await page.getByTestId('tab-secrets').click();
    await expect(page.getByTestId('entry-row')).toContainText('Registry token');
    expect(await page.content()).not.toContain('synthetic-theme-secret');
    await checkViewport(page);
    await page.screenshot({ path: path.join(OUT, `${slug}-secrets.png`) });

    await page.getByTestId('add-entry').click();
    await expect(page.getByTestId('field-name')).toBeVisible();
    await checkViewport(page);
    await page.screenshot({ path: path.join(OUT, `${slug}-editor.png`) });

    await page.getByTestId('settings').click();
    await expect(page.getByLabel('Theme')).toHaveValue(theme);
    await checkViewport(page);
    await page.screenshot({ path: path.join(OUT, `${slug}-settings.png`) });

    await page.setViewportSize({ width: 680, height: 420 });
    await boot(page, { ...SEED, settings }, 'finder');
    await expect(page.getByTestId('result-row')).toHaveCount(2);
    await checkViewport(page);
    await page.screenshot({ path: path.join(OUT, `${slug}-finder.png`) });
  });
}
