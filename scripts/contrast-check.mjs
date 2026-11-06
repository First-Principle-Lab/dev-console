#!/usr/bin/env node
/*
 * Contrast audit for the design tokens in src/styles/tokens.css.
 *
 * It reads the palettes out of the stylesheet itself (it does not carry a copy of
 * any colour value), composites every translucent layer over the surface it is
 * actually painted on, and checks the WCAG 2.1 relative-luminance ratio:
 *
 *   text       >= 4.5:1   (normal text; nothing in this UI is large text -
 *                          the largest token is --text-xl at 18px/400, and
 *                          WCAG only relaxes to 3:1 from 24px or 18.66px bold)
 *   boundary   >= 3.0:1   (non-text: focus ring, caret, selection marker, kind
 *                          and status tints that carry meaning - WCAG 1.4.11)
 *
 * Each pair in CHECKS is a real pairing taken from a rule under src/, and the
 * `where` note names it, so the table is a review of the app and not a survey of
 * every possible combination. Every colour token declared in the palettes must
 * appear in CHECKS or in EXEMPT (with a reason); the completeness sweep fails the
 * run if a new token escapes the audit, and EXEMPT rows are still printed with
 * their measured ratio so nothing is quietly hidden.
 *
 * Deliberate exemptions (WCAG 1.4.11 does not require these to reach 3:1):
 * separators between regions, the soft edge of a selection band whose state is
 * already carried by the solid inset marker, decorative elevation shadows, and
 * disabled components. See docs/raycast-ui-research-2026-11-02.md.
 *
 * Usage:
 *   bun scripts/contrast-check.mjs            full table
 *   bun scripts/contrast-check.mjs --summary  one line per group (test output)
 * Exit code 0 = every checked pair clears its threshold in both palettes.
 */
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const TOKENS_FILE = join(ROOT, 'src', 'styles', 'tokens.css');
const css = readFileSync(TOKENS_FILE, 'utf8');
const omarchyCss = readFileSync(join(ROOT, 'src', 'styles', 'omarchy.css'), 'utf8');

const BLOCKS = {
  dark: [/:root,\s*:root\[data-theme='dark'\]\s*\{([\s\S]*?)\n\}/, 'dark'],
  light: [/:root\[data-theme='light'\]\s*\{([\s\S]*?)\n\}/, 'light ([data-theme=light])'],
  systemLight: [
    /@media \(prefers-color-scheme: light\)\s*\{\s*:root\s*\{([\s\S]*?)\n {2}\}/,
    'light (prefers-color-scheme)',
  ],
};

function parseBlock(source) {
  const values = {};
  for (const declaration of source.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    values[declaration[1]] = declaration[2].trim();
  }
  return values;
}

const palettes = {};
/* the shared :root block carries the sizing tokens (radii, --edge-w) that shorthands
   such as --selected-marker are written from; palettes override it. */
const baseBlock = css.match(/^:root\s*\{([\s\S]*?)\n\}/m);
if (!baseBlock) throw new Error("tokens.css: no base ':root' block");
const base = parseBlock(baseBlock[1]);
for (const [key, [pattern, label]] of Object.entries(BLOCKS)) {
  const match = css.match(pattern);
  if (!match) throw new Error(`tokens.css: no block matching the ${label} palette`);
  palettes[key] = { ...base, ...parseBlock(match[1]) };
}
for (const match of omarchyCss.matchAll(/:root\[data-theme='(omarchy-[\w-]+)'\]\s*\{([\s\S]*?)\n\}/g)) {
  palettes[match[1]] = { ...base, ...parseBlock(match[2]) };
}

/** Resolve var() references (with fallbacks) down to a plain CSS value. */
function resolve(palette, value, depth = 0) {
  if (depth > 24) throw new Error(`var() cycle at: ${value}`);
  return value.replace(/var\((--[\w-]+)(?:\s*,\s*([^)]*))?\)/g, (_all, name, fallback) => {
    if (palette[name] !== undefined) return resolve(palette, palette[name], depth + 1);
    if (fallback !== undefined) return resolve(palette, fallback, depth + 1);
    throw new Error(`undeclared token var(${name}) in: ${value}`);
  }).trim();
}

