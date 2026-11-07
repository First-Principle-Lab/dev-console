import { expect, test, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';

import { boot } from './helpers';

/**
 * Visual review captures. These are UI-shell checks: the browser preview cannot reach native
 * IPC, so every page here runs against the documented IPC test double (`ipc-mock.js`) and the
 * native binary is exercised separately by `scripts/smoke-native.sh`.
 *
 *   SHOT_SET=before           # historical pre-token set, frozen: artifacts/ui-review/before/
 *   SHOT_SET=after            # previous review set, frozen: artifacts/ui-review/after/
 *   SHOT_SET=native           # written by the native smoke session, not by this file
 *   SHOT_SET=raycast-baseline # untouched UI before the Raycast-inspired pass (docs/design/raycast-ui-research-2026-11-02.md)
 *   SHOT_SET=raycast-after    # the redesigned UI
 *
 * `before`, `after` and `native` stay on disk untouched: they are the review record for earlier
 * work, and the redesign compares against `raycast-baseline` instead of rewriting them.
 *
 * Secrets use obviously synthetic values and are captured masked unless the state under
 * review is explicitly a reveal state (there is none). `capture()` proves that for every
 * capture instead of relying on the reviewer to look: a synthetic value reaching the serialized
 * DOM fails the run. `SHOT_HTML=1` additionally writes the page source beside the PNGs under
 * `test-results/screenshots/<set>/` (gitignored) for grepping a whole set at once.
 */

// `before`, `after` and `native` are frozen review records for earlier work, so the default is no
// longer `after`: a plain `bunx playwright test` (which is what `scripts/verify.sh` runs) would
// otherwise overwrite 41 tracked PNGs with whatever this machine's font rendering produces.
// docs/design/raycast-ui-research-2026-11-02.md owns the capture protocol.
const SET = process.env.SHOT_SET ?? 'raycast-after';
const OUT_DIR = path.join(import.meta.dirname, '..', '..', 'artifacts', 'ui-review', SET);
const HTML_DIR = path.join(import.meta.dirname, '..', '..', 'test-results', 'screenshots', SET);

/** Distinctive prefixes of every synthetic secret value seeded below. */
const SECRET_MARKERS = ['synthetic-masked-value', 'synthetic-db-password', 'synthetic-long-secret'];

const ago = (hours: number): string => new Date(Date.now() - hours * 3600_000).toISOString();

const SEED = {
  commands: [
    {
      id: 'c1', revision: 4, name: 'Deploy staging', type: 'Shell', tags: ['work', 'ci'],
      description: 'Builds and ships the staging image', notes: 'Needs `make db-migrate` first.',
      fields: [
        { id: 'f1', label: 'Project', value: 'local-api', display: 'text' },
        { id: 'f2', label: 'URL', value: 'https://staging.example.test', display: 'link' }
      ],
      pinned: true, command: 'docker compose up --build api worker', workingDirectory: '~/src/api', usageCount: 27, updatedAt: ago(2)
    },
    {
      id: 'c2', revision: 2, name: 'Rebase main', type: 'Git', tags: ['git'],
      description: '', notes: '', fields: [], pinned: false,
      command: 'git fetch origin && git rebase origin/main', workingDirectory: '', usageCount: 3, updatedAt: ago(26)
    },
    {
      id: 'c3', revision: 1, name: 'Tunnel to staging db', type: 'Shell', tags: ['db', 'work'],
      description: 'Opens an SSH tunnel to the staging Postgres', notes: '',
      fields: [{ id: 'f3', label: 'Environment', value: 'staging', display: 'text' }],
      pinned: false, command: 'ssh -N -L 5433:db.staging:5432 bastion', workingDirectory: '', usageCount: 11, updatedAt: ago(96)
    }
  ],
  secrets: [
    {
      id: 's1', revision: 3, name: 'Registry token', type: 'API key', tags: ['work', 'infra'],
      description: 'Used by the publish job', notes: 'Rotated quarterly.',
      fields: [{ id: 'f1', label: 'Account', value: 'svc-build', display: 'text' }],
      pinned: false, value: 'synthetic-masked-value', updatedAt: ago(5)
    },
    {
      id: 's2', revision: 1, name: 'Staging Postgres', type: 'Database', tags: ['db'],
      description: '', notes: '', fields: [], pinned: true, value: 'synthetic-db-password',
      updatedAt: ago(1720)
    }
  ]
};

test('finder at the real palette size (680x420)', async ({ page }) => {
  // The Finder ships as a 680x420 borderless window; reviewing it inside a 1440px page shows the
  // shell but not how the palette actually sits at the size it is created with.
  await boot(page, SEED, 'finder');
  await capture(page, '13-finder-palette-680x420', 680, 420);
});

// The chip is muted text on a hairline border, which is exactly what a theme can wash out, so it is
// reviewed in a dark, a light and two Omarchy palettes. Dark keeps the original file name.
const CONTEXT_THEMES = [
  ['dark', ''],
  ['light', '-light'],
  ['omarchy-tokyo-night', '-tokyo-night'],
  ['omarchy-catppuccin-latte', '-catppuccin-latte']
] as const;

for (const [theme, suffix] of CONTEXT_THEMES) {
  test(`finder with a detected project, at the real palette size (${theme})`, async ({ page }) => {
    // The chip and the row markers share the search line and the row's action area with the vault
    // chip and the Enter keycap, so this is the state where they would collide.
    const seed = {
      ...SEED,
      context: { root: '/home/mock/src/api', name: 'api' },
      settings: {
        theme, autostart: false, clipboardExpirySeconds: 30,
        terminal: ['x-terminal-emulator', '-e'], projectContextEnabled: true
      }
    };
    await boot(page, seed, 'finder');
    // A review PNG of the palette without its chip would be worthless, so the state is asserted
    // rather than trusted: the chip names the project, and the one command saved inside it is marked.
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    await expect(page.getByTestId('finder-context')).toHaveText('api');
    await expect(page.getByTestId('row-in-project')).toHaveCount(1);
    await expect(page.getByRole('img', { name: 'in api' })).toHaveCount(1);
    await capture(page, `18-finder-project-context${suffix}`, 680, 420);
    await expect(page.getByTestId('finder-context'), 'after capture').toHaveText('api');
  });
}

test('library with a long name and long values (overflow check)', async ({ page }) => {
  await boot(page, LONG);
  await page.getByTestId('entry-row').first().click();
  await capture(page, '14-library-long-content', 1440, 900);
});

test('settings at the real library size and at the minimum window size', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('settings').click();
  await page.getByTestId('settings-panel').waitFor();
  await capture(page, '15-settings', 1040, 700);
  await boot(page, SEED);
  await page.getByTestId('settings').click();
  await page.getByTestId('settings-panel').waitFor();
  await capture(page, '15-settings-minimum', 760, 520);
});

