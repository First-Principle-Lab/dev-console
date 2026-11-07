import { expect, test, type Page } from '@playwright/test';

import { boot } from './helpers';

const ago = (hours: number): string => new Date(Date.now() - hours * 3600_000).toISOString();

const SEED = {
  commands: [
    {
      id: 'c1', revision: 1, name: 'Deploy staging', type: 'Shell', tags: [], description: '',
      notes: '', fields: [], pinned: false, command: 'make deploy', workingDirectory: '',
      usageCount: 1, updatedAt: ago(1)
    },
    {
      id: 'c2', revision: 1, name: 'Rebase main', type: 'Git', tags: [], description: '',
      notes: '', fields: [], pinned: false, command: 'git rebase', workingDirectory: '',
      usageCount: 0, updatedAt: ago(2)
    }
  ],
  secrets: [
    {
      id: 's1', revision: 1, name: 'Registry token', type: 'API key', tags: [], description: '',
      notes: '', fields: [], pinned: false, value: 'synthetic-value', updatedAt: ago(3)
    }
  ]
};

async function openSettings(page: import('@playwright/test').Page): Promise<void> {
  await boot(page, SEED);
  await page.getByTestId('settings').click();
  await expect(page.getByTestId('settings-panel')).toBeVisible();
}

test('import requires a review that shows the archive date, counts and what gets replaced', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('import-path').fill('/tmp/portable.age');
  await page.getByTestId('import-archive').click();
  await expect(page.getByTestId('review-card')).toBeVisible();
  await expect(page.getByTestId('review-warning')).toContainText('replaces your entire library');
  // Archive side: the mock summary mirrors the seed counts; the current side counts live data.
  await expect(page.getByTestId('review-card')).toContainText('2 commands');
  await expect(page.getByTestId('review-card')).toContainText('1 secrets');
  await expect(page.getByTestId('review-current')).toContainText('2 commands');
  // Cancel applies nothing.
  await page.getByTestId('review-cancel').click();
  await expect(page.getByTestId('review-card')).toHaveCount(0);
  await expect(page.getByTestId('backup-message')).toHaveCount(0);
});

test('the confirm step offers a pre-restore backup and says it took one', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('import-path').fill('/tmp/portable.age');
  await page.getByTestId('import-archive').click();
  await expect(page.getByTestId('prebackup')).toBeChecked();
  await page.getByTestId('review-confirm').click();
  await expect(page.getByTestId('backup-message')).toContainText('backed up first');
  // The safety backup itself shows up in the list.
  await expect(page.getByTestId('backup-list').getByTestId('restore-backup')).toHaveCount(1);
});

test('export refuses an existing file and only replaces after explicit confirmation', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('archive-path').fill('/tmp/once.age');
  await page.getByTestId('export-archive').click();
  await expect(page.getByTestId('backup-message')).toContainText('Archive exported');

  await page.getByTestId('export-archive').click();
  await expect(page.getByTestId('backup-error')).toContainText('already exists');
  await expect(page.getByTestId('export-overwrite-card')).toBeVisible();

  await page.getByTestId('export-overwrite-cancel').click();
  await expect(page.getByTestId('export-overwrite-card')).toHaveCount(0);
  await expect(page.getByTestId('backup-message')).toHaveCount(0);

  await page.getByTestId('export-archive').click();
  await page.getByTestId('export-overwrite-confirm').click();
  await expect(page.getByTestId('backup-message')).toContainText('Archive exported');
});

test('the recovery credential stays hidden until revealed and shows the restore steps', async ({ page }) => {
  await openSettings(page);
  await expect(page.getByTestId('recovery-reveal')).toBeVisible();
  await expect(page.getByTestId('recovery-secret')).toHaveCount(0);
  await page.getByTestId('recovery-reveal').click();
  await expect(page.getByTestId('recovery-secret')).toContainText('AGE-SECRET-KEY-1');
  await expect(page.getByTestId('recovery-steps')).toContainText('recovery.agekey');

  // Locked vault: revealing must fail loudly instead of returning the credential.
  await boot(page, SEED);
  await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
  await page.getByTestId('settings').click();
  await page.getByTestId('recovery-reveal').click();
  await expect(page.getByTestId('recovery-error')).toContainText('vault is locked');
});

