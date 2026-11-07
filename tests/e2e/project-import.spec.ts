import { expect, test, type Page } from '@playwright/test';
import { boot, mockState } from './helpers';

/**
 * Importing the tasks a project folder already defines (`package.json` scripts, Makefile targets,
 * just recipes, Docker Compose services).
 *
 * The mock answers `scan_project` and `import_project` with a synthetic folder whose shape matches
 * the Rust DTOs field for field, so these specs exercise the review list, the selection rules and
 * the wording that follows a write. The folder is only ever described to the user: nothing here
 * executes anything it found.
 */
const PROJECT = '/home/dev/projects/webapp';

/** The row the suite seeds as already saved: `webapp·dev` in this very folder.
 *  Seeding it here rather than in every spec keeps the interesting assertions short. */
const SAVED_TASK = {
  id: 'cmd-saved',
  revision: 1,
  kind: 'command' as const,
  name: 'api·dev',
  type: 'bun',
  tags: ['project:webapp'],
  description: 'package.json script: vite',
  notes: '',
  fields: [],
  pinned: false,
  command: 'bun run dev',
  workingDirectory: PROJECT,
  usageCount: 0
};

/** The mock's narrow row type only covers what the older specs read; these cases need more. */
type CommandRow = Record<string, unknown>;

async function commandRows(page: Page): Promise<CommandRow[]> {
  const state = (await mockState(page)) as unknown as { commands: CommandRow[] };
  return state.commands;
}

/** Rows this suite wrote, i.e. everything the scanner named after the webapp. */
function imported(rows: CommandRow[]): CommandRow[] {
  return rows.filter((row) => String(row.name).startsWith('webapp·'));
}

async function openDialog(page: Page, path = PROJECT): Promise<void> {
  await page.getByTestId('import-project').click();
  await expect(page.getByRole('dialog', { name: 'Import tasks from a project' })).toBeVisible();
  if (path.length > 0) await scan(page, path);
}

async function scan(page: Page, path: string): Promise<void> {
  await page.getByTestId('project-path').fill(path);
  await page.getByTestId('project-scan').click();
  await expect(page.getByTestId('project-scan-summary')).toBeVisible();
}

const SEED = { commands: [] as unknown[] };

test('offers the action beside New and explains what a scan reads', async ({ page }) => {
  await boot(page, SEED);
  await openDialog(page, '');
  await expect(page.getByTestId('project-scan-summary')).toHaveCount(0);
  await expect(page.getByTestId('import-project')).toBeVisible();
  // Before a scan the dialog says what it reads and promises not to run any of it.
  await expect(
    page.getByText('reads the task definitions a folder already publishes')
  ).toBeVisible();
  await expect(page.getByText('Nothing inside the folder is executed')).toBeVisible();
  await expect(page.getByTestId('project-import')).toBeDisabled();
});

test('refuses an empty folder without reading anything', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('import-project').click();
  await page.getByTestId('project-scan').click();
  await expect(page.getByTestId('status-error')).toContainText('Enter a project folder first');
  await expect(page.getByTestId('project-scan-summary')).toHaveCount(0);
});

test('shows the backend reason when the path is not a folder', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('import-project').click();
  await page.getByTestId('project-path').fill('/home/dev/notes.txt');
  await page.getByTestId('project-scan').click();
  await expect(page.getByTestId('status-error')).toContainText('Not a directory');
  await expect(page.getByTestId('project-scan-summary')).toHaveCount(0);
  await expect(page.getByTestId('project-import')).toBeDisabled();
});

test('says so when a folder defines nothing it can offer', async ({ page }) => {
  await boot(page, { ...SEED, projectEmpty: true });
  await openDialog(page, '/home/dev/projects/blank');
  await expect(page.getByTestId('project-empty')).toBeVisible();
  await expect(page.getByTestId('project-import')).toBeDisabled();
});