test('settings import review card at the real library size', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('settings').click();
  await page.getByTestId('import-path').fill('/tmp/portable-review.age');
  await page.getByTestId('import-archive').click();
  await page.getByTestId('review-card').waitFor();
  await capture(page, '16-settings-import-review', 1040, 700);
});

test('library record at the real minimum window size', async ({ page }) => {
  // 760 x 520 is the smallest window the shell is created with, and under 900px the library
  // switches to one pane: this is the state where an action area that scrolled off the bottom
  // would be unreachable.
  await page.setViewportSize({ width: 760, height: 520 });
  await boot(page, LONG);
  await page.getByTestId('entry-row').first().click();
  await page.getByTestId('entry-details').waitFor();
  await capture(page, '17-library-minimum-window', 760, 520);
});

/** Opens the project-task dialog over a scanned folder; the dialog is the surface under review. */
async function importDialog(page: Page, large = false, theme = 'dark'): Promise<void> {
  await boot(page, { ...EMPTY, projectLarge: large, settings: {
    theme, autostart: false, clipboardExpirySeconds: 30, terminal: ['x-terminal-emulator', '-e']
  } });
  await page.getByTestId('import-project').click();
  await page.getByTestId('project-path').fill(
    large ? '/home/dev/projects/bigrepo' : '/home/dev/projects/webapp'
  );
  await page.getByTestId('project-scan').click();
  await page.getByTestId('project-scan-summary').waitFor();
}

