import type { SearchResult } from './models';

export interface FinderState {
  query: string;
  results: SearchResult[];
  selectedId: string | null;
  /** Completion text as displayed (query + muted remainder) for the selected row. */
  completion: string | null;
}

export function selectedIndex(results: SearchResult[], selectedId: string | null): number {
  if (!selectedId) return -1;
  return results.findIndex((r) => r.id === selectedId);
}

/** Always keeps a valid selection when results exist; keeps the same id when still present. */
export function nextSelection(
  results: SearchResult[],
  selectedId: string | null,
  delta: number
): string | null {
  if (results.length === 0) return null;
  const current = selectedIndex(results, selectedId);
  if (current === -1) return delta >= 0 ? results[0].id : results[results.length - 1].id;
  const next = Math.min(results.length - 1, Math.max(0, current + delta));
  return results[next].id;
}

export function selected(results: SearchResult[], selectedId: string | null): SearchResult | null {
  return results.find((r) => r.id === selectedId) ?? null;
}

/**
 * The text shown in the input while a row is selected: the query the user typed plus a muted
 * completion. For prefix matches the remainder is appended; for fuzzy matches nothing is appended
 * so the caret stays meaningful (Tab then replaces the query with the full name).
 */
export function displayedQuery(query: string, entry: SearchResult | null): string | null {
  if (!entry) return null;
  const name = entry.name;
  if (query.length === 0) return null;
  if (name.length > query.length && name.toLowerCase().startsWith(query.toLowerCase())) {
    return name;
  }
  return null;
}

export function tabCompletion(query: string, entry: SearchResult | null): string | null {
  if (!entry) return null;
  const lowerQuery = query.toLowerCase();
  if (entry.name.toLowerCase().startsWith(lowerQuery)) {
    if (entry.name === query) return null;
    return entry.name;
  }
  return entry.name;
}

export function shouldCommit(event: { isComposing: boolean; keyCode: number }): boolean {
  // Ignore keys delivered as part of IME composition (including the 229 phantom keyCode).
  return !event.isComposing && event.keyCode !== 229;
}
