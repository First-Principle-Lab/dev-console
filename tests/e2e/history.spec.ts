import { expect, test } from '@playwright/test';

import { boot, mockState } from './helpers';

/**
 * The shell-history suggestion panel, against the IPC double's seeded history. The ranking, the
 * digest and the "already saved" filter are the Rust `history` module's job and are covered by
 * its own tests; here the contract is the panel's behaviour: what it offers, what a click costs,
 * and that a dismissal is the only thing remembered.
 */

type HistoryEntry = { command: string; count: number };

const history = (entries: HistoryEntry[]): Record<string, unknown> => ({ history: entries });

const SETTINGS_OFF = {
  theme: 'dark',
  autostart: false,
  clipboardExpirySeconds: 30,
  terminal: ['x-terminal-emulator', '-e'],
  historySuggestionsEnabled: false
};

const SEED = {
  commands: [
    {
      id: 'c1', revision: 1, name: 'Deploy staging', type: 'Shell', tags: [],
      description: '', notes: '', fields: [], pinned: false,
      command: 'make deploy staging', workingDirectory: '', usageCount: 2
    }
  ],
  secrets: []
};

test('offers the ranked history above the commands, minus what is already saved', async ({ page }) => {
  await boot(page, {
    ...SEED,
    history: [
      { command: 'make deploy staging', count: 40 },
      { command: 'docker compose up', count: 12 },
      { command: 'bun run test:watch', count: 7 },
      { command: 'kubectl rollout restart deploy/api', count: 3 }
    ]
  });

  const panel = page.getByTestId('history-suggestions');
  await expect(panel).toBeVisible();
  const rows = page.getByTestId('history-suggestion-row');
  // Most-used first, and `make deploy staging` is missing: it is already in the library.
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(0)).toContainText('docker compose up');
  await expect(rows.nth(0)).toContainText('used 12×');
  await expect(rows.nth(2)).toContainText('kubectl rollout restart');
  // The library already holds that exact command, so it is not offered back.
  await expect(page.getByTestId('history-suggestion-list')).not.toContainText('make deploy');
  // The commands themselves are untouched by any of this.
  await expect(page.getByTestId('entry-row')).toHaveCount(1);

  // Each keyboard target names the command, the count and both actions.
  const targets = page.getByTestId('history-suggestion-command');
  await expect(targets).toHaveCount(3);
  await expect(targets.nth(0)).toHaveAttribute(
    'aria-label',
    /^docker compose up - used 12 times\. Enter saves it as “docker-compose-up”, Delete dismisses it\.$/
  );
  await expect(targets.nth(1)).toHaveAttribute(
    'aria-label',
    /^bun run test:watch - used 7 times\./
  );
});

test('saving opens the editor prefilled and writes nothing until it is confirmed', async ({ page }) => {
  await boot(page, {
    ...SEED,
    history: [{ command: 'docker compose up', count: 12 }]
  });

  await page.getByTestId('history-suggestion-save').first().click();
  await expect(page.getByTestId('entry-editor')).toBeVisible();
  await expect(page.getByTestId('field-name')).toHaveValue('docker-compose-up');
  await expect(page.getByTestId('field-command')).toHaveValue('docker compose up');
  // Opening the editor is not a write.
  const state = await mockState(page);
  expect(state.commands).toHaveLength(1);

  await page.getByTestId('save').click();
  await expect(page.getByTestId('entry-row')).toHaveCount(2);
  const saved = await page.evaluate(() =>
    (
      window as unknown as {
        __mock: { state: { commands: { command: string }[] } };
      }
    ).__mock.state.commands.map((entry) => entry.command)
  );
  expect(saved).toContain('docker compose up');
  // A command that is in the library stops being offered.
  await expect(page.getByTestId('history-suggestion-row')).toHaveCount(0);
});