test('Settings loads in a locked profile and a general setting can still be changed and saved', async ({ page }) => {
  await boot(page, { ...SEED, manualLock: true });
  await page.getByTestId('settings').click();
  await expect(page.getByTestId('settings-panel')).toBeVisible();
  // The General controls must be real inputs, not an endless "Loading".
  await expect(page.getByTestId('clipboard-expiry')).toBeEditable();
  await expect(page.getByTestId('terminal')).toBeEditable();
  await expect(page.getByTestId('settings-panel')).not.toContainText('Loading');
  // The backup folder comes from app_state, which a locked vault still answers.
  await expect(page.getByTestId('backup-dir')).toContainText('/home/mock');

  await page.getByTestId('clipboard-expiry').fill('45');
  await page.getByTestId('save-settings').click();
  const stored = await page.evaluate(
    () => (window as unknown as { __mock: { state: { settings: { clipboardExpirySeconds: number } } } }).__mock
      .state.settings.clipboardExpirySeconds
  );
  expect(stored).toBe(45);

  // While locked there is no secret count to show, but the command count is still there.
  await page.getByTestId('import-path').fill('/tmp/portable.age');
  await page.getByTestId('import-archive').click();
  await expect(page.getByTestId('review-card')).toBeVisible();
  await expect(page.getByTestId('review-current')).toContainText('2 commands');
  await expect(page.getByTestId('review-current')).toContainText('unavailable');
});

test('a revealed recovery credential disappears the moment the vault locks', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('recovery-reveal').click();
  await expect(page.getByTestId('recovery-secret')).toContainText('AGE-SECRET-KEY-1');

  await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
  await expect(page.getByTestId('recovery-secret')).toHaveCount(0);
  await expect(page.getByTestId('recovery-reveal')).toBeVisible();

  // Closing and reopening must not resurrect it either.
  await page.getByTestId('close-settings').click();
  await page.getByTestId('settings').click();
  await expect(page.getByTestId('recovery-secret')).toHaveCount(0);
});

test('a revealed recovery credential is not kept across a close and reopen', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('recovery-reveal').click();
  await expect(page.getByTestId('recovery-secret')).toContainText('AGE-SECRET-KEY-1');
  await page.getByTestId('close-settings').click();
  await page.getByTestId('settings').click();
  await expect(page.getByTestId('recovery-secret')).toHaveCount(0);
  await expect(page.getByTestId('recovery-reveal')).toBeVisible();
});

test('the recovery credential copies through the backend clipboard path, never the browser one', async ({ page }) => {
  await openSettings(page);
  await page.evaluate(() => {
    const store = window as unknown as { __browserCopies: string[] };
    store.__browserCopies = [];
    Object.defineProperty(window.navigator, 'clipboard', {
      configurable: true,
      value: { writeText: async (text: string) => void store.__browserCopies.push(text) }
    });
  });

  await page.getByTestId('recovery-reveal').click();
  await page.getByTestId('recovery-copy').click();
  await expect(page.getByTestId('recovery-copy')).toHaveText('Copied');

  const copied = await page.evaluate(
    () => (window as unknown as { __mock: { state: { copied: { id: string; owned?: boolean }[] } } }).__mock.state
      .copied
  );
  expect(copied.filter((c) => c.id === 'recovery-credential')).toHaveLength(1);
  // The web clipboard was never touched, so the value bypasses nothing the native path guards.
  expect(await page.evaluate(() => (window as unknown as { __browserCopies: string[] }).__browserCopies)).toEqual([]);
  // The status line must not repeat the credential.
  await expect(page.getByTestId('recovery-copy-note')).not.toContainText('AGE-SECRET');

  // An unrelated clipboard value that was there already survives the lock; the app-owned one does not.
  const after = await page.evaluate(() => {
    const mock = (window as unknown as { __mock: { state: { copied: unknown[] }; lockVault: () => void } }).__mock;
    mock.state.copied.push({ id: 'someone-else', kind: 'command', value: 'copied-by-another-app' });
    mock.lockVault();
    return mock.state.copied.map((c) => (c as { id: string }).id);
  });
  expect(after).not.toContain('recovery-credential');
  expect(after).toContain('someone-else');
});

