/* CLI Proxy usage math and formatting, ported from T3 Code's usage page
 * (packages/shared/src/usageFormat.ts, usageLimits.ts, usageMerge.ts and
 * apps/web/src/components/usage/UsageProviderChart.tsx) so the CLI Proxy tab
 * reads and behaves like T3's Usage panel. Pure functions only: the Svelte
 * components consume these, and tests/unit/usage.test.ts pins the maths.
 */

export type UsageMetric = 'cost' | 'tokens' | 'limits';

export interface UsageRecord {
  /** ISO timestamp of the response. */
  ts: string;
  /** Codex thread/session id. */
  session: string;
  model: string;
  inputTokens: number;
  cachedInputTokens: number;
  cacheCreationTokens: number;
  outputTokens: number;
  reasoningTokens: number;
}

export interface LimitWindow {
  id: string;
  kind: 'session' | 'weekly' | 'monthly' | 'other';
  label: string;
  usedPercent: number;
  windowDurationMins?: number;
  resetsAt?: string;
}

export interface ResetCredits {
  availableCount: number;
  nextExpiresAt?: string;
}

export interface CliproxyLimits {
  checkedAt: string;
  plan: string | null;
  windows: LimitWindow[];
  resetCredits: ResetCredits | null;
}

/** What the `cliproxy_usage` IPC command returns. */
export interface CliproxyUsage {
  account: { email: string | null; plan: string | null };
  limits: CliproxyLimits | null;
  limitsError: string | null;
  records: UsageRecord[];
}

/* ---------------------------------------------------------------- formatting */

const CURRENCY = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  minimumFractionDigits: 2,
  maximumFractionDigits: 2
});

const INTEGER = new Intl.NumberFormat('en-US');

export function formatUsd(value: number): string {
  return CURRENCY.format(value);
}

export function formatCount(value: number): string {
  return INTEGER.format(Math.round(value));
}

export function formatTokens(value: number): string {
  const abs = Math.abs(value);
  if (abs >= 1e12) return `${trim(value / 1e12)}T`;
  if (abs >= 1e9) return `${trim(value / 1e9)}B`;
  if (abs >= 1e6) return `${trim(value / 1e6)}M`;
  if (abs >= 1e3) return `${trim(value / 1e3)}K`;
  return INTEGER.format(Math.round(value));
}

function trim(value: number): string {
  const abs = Math.abs(value);
  const digits = abs >= 100 ? 0 : abs >= 10 ? 1 : 2;
  return value.toFixed(digits).replace(/\.0+$/, '');
}

