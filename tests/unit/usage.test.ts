import { describe, expect, it } from 'vitest';

import {
  chartScale,
  costTypeSegments,
  fillPeriods,
  formatDuration,
  formatPercent,
  formatResetsIn,
  formatTokens,
  formatUsd,
  lookupRate,
  mergeUsage,
  monotoneTangents,
  niceScale,
  paceOf,
  priceRecord,
  remainingPercent,
  tokenTypeSegments,
  valueToY,
  VIEW_HEIGHT,
  type UsageRecord
} from '../../src/lib/usage';

function record(overrides: Partial<UsageRecord> = {}): UsageRecord {
  return {
    ts: '2026-11-12T10:00:00Z',
    session: 's1',
    model: 'gpt-6.1-sol',
    inputTokens: 1000,
    cachedInputTokens: 0,
    cacheCreationTokens: 0,
    outputTokens: 0,
    reasoningTokens: 0,
    ...overrides
  };
}

describe('formatting', () => {
  it('formats USD with two decimals', () => {
    expect(formatUsd(1.5)).toBe('$1.50');
    expect(formatUsd(1234.56)).toBe('$1,234.56');
  });

  it('formats token counts with suffixes', () => {
    expect(formatTokens(0)).toBe('0');
    expect(formatTokens(999)).toBe('999');
    expect(formatTokens(1500)).toBe('1.50K'); // T3 keeps the trailing digit at this magnitude
    expect(formatTokens(15_000)).toBe('15K');
    expect(formatTokens(2_000_000)).toBe('2M');
    expect(formatTokens(3_400_000_000)).toBe('3.40B'); // same two-digit rule applies to B
  });

  it('shows a sub-threshold percent with a < sign', () => {
    expect(formatPercent(0.0001)).toBe('<0.1%');
    expect(formatPercent(0)).toBe('0.0%');
    expect(formatPercent(0.5)).toBe('50.0%');
    expect(formatPercent(0.5, 0)).toBe('50%');
  });

  it('formats reset durations in coarse units', () => {
    expect(formatDuration(90 * 60_000)).toBe('1h 30m');
    expect(formatDuration(2 * 86_400_000)).toBe('2d 0h');
    expect(formatDuration(-5)).toBe('0m');
  });
});

describe('limits math', () => {
  const base = {
    id: 'w1',
    kind: 'session' as const,
    label: 'Session',
    usedPercent: 34,
    windowDurationMins: 300
  };

  it('reports remaining quota, clamped', () => {
    expect(remainingPercent({ ...base, usedPercent: 34 })).toBe(66);
    expect(remainingPercent({ ...base, usedPercent: 0 })).toBe(100);
    expect(remainingPercent({ ...base, usedPercent: 120 })).toBe(0);
  });

  it('classifies pace against elapsed window time', () => {
    const now = Date.parse('2026-11-12T12:00:00Z');
    // 5h window ending 6h after now: 5/6 of the window already elapsed.
    const window = { ...base, resetsAt: '2026-11-12T18:00:00Z' };
    expect(paceOf(window, now)).toBe('ahead'); // 34% used vs ~16.7% elapsed
    expect(remainingPercent(window)).toBe(66);
    expect(formatResetsIn(window, now)).toBe('resets in 6h 0m');
  });

  it('returns null pace without a reset clock', () => {
    expect(paceOf(base, Date.now())).toBeNull();
  });
});

describe('pricing', () => {
  it('prices a plain request from the rate table', () => {
    const { costUsd } = priceRecord(
      record({ model: 'gpt-6-sol', inputTokens: 1_000_000, outputTokens: 1_000_000 })
    );
    expect(costUsd).toBeCloseTo(12, 6);
  });

  it('bills cached input at the cached rate', () => {
    const { costUsd } = priceRecord(
      record({ model: 'gpt-6-sol', inputTokens: 0, cachedInputTokens: 1_000_000 })
    );
    expect(costUsd).toBeCloseTo(0.2, 6);
  });

  it('bills cache writes at 1.25x input', () => {
    const { costUsd } = priceRecord(
      record({ model: 'gpt-6-sol', inputTokens: 0, cacheCreationTokens: 1_000_000 })
    );
    expect(costUsd).toBeCloseTo(2.5, 6);
  });

  it('applies the long-context tier', () => {
    const { costUsd } = priceRecord(
      record({
        model: 'gpt-6.1-sol',
        inputTokens: 300_000,
        outputTokens: 1_000_000,
        cachedInputTokens: 0
      })
    );
    // 2x input tier + 1.5x output tier: 300k*2/1M*$2 + 1M*1.5/1M*$10.
    expect(costUsd).toBeCloseTo(16.2, 6);
  });

  it('leaves reviewer models unpriced', () => {
    expect(lookupRate('codex-auto-review')).toBeNull();
    const { costUsd, category } = priceRecord(record({ model: 'codex-auto-review' }));
    expect(costUsd).toBe(0);
    expect(category).toBeNull();
  });

  it('prices dated model snapshots like their family prefix', () => {
    expect(lookupRate('gpt-6.1-sol-2026-01-01')?.outputPerToken).toBe(
      lookupRate('gpt-6.1-sol')?.outputPerToken
    );
    expect(lookupRate('totally-unknown-model')).toBeNull();
  });
});

