import { test, expect } from '@playwright/test';
import { boot } from './helpers';

test('CLI Proxy shows cost, tokens, period breakdown and remaining quota', async ({ page }) => {
  await boot(page, {});
  await page.getByTestId('tab-cliproxy').click();
  await expect(page.getByTestId('cliproxy-breakdown-model')).toBeVisible();
  await expect(page.getByTestId('cliproxy-metric-cost')).toHaveAttribute('aria-selected', 'true');
  await page.getByTestId('cliproxy-metric-tokens').click();
  await expect(page.getByRole('heading', { name: 'Daily processed tokens' })).toBeVisible();
  await page.getByTestId('cliproxy-window-1').click();
  await expect(page.getByRole('heading', { name: 'Hourly processed tokens' })).toBeVisible();
  await page.getByRole('tab', { name: 'Hour', exact: true }).click();
  await expect(page.getByTestId('cliproxy-breakdown-time')).toBeVisible();
  await page.getByTestId('cliproxy-metric-limits').click();
  await expect(page.getByTestId('cliproxy-limits')).toContainText('66%');
  await expect(page.getByTestId('cliproxy-limits')).toContainText('91%');
  await page.getByTestId('cliproxy-refresh').click();
  await expect(page.getByTestId('cliproxy-refresh')).toBeEnabled();
});

test('missing quota does not hide token history', async ({ page }) => {
  await boot(page, { cliproxy: { limitsError: 'Codex token expired' } });
  await page.getByTestId('tab-cliproxy').click();
  await expect(page.getByTestId('cliproxy-breakdown-model')).toBeVisible();
  await page.getByTestId('cliproxy-metric-limits').click();
  await expect(page.getByText('Codex token expired')).toBeVisible();
});

test('empty data and a failed request are explicit', async ({ page }) => {
  await boot(page, { cliproxy: { records: 'empty' } });
  await page.getByTestId('tab-cliproxy').click();
  await expect(page.getByTestId('cliproxy-breakdown-model')).toContainText('No activity');
  await boot(page, { cliproxy: { fail: 'Usage request unavailable' } });
  await page.getByTestId('tab-cliproxy').click();
  await expect(page.getByTestId('cliproxy-error')).toHaveText('Usage request unavailable');
});

test('pending usage shows loading instead of zero totals', async ({ page }) => {
  await boot(page, { cliproxy: { delayMs: 1200 } });
  await page.getByTestId('tab-cliproxy').click();
  await expect(page.getByTestId('cliproxy-loading')).toBeVisible();
  await expect(page.getByTestId('cliproxy-breakdown-model')).not.toBeVisible();
  await expect(page.getByText('$0.00', { exact: true })).not.toBeVisible();
  await expect(page.getByTestId('cliproxy-breakdown-model')).toBeVisible();
  await expect(page.getByTestId('cliproxy-loading')).not.toBeVisible();
});