const HEX = /^#([0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;
const FUNC = /^rgba?\(([^)]*)\)$/i;

/** Parse one flat colour, or null when the value is not a single colour. */
function colour(value) {
  const text = value.trim();
  if (text.length === 0) return null;
  if (/^transparent$/i.test(text)) return [0, 0, 0, 0];
  const hex = text.match(HEX);
  if (hex) {
    let digits = hex[1];
    if (digits.length === 3 || digits.length === 4) {
      digits = [...digits].map((d) => d + d).join('');
    }
    if (digits.length === 8) {
      digits = digits.slice(0, 6) + digits.slice(6);
      return [
        parseInt(digits.slice(0, 2), 16),
        parseInt(digits.slice(2, 4), 16),
        parseInt(digits.slice(4, 6), 16),
        parseInt(digits.slice(6, 8), 16) / 255,
      ];
    }
    return [
      parseInt(digits.slice(0, 2), 16),
      parseInt(digits.slice(2, 4), 16),
      parseInt(digits.slice(4, 6), 16),
      1,
    ];
  }
  const func = text.match(FUNC);
  if (func) {
    const parts = func[1].split(/[,\s/]+/).filter((p) => p.length > 0);
    if (parts.length < 3 || parts.length > 4) return null;
    const channel = (raw, index) =>
      raw.endsWith('%') ? (parseFloat(raw) / 100) * (index === 3 ? 1 : 255) : parseFloat(raw);
    const alpha = parts.length === 4 ? channel(parts[3], 3) : 1;
    if (!Number.isFinite(alpha)) return null;
    return [channel(parts[0]), channel(parts[1]), channel(parts[2]), alpha];
  }
  return null;
}

/**
 * A token can hold a flat colour (--danger) or a shorthand that contains one
 * (--selected-marker is `inset 2px 0 0 var(--accent)`); both resolve to the
 * colour the browser finally paints.
 */
function resolveColour(palette, reference) {
  const value = reference.startsWith('--') && palette[reference] !== undefined
    ? resolve(palette, palette[reference])
    : resolve(palette, reference);
  const direct = colour(value);
  if (direct) return direct;
  for (const part of value.split(/[\s,]+/)) {
    const parsed = colour(part);
    if (parsed) return parsed;
  }
  throw new Error(`no colour found in: ${reference} -> ${value}`);
}

function over(foreground, background) {
  const [, , , alpha] = foreground;
  if (alpha >= 1) return [foreground[0], foreground[1], foreground[2], 1];
  return [
    foreground[0] * alpha + background[0] * (1 - alpha),
    foreground[1] * alpha + background[1] * (1 - alpha),
    foreground[2] * alpha + background[2] * (1 - alpha),
    1,
  ];
}

/** Paint a list of layers (topmost first) onto black, returning an opaque rgb. */
function stack(palette, layers) {
  let out = [0, 0, 0, 1];
  for (let index = layers.length - 1; index >= 0; index -= 1) {
    out = over(resolveColour(palette, layers[index]), out);
  }
  return out;
}

function luminance(rgb) {
  const channel = (raw) => {
    const value = raw / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2]);
}