export function formatPercent(share: number, digits = 1): string {
  const percent = share * 100;
  const smallest = 10 ** -digits;
  if (percent > 0 && percent < smallest) return `<${smallest.toFixed(digits)}%`;
  return `${percent.toFixed(digits)}%`;
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

export function formatDayShort(day: string): string {
  const [year, month, dayOfMonth] = day.split('-').map((part) => Number(part));
  if (year === undefined || month === undefined || dayOfMonth === undefined) return day;
  return `${MONTHS[month - 1] ?? ''} ${dayOfMonth}`;
}

export function formatHourShort(hourStart: string): string {
  const instant = new Date(hourStart);
  if (Number.isNaN(instant.getTime())) return hourStart;
  const formatter = new Intl.DateTimeFormat('en-US', { hour: 'numeric' });
  return formatter.format(instant);
}

export function enumerateDays(sinceDay: string, untilDay: string): readonly string[] {
  const days: string[] = [];
  const start = Date.parse(`${sinceDay}T00:00:00Z`);
  const end = Date.parse(`${untilDay}T00:00:00Z`);
  if (Number.isNaN(start) || Number.isNaN(end) || end < start) return days;
  for (let cursor = start; cursor <= end; cursor += 86_400_000) {
    days.push(new Date(cursor).toISOString().slice(0, 10));
  }
  return days;
}

const HOUR_MS = 60 * 60 * 1000;

/** The start of the viewer's clock hour containing `ms`, so a +05:30 zone buckets 19:00-20:00
 * local rather than 18:30-19:30 UTC. */
function floorLocalHour(ms: number): number {
  const date = new Date(ms);
  date.setMinutes(0, 0, 0);
  return date.getTime();
}

/** The local hour starts covering the window; the same keys `localHourKey` gives records. */
export function enumerateHourStarts(sinceTime: string, untilTime: string): readonly string[] {
  const starts: string[] = [];
  const start = Date.parse(sinceTime);
  const end = Date.parse(untilTime);
  if (Number.isNaN(start) || Number.isNaN(end) || end <= start) return starts;
  for (let cursor = floorLocalHour(start); cursor < end; cursor += HOUR_MS) {
    starts.push(new Date(cursor).toISOString());
  }
  return starts;
}

export interface UsageWindowSpec {
  days: number;
  sinceDay: string;
  untilDay: string;
  resolution: 'hour' | 'day';
  sinceTime?: string;
  untilTime?: string;
}

/** Rolling calendar-day window in the viewer's zone, matching T3's makeWindow. */
export function makeWindow(days: number, now = new Date()): UsageWindowSpec {
  let timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
  let format: Intl.DateTimeFormat;
  try {
    format = new Intl.DateTimeFormat('en-CA', {
      timeZone,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit'
    });
  } catch {
    timeZone = 'UTC';
    format = new Intl.DateTimeFormat('en-CA', {
      timeZone: 'UTC',
      year: 'numeric',
      month: '2-digit',
      day: '2-digit'
    });
  }
  const untilDay = format.format(now);
  if (days === 1) {
    const untilTimeMs = Math.floor(now.getTime() / 60_000) * 60_000;
    const sinceTimeMs = untilTimeMs - 24 * HOUR_MS;
    const since = new Date(sinceTimeMs);
    const until = new Date(untilTimeMs);
    return {
      days,
      sinceDay: format.format(since),
      untilDay: format.format(until),
      resolution: 'hour',
      sinceTime: since.toISOString(),
      untilTime: until.toISOString()
    };
  }
  const [year = 0, month = 1, dayOfMonth = 1] = untilDay.split('-').map((part) => Number.parseInt(part, 10));
  const start = new Date(Date.UTC(year, month - 1, dayOfMonth - (days - 1)));
  return {
    days,
    sinceDay: start.toISOString().slice(0, 10),
    untilDay,
    resolution: 'day'
  };
}

/* ------------------------------------------------------------------ limits */
/* Ported from packages/shared/src/usageLimits.ts. Bars show quota remaining,
 * so "on pace" means the time left in the window matches the quota left. */

const MINUTE_MS = 60_000;

export function remainingPercent(window: LimitWindow): number {
  return Math.round(100 - Math.max(0, Math.min(100, window.usedPercent)));
}

function resetMillis(window: LimitWindow): number | null {
  if (window.resetsAt === undefined) return null;
  const at = Date.parse(window.resetsAt);
  return Number.isFinite(at) ? at : null;
}

/** Fraction of the window that has already elapsed, or null without a reset clock. */
export function elapsedShare(window: LimitWindow, now: number): number | null {
  const resetsAt = resetMillis(window);
  if (resetsAt === null || window.windowDurationMins === undefined) return null;
  const length = window.windowDurationMins * MINUTE_MS;
  if (length <= 0) return null;
  return Math.max(0, Math.min(1, (length - (resetsAt - now)) / length));
}

export function paceRemainingPercent(window: LimitWindow, now: number): number | null {
  const elapsed = elapsedShare(window, now);
  return elapsed === null ? null : Math.round((1 - elapsed) * 100);
}

export type LimitPace = 'ahead' | 'on' | 'under';

export function paceOf(window: LimitWindow, now: number): LimitPace | null {
  const elapsed = elapsedShare(window, now);
  if (elapsed === null) return null;
  const gap = window.usedPercent - elapsed * 100;
  if (gap > 5) return 'ahead';
  if (gap < -5) return 'under';
  return 'on';
}

export const PACE_LABEL: Record<LimitPace, string> = {
  ahead: 'Ahead of pace: spending faster than the window elapses',
  on: 'On pace with the window',
  under: 'Under pace: headroom left for the rest of the window'
};

export function formatDuration(ms: number): string {
  const remaining = Math.max(0, ms);
  const days = Math.floor(remaining / 86_400_000);
  const hours = Math.floor((remaining % 86_400_000) / 3_600_000);
  const minutes = Math.floor((remaining % 3_600_000) / MINUTE_MS);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

export function formatResetsIn(window: LimitWindow, now: number): string | null {
  const resetsAt = resetMillis(window);
  if (resetsAt === null) return null;
  if (resetsAt <= now) return 'resets now';
  if (resetsAt - now < MINUTE_MS) return 'Resets momentarily';
  return `resets in ${formatDuration(resetsAt - now)}`;
}

/** The tick position, as a % of the bar where "even spending" sits, clamped for the slot. */
export function paceTickPercent(window: LimitWindow, now: number): number | null {
  return paceRemainingPercent(window, now);
}

/* ------------------------------------------------------------------ pricing */
/* OpenAI list prices, per token, from developers.openai.com/api/docs/pricing
 * (November 2026). Requests above 272K input tokens bill 2x input and cached
 * rates and 1.5x output for the full request. Cache writes bill 1.25x the
 * uncached input rate. Unknown models stay unpriced, exactly like T3. */

export interface ModelRate {
  inputPerToken: number;
  cachedPerToken: number;
  outputPerToken: number;
  /** Sorted by descending threshold. */
  contextTiers: readonly { thresholdTokens: number; multiplier: number }[];
}

const PER_MILLION = 1 / 1_000_000;

const RATE_TABLE: Readonly<Record<string, ModelRate>> = {
  'gpt-6.1-sol': {
    inputPerToken: 2 * PER_MILLION,
    cachedPerToken: 0.1 * PER_MILLION,
    outputPerToken: 10 * PER_MILLION,
    contextTiers: [{ thresholdTokens: 272_000, multiplier: 2 }]
  },
  'gpt-6-sol': {
    inputPerToken: 2 * PER_MILLION,
    cachedPerToken: 0.2 * PER_MILLION,
    outputPerToken: 10 * PER_MILLION,
    contextTiers: []
  },
  'gpt-6-astra': {
    inputPerToken: 10 * PER_MILLION,
    cachedPerToken: 1 * PER_MILLION,
    outputPerToken: 50 * PER_MILLION,
    contextTiers: [{ thresholdTokens: 272_000, multiplier: 2 }]
  },
  'gpt-6.1-terra': {
    inputPerToken: 1 * PER_MILLION,
    cachedPerToken: 0.05 * PER_MILLION,
    outputPerToken: 5 * PER_MILLION,
    contextTiers: [{ thresholdTokens: 272_000, multiplier: 2 }]
  },
  'gpt-6-luna': {
    inputPerToken: 0.1 * PER_MILLION,
    cachedPerToken: 0.01 * PER_MILLION,
    outputPerToken: 0.5 * PER_MILLION,
    contextTiers: [{ thresholdTokens: 272_000, multiplier: 2 }]
  },
  'gpt-5.6-sol': {
    inputPerToken: 4 * PER_MILLION,
    cachedPerToken: 0.4 * PER_MILLION,
    outputPerToken: 20 * PER_MILLION,
    contextTiers: []
  },
  'gpt-5.6-luna': {
    inputPerToken: 0.2 * PER_MILLION,
    cachedPerToken: 0.02 * PER_MILLION,
    outputPerToken: 1.2 * PER_MILLION,
    contextTiers: []
  },
  'gpt-5.3-codex': {
    inputPerToken: 1.75 * PER_MILLION,
    cachedPerToken: 0.175 * PER_MILLION,
    outputPerToken: 14 * PER_MILLION,
    contextTiers: []
  },
  'gpt-5.3': {
    inputPerToken: 1.75 * PER_MILLION,
    cachedPerToken: 0.175 * PER_MILLION,
    outputPerToken: 14 * PER_MILLION,
    contextTiers: []
  }
};

const CACHE_WRITE_MULTIPLIER = 1.25;
/** Internal reviewer models never bill tokens. */
const UNPRICEABLE_MODELS = new Set(['codex-auto-review', '<synthetic>', 'synthetic']);

/** Exact name first, then the longest known prefix, so dated snapshots price like their family. */
export function lookupRate(model: string): ModelRate | null {
  const key = model.trim().toLowerCase();
  if (key.length === 0 || UNPRICEABLE_MODELS.has(key)) return null;
  if (RATE_TABLE[key]) return RATE_TABLE[key];
  let best: { rate: ModelRate; length: number } | null = null;
  for (const name of Object.keys(RATE_TABLE)) {
    if (key.startsWith(name) && (best === null || name.length > best.length)) {
      best = { rate: RATE_TABLE[name], length: name.length };
    }
  }
  return best?.rate ?? null;
}

export interface CategoryCost {
  input: number;
  cacheRead: number;
  cacheWrite: number;
  output: number;
}

export function priceRecord(record: UsageRecord): { costUsd: number; category: CategoryCost | null } {
  const rate = lookupRate(record.model);
  if (rate === null) return { costUsd: 0, category: null };
  const prompt = record.inputTokens + record.cachedInputTokens + record.cacheCreationTokens;
  const tier = rate.contextTiers.find((entry) => prompt > entry.thresholdTokens);
  const multiplier = tier?.multiplier ?? 1;
  const category: CategoryCost = {
    input: record.inputTokens * rate.inputPerToken * multiplier,
    cacheRead: record.cachedInputTokens * rate.cachedPerToken * multiplier,
    cacheWrite: record.cacheCreationTokens * rate.inputPerToken * CACHE_WRITE_MULTIPLIER * multiplier,
    output: record.outputTokens * rate.outputPerToken * (multiplier === 2 ? 1.5 : 1)
  };
  return {
    costUsd: category.input + category.cacheRead + category.cacheWrite + category.output,
    category
  };
}

/* ------------------------------------------------------------- aggregation */

export interface MergedTotals {
  costUsd: number;
  totalTokens: number;
  cachedInputTokens: number;
  uncachedInputTokens: number;
  cacheCreationTokens: number;
  outputTokens: number;
  sessions: number;
  categoryCost: CategoryCost;
  cacheSavingsUsd: number;
  unpricedTokens: number;
}

export interface PeriodTotals {
  key: string;
  costUsd: number;
  totalTokens: number;
}

export interface ModelTotals {
  model: string;
  costUsd: number;
  totalTokens: number;
  tokenShare: number;
  costShare: number | null;
  unpriced: boolean;
}

export interface MergedUsage {
  totals: MergedTotals;
  daily: readonly PeriodTotals[];
  hourly: readonly PeriodTotals[];
  models: readonly ModelTotals[];
}

function emptyTotals(): MergedTotals {
  return {
    costUsd: 0,
    totalTokens: 0,
    cachedInputTokens: 0,
    uncachedInputTokens: 0,
    cacheCreationTokens: 0,
    outputTokens: 0,
    sessions: 0,
    categoryCost: { input: 0, cacheRead: 0, cacheWrite: 0, output: 0 },
    cacheSavingsUsd: 0,
    unpricedTokens: 0
  };
}

function recordTotalTokens(record: UsageRecord): number {
  return (
    record.inputTokens +
    record.cachedInputTokens +
    record.cacheCreationTokens +
    record.outputTokens
  );
}

/** Aggregates records that already fall inside the window. Session count counts
 * distinct thread ids, the way T3 counts transcripts. */
export function mergeUsage(records: readonly UsageRecord[]): MergedUsage {
  const totals = emptyTotals();
  const sessions = new Set<string>();
  const byDay = new Map<string, PeriodTotals>();
  const byHour = new Map<string, PeriodTotals>();
  const byModel = new Map<string, { model: string; costUsd: number; totalTokens: number; unpriced: boolean }>();

  for (const record of records) {
    const tokens = recordTotalTokens(record);
    const { costUsd, category } = priceRecord(record);
    const day = localDayKey(record.ts);
    const hourStart = localHourKey(record.ts);
    sessions.add(record.session);
    totals.costUsd += costUsd;
    totals.totalTokens += tokens;
    totals.cachedInputTokens += record.cachedInputTokens;
    totals.uncachedInputTokens += record.inputTokens;
    totals.cacheCreationTokens += record.cacheCreationTokens;
    totals.outputTokens += record.outputTokens;
    if (category === null) totals.unpricedTokens += tokens;
    else {
      totals.categoryCost.input += category.input;
      totals.categoryCost.cacheRead += category.cacheRead;
      totals.categoryCost.cacheWrite += category.cacheWrite;
      totals.categoryCost.output += category.output;
    }

    accumulate(byDay, day, costUsd, tokens);
    accumulate(byHour, hourStart, costUsd, tokens);

    const model = byModel.get(record.model) ?? { model: record.model, costUsd: 0, totalTokens: 0, unpriced: false };
    model.costUsd += costUsd;
    model.totalTokens += tokens;
    if (category === null) model.unpriced = true;
    byModel.set(record.model, model);

    const rate = lookupRate(record.model);
    if (rate !== null && record.cachedInputTokens > 0) {
      totals.cacheSavingsUsd += record.cachedInputTokens * (rate.inputPerToken - rate.cachedPerToken);
    }
  }

  totals.sessions = sessions.size;
  const sorted = (map: Map<string, PeriodTotals>) => [...map.values()].sort((a, b) => a.key.localeCompare(b.key));
  const models = [...byModel.values()]
    .map((model) => ({
      ...model,
      tokenShare: totals.totalTokens > 0 ? model.totalTokens / totals.totalTokens : 0,
      costShare: model.unpriced || totals.costUsd <= 0 ? null : model.costUsd / totals.costUsd
    }))
    .sort((a, b) => b.totalTokens - a.totalTokens || b.costUsd - a.costUsd);
  return { totals, daily: sorted(byDay), hourly: sorted(byHour), models };
}

function localDayKey(ts: string): string {
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return 'unknown';
  const formatter = new Intl.DateTimeFormat('en-CA', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit'
  });
  return formatter.format(date);
}

function localHourKey(ts: string): string {
  const date = new Date(ts);
  if (Number.isNaN(date.getTime())) return 'unknown';
  return new Date(floorLocalHour(date.getTime())).toISOString();
}

function accumulate(
  map: Map<string, PeriodTotals>,
  key: string,
  costUsd: number,
  totalTokens: number
): void {
  const entry = map.get(key) ?? { key, costUsd: 0, totalTokens: 0 };
  entry.costUsd += costUsd;
  entry.totalTokens += totalTokens;
  map.set(key, entry);
}

/** Fills every period the window covers so the chart x-axis never skips quiet days. */
export function fillPeriods(
  periods: readonly string[],
  totals: readonly PeriodTotals[]
): readonly PeriodTotals[] {
  const byKey = new Map(totals.map((entry) => [entry.key, entry]));
  return periods.map(
    (key) => byKey.get(key) ?? { key, costUsd: 0, totalTokens: 0 }
  );
}

/* -------------------------------------------------------------- share bars */

const ink = (percent: number) => `color-mix(in oklab, var(--fg) ${percent}%, var(--bg))`;

export interface ShareSegment {
  label: string;
  value: number;
  color: string;
}

export function costTypeSegments(cost: CategoryCost): readonly ShareSegment[] {
  return [
    { label: 'Input', value: cost.input, color: ink(60) },
    { label: 'Cache read', value: cost.cacheRead, color: ink(30) },
    { label: 'Cache write', value: cost.cacheWrite, color: ink(72) },
    { label: 'Output', value: cost.output, color: ink(100) }
  ];
}

export function tokenTypeSegments(totals: {
  uncachedInputTokens: number;
  cachedInputTokens: number;
  cacheCreationTokens: number;
  outputTokens: number;
}): readonly ShareSegment[] {
  return [
    { label: 'Input', value: totals.uncachedInputTokens, color: ink(60) },
    { label: 'Cache read', value: totals.cachedInputTokens, color: ink(30) },
    { label: 'Cache write', value: totals.cacheCreationTokens, color: ink(72) },
    { label: 'Output', value: totals.outputTokens, color: ink(100) }
  ];
}

/* -------------------------------------------------------------- chart math */
/* Ported verbatim from UsageProviderChart.tsx: monotone cubic interpolation,
 * layered series (each starts at zero), nice tick scale. */

export const VIEW_WIDTH = 960;
export const VIEW_HEIGHT = 260;
export const TICK_COUNT = 4;
export const PLOT_TOP = 8;

export interface Point {
  x: number;
  y: number;
}

export function monotoneTangents(points: readonly Point[]): readonly number[] {
  const count = points.length;
  if (count < 2) return [0];

  const slopes: number[] = [];
  for (let index = 0; index < count - 1; index += 1) {
    const dx = (points[index + 1]?.x ?? 0) - (points[index]?.x ?? 0);
    const dy = (points[index + 1]?.y ?? 0) - (points[index]?.y ?? 0);
    slopes.push(dx === 0 ? 0 : dy / dx);
  }

  const tangents: number[] = Array.from({ length: count }, () => 0);
  tangents[0] = slopes[0] ?? 0;
  tangents[count - 1] = slopes[count - 2] ?? 0;
  for (let index = 1; index < count - 1; index += 1) {
    const previous = slopes[index - 1] ?? 0;
    const next = slopes[index] ?? 0;
    tangents[index] = previous * next <= 0 ? 0 : (previous + next) / 2;
  }

  for (let index = 0; index < count - 1; index += 1) {
    const slope = slopes[index] ?? 0;
    if (slope === 0) {
      tangents[index] = 0;
      tangents[index + 1] = 0;
      continue;
    }
    const a = (tangents[index] ?? 0) / slope;
    const b = (tangents[index + 1] ?? 0) / slope;
    const magnitude = a * a + b * b;
    if (magnitude > 9) {
      const scale = 3 / Math.sqrt(magnitude);
      tangents[index] = scale * a * slope;
      tangents[index + 1] = scale * b * slope;
    }
  }

  return tangents;
}

export interface CurveSegment {
  from: Point;
  c1: Point;
  c2: Point;
  to: Point;
}

export function smoothCurve(points: readonly Point[]): readonly CurveSegment[] {
  if (points.length < 2) return [];
  const tangents = monotoneTangents(points);
  const segments: CurveSegment[] = [];
  for (let index = 0; index < points.length - 1; index += 1) {
    const from = points[index];
    const to = points[index + 1];
    if (from === undefined || to === undefined) continue;
    const dx = to.x - from.x;
    segments.push({
      from,
      c1: { x: from.x + dx / 3, y: from.y + ((tangents[index] ?? 0) * dx) / 3 },
      c2: { x: to.x - dx / 3, y: to.y - ((tangents[index + 1] ?? 0) * dx) / 3 },
      to
    });
  }
  return segments;
}

export function curvePath(segments: readonly CurveSegment[]): string {
  const first = segments[0];
  if (first === undefined) return '';
  let path = `M${first.from.x.toFixed(2)},${first.from.y.toFixed(2)}`;
  for (const segment of segments) {
    path += ` C${segment.c1.x.toFixed(2)},${segment.c1.y.toFixed(2)} ${segment.c2.x.toFixed(2)},${segment.c2.y.toFixed(2)} ${segment.to.x.toFixed(2)},${segment.to.y.toFixed(2)}`;
  }
  return path;
}

export function areaPath(line: string): string {
  return line === '' ? '' : `${line} L${VIEW_WIDTH},${VIEW_HEIGHT} L0,${VIEW_HEIGHT} Z`;
}

export function niceScale(peak: number, count: number): { max: number; ticks: readonly number[] } {
  if (peak <= 0) return { max: 0, ticks: [0] };
  const rawStep = peak / count;
  const magnitude = 10 ** Math.floor(Math.log10(rawStep));
  const normalized = rawStep / magnitude;
  const step = (normalized > 5 ? 10 : normalized > 2 ? 5 : normalized > 1 ? 2 : 1) * magnitude;
  const max = Math.ceil(peak / step) * step;
  const ticks: number[] = [];
  for (let value = 0; value <= max + step * 1e-6; value += step) ticks.push(value);
  return { max, ticks };
}

export function valueToY(value: number, max: number): number {
  return max === 0 ? VIEW_HEIGHT : VIEW_HEIGHT - (value / max) * (VIEW_HEIGHT - PLOT_TOP);
}

/** Layered series each start at zero, so the peak is the tallest single value. */
export function chartScale(values: readonly number[]): { max: number; ticks: readonly number[] } {
  const peak = values.reduce((max, value) => Math.max(max, value), 0);
  return niceScale(peak, TICK_COUNT);
}
