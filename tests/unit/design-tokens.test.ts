import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

/**
 * tokens.css states the rule the whole UI is built on: every colour, radius,
 * space, control height and shadow comes from that file. These checks keep the
 * two mechanical halves of that promise true without a human re-reading CSS:
 *
 * 1. the dark palette and *both* light palettes declare the same token names,
 *    so a token added to one theme cannot silently be missing from the other;
 * 2. no component declares a colour literal of its own.
 */

const TOKENS_FILE = join(import.meta.dirname, '..', '..', 'src', 'styles', 'tokens.css');
const SRC_DIR = join(import.meta.dirname, '..', '..', 'src');

const css = readFileSync(TOKENS_FILE, 'utf8');

/** Custom property names declared inside the block that a selector opens. */
function declaredNames(selectorPattern: RegExp): string[] {
  const match = css.match(selectorPattern);
  if (!match) throw new Error(`no block matching ${selectorPattern}`);
  return [...match[1].matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1] as string);
}

const dark = declaredNames(/:root,\s*:root\[data-theme='dark'\]\s*\{([\s\S]*?)\n\}/);
const systemLight = declaredNames(
  /@media \(prefers-color-scheme: light\)\s*\{\s*:root\s*\{([\s\S]*?)\n {2}\}/
);
const explicitLight = declaredNames(/:root\[data-theme='light'\]\s*\{([\s\S]*?)\n\}/);

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) return sourceFiles(path);
    return /\.(svelte|ts)$/.test(entry) ? [path] : [];
  });
}

describe('design tokens', () => {
  it('declares the same token names in the dark and both light palettes', () => {
    expect(dark.length).toBeGreaterThan(40);
    expect(systemLight.sort()).toEqual([...dark].sort());
    expect(explicitLight.sort()).toEqual([...dark].sort());
  });

  it('keeps colour literals out of components: tokens.css is the only source', () => {
    const hex = /#[0-9a-fA-F]{3,8}\b(?![\w-])/g;
    const rgb = /\brgba?\(\s*[\d.]/g;
    const offenders = sourceFiles(SRC_DIR)
      .filter((path) => !path.endsWith(join('styles', 'tokens.css')))
      .flatMap((path) => {
        const text = readFileSync(path, 'utf8');
        const lines = text.split('\n');
        return lines.flatMap((line, index) => {
          const isColour = hex.test(line) || rgb.test(line);
          hex.lastIndex = 0;
          rgb.lastIndex = 0;
          return isColour ? [`${path}:${index + 1}: ${line.trim()}`] : [];
        });
      });
    expect(offenders).toEqual([]);
  });

  it('ships material disabled: blur is zero unless transparency is requested', () => {
    const defaults = css.slice(0, css.indexOf(':root[data-transparency'));
    expect(defaults).toContain('--material-blur: 0px');
    expect(css).toContain('@media (prefers-reduced-transparency: reduce)');
    expect(css).toContain(":root[data-transparency='off']");
  });
});