test('dismissing takes the suggestion away and is the only thing remembered', async ({ page }) => {
  await boot(page, {
    ...SEED,
    history: [
      { command: 'docker compose up', count: 12 },
      { command: 'bun run test:watch', count: 7 }
    ]
  });

  const before = await mockState(page);
  await page.getByTestId('history-suggestion-dismiss').first().click();
  await expect(page.getByTestId('history-suggestion-row')).toHaveCount(1);
  await expect(page.getByTestId('history-suggestion-row').first()).toContainText('bun run');

  const after = await mockState(page);
  // A dismissal creates no command, and what it persists is a digest rather than the line.
  expect(after.commands).toHaveLength(before.commands.length);
  const persisted = await page.evaluate(
    () =>
      JSON.parse(
        window.localStorage.getItem('dev-console-test-dismissed-suggestions') ?? '[]'
      ) as string[]
  );
  expect(persisted, 'exactly one dismissal is remembered').toHaveLength(1);
  expect(persisted[0], 'a digest, in the shape the backend stores').toMatch(/^[0-9a-f]{64}$/);

  // Reload, not re-boot: the panel must still not offer it.
  await page.reload();
  await expect(page.getByTestId('history-suggestion-row')).toHaveCount(1);
  await expect(page.getByTestId('history-suggestion-row').first()).toContainText('bun run');
});

test('the panel is absent entirely when the setting is off', async ({ page }) => {
  await boot(page, {
    ...SEED,
    ...history([{ command: 'docker compose up', count: 12 }]),
    settings: SETTINGS_OFF
  });
  await expect(page.getByTestId('history-suggestions')).toHaveCount(0);
  await expect(page.getByTestId('entry-row')).toHaveCount(1);

  const scans = await page.evaluate(
    () => (window as unknown as { __mock: { state: { historyScans: number } } }).__mock.state.historyScans
  );
  expect(scans, 'the history file is not even opened while the setting is off').toBe(0);
});

test('turning the setting off in Settings hides the panel without a reload', async ({ page }) => {
  await boot(page, { ...SEED, history: [{ command: 'docker compose up', count: 12 }] });
  await expect(page.getByTestId('history-suggestion-row')).toHaveCount(1);

  await page.getByTestId('settings').click();
  await page.getByTestId('history-suggestions-setting').uncheck();
  await page.getByTestId('save-settings').click();
  await page.getByTestId('close-settings').click();

  await expect(page.getByTestId('history-suggestions')).toHaveCount(0);
  const stored = await page.evaluate(
    () =>
      (window as unknown as {
        __mock: { state: { settings: { historySuggestionsEnabled: boolean } } };
      }).__mock.state.settings.historySuggestionsEnabled
  );
  expect(stored).toBe(false);
});

test('the panel is keyboard-only reachable: focus, Enter saves, Delete dismisses', async ({ page }) => {
  await boot(page, {
    ...SEED,
    history: [
      { command: 'docker compose up', count: 12 },
      { command: 'bun run test:watch', count: 7 }
    ]
  });

  // The command itself is the keyboard target, so Tab reaches it and Enter runs the same action
  // as its Save button.
  await page.getByTestId('history-suggestion-command').first().focus();
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('entry-editor')).toBeVisible();
  await expect(page.getByTestId('field-command')).toHaveValue('docker compose up');
  await page.getByTestId('cancel-edit').click();

  await page.getByTestId('history-suggestion-command').first().focus();
  await page.keyboard.press('Delete');
  await expect(page.getByTestId('history-suggestion-row')).toHaveCount(1);
  await expect(page.getByTestId('history-suggestion-row').first()).toContainText('bun run');
});

test('rescan picks up commands added to the history afterwards', async ({ page }) => {
  await boot(page, { ...SEED, history: [{ command: 'bun run test:watch', count: 7 }] });
  await expect(page.getByTestId('history-suggestion-row')).toHaveCount(1);

  await page.evaluate(() => {
    const mock = (window as unknown as {
      __mock: { state: { history: { command: string; count: number }[] } };
    }).__mock;
    mock.state.history = [
      { command: 'bun run test:watch', count: 7 },
      { command: 'kubectl rollout status deploy/api', count: 21 }
    ];
  });
  await page.getByTestId('history-suggestions-rescan').click();

  const rows = page.getByTestId('history-suggestion-row');
  await expect(rows).toHaveCount(2);
  await expect(rows.first()).toContainText('kubectl rollout status');
});