describe('mergeUsage', () => {
  const bare = { inputTokens: 0 };

  it('totals tokens and counts distinct sessions', () => {
    const merged = mergeUsage([
      record({ session: 'a', ...bare, outputTokens: 500 }),
      record({ session: 'a', ...bare, outputTokens: 500 }),
      record({ session: 'b', ...bare, outputTokens: 100 })
    ]);
    expect(merged.totals.sessions).toBe(2);
    expect(merged.totals.outputTokens).toBe(1100);
    expect(merged.totals.totalTokens).toBe(1100);
  });

  it('splits models by share and marks unpriced ones', () => {
    const merged = mergeUsage([
      record({ model: 'gpt-6-sol', ...bare, outputTokens: 900 }),
      record({ model: 'codex-auto-review', ...bare, outputTokens: 100 })
    ]);
    const review = merged.models.find((model) => model.model === 'codex-auto-review');
    expect(review?.unpriced).toBe(true);
    expect(review?.costShare).toBeNull();
    const sol = merged.models.find((model) => model.model === 'gpt-6-sol');
    expect(sol?.tokenShare).toBeCloseTo(0.9, 6);
    expect(sol?.costShare).toBe(1);
  });

  it('buckets records by local day and hour, sorted', () => {
    const merged = mergeUsage([
      record({ ts: '2026-11-12T10:30:00Z', ...bare, outputTokens: 100 }),
      record({ ts: '2026-11-11T08:00:00Z', ...bare, outputTokens: 200 })
    ]);
    expect(merged.daily.map((entry) => entry.key)).toEqual(['2026-11-11', '2026-11-12']);
    expect(merged.hourly).toHaveLength(2);
    expect(merged.daily[1]?.totalTokens).toBe(100);
  });

  it('credits cache savings against the uncached rate', () => {
    const merged = mergeUsage([
      record({ model: 'gpt-6-sol', ...bare, cachedInputTokens: 1_000_000 })
    ]);
    expect(merged.totals.cacheSavingsUsd).toBeCloseTo(1.8, 6);
  });
});

describe('share bars and chart math', () => {
  it('builds four cost and token segments in T3 order', () => {
    const segments = costTypeSegments({ input: 1, cacheRead: 2, cacheWrite: 3, output: 4 });
    expect(segments.map((segment) => segment.label)).toEqual([
      'Input',
      'Cache read',
      'Cache write',
      'Output'
    ]);
    const tokenSegments = tokenTypeSegments({
      uncachedInputTokens: 1,
      cachedInputTokens: 2,
      cacheCreationTokens: 3,
      outputTokens: 4
    });
    expect(tokenSegments.every((segment) => segment.color.startsWith('color-mix'))).toBe(true);
  });

  it('picks a nice tick scale above the peak', () => {
    const { max, ticks } = niceScale(9.7, 4);
    expect(max).toBe(10);
    expect(ticks).toEqual([0, 5, 10]); // T3's step rule: normalized 2.425 → step 5
    expect(chartScale([3, 9.7, 1]).max).toBe(10);
    expect(niceScale(0, 4)).toEqual({ max: 0, ticks: [0] });
  });

  it('maps zero to the baseline and the peak to the plot top', () => {
    expect(valueToY(0, 10)).toBe(VIEW_HEIGHT);
    expect(valueToY(10, 10)).toBe(8);
  });

  it('keeps monotone tangents flat across local extrema', () => {
    const points = [
      { x: 0, y: 0 },
      { x: 10, y: 10 },
      { x: 20, y: 0 }
    ];
    const tangents = monotoneTangents(points);
    expect(tangents[1]).toBe(0);
    expect(tangents.every((t) => Number.isFinite(t))).toBe(true);
  });
});

describe('fillPeriods', () => {
  it('zero-fills quiet periods so the axis never skips days', () => {
    const filled = fillPeriods(['d1', 'd2', 'd3'], [{ key: 'd2', costUsd: 1, totalTokens: 5 }]);
    expect(filled.map((entry) => entry.totalTokens)).toEqual([0, 5, 0]);
    expect(filled[0]?.key).toBe('d1');
  });
});
