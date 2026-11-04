import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

/**
 * The contrast promise in docs/raycast-ui-redesign.md ("normal text >= 4.5:1,
 * large text and meaningful control boundaries >= 3:1 in both themes") is proved
 * by scripts/contrast-check.mjs, which reads the palettes out of tokens.css,
 * composites the translucent layers over the surface they are painted on and
 * measures every real pairing. Running it here puts the computed ratios inside
 * `bun run test`, and therefore inside scripts/verify.sh, so a token retune that
 * breaks contrast fails the normal gate instead of a human eye.
 */

const SCRIPT = join(import.meta.dirname, '..', '..', 'scripts', 'contrast-check.mjs');

function run(): string {
  return execFileSync(process.execPath, [SCRIPT, '--summary'], { encoding: 'utf8' });
}

describe('token contrast', () => {
  it('measures contrast from tokens.css and clears the thresholds in both themes', () => {
    expect(existsSync(SCRIPT)).toBe(true);
    let output = '';
    let failure: unknown = null;
    try {
      output = run();
    } catch (problem) {
      failure = problem;
    }
    console.log(output);
    expect(failure).toBeNull();
    expect(output).toContain('dark palette');
    expect(output).toContain('light palette');
    expect(output).toContain('all checks passed');
  });
});