test('groups a scanned folder and pre-selects every task it defines', async ({ page }) => {
  await boot(page, SEED);
  await openDialog(page);
  await expect(page.getByTestId('project-scan-summary')).toContainText('webapp');
  await expect(page.getByTestId('project-group')).toHaveCount(4);
  await expect(page.getByTestId('project-candidate')).toHaveCount(7);
  await expect(page.getByText('package.json scripts')).toBeVisible();
  await expect(page.getByText('Makefile targets')).toBeVisible();
  await expect(page.getByText('just recipes')).toBeVisible();
  await expect(page.getByText('Docker Compose services')).toBeVisible();
  // Every row shows the exact text it will run; nothing here is editable.
  await expect(page.getByText('bun run test', { exact: true })).toBeVisible();
  await expect(page.getByText('docker compose up -d db', { exact: true })).toBeVisible();
  await expect(page.getByTestId('project-import-count')).toHaveText('7 of 7 selected');
  await expect(page.getByTestId('project-import')).toHaveText('Import 7');
  expect(await commandRows(page)).toHaveLength(0);
});

test('marks a task the library already has and leaves it out of the count', async ({ page }) => {
  await boot(page, { commands: [SAVED_TASK] });
  await openDialog(page);
  const saved = page.getByTestId('project-already-saved');
  await expect(saved).toHaveCount(1);
  await expect(saved).toHaveText('Already saved');
  const row = page.getByTestId('project-candidate').filter({ hasText: 'bun run dev' });
  await expect(row.locator('input[type=checkbox]')).toBeChecked();
  await expect(row.locator('input[type=checkbox]')).toBeDisabled();
  // The saved row is not part of what the import can write.
  await expect(page.getByTestId('project-import-count')).toHaveText('6 of 6 selected');
  await expect(page.getByTestId('project-import')).toHaveText('Import 6');
  await expect(page.getByText(/already run in this folder, so they are left alone/)).toBeVisible();
});

test('selections drive the count: one row, a whole group, then everything', async ({ page }) => {
  await boot(page, SEED);
  await openDialog(page);
  const rows = page.getByTestId('project-candidate');

  await rows.filter({ hasText: 'webapp·fmt' }).locator('input[type=checkbox]').uncheck();
  await expect(page.getByTestId('project-import-count')).toHaveText('6 of 7 selected');
  await expect(page.getByTestId('project-import')).toHaveText('Import 6');

  // The just group holds only that recipe, so its toggle offers it back...
  const justGroup = page.getByTestId('project-group').filter({ hasText: 'just recipes' });
  await justGroup.getByTestId('project-group-all').click();
  await expect(page.getByTestId('project-import-count')).toHaveText('7 of 7 selected');
  // ...and the npm group's toggle clears its three scripts at once.
  await page
    .getByTestId('project-group')
    .filter({ hasText: 'package.json scripts' })
    .getByTestId('project-group-all')
    .click();
  await expect(page.getByTestId('project-import-count')).toHaveText('4 of 7 selected');

  await page.getByTestId('project-select-all').click();
  await expect(page.getByTestId('project-import-count')).toHaveText('7 of 7 selected');
  await expect(page.getByTestId('project-select-all')).toHaveText('Select none');
  await page.getByTestId('project-select-all').click();
  await expect(page.getByTestId('project-import-count')).toHaveText('0 of 7 selected');
  await expect(page.getByTestId('project-import')).toBeDisabled();
});

test('imports the selection as commands that run in the folder, and shows them in the library', async ({
  page
}) => {
  await boot(page, SEED);
  await openDialog(page);
  await page
    .getByTestId('project-candidate')
    .filter({ hasText: 'webapp·dev' })
    .locator('input[type=checkbox]')
    .uncheck();
  await page.getByTestId('project-import').click();

  await expect(page.getByRole('dialog', { name: 'Import tasks from a project' })).toHaveCount(0);
  await expect(page.getByText('Imported 6 tasks.')).toBeVisible();

  const written = imported(await commandRows(page));
  expect(written).toHaveLength(6);
  expect(written.map((row) => row.name).sort()).toEqual([
    'webapp·assets',
    'webapp·down',
    'webapp·fmt',
    'webapp·seed',
    'webapp·test',
    'webapp·up-db'
  ]);
  for (const row of written) {
    expect(row).toMatchObject({ workingDirectory: PROJECT, kind: 'command' });
    expect(row.tags).toEqual(['project:webapp']);
  }
  // The type is the tool that runs it, which is what the library's type filter shows.
  expect(written.find((row) => row.name === 'webapp·assets')).toMatchObject({ type: 'make' });

  // And they are in the list the library just re-read: the tag rides along, and the record shows
  // the folder it will run in.
  const rows = page.getByTestId('entry-row');
  await expect(rows).toHaveCount(6);
  await rows.filter({ hasText: 'webapp·assets' }).click();
  const details = page.getByTestId('entry-details');
  await expect(details).toContainText('project:webapp');
  await expect(details.getByTestId('command-text')).toHaveText('make assets');
  await expect(details).toContainText(PROJECT);
  expect((await commandRows(page)).every((row) => row.workingDirectory === PROJECT)).toBe(true);
});