test('project task review at the real library size and at the minimum window size', async ({
  page
}) => {
  // The review list scrolls inside the dialog, so the Import action must stay reachable at both
  // the size the shell is usually created at and the smallest one it accepts.
  await importDialog(page);
  await capture(page, '18-project-import', 1040, 700);
  await importDialog(page);
  await capture(page, '18-project-import-minimum', 760, 520);
  await importDialog(page);
  await capture(page, '18-project-import-mobile', 390, 780);
});

test('project task review with far more tasks than fit on screen', async ({ page }) => {
  await importDialog(page, true);
  await capture(page, '19-project-import-long-list', 1440, 900);
  await importDialog(page, true);
  await capture(page, '19-project-import-long-list-minimum', 760, 520);
});

const EMPTY = { commands: [], secrets: [] };

/** Deliberately awkward content: long names, tags, values and paths must not break the layout. */
const LONG = {
  commands: [
    {
      id: 'L1', revision: 9,
      name: 'Rebuild and republish the staging image for the payments service with verbose logging',
      type: 'Docker / compose with a very long custom type name',
      tags: ['work', 'ci', 'payments', 'europe-west1', 'needs-review'],
      description: 'Recreates the compose project, runs migrations and smoke-tests the health endpoint before switching the traffic over.',
      notes: 'Only from the VPN.\nThe bastion key must already be loaded.\n\nssh-add ~/.ssh/id_ed25519_work',
      fields: [
        { id: 'f1', label: 'URL', value: 'https://staging.payments.example.test/very/long/path/that/keeps/going?query=aaaaaaaaaaaaaaaaaaaaaaaa', display: 'link' },
        { id: 'f2', label: 'Environment', value: 'europe-west1/staging/payments', display: 'text' }
      ],
      pinned: true,
      command: 'DOCKER_BUILDKIT=1 docker compose -f compose.yml -f compose.staging.yml build --pull payments && docker compose up -d --force-recreate payments',
      workingDirectory: '/home/user/src/very/long/project/path/services/payments-api',
      usageCount: 1284, updatedAt: ago(2)
    }
  ],
  secrets: [
    {
      id: 'L2', revision: 2,
      name: 'Very long named production signing key for the payments service',
      type: 'PEM private key', tags: ['prod', 'payments'],
      description: '', notes: '', fields: [], pinned: false,
      value: 'synthetic-long-secret-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      updatedAt: ago(7200)
    }
  ]
};

const VIEWPORTS = [
  { name: 'desktop-1440', width: 1440, height: 900 },
  { name: 'tablet-768', width: 768, height: 900 },
  { name: 'mobile-390', width: 390, height: 780 }
];

async function libraryShell(page: Page, seed: typeof SEED | typeof EMPTY): Promise<void> {
  await boot(page, seed);
  await page.getByTestId('entry-row').first().waitFor({ timeout: 5000 }).catch(() => undefined);
}

async function capture(page: Page, name: string, width: number, height: number): Promise<void> {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  await page.setViewportSize({ width, height });
  await page.waitForTimeout(150);
  const source = await page.content();
  for (const marker of SECRET_MARKERS) {
    expect(source, `${name}: synthetic secret "${marker}" reached the captured DOM`).not.toContain(marker);
  }
  if (process.env.SHOT_HTML === '1') {
    fs.mkdirSync(HTML_DIR, { recursive: true });
    fs.writeFileSync(path.join(HTML_DIR, `${name}__${width}.html`), source);
  }
  await page.screenshot({ path: path.join(OUT_DIR, `${name}__${width}.png`) });
}