function ratio(a, b) {
  const first = luminance(a);
  const second = luminance(b);
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

const THRESHOLD = { text: 4.5, boundary: 3 };

/** The surfaces a token can genuinely end up painted on, with the rule that puts it there. */
const SHELL = ['--shell-bg'];
const FINDER_ROW = ['--selected', ...SHELL];
const FINDER_HOVER = ['--hover-fill', ...SHELL];
const FINDER_SEARCH = ['--search-focus-bg', ...SHELL];
const LIST_ROW = ['--bg-panel'];
const LIST_SELECTED = ['--selected', ...LIST_ROW];
const FIELD = ['--bg-sunken'];
const INSET_FIELD = ['--inset-fill', ...LIST_ROW];

const CHECKS = [
  /* ---- body text on every surface family it is painted on ---- */
  group('text on surfaces', [
    ['--fg', ['--bg'], 'text', 'EntryDetails in the library detail pane'],
    ['--fg', SHELL, 'text', '.palette shell (Finder.svelte)'],
    ['--fg', LIST_ROW, 'text', 'library panes, Dialog body'],
    ['--fg', ['--bg-raised'], 'text', 'button, kbd, tab count badge'],
    ['--fg', FIELD, 'text', 'input / textarea text'],
    ['--fg', INSET_FIELD, 'text', '.recovery credential block (SettingsPanel)'],
    ['--fg', ['--bg-hover'], 'text', 'button:hover fill'],
    ['--fg', ['--bg-active'], 'text', 'button:active fill'],
    ['--fg', FINDER_ROW, 'text', '.name on the selected Finder row'],
    ['--fg', LIST_SELECTED, 'text', '.name on the selected EntryList row'],
    ['--selected-fg', FINDER_ROW, 'text', '.row.active colour (Finder.svelte)'],
    ['--selected-fg', LIST_SELECTED, 'text', '.row-wrap.selected colour (EntryList.svelte)'],
    ['--fg-muted', ['--bg'], 'text', 'EntryDetails labels, EmptyState hint'],
    ['--fg-muted', SHELL, 'text', 'Dialog footer, vault chip, Finder context line'],
    ['--fg-muted', LIST_ROW, 'text', '.label, meta line, SettingsPanel headings'],
    ['--fg-muted', ['--bg-raised'], 'text', 'kbd colour, tab count badge'],
    ['--fg-muted', FINDER_HOVER, 'text', 'context line on a hovered row'],
    ['--fg-muted', FINDER_ROW, 'text', 'context line on the selected row'],
    ['--fg-muted', LIST_SELECTED, 'text', 'meta line on the selected row'],
    ['--fg-muted', ['--warn-soft', ...LIST_ROW], 'text', '.locked-note (EntryList.svelte)'],
    ['--fg-dim', SHELL, 'text', 'search icon, .tail, .sub in the palette'],
    ['--fg-dim', ['--bg-raised'], 'text', 'kbd inside a keycap hint'],
    ['--fg-dim', FIELD, 'text', 'input::placeholder, Library search icon over the field'],
    ['--fg-dim', FINDER_ROW, 'text', '.ctx-part separator dot on the selected row'],
    ['--fg-dim', LIST_SELECTED, 'text', '.tag / .dot on the selected row'],
    ['--footer-fg', SHELL, 'text', 'Finder footer hints'],
    /* ---- search line ---- */
    ['--search-fg', SHELL, 'text', 'Finder search input, unfocused'],
    ['--search-fg', FINDER_SEARCH, 'text', 'Finder search input, focus wash'],
    ['--search-placeholder', SHELL, 'text', 'Finder placeholder, unfocused'],
    ['--search-placeholder', FINDER_SEARCH, 'text', 'Finder placeholder, focus wash'],
    /* ---- accent as text ---- */
    ['--accent', ['--bg'], 'text', 'ExtraFields .value.url in the detail pane'],
    ['--accent', LIST_ROW, 'text', 'accent text on a library pane'],
    ['--fg', ['--accent-soft', '--selected', ...LIST_ROW], 'text', '.tab.active .count badge'],
    ['--accent-fg', ['--accent'], 'text', 'primary button label'],
    ['--accent-fg', ['--accent-strong'], 'text', 'primary button label, hover'],
    /* ---- status text on its own soft fill ---- */
    ['--danger', LIST_ROW, 'text', 'VaultStatus error, destructive button'],
    ['--danger', ['--danger-soft', ...LIST_ROW], 'text', '.error-banner (import review)'],
    ['--danger', ['--danger-soft', ...SHELL], 'text', 'Finder copy-failure line'],
    ['--ok', LIST_ROW, 'text', 'VaultStatus unlocked'],
    ['--ok', ['--ok-soft', ...LIST_ROW], 'text', 'StatusLine ok tone'],
    ['--ok', ['--ok-soft', ...SHELL], 'text', 'Finder copied line'],
    ['--ok', SHELL, 'text', 'vaultchip[data-tone=ok]'],
    ['--warn', LIST_ROW, 'text', 'backup warning, .warning (SettingsPanel)'],
    ['--warn', ['--warn-soft', ...SHELL], 'text', 'Finder system row glyph'],
    ['--warn', FINDER_ROW, 'text', 'Finder system row name on the band'],
    ['--warn', SHELL, 'text', 'vaultchip[data-tone=warn]'],
  ]),

  /* ---- non-text: state indicators and meaningful tint ---- */
  group('focus ring on every surface it can be drawn on', [
    ['--focus-ring', ['--bg'], 'boundary', ':focus-visible outline in the detail pane'],
    ['--focus-ring', SHELL, 'boundary', 'outline inside the palette'],
    ['--focus-ring', LIST_ROW, 'boundary', 'outline on library panes'],
    ['--focus-ring', ['--bg-raised'], 'boundary', 'outline on a button'],
    ['--focus-ring', FIELD, 'boundary', 'outline on a field'],
    ['--focus-ring', FINDER_ROW, 'boundary', 'outline on the selected row'],
    ['--focus-ring', FINDER_HOVER, 'boundary', 'outline on a hovered row'],
    ['--focus-ring', INSET_FIELD, 'boundary', 'outline in the recovery block'],
  ]),
  group('selection perceivable without colour', [
    ['--selected-marker', FINDER_ROW, 'boundary', 'inset marker vs the band it sits in'],
    ['--selected-marker', SHELL, 'boundary', 'marker colour vs the unselected row'],
    ['--selected-marker', LIST_SELECTED, 'boundary', 'EntryList marker vs the band'],
    ['--selected-marker', LIST_ROW, 'boundary', 'EntryList marker vs the unselected row'],
    ['--selected-marker', ['--bg-raised'], 'boundary', 'marker vs a tab/button surface'],
  ]),
  group('caret and field affordance', [
    ['--caret', SHELL, 'boundary', 'Finder caret (the search line has no box)'],
    ['--caret', FINDER_SEARCH, 'boundary', 'Finder caret on the focus wash'],
    ['--caret', FIELD, 'boundary', 'caret-color on inputs (tokens.css)'],
    ['--fg-muted', FIELD, 'boundary', 'select arrow drawn from a gradient'],
  ]),
  group('kind and status tints that carry meaning', [
    ['--kind-command', ['--accent-soft', ...SHELL], 'boundary', 'Finder glyph tile'],
    ['--kind-command', ['--accent-soft', ...FINDER_ROW], 'boundary', 'glyph tile on the band'],
    ['--kind-command', ['--accent-soft', ...LIST_ROW], 'boundary', 'EntryList / EntryDetails glyph'],
    ['--accent', ['--accent-soft', ...LIST_ROW], 'boundary', 'EmptyState / brand glyph tile'],
    ['--accent', LIST_SELECTED, 'boundary', '.pin icon on a selected row (has a text label)'],
    ['--accent', LIST_ROW, 'boundary', '.pin icon in the library list'],
    ['--kind-secret', ['--secret-soft', ...SHELL], 'boundary', 'Finder secret glyph tile'],
    ['--kind-secret', ['--secret-soft', ...FINDER_ROW], 'boundary', 'secret glyph on the band'],
    ['--kind-secret', ['--secret-soft', ...LIST_ROW], 'boundary', '.masked pill border (EntryList)'],
    ['--kind-secret', ['--secret-soft', ...LIST_SELECTED], 'boundary', 'secret glyph on a selected row'],
    ['--danger', ['--danger-soft', ...LIST_ROW], 'boundary', 'error-banner border over its fill'],
    ['--warn', LIST_ROW, 'boundary', '.confirm border (SettingsPanel)'],
    ['--accent', FIELD, 'boundary', 'checkbox accent-color'],
  ]),
];

/**
 * Colour tokens that are deliberately not contrast-checked, with the reason.
 * They are still printed so the exemption is visible in the evidence.
 */
const EXEMPT = {
  '--bg': 'surface only: text on it is checked, the surface needs no ratio of its own',
  '--bg-panel': 'surface only',
  '--bg-raised': 'surface only',
  '--bg-sunken': 'surface only',
  '--bg-hover': 'surface only (hover fill; the hovered row keeps --fg, checked above)',
  '--bg-active': 'surface only',
  '--shell-bg': 'surface only: the opaque shell (see the native material note)',
  '--line': 'separator hairline between regions; WCAG 1.4.11 exempts non-informational borders, and every field this outlines has a visible label plus a 3:1 focus ring',
  '--line-strong': 'hover separator on a control that also changes fill and keeps a >=4.5:1 label',
  '--line-shell': 'shell separator, decorative',
  '--line-hair': 'hairline separator, decorative',
  '--shell-edge': '1px inner highlight that shapes the shell edge, decorative',
  '--selected-edge': 'soft edge of the selection band; the state is carried by --selected-marker, which is checked at 3:1',
  '--hover-fill': 'hover fill is a state hint on top of the caret/label contrast, not an identifier',
  '--inset-fill': 'surface only (recovery block)',
  '--search-focus-bg': 'surface only (search focus wash)',
  '--accent-soft': 'surface only (kind tile fill)',
  '--command-accent': 'alias of --accent, measured through --kind-command',
  '--secret-accent': 'alias measured through --kind-secret, which resolves to the same value',
  '--accent-line': 'focus border on top of the 3:1 :focus-visible ring in --focus-ring',
  '--selected': 'surface only (selection band); non-colour cue is --selected-marker',
  '--danger-soft': 'surface only',
  '--ok-soft': 'surface only',
  '--warn-soft': 'surface only',
  '--secret-soft': 'surface only',
  '--backdrop': 'scrim behind a modal dialog; not a text or control boundary',
  '--material-tint': 'transparent by default; the opt-in value is an unverified native prototype',
};

function group(name, rows) {
  return { name, rows: rows.map(([token, layers, kind, where]) => ({ token, layers, kind, where })) };
}

/** Every stylesheet / component the shipped UI is built from. */
function sourceFiles(dir) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...sourceFiles(path));
    else if (/\.(svelte|css|ts)$/.test(entry.name)) out.push(path);
  }
  return out;
}