test('a task that reached the library after the scan is reported, never written twice', async ({
  page
}) => {
  // Boot once to read the mock's own fixture, then boot again with all seven rows already in the
  // library and a scanner that has not noticed them: the write has to discover the existing rows
  // and say so instead of doubling them.
  await boot(page, SEED);
  const tasks = await page.evaluate(() => {
    const w = window as unknown as { __mockProjectTasks: Record<string, unknown>[] };
    return w.__mockProjectTasks;
  });
  await boot(page, {
    projectStale: true,
    commands: tasks.map((task, i) => ({
      id: `cmd-seed-${i}`,
      revision: 1,
      kind: 'command',
      name: task.name,
      type: task.entryType,
      tags: ['project:webapp'],
      description: task.description,
      notes: '',
      fields: [],
      pinned: false,
      command: task.command,
      workingDirectory: PROJECT,
      usageCount: 0
    }))
  });

  await openDialog(page);
  // The stale answer says nothing is saved yet, so all seven are selectable.
  await expect(page.getByTestId('project-already-saved')).toHaveCount(0);
  await expect(page.getByTestId('project-import-count')).toHaveText('7 of 7 selected');
  await page.getByTestId('project-import').click();

  // Nothing was written, and the dialog says why rather than closing as if it had worked...
  await expect(page.getByRole('dialog', { name: 'Import tasks from a project' })).toBeVisible();
  await expect(page.getByTestId('status')).toContainText('All 7 tasks are already saved here');
  // ...no row was doubled...
  expect(await commandRows(page)).toHaveLength(7);
  // ...and the re-read of the folder now recognises all seven.
  await expect(page.getByTestId('project-already-saved')).toHaveCount(7);
});

test('an untouched folder still lists its tasks as already saved on the second scan', async ({
  page
}) => {
  await boot(page, SEED);
  await openDialog(page);
  await page.getByTestId('project-import').click();
  await expect(page.getByRole('dialog', { name: 'Import tasks from a project' })).toHaveCount(0);

  await page.getByTestId('import-project').click();
  await scan(page, PROJECT);
  await expect(page.getByTestId('project-already-saved')).toHaveCount(7);
  await expect(page.getByTestId('project-import-count')).toHaveText('0 of 0 selected');
  await expect(page.getByTestId('project-import')).toBeDisabled();
  expect(await commandRows(page)).toHaveLength(7);
});

test('Enter in the folder field scans, and Escape leaves nothing behind', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('import-project').click();
  await page.getByTestId('project-path').fill(PROJECT);
  await page.getByTestId('project-path').press('Enter');
  await expect(page.getByTestId('project-scan-summary')).toBeVisible();
  expect(await commandRows(page)).toHaveLength(0);

  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'Import tasks from a project' })).toHaveCount(0);
  expect(await commandRows(page)).toHaveLength(0);
});

test('cancelling writes nothing', async ({ page }) => {
  await boot(page, SEED);
  await openDialog(page);
  await page.getByTestId('project-cancel').click();
  await expect(page.getByRole('dialog', { name: 'Import tasks from a project' })).toHaveCount(0);
  expect(await commandRows(page)).toHaveLength(0);
});