test('the panel folds away without touching the list underneath it', async ({ page }) => {
  await boot(page, { ...SEED, history: [{ command: 'bun run test:watch', count: 7 }] });
  const toggle = page.getByTestId('history-suggestions-toggle');
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');

  await toggle.click();
  await expect(page.getByTestId('history-suggestion-list')).toHaveCount(0);
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByTestId('entry-row')).toHaveCount(1);

  await toggle.click();
  await expect(page.getByTestId('history-suggestion-row')).toHaveCount(1);
});

test('an unreadable history is silent: no panel, no error, list intact', async ({ page }) => {
  await boot(page, SEED);
  await expect(page.getByTestId('history-suggestions')).toHaveCount(0);
  await expect(page.getByTestId('entry-row')).toHaveCount(1);
  await expect(page.getByTestId('status-line')).toHaveCount(0);
});

test('Settings brings every dismissed suggestion back, and the panel offers it again', async ({ page }) => {
  await boot(page, {
    ...SEED,
    history: [
      { command: 'docker compose up', count: 12 },
      { command: 'bun run test:watch', count: 7 }
    ]
  });
  await page.getByTestId('history-suggestion-dismiss').first().click();
  await expect(page.getByTestId('history-suggestion-row')).toHaveCount(1);

  await page.getByTestId('settings').click();
  const restore = page.getByTestId('history-suggestions-restore');
  await expect(restore).toHaveText('Bring back 1 dismissed suggestion');
  await restore.click();
  await expect(restore).toHaveCount(0);
  await page.getByTestId('close-settings').click();

  await expect(page.getByTestId('history-suggestion-row')).toHaveCount(2);
  await expect(page.getByTestId('history-suggestion-row').first()).toContainText('docker compose up');
});

test('with nothing dismissed, Settings shows no bring-back button', async ({ page }) => {
  await boot(page, { ...SEED, history: [{ command: 'docker compose up', count: 12 }] });
  await page.getByTestId('settings').click();
  await page.getByTestId('history-suggestions-setting').waitFor();
  await expect(page.getByTestId('history-suggestions-restore')).toHaveCount(0);
});

test('Backspace on a suggestion does nothing: only Delete dismisses', async ({ page }) => {
  await boot(page, { ...SEED, history: [{ command: 'docker compose up', count: 12 }] });
  await page.getByTestId('history-suggestion-command').first().focus();
  await page.keyboard.press('Backspace');
  await expect(page.getByTestId('history-suggestion-row')).toHaveCount(1);
});

test('every row keeps its name, count and buttons on one line, under the command, in the narrow list pane', async ({ page }) => {
  await page.setViewportSize({ width: 941, height: 1004 });
  await boot(page, {
    ...SEED,
    history: [
      { command: 'docker compose up -d', count: 9 },
      { command: 'cargo test --workspace', count: 6 },
      { command: 'kubectl get pods -n staging', count: 4 }
    ]
  });
  const rows = page.getByTestId('history-suggestion-row');
  await expect(rows).toHaveCount(3);
  for (let index = 0; index < 3; index += 1) {
    const row = rows.nth(index);
    const count = await row.getByTestId('history-suggestion-count').boundingBox();
    const save = await row.getByTestId('history-suggestion-save').boundingBox();
    expect(count && save, `row ${index} laid out`).toBeTruthy();
    // Same line: the Save button's vertical centre sits within the count chip's height.
    const saveMiddle = save!.y + save!.height / 2;
    expect(saveMiddle).toBeGreaterThan(count!.y - 4);
    expect(saveMiddle).toBeLessThan(count!.y + count!.height + 4);
    // And the line starts where the command does, rather than floating in the middle of the row.
    const command = await row.getByTestId('history-suggestion-command').boundingBox();
    const name = await row.locator('.name').boundingBox();
    expect(Math.abs(name!.x - command!.x), `row ${index} name aligned`).toBeLessThan(2);
  }
});