const summary = process.argv.includes('--summary');
const failures = [];
const checkedTokens = new Set();
const lines = [];

for (const theme of ['dark', 'light', ...Object.keys(palettes).filter((key) => key.startsWith('omarchy-'))]) {
  const palette = palettes[theme];
  lines.push(`\n${theme} palette (src/styles/tokens.css)`, '');
  for (const section of CHECKS) {
    const measured = section.rows.map((row) => {
      const foreground = resolveColour(palette, row.token);
      const background = stack(palette, row.layers);
      const value = ratio(foreground, background);
      return { ...row, value };
    });
    const worst = measured.reduce((a, b) => (b.value < a.value ? b : a));
    const threshold = THRESHOLD[section.rows[0].kind];
    lines.push(
      `  ${section.name.padEnd(46)} ${measured.length} pairs  min ${worst.value.toFixed(2)}:1  ` +
        `(>= ${threshold}:1)  ${worst.token} on ${worst.layers.join(' + ')}  ${measured.every((m) => m.value >= threshold) ? 'ok' : 'FAIL'}`
    );
    if (!summary) {
      for (const row of measured) {
        const pass = row.value >= THRESHOLD[row.kind];
        checkedTokens.add(row.token);
        row.layers.forEach((layer) => {
          if (layer.startsWith('--')) checkedTokens.add(layer);
        });
        if (!pass) failures.push(`${theme}: ${row.token} on ${row.layers.join(' + ')} = ${row.value.toFixed(2)}:1 (< ${THRESHOLD[row.kind]}:1) - ${row.where}`);
        lines.push(
          `      ${row.value >= THRESHOLD[row.kind] ? 'ok  ' : 'FAIL'} ${row.value.toFixed(2).padStart(6)}:1  ` +
            `${row.token} on ${row.layers.join(' + ')}  [${row.where}]`
        );
      }
    } else {
      measured.forEach((row) => {
        checkedTokens.add(row.token);
        row.layers.forEach((layer) => {
          if (layer.startsWith('--')) checkedTokens.add(layer);
        });
        if (row.value < THRESHOLD[row.kind]) {
          failures.push(`${theme}: ${row.token} on ${row.layers.join(' + ')} = ${row.value.toFixed(2)}:1 (< ${THRESHOLD[row.kind]}:1) - ${row.where}`);
        }
      });
    }
  }

  lines.push('', '  exempt by design (measured against --bg-panel for reference)', '');
  const reference = stack(palette, ['--bg-panel']);
  for (const [token, reason] of Object.entries(EXEMPT)) {
    if (palette[token] === undefined) {
      failures.push(`${theme}: exempt token ${token} no longer exists in tokens.css`);
      continue;
    }
    checkedTokens.add(token);
    if (summary) continue;
    let text = 'n/a';
    try {
      text = `${ratio(resolveColour(palette, token), reference).toFixed(2)}:1`;
    } catch {
      text = 'not a flat colour';
    }
    lines.push(`      ${token.padEnd(18)} ${text.padStart(8)}  ${reason}`);
  }
}