test('the action is not offered while the secrets tab is showing', async ({ page }) => {
  await boot(page, { ...SEED, secrets: [{ id: 's1', revision: 1, name: 'Token', type: 'API key', tags: [], description: '', notes: '', fields: [], pinned: false, value: 'v' }] });
  await page.getByTestId('tab-secrets').click();
  // Importing creates commands, so the action moves the user back to the tab that shows them.
  await expect(page.getByTestId('import-project')).toBeEnabled();
  await page.getByTestId('import-project').click();
  await expect(page.getByTestId('tab-commands')).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('dialog', { name: 'Import tasks from a project' })).toBeVisible();
});

test('a folder with far more tasks than fit on screen scrolls instead of growing', async ({
  page
}) => {
  await boot(page, { ...SEED, projectLarge: true });
  await openDialog(page, '/home/dev/projects/bigrepo');
  await expect(page.getByTestId('project-candidate')).toHaveCount(45);
  await expect(page.getByTestId('project-import-count')).toHaveText('45 of 45 selected');

  const scrollable = await page.getByTestId('project-groups').evaluate((list) => {
    const el = list as HTMLElement;
    return el.scrollHeight > el.clientHeight;
  });
  expect(scrollable).toBe(true);

  // The last task is below the fold, and still reachable inside the dialog rather than by
  // growing the sheet past the window.
  await page.getByTestId('project-candidate').last().scrollIntoViewIfNeeded();
  await expect(page.getByTestId('project-candidate').last()).toBeInViewport();
  await expect(page.getByTestId('project-import')).toBeInViewport();
  await page.getByTestId('project-import').click();
  await expect(page.getByText('Imported 45 tasks.')).toBeVisible();
});


test('keeps keyboard focus inside the modal and restores it on close', async ({ page }) => {
  await boot(page, SEED);
  await openDialog(page);
  const path = page.getByTestId('project-path');
  const confirm = page.getByTestId('project-import');
  await path.focus();
  await page.keyboard.press('Shift+Tab');
  await expect(confirm).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(path).toBeFocused();
  // showModal makes background controls inert even to attempted programmatic focus.
  await page.getByTestId('add-entry').evaluate((button) => (button as HTMLElement).focus());
  await expect(path).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('import-project')).toBeFocused();
});

test('reports a partially vanished selection and rescans the saved rows', async ({ page }) => {
  await boot(page, { ...SEED, projectMissingOnImport: true });
  await openDialog(page);
  await page.getByTestId('project-import').click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByTestId('status-error')).toContainText('webapp·test');
  await expect(page.getByTestId('status-error')).toContainText('no longer in this folder');
  expect(await commandRows(page)).toHaveLength(6);
  await expect(page.getByTestId('project-candidate')).toHaveCount(6);
  await expect(page.getByTestId('project-already-saved')).toHaveCount(6);
  await expect(page.getByTestId('project-import')).toBeDisabled();
});

for (const theme of ['dark', 'light', 'omarchy-catppuccin', 'omarchy-gruvbox']) {
  for (const [width, height] of [[1440, 900], [760, 520], [390, 780]]) {
    test(`long-list footer stays fully visible in ${theme} at ${width}`, async ({ page }) => {
      await page.setViewportSize({ width, height });
      await boot(page, { ...SEED, projectLarge: true, settings: {
        theme, autostart: false, clipboardExpirySeconds: 30, terminal: ['x-terminal-emulator', '-e']
      } });
      await openDialog(page);
      const dialogColour = await page.getByRole('dialog').evaluate((dialog) => ({
        actual: getComputedStyle(dialog).color,
        expected: getComputedStyle(document.body).color
      }));
      expect(dialogColour.actual).toBe(dialogColour.expected);
      const fullyVisible = await page.getByTestId('project-import').evaluate((button) => {
        const rect = button.getBoundingClientRect();
        const dialog = button.closest('dialog')!.getBoundingClientRect();
        return rect.top >= dialog.top && rect.bottom <= dialog.bottom &&
          rect.left >= dialog.left && rect.right <= dialog.right &&
          rect.top >= 0 && rect.bottom <= innerHeight;
      });
      expect(fullyVisible).toBe(true);
      await page.getByTestId('project-candidate').last().scrollIntoViewIfNeeded();
      await expect(page.getByTestId('project-candidate').last()).toBeInViewport();
      await page.getByTestId('project-import').click();
      await expect(page.getByText('Imported 45 tasks.')).toBeVisible();
    });
  }
}
