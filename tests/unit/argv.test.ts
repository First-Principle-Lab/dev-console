import { describe, expect, it } from 'vitest';

import { formatArgv, parseArgv } from '../../src/lib/argv';

describe('parseArgv', () => {
  it('splits plain words like the old splitter did', () => {
    expect(parseArgv('x-terminal-emulator -e')).toEqual(['x-terminal-emulator', '-e']);
    expect(parseArgv('  spaced   out  \ttabs\t')).toEqual(['spaced', 'out', 'tabs']);
    expect(parseArgv('')).toEqual([]);
  });

  it('keeps paths and arguments that contain spaces when quoted', () => {
    expect(parseArgv('"/usr/local/bin/my terminal" -e')).toEqual([
      '/usr/local/bin/my terminal',
      '-e'
    ]);
    expect(parseArgv("'/home/me/My Terminal.app/Contents/MacOS/run' --wait")).toEqual([
      '/home/me/My Terminal.app/Contents/MacOS/run',
      '--wait'
    ]);
    expect(parseArgv('foot --title "Dev Console" -e')).toEqual([
      'foot',
      '--title',
      'Dev Console',
      '-e'
    ]);
  });

  it('supports escapes and empty quoted words', () => {
    expect(parseArgv('run\\ with\\ spaces --x')).toEqual(['run with spaces', '--x']);
    expect(parseArgv('a "" b')).toEqual(['a', '', 'b']);
    expect(parseArgv('"say \\"hi\\"" -e')).toEqual(['say "hi"', '-e']);
  });

  it('rejects unbalanced quotes instead of guessing', () => {
    expect(() => parseArgv('foot --title "broken')).toThrow(/unbalanced double quote/);
    expect(() => parseArgv("alacritty -e 'broken")).toThrow(/unbalanced single quote/);
  });

  it('round-trips argv through the input field', () => {
    const argv = ['/usr/local/bin/my terminal', '--title', 'Dev Console', '-e'];
    expect(parseArgv(formatArgv(argv))).toEqual(argv);
  });
});
