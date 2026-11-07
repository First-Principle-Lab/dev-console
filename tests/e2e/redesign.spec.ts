import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';

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

test('secret rows show only the name and actions without exposing the value', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('tab-secrets').click();
  const row = page.getByTestId('entry-row').first();
  await expect(row).toContainText('Registry token');
  await expect(row).toHaveText('Registry token');
  await expect(row.getByRole('button', { name: 'Copy Registry token' })).toBeVisible();
  const body = await page.content();
  expect(body).not.toContain('synthetic-value-1');
});

test('the editor reports unsaved changes and clears them on cancel', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('add-entry').click();
  await expect(page.getByTestId('dirty-indicator')).toHaveCount(0);
  await page.getByTestId('field-name').fill('half-typed');
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

test('minimal list rows show names and actions without overflowing', async ({ page }) => {
  await boot(page, SEED);
  const row = page.getByTestId('entry-row').first();
  await expect(row.getByTestId('select-entry')).toHaveText('Deploy staging');
  await expect(row.getByRole('button', { name: 'Copy Deploy staging' })).toBeVisible();
  await expect(row.getByRole('button', { name: 'Delete Deploy staging' })).toBeVisible();
  expect(await row.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
});

test('a fresh editor asks only for name and command before saving', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('add-entry').click();
  await expect(page.getByTestId('validation-hint')).toHaveCount(0);
  await expect(page.getByTestId('save-error')).toHaveCount(0);
  await expect(page.getByTestId('entry-editor').locator('input, textarea')).toHaveCount(3);
  await page.getByTestId('field-name').fill('Deploy');
  await page.getByTestId('save').click();
  await expect(page.getByTestId('save-error')).toContainText('Command text is required.');
  const after = await mockState(page);
  expect(after.commands).toHaveLength(2);
  // …and the hint follows further typing once the user has tried to save.
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


/** Relative luminance + contrast ratio, computed from what the browser really painted. */
function parseRgb(value: string): [number, number, number] {
  const match = value.match(/rgba?\(([^)]+)\)/);
  if (!match) throw new Error(`not an rgb colour: ${value}`);
  const [r, g, b] = match[1].split(/[,\s/]+/).filter(Boolean).map(Number);
  return [r, g, b];
}

function luminance(rgb: [number, number, number]): number {
  const channel = (raw: number) => {
    const value = raw / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2]);
}

function contrast(a: string, b: string): number {
  const first = luminance(parseRgb(a));
  const second = luminance(parseRgb(b));
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

function paletteColours(page: Page): Promise<{ shell: string; footer: string; selected: string; radius: string }> {
  return page.evaluate(() => {
    const style = (selector: string, property: keyof CSSStyleDeclaration) => {
      const el = document.querySelector(selector);
      return el ? String(getComputedStyle(el)[property]) : '';
    };
    return {
      shell: style('.palette', 'backgroundColor'),
      footer: style('footer', 'color'),
      selected: style('.row.active', 'backgroundColor'),
      radius: style('.palette', 'borderRadius')
    };
  });
}

test('both themes apply through the same tokens and stay above 4.5:1 in the live DOM', async ({ page }) => {
  await boot(page, SEED, 'finder');
  const dark = await paletteColours(page);
  expect(dark.shell, 'the mock starts in the dark theme').not.toBe('');
  expect(contrast(dark.footer, dark.shell), 'dark footer text').toBeGreaterThanOrEqual(4.5);

  // The selected row eases between themes; without transitions each switch lands at once.
  await page.addStyleTag({ content: '*, *::before, *::after { transition: none !important; }' });
  await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'light'));
  const explicit = await paletteColours(page);
  expect(explicit.radius, 'shape tokens are shared by both themes').toBe(dark.radius);
  expect(luminance(parseRgb(explicit.shell)), 'light must actually apply to the shell').toBeGreaterThan(
    luminance(parseRgb(dark.shell))
  );
  expect(contrast(explicit.footer, explicit.shell), 'light footer text').toBeGreaterThanOrEqual(4.5);

  // "Follow the system" has to land on the same values as the explicit setting.
  await page.evaluate(() => document.documentElement.removeAttribute('data-theme'));
  await page.emulateMedia({ colorScheme: 'light' });
  const system = await paletteColours(page);
  expect(system.shell).toBe(explicit.shell);
  expect(system.footer).toBe(explicit.footer);
  expect(system.selected).toBe(explicit.selected);
});

