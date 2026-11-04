import { describe, expect, it } from 'vitest';
import { displayedQuery, nextSelection, selected, shouldCommit, tabCompletion } from '../../src/lib/finder-state';
import type { SearchResult } from '../../src/lib/models';

function row(id: string, name: string, kind: 'command' | 'secret' = 'command'): SearchResult {
  return {
    id,
    kind,
    name,
    type: 'Git',
    description: '',
    pinned: false,
    usageCount: 0,
    tags: [],
    lastUsedAt: null
  };
}

const rows = [row('1', 'docker'), row('2', 'docker compose up'), row('3', 'git revert HEAD')];

describe('finder selection', () => {
  it('selects the first result automatically', () => {
    expect(nextSelection(rows, null, 0)).toBe('1');
  });

  it('moves and clamps without wrapping past the ends', () => {
    expect(nextSelection(rows, '1', 1)).toBe('2');
    expect(nextSelection(rows, '2', 1)).toBe('3');
    expect(nextSelection(rows, '3', 1)).toBe('3');
    expect(nextSelection(rows, '3', -1)).toBe('2');
    expect(nextSelection(rows, '1', -1)).toBe('1');
  });

  it('keeps the stable id when the list is replaced with a matching row', () => {
    expect(nextSelection(rows, '2', 0)).toBe('2');
    expect(selected(rows, '2')?.name).toBe('docker compose up');
  });

  it('has no selection when there are no results', () => {
    expect(nextSelection([], '1', 1)).toBeNull();
    expect(selected([], '1')).toBeNull();
  });
});

describe('finder completion', () => {
  it('shows a muted completion only for a real name prefix', () => {
    expect(displayedQuery('doc', rows[1])).toBe('docker compose up');
    expect(displayedQuery('compose', rows[1])).toBeNull();
    expect(displayedQuery('', rows[0])).toBeNull();
  });

  it('Tab accepts a prefix completion and replaces the query for a fuzzy match', () => {
    expect(tabCompletion('doc', rows[1])).toBe('docker compose up');
    expect(tabCompletion('rvr', row('9', 'git revert HEAD'))).toBe('git revert HEAD');
    expect(tabCompletion('docker', rows[0])).toBeNull();
    expect(tabCompletion('doc', null)).toBeNull();
  });

  it('never lets completion change the selection identity', () => {
    const completed = tabCompletion('doc', rows[1]);
    expect(completed).toBe('docker compose up');
    expect(rows.find((r) => r.name === completed)?.id).toBe('2');
  });
});

describe('IME handling', () => {
  it('ignores keys delivered during composition', () => {
    expect(shouldCommit({ isComposing: true, keyCode: 13 })).toBe(false);
    expect(shouldCommit({ isComposing: false, keyCode: 229 })).toBe(false);
    expect(shouldCommit({ isComposing: false, keyCode: 13 })).toBe(true);
  });
});