/* completeness: a new colour token must be checked or exempt with a reason */
for (const [token, raw] of Object.entries(palettes.dark)) {
  const isColour = colour(resolve(palettes.dark, raw)) !== null;
  if (isColour && !checkedTokens.has(token)) {
    failures.push(`dark: colour token ${token} is neither contrast-checked nor exempt`);
  }
}
const lightNames = Object.keys(palettes.light).sort().join();
const darkNames = Object.keys(palettes.dark).sort().join();
if (lightNames !== darkNames) {
  failures.push('the light palette does not declare the same token names as the dark palette');
}
for (const name of Object.keys(palettes.light)) {
  if (palettes.systemLight[name] !== palettes.light[name]) {
    failures.push(`light drift: ${name} differs between the two light palettes`);
  }
}

/* structural promises the brief makes about motion, material and focus */
const STRUCTURE = [
  [
    ':focus-visible ring is defined from --focus-ring',
    /:focus-visible\s*\{[^}]*outline:\s*2px solid var\(--focus-ring\)/.test(css),
  ],
  [
    'prefers-reduced-motion collapses every transition',
    /@media \(prefers-reduced-motion: reduce\)[\s\S]*?transition-duration:\s*0\.01ms !important/.test(css),
  ],
  [
    'prefers-reduced-transparency forces the opaque material back',
    /@media \(prefers-reduced-transparency: reduce\)\s*\{\s*:root,\s*:root\[data-transparency='on'\]\s*\{\s*--material-blur:\s*0px;\s*--material-tint:\s*transparent;/.test(
      css
    ),
  ],
  [
    "data-transparency='off' forces the opaque material back",
    /:root\[data-transparency='off'\]\s*\{\s*--material-blur:\s*0px;\s*--material-tint:\s*transparent;/.test(css),
  ],
  ['the shipped default paints no blur at all', css.includes('--material-blur: 0px')],
  [
    'no rule under src/ requests a compositor blur',
    !sourceFiles(join(ROOT, 'src')).some((file) => readFileSync(file, 'utf8').includes('backdrop-filter')),
  ],
  [
    'every confirm frame comes from --border-warn, none hand-writes a warn border',
    !sourceFiles(join(ROOT, 'src'))
      .filter((file) => file.endsWith('.svelte'))
      .some((file) => /border[^;]*:\s*[^;]*solid var\(--warn\)/.test(readFileSync(file, 'utf8'))),
  ],
];
lines.push('', 'structural checks', '');
for (const [label, pass] of STRUCTURE) {
  if (!pass) failures.push(`structural check failed: ${label}`);
  lines.push(`  ${pass ? 'ok  ' : 'FAIL'} ${label}`);
}

if (!summary) console.log(lines.join('\n'));
else console.log(lines.filter((line) => !line.startsWith('      ')).join('\n'));

if (failures.length > 0) {
  console.error('\ncontrast failures:');
  for (const failure of failures) console.error(`  - ${failure}`);
  console.error(`\n${failures.length} failure(s)`);
  process.exit(1);
}
console.log(`\nall checks passed: every pair clears its threshold in ${Object.keys(palettes).length - 1} palettes`);