test('at the minimum window size the footer and a confirmation card stay reachable', async ({ page }) => {
  await openSettings(page);
  await page.setViewportSize({ width: 760, height: 520 });

  // The footer is pinned outside the scrolling body, so it never needs a scroll to reach.
  await expect(page.getByTestId('save-settings')).toBeInViewport();

  // Keyboard only: Tab has to walk from a control in the scrolling body into the footer.
  await page.getByTestId('import-path').focus();
  let reached = false;
  for (let i = 0; i < 8 && !reached; i += 1) {
    await page.keyboard.press('Tab');
    reached = await page.evaluate(() => {
      const active = document.activeElement as HTMLElement | null;
      return active?.getAttribute('data-testid') === 'save-settings';
    });
  }
  expect(reached, 'Tab must walk from the body into the footer Save action').toBe(true);

  // A confirmation card renders below the fold; it must scroll into view *and* take focus.
  await page.getByTestId('import-path').fill('/tmp/portable.age');
  await page.getByTestId('import-archive').click();
  await expect(page.getByTestId('review-card')).toBeInViewport();
  await expect(page.getByTestId('review-confirm')).toBeInViewport();
  await expect(page.getByTestId('review-cancel')).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('review-card')).toHaveCount(0);
});

test('the terminal field parses quoted arguments instead of splitting on spaces', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('terminal').fill('foot --title "Dev Console -e');
  await expect(page.getByTestId('terminal-error')).toBeVisible();
  await page.getByTestId('terminal').fill('"/usr/bin/my terminal" --title "Dev Console" -e');
  await expect(page.getByTestId('terminal-error')).toHaveCount(0);
  const chips = page.getByTestId('terminal-tokens').locator('code');
  await expect(chips).toHaveCount(4);
  await expect(chips.nth(0)).toHaveText('/usr/bin/my terminal');
  await page.getByTestId('save-settings').click();
  const stored = await page.evaluate(() => (window as unknown as { __mock: { state: { settings: { terminal: string[] } } } }).__mock.state.settings.terminal);
  expect(stored).toEqual(['/usr/bin/my terminal', '--title', 'Dev Console', '-e']);
});

test('Settings lists installed terminals and saves the selected launch command', async ({ page }) => {
  await openSettings(page);
  const choice = page.getByTestId('terminal-choice');
  await expect(choice.getByRole('option')).toHaveText(['Ghostty', 'Kitty', 'Custom command']);
  await choice.selectOption('kitty');
  await expect(page.getByTestId('terminal')).toHaveCount(0);
  await expect(page.getByTestId('terminal-tokens')).toContainText('/usr/bin/kitty');
  await page.screenshot({ path: 'artifacts/ui-review/terminal-settings.png' });
  await page.getByTestId('save-settings').click();
  const stored = await page.evaluate(() => (window as unknown as { __mock: { state: { settings: { terminal: string[] } } } }).__mock.state.settings.terminal);
  expect(stored).toEqual(['/usr/bin/kitty', '--']);
  await page.getByTestId('close-settings').click();
  await page.getByTestId('settings').click();
  await expect(page.getByTestId('terminal-choice')).toHaveValue('kitty');
});

test('an unbalanced quote blocks saving the terminal setting', async ({ page }) => {
  await openSettings(page);
  await page.getByTestId('terminal').fill('foot --title "broken');
  await expect(page.getByTestId('terminal-error')).toContainText('unbalanced');
  await expect(page.getByTestId('save-settings')).toBeDisabled();
});

test('Settings offers the project ranking switch and stores it', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('settings').click();
  await page.getByTestId('settings-panel').waitFor();
  const toggle = page.getByTestId('project-context');
  await expect(toggle).toBeChecked();
  await expect(page.getByTestId('project-context-detail')).toContainText('/proc');
  await toggle.uncheck();
  await page.getByTestId('save-settings').click();
  expect(await storedSetting(page, 'projectContextEnabled')).toBe(false);
  await page.getByTestId('project-context').check();
  await page.getByTestId('save-settings').click();
  expect(await storedSetting(page, 'projectContextEnabled')).toBe(true);
});

function storedSetting(page: Page, key: string): Promise<unknown> {
  return page.evaluate(
    (name) =>
      (window as unknown as { __mock: { state: { settings: Record<string, unknown> } } })
        .__mock.state.settings[name],
    key
  );
}
