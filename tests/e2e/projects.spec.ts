import { expect, test } from '@playwright/test';
import { boot, mockState } from './helpers';

const root = '/home/dev/projects/ChatterStack';
const commands = Array.from({ length: 18 }, (_, index) => ({
  id: `task-${index}`, revision: 1, name: `ChatterStack·task-${index}`, type: 'Shell',
  tags: ['project:chatterstack'], command: `make task-${index}`, workingDirectory: index % 2 ? `${root}/api` : root,
  project: { name: 'ChatterStack', root }, pinned: false, description: '', notes: '', fields: []
}));

test('eighteen commands share one collapsed project and search expands only matches', async ({ page }) => {
  await boot(page, { commands });
  const project = page.getByTestId('command-project-toggle');
  await expect(project).toHaveCount(1);
  await expect(project).toContainText('ChatterStack');
  await expect(project).toContainText('18');
  await expect(project).toHaveAttribute('aria-expanded', 'false');
  await expect(page.getByTestId('entry-row')).toHaveCount(0);
  await page.screenshot({ path: 'artifacts/ui-review/project-groups-collapsed.png' });
  await page.setViewportSize({ width: 390, height: 780 });
  await page.getByTestId('back-to-list').click();
  await expect(project).toBeInViewport();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: 'artifacts/ui-review/project-groups-narrow.png' });
  await project.click();
  await expect(page.getByTestId('entry-row')).toHaveCount(18);
  await project.click();
  await page.getByTestId('tab-search').fill('task-17');
  await expect(page.getByTestId('entry-row')).toHaveCount(1);
  await expect(page.getByTestId('entry-row')).toContainText('task-17');
});

test('same-named projects at different roots stay separate and General stays available', async ({ page }) => {
  await boot(page, { commands: [commands[0], { ...commands[1], workingDirectory: '/work/ChatterStack', project: { name: 'ChatterStack', root: '/work/ChatterStack' } },
    { ...commands[2], name: 'global-command', workingDirectory: '', project: null, tags: [] }] });
  await expect(page.getByTestId('command-project-toggle')).toHaveCount(2);
  await expect(page.getByTestId('entry-row')).toHaveCount(1);
  await expect(page.getByTestId('entry-row')).toContainText('global-command');
});

test('a tag-only command joins a uniquely identified project without changing its data', async ({ page }) => {
  await boot(page, { commands: [commands[0], { ...commands[1], workingDirectory: '', project: null }] });
  await expect(page.getByTestId('command-project-toggle')).toHaveCount(1);
  await expect(page.getByTestId('command-project-toggle')).toContainText('2');
  await page.getByTestId('command-project-toggle').click();
  await expect(page.getByTestId('entry-row')).toHaveCount(2);
});

test('a manually saved command can be assigned with the project folder picker', async ({ page }) => {
  await boot(page, { commands: [], projectFolder: root });
  await page.getByTestId('add-entry').click();
  await page.getByTestId('field-name').fill('test-project');
  await page.getByTestId('field-command').fill('bun run test');
  await page.getByTestId('editor-project-browse').click();
  await expect(page.getByTestId('field-working-directory')).toHaveValue(root);
  await page.getByTestId('save').click();
  await expect(page.getByTestId('command-project-toggle')).toContainText('ChatterStack');
  const state = await mockState(page);
  expect((state.commands[0] as unknown as { workingDirectory: string }).workingDirectory).toBe(root);
});

test('history shows the five busiest projects and filters routine launchers', async ({ page }) => {
  const history = Array.from({ length: 6 }, (_, project) => Array.from({ length: 7 }, (_, command) => ({
    project: `project-${project}`, projectRoot: `/projects/p${project}`,
    command: `make task-${command}`, count: project + 1
  }))).flat();
  history.push({ project: 'project-5', projectRoot: '/projects/p5', command: 'code .', count: 100 });
  await boot(page, { commands: [], history });
  await expect(page.getByTestId('history-project')).toHaveCount(5);
  await page.screenshot({ path: 'artifacts/ui-review/project-history-five-projects.png' });
  await expect(page.getByTestId('history-project').first()).toContainText('project-5');
  await expect(page.getByTestId('history-project').first().getByTestId('history-suggestion-row')).toHaveCount(5);
  await expect(page.getByTestId('history-suggestion-list')).not.toContainText('code .');
  await expect(page.getByTestId('history-suggestion-list')).not.toContainText('project-0');
});

test('saving a history command retains its project and does not suppress another project', async ({ page }) => {
  await boot(page, { commands: [], history: [
    { project: 'api', projectRoot: '/projects/api', workingDirectory: '/projects/api/server', command: 'bun run dev', count: 8 },
    { project: 'web', projectRoot: '/projects/web', command: 'bun run dev', count: 3 }
  ] });
  await page.getByTestId('history-project').first().locator('summary').click();
  await page.getByTestId('history-suggestion-save').first().click();
  await expect(page.getByTestId('field-working-directory')).toHaveValue('/projects/api/server');
  await page.getByTestId('save').click();
  await expect(page.getByTestId('history-suggestion-row')).toHaveCount(1);
  await expect(page.getByTestId('history-project')).toContainText('web');
});
