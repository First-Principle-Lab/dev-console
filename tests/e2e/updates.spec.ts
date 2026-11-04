import { expect, test } from '@playwright/test';

import { boot } from './helpers';

const SEED = { commands: [], secrets: [] };

type UpdaterState = { checks: number; installs: number; relaunches: number; available?: string; failCheck?: string };

function updater(page: import('@playwright/test').Page): Promise<UpdaterState> {
  return page.evaluate(() => (window as never as { __mock: { state: { updater: UpdaterState } } }).__mock.state.updater);
}

test('Settings shows the running version and says when it is current', async ({ page }) => {
  await boot(page, { ...SEED, updater: { current: '0.0.4' } });
  await page.getByTestId('settings').click();
  await expect(page.getByTestId('current-version')).toHaveText('Dev Console 0.0.4');
  await expect(page.getByTestId('update-status')).toContainText("You're on the latest version");
  await expect(page.getByTestId('install-update')).toHaveCount(0);
});

test('Check for updates finds a release that appeared after launch and installs it', async ({ page }) => {
  await boot(page, { ...SEED, updater: { current: '0.0.4' } });
  await page.getByTestId('settings').click();
  await expect(page.getByTestId('update-status')).toContainText('latest version');
  const checksBefore = (await updater(page)).checks;

  await page.evaluate(() => {
    (window as never as { __mock: { state: { updater: { available: string } } } }).__mock.state.updater.available = '0.0.5';
  });
  await page.getByTestId('check-updates').click();
  await expect(page.getByTestId('update-status')).toContainText('Version 0.0.5 is available');
  expect((await updater(page)).checks).toBe(checksBefore + 1);
  // The header offers it too, from the same state.
  await expect(page.getByTestId('install-update')).toHaveText(/Update to 0\.0\.5/);

  await page.getByTestId('settings-install-update').click();
  await expect.poll(async () => (await updater(page)).relaunches).toBe(1);
  expect((await updater(page)).installs).toBe(1);
});

test('a failed check shows the reason in Settings and a way there from the header', async ({ page }) => {
  await boot(page, { ...SEED, updater: { current: '0.0.4', failCheck: 'network unreachable' } });
  await page.getByTestId('update-failed').click();
  await expect(page.getByTestId('update-error')).toHaveText('network unreachable');
  await page.evaluate(() => {
    (window as never as { __mock: { state: { updater: { failCheck?: string } } } }).__mock.state.updater.failCheck = undefined;
  });
  await page.getByTestId('check-updates').click();
  await expect(page.getByTestId('update-status')).toContainText('latest version');
  await expect(page.getByTestId('update-error')).toHaveCount(0);
});

test('a browser preview says updates need the installed app', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('settings').click();
  await expect(page.getByTestId('update-status')).toHaveText('Updates are only available in the installed app.');
  await expect(page.getByTestId('check-updates')).toHaveCount(0);
});