test('reduced motion collapses the transitions and the shipped shell stays opaque', async ({ page }) => {
  await boot(page, SEED, 'finder');
  const before = await page.getByTestId('result-row').first().evaluate((el) => getComputedStyle(el).transitionDuration);
  expect(Number.parseFloat(before)).toBeGreaterThan(0);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  const reduced = await page
    .getByTestId('result-row')
    .first()
    .evaluate((el) => getComputedStyle(el).transitionDuration);
  expect(Number.parseFloat(reduced), 'reduced motion must collapse the transition').toBeLessThan(0.001);

  // Material ships off: the blur token is 0px, the tint is transparent, and nothing paints a
  // backdrop blur. This is the honest claim on this host - compositor blur is UNVERIFIED, so
  // the app must not depend on it (docs/design/raycast-ui-redesign.md, "Native material outcome").
  const shipped = await page.evaluate(() => {
    const root = getComputedStyle(document.documentElement);
    const palette = getComputedStyle(document.querySelector('.palette') as Element);
    return {
      blur: root.getPropertyValue('--material-blur').trim(),
      tint: root.getPropertyValue('--material-tint').trim(),
      shellBlur: palette.backdropFilter,
      shellFill: palette.backgroundColor,
      rowBlur: getComputedStyle(document.querySelector('.row') as Element).backdropFilter
    };
  });
  expect(shipped.blur, 'the shipped default paints no blur').toBe('0px');
  expect(shipped.tint).toBe('transparent');
  expect(shipped.shellBlur).toBe('none');
  expect(shipped.rowBlur, 'rows are never blurred').toBe('none');
  expect(shipped.shellFill, 'the shell is a solid fill, not a translucent tint').not.toBe('rgba(0, 0, 0, 0)');

  // Flipping the unverified native prototype switch must not change a single painted pixel of
  // the CSS shell: no rule reads --material-blur, so the opaque fallback cannot be lost.
  await page.evaluate(() => document.documentElement.setAttribute('data-transparency', 'on'));
  const flipped = await page.evaluate(() => {
    const root = getComputedStyle(document.documentElement);
    const palette = getComputedStyle(document.querySelector('.palette') as Element);
    return {
      blur: root.getPropertyValue('--material-blur').trim(),
      shellBlur: palette.backdropFilter,
      shellFill: palette.backgroundColor
    };
  });
  expect(flipped.blur, 'the switch does exist for the native prototype').toBe('24px');
  expect(flipped.shellBlur, 'but the CSS layer still requests no blur').toBe('none');
  expect(flipped.shellFill, 'and the shell keeps its opaque fill').toBe(shipped.shellFill);
  await page.evaluate(() => document.documentElement.removeAttribute('data-transparency'));
});

test('keyboard focus draws the ring from --focus-ring', async ({ page }) => {
  await boot(page, SEED);
  const ring = await page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue('--focus-ring').trim()
  );
  expect(ring).not.toBe('');
  await page.locator('.tabbar').focus();
  await page.keyboard.press('Tab');
  const focused = await page.evaluate(() => {
    const el = document.activeElement as HTMLElement | null;
    if (!el) return null;
    const cs = getComputedStyle(el);
    return { tag: el.tagName, width: cs.outlineWidth, style: cs.outlineStyle, color: cs.outlineColor };
  });
  expect(focused, 'Tab must land on a control').not.toBeNull();
  expect(focused!.style, 'focus indicator must be an outline, not a hue change').toBe('solid');
  expect(Number.parseFloat(focused!.width), 'focus ring must be at least 2px').toBeGreaterThanOrEqual(2);
  expect(focused!.color).not.toBe('rgba(0, 0, 0, 0)');
});

/* ------------------------------------------------------------------ library pass */

test('the inspector keeps every metadata value but only one chip', async ({ page }) => {
  await boot(page, SEED);
  await page.getByTestId('entry-row').first().click();
  const details = page.getByTestId('entry-details');
  // Nothing was lost on the way to "fewer chips": every value that used to carry its own
  // bordered pill is still on screen, now as one quiet context line in the palette idiom.
  await expect(details).toContainText('Shell');
  await expect(details).toContainText('#work');
  await expect(details).toContainText('Pinned');
  await expect(details).toContainText('r2');
  await expect(details).toContainText('updated');
  await expect(details.locator('.ctx-part')).toHaveCount(5);
  // and only the kind is allowed to be a chip, so the name stays the loudest thing here.
  await expect(details.locator('.chip')).toHaveCount(1);
  await expect(details.locator('.chip')).toHaveText('Command');

  // the same promise on the secret screen, where the mask must not have moved either
  await page.getByTestId('tab-secrets').click();
  await page.getByTestId('entry-row').first().click();
  await expect(page.getByTestId('entry-details').locator('.chip')).toHaveText('Secret');
  await expect(page.getByTestId('secret-value')).toHaveText('••••••••••');
  await expect(page.getByTestId('reveal')).toBeVisible();
  expect(await page.content(), 'a masked secret must not reach the DOM').not.toContain('synthetic-value-1');
});