for (const viewport of VIEWPORTS) {
  test(`library review set @ ${viewport.name}`, async ({ page }) => {
    const { width, height } = viewport;

    await libraryShell(page, SEED);
    await page.getByTestId('entry-row').first().click();
    await capture(page, '01-library-commands', width, height);

    await page.getByTestId('tab-secrets').click();
    await page.getByTestId('entry-row').first().click();
    await capture(page, '02-library-secrets-masked', width, height);

    await libraryShell(page, EMPTY);
    await capture(page, '03-library-empty', width, height);

    await libraryShell(page, SEED);
    await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
    await page.getByTestId('tab-secrets').click();
    await capture(page, '04-vault-locked', width, height);

    await libraryShell(page, SEED);
    await page.getByTestId('add-entry').click();
    await page.getByTestId('field-name').fill('nightly-job');
    await page.getByTestId('field-command').fill('bun run verify');
    await capture(page, '05-editor-command-dirty', width, height);

    await libraryShell(page, SEED);
    await page.getByTestId('tab-secrets').click();
    await page.getByTestId('entry-row').first().click();
    await page.getByTestId('edit').click();
    await capture(page, '06-editor-secret-masked', width, height);

    await libraryShell(page, SEED);
    await page.getByTestId('entry-row').first().click();
    await page.getByTestId('delete').click();
    await capture(page, '07-delete-confirmation', width, height);
  });

  test(`finder review set @ ${viewport.name}`, async ({ page }) => {
    const { width, height } = viewport;

    await boot(page, SEED, 'finder');
    await capture(page, '08-finder-results', width, height);

    await boot(page, SEED, 'finder');
    await page.getByTestId('finder-input').fill('Reg');
    await page.waitForTimeout(200);
    await capture(page, '09-finder-inline-completion', width, height);

    await boot(page, SEED, 'finder');
    await page.getByTestId('finder-input').fill('nothing matches this');
    await page.waitForTimeout(200);
    await capture(page, '10-finder-no-matches', width, height);

    await boot(page, SEED, 'finder');
    await page.getByTestId('finder-input').fill('Deploy');
    await page.keyboard.press('Enter');
    await page.waitForTimeout(200);
    await capture(page, '11-finder-copied', width, height);

    await boot(page, SEED, 'finder');
    await page.evaluate(() => (window as unknown as { __mock: { lockVault: () => void } }).__mock.lockVault());
    await page.getByTestId('finder-input').fill('token');
    await page.waitForTimeout(200);
    await capture(page, '12-finder-locked-row', width, height);
  });
}

/**
 * The shell-history suggestion panel, reviewed at the widest and the narrowest size the library is
 * used at: a row is a three-column grid, and 390px is where it has to collapse.
 */
const HISTORY_SEED = {
  ...SEED,
  settings: {
    theme: 'dark',
    autostart: false,
    clipboardExpirySeconds: 30,
    terminal: ['x-terminal-emulator', '-e'],
    historySuggestionsEnabled: true
  },
  history: [
    { command: 'docker compose -f compose.yml up --build api worker', count: 14 },
    { command: 'bun run test:watch', count: 9 },
    { command: 'kubectl -n staging logs -f api-6d9f7b', count: 4 }
  ]
};

if (SET === 'raycast-after') {
  test('shell history suggestions @ desktop-1440 and mobile-390', async ({ page }) => {
    // 1440 is the width the panel was designed against and 390 is where it has to give something
    // up; both are reviewed in each palette.
    await boot(page, HISTORY_SEED);
    await expect(page.getByTestId('history-suggestion-row')).toHaveCount(3);
    await capture(page, '18-history-suggestions', 1440, 900);

    await boot(page, {
      ...HISTORY_SEED,
      settings: { ...HISTORY_SEED.settings, theme: 'light' }
    });
    await expect(page.getByTestId('history-suggestion-row')).toHaveCount(3);
    await capture(page, '18-history-suggestions-light', 1440, 900);

    // Narrow: under 900px the library shows one pane, and a row selected at the wider size would
    // leave the list - and the panel above it - off screen.
    await page.setViewportSize({ width: 390, height: 780 });
    await boot(page, HISTORY_SEED);
    await expect(page.getByTestId('history-suggestion-row')).toHaveCount(3);
    await capture(page, '18-history-suggestions', 390, 780);

    await page.getByTestId('history-suggestions-toggle').click();
    await expect(page.getByTestId('history-suggestion-list')).toHaveCount(0);
    await capture(page, '19-history-suggestions-collapsed', 390, 780);
  });
}

for (const theme of ['dark', 'light', 'omarchy-catppuccin', 'omarchy-gruvbox']) {
  test(`project import review captures in ${theme}`, async ({ page }) => {
    for (const [width, height] of [[1440, 900], [390, 780]]) {
      await importDialog(page, false, theme);
      await capture(page, `20-project-import-${theme}`, width, height);
      await importDialog(page, true, theme);
      await capture(page, `21-project-import-long-${theme}`, width, height);
    }
  });
}
