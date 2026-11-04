import { describe, expect, it } from 'vitest';
import { EMPTY_DRAFT } from '../../src/lib/models';
import type { EntryDraft } from '../../src/lib/models';
import { isHttpUrl, validateDraft } from '../../src/lib/validation';

function command(overrides: Partial<EntryDraft> = {}): EntryDraft {
  return {
    ...EMPTY_DRAFT('command'),
    name: 'prune-containers',
    type: 'Docker',
    command: 'docker container prune',
    ...overrides
  };
}

describe('draft validation', () => {
  it('accepts a complete command draft', () => {
    expect(validateDraft(command())).toBeNull();
  });

  it('requires name, type and command text', () => {
    expect(validateDraft(command({ name: '  ' }))).toMatch(/name/i);
    expect(validateDraft(command({ type: '' }))).toMatch(/type/i);
    expect(validateDraft(command({ command: '  ' }))).toMatch(/command/i);
  });

  it('rejects spaces inside a name because Space copies in the finder', () => {
    expect(validateDraft(command({ name: 'wireguard off' }))).toMatch(/spaces/i);
    expect(validateDraft(command({ name: 'wireguard\toff' }))).toMatch(/spaces/i);
    expect(validateDraft(command({ name: '  wireguard-off  ' }))).toBeNull();
  });

  it('requires a secret value so an empty reveal cannot wipe a stored secret', () => {
    const secret = { ...EMPTY_DRAFT('secret'), name: 'token', type: 'API key', value: '' };
    expect(validateDraft(secret)).toMatch(/secret value/i);
    expect(validateDraft({ ...secret, value: 'hunter2' })).toBeNull();
  });

  it('rejects blank labels and non-http URLs in custom fields', () => {
    const blank = command({ fields: [{ id: 'f1', label: ' ', value: 'x', display: 'text' }] });
    expect(validateDraft(blank)).toMatch(/label/i);
    const badUrl = command({ fields: [{ id: 'f1', label: 'URL', value: 'ftp://x.test', display: 'url' }] });
    expect(validateDraft(badUrl)).toMatch(/http/i);
    const goodUrl = command({ fields: [{ id: 'f1', label: 'URL', value: 'https://x.test/a', display: 'url' }] });
    expect(validateDraft(goodUrl)).toBeNull();
  });

  it('only treats http(s) as openable URLs', () => {
    expect(isHttpUrl('https://x.test')).toBe(true);
    expect(isHttpUrl('http://localhost:8080')).toBe(true);
    expect(isHttpUrl('javascript:alert(1)')).toBe(false);
    expect(isHttpUrl('file:///etc/passwd')).toBe(false);
  });
});