test('the action area stays put at the minimum window and keyboard focus lands on it', async ({ page }) => {
  // Content long enough that the inspector really scrolls, so "stable" can fail.
  const long = {
    commands: [
      {
        id: 'c9', revision: 1, name: 'Long record', type: 'Shell', tags: ['work'],
        description: 'Ships the thing. '.repeat(14), notes: 'Note line.\n'.repeat(26),
        fields: [], pinned: false, command: 'echo long', workingDirectory: '', usageCount: 1
      }
    ],
    secrets: []
  };
  await page.setViewportSize({ width: 760, height: 520 });
  await boot(page, long);
  await page.getByTestId('select-entry').first().focus();
  await page.keyboard.press('Enter');
  const details = page.getByTestId('entry-details');
  await expect(details).toBeVisible();
  // one-pane navigation is retained at this width: the list gave way to the record
  await expect(page.getByTestId('entry-row')).toHaveCount(0);
  expect(
    await details.evaluate((el) => el.scrollHeight > el.clientHeight + 40),
    'the record has to actually scroll for this test to mean anything'
  ).toBe(true);

  let reached = false;
  for (let i = 0; i < 12 && !reached; i += 1) {
    await page.keyboard.press('Tab');
    reached = await page.evaluate(
      () => (document.activeElement as HTMLElement | null)?.getAttribute('data-testid') === 'copy'
    );
  }
  expect(reached, 'Tab must reach the primary action at the minimum window size').toBe(true);
  const box = await page.getByTestId('copy').boundingBox();
  expect(box, 'the focused action has to be on screen').not.toBeNull();
  expect(box!.y + box!.height, 'the action area must sit inside the 760x520 window').toBeLessThanOrEqual(521);

  // stable: scrolling the record to its top must not move the action area out of view
  await details.evaluate((el) => el.scrollTo(0, 0));
  await expect(page.getByTestId('copy')).toBeInViewport();
  await expect(page.getByTestId('delete')).toBeInViewport();
});

test('settings, the delete dialog and the palette draw one shared shell frame', async ({ page }) => {
  const frameOf = (selector: string): Promise<{ radius: string; shadow: string; border: string }> =>
    page.evaluate((sel) => {
      const el = document.querySelector(sel);
      if (!el) throw new Error(`missing ${sel}`);
      const cs = getComputedStyle(el);
      return {
        radius: cs.borderTopLeftRadius,
        shadow: cs.boxShadow,
        border: `${cs.borderTopWidth} ${cs.borderTopStyle}`
      };
    }, selector);

  await boot(page, SEED);
  const shell = await page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue('--radius-shell').trim()
  );
  expect(shell, 'the shell arc comes from a token').not.toBe('');

  await page.getByTestId('settings').click();
  await page.getByTestId('settings-panel').waitFor();
  const settings = await frameOf('[data-testid="settings-panel"]');
  expect(settings.radius, 'settings uses the shared shell arc').toBe(shell);
  expect(settings.shadow, 'settings uses the shared shell elevation').not.toBe('none');
  await page.getByTestId('close-settings').click();

  await page.getByTestId('entry-row').first().click();
  await page.getByTestId('delete').click();
  const dialog = await frameOf('dialog.surface-shell');
  expect(dialog.radius, 'a confirmation is the same object as settings').toBe(settings.radius);
  expect(dialog.border, 'one hairline edge, not a stack of frames').toBe('1px solid');
  expect(dialog.shadow).not.toBe('none');
  await page.getByTestId('cancel-delete').click();

  await boot(page, SEED, 'finder');
  const palette = await frameOf('.palette');
  expect(palette.radius, 'the palette shares that frame too').toBe(settings.radius);
});

test('the empty library and populated inspector use the same icon style and chips', async ({ page }) => {
  await boot(page, { commands: [], secrets: [] });
  const empty = await page
    .getByTestId('empty-state')
    .first()
    .locator('.glyph')
    .evaluate((el) => {
      const cs = getComputedStyle(el);
      return { fill: cs.backgroundColor, colour: cs.color };
    });
  const hint = await page.getByTestId('shortcut-hint').evaluate((el) => {
    const cs = getComputedStyle(el);
    return {
      radius: cs.borderTopLeftRadius,
      fill: cs.backgroundColor,
      keycap: getComputedStyle(el.querySelector('kbd') as Element).backgroundColor
    };
  });
  expect(hint.radius, 'chips are the one shared pill').toBe('999px');
  expect(hint.keycap, 'a keycap must not disappear into the chip that carries it').not.toBe(hint.fill);

  await boot(page, SEED);
  const tile = await page
    .getByTestId('entry-details')
    .locator('.glyph')
    .evaluate((el) => {
      const cs = getComputedStyle(el);
      return { fill: cs.backgroundColor, colour: cs.color };
    });
  expect(empty.fill, 'the empty state and inspector use the same tile fill').toBe(tile.fill);
  expect(empty.colour).not.toBe('rgba(0, 0, 0, 0)');
  expect(tile.colour).not.toBe('rgba(0, 0, 0, 0)');
});
