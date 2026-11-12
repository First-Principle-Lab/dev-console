<script lang="ts">
  /* The CLI Proxy tab: a replication of T3 Code's Usage page fed by the same
   * Codex backend the CLIProxyAPI stores a token for. Segmented metric tabs
   * (Cost / Tokens / Limits), period tabs (Past 24h / 7 / 30 / 90 days), a
   * refresh button, the overview (figure, provider row, chart, totals, share
   * bars, breakdown) and the limit-window cards. */
  import Icon from '../Icon.svelte';
  import UsageChart from './UsageChart.svelte';
  import UsageLimitsView from './UsageLimitsView.svelte';
  import { api } from '../../lib/api';
  import {
    enumerateDays,
    enumerateHourStarts,
    fillPeriods,
    formatCount,
    formatDayShort,
    formatHourShort,
    formatPercent,
    formatTokens,
    formatUsd,
    makeWindow,
    costTypeSegments,
    mergeUsage,
    tokenTypeSegments,
    type CliproxyUsage,
    type UsageMetric
  } from '../../lib/usage';

  const METRIC_OPTIONS: readonly { value: UsageMetric; label: string }[] = [
    { value: 'cost', label: 'Cost' },
    { value: 'tokens', label: 'Tokens' },
    { value: 'limits', label: 'Limits' }
  ];
  const WINDOW_OPTIONS: readonly { days: number; label: string }[] = [
    { days: 1, label: 'Past 24h' },
    { days: 7, label: '7 days' },
    { days: 30, label: '30 days' },
    { days: 90, label: '90 days' }
  ];

  let metric = $state<UsageMetric>('cost');
  let windowDays = $state(7);
  let breakdown = $state<'model' | 'time'>('model');
  let data = $state<CliproxyUsage | null>(null);
  let loading = $state(true);
  let refreshing = $state(false);
  let error = $state<string | null>(null);
  /** Limits are always current: read once per limits render, never ticking. */
  let limitsNow = $state(Date.now());

  const showingLimits = $derived(metric === 'limits');
  let requestId = 0;
  const usageWindow = $derived(makeWindow(windowDays));
  async function load(): Promise<void> {
    const id = ++requestId;
    refreshing = true;
    error = null;
    try {
      const since =
        usageWindow.sinceTime ??
        new Date(`${usageWindow.sinceDay}T00:00:00`).toISOString();
      const until = usageWindow.untilTime ?? new Date().toISOString();
      const result = await api.cliproxyUsage(since, until);
      if (id === requestId) data = result;
    } catch (cause: unknown) {
      if (id !== requestId) return;
      error = cause instanceof Error && cause.message ? cause.message : 'Could not load CLI Proxy usage.';
    } finally {
      if (id === requestId) { loading = false; refreshing = false; }
    }
  }

  $effect(() => {
    void windowDays;
    void metric;
    if (metric === 'limits') limitsNow = Date.now();
    void load();
  });

  function selectMetric(next: UsageMetric): void {
    metric = next;
  }

  function selectWindow(days: number): void {
    if (days === windowDays) return;
    windowDays = days;
  }

  function refresh(): void {
    if (refreshing) return;
    void load();
  }

  /** mergeUsage sees every record the backend returned; the headline numbers
   * must only count the displayed window, so re-filter by timestamp. While a
   * bigger window is still loading, the kept records show the matching slice. */
  function inWindow(ts: string): boolean {
    const at = Date.parse(ts);
    if (Number.isNaN(at)) return false;
    const since = Date.parse(usageWindow.sinceTime ?? new Date(`${usageWindow.sinceDay}T00:00:00`).toISOString());
    const until = Date.parse(usageWindow.untilTime ?? new Date().toISOString());
    return at >= since && at <= until;
  }

  const inWindowRecords = $derived((data?.records ?? []).filter((record) => inWindow(record.ts)));
  const windowMerged = $derived(mergeUsage(inWindowRecords));
  const showingHourly = $derived(usageWindow.resolution === 'hour');
  const periods = $derived(
    showingHourly
      ? enumerateHourStarts(usageWindow.sinceTime ?? '', usageWindow.untilTime ?? '')
      : enumerateDays(usageWindow.sinceDay, usageWindow.untilDay)
  );
  const filled = $derived(fillPeriods(periods, showingHourly ? windowMerged.hourly : windowMerged.daily));
  const breakdownPeriods = $derived(
    (showingHourly ? [...windowMerged.hourly] : [...windowMerged.daily]).slice().reverse()
  );
  const breakdownPeak = $derived(
    windowMerged.models.reduce(
      (peak, model) => Math.max(peak, metric === 'tokens' ? model.totalTokens : model.costUsd),
      0
    )
  );
  const unpricedShare = $derived(
    windowMerged.totals.totalTokens > 0
      ? windowMerged.totals.unpricedTokens / windowMerged.totals.totalTokens
      : 0
  );
  const figure = $derived(metric === 'tokens' ? formatTokens(windowMerged.totals.totalTokens) : formatUsd(windowMerged.totals.costUsd));
  const sessionLabel = $derived(
    `${formatCount(windowMerged.totals.sessions)} session${windowMerged.totals.sessions === 1 ? '' : 's'}`
  );
  const chartHeading = $derived(
    `${showingHourly ? 'Hourly' : 'Daily'} ${metric === 'tokens' ? 'processed tokens' : 'cost'}`
  );
  const chartTitle = $derived(
    (showingHourly
      ? `${formatHourShort(usageWindow.sinceTime ?? '')} to ${formatHourShort(usageWindow.untilTime ?? '')}`
      : `${formatDayShort(usageWindow.sinceDay)} to ${formatDayShort(usageWindow.untilDay)}`)
  );
  const shareHeading = $derived(metric === 'tokens' ? 'Tokens by type' : 'Cost by type');
  const shareFormat = $derived(metric === 'tokens' ? formatTokens : formatUsd);
  const shareSegments = $derived(
    metric === 'tokens'
      ? tokenTypeSegments(windowMerged.totals)
      : costTypeSegments(windowMerged.totals.categoryCost)
  );
  const visibleSegments = $derived(shareSegments.filter((segment) => segment.value > 0));
</script>

<div class="usage" data-testid="cliproxy-usage">
  <div class="toolbar">
    {#if !showingLimits}
      <div class="segmented" role="tablist" aria-label="Usage period">
        {#each WINDOW_OPTIONS as option (option.days)}
          <button
            type="button"
            role="tab"
            aria-selected={windowDays === option.days}
            class:active={windowDays === option.days}
            onclick={() => selectWindow(option.days)}
            data-testid={`cliproxy-window-${option.days}`}
          >{option.label}</button>
        {/each}
      </div>
    {/if}
    <div class="segmented" role="tablist" aria-label="Usage metric">
      {#each METRIC_OPTIONS as option (option.value)}
        <button
          type="button"
          role="tab"
          aria-selected={metric === option.value}
          class:active={metric === option.value}
          onclick={() => selectMetric(option.value)}
          data-testid={`cliproxy-metric-${option.value}`}
        >{option.label}</button>
      {/each}
    </div>
    <span class="spacer"></span>
    <button
      type="button"
      class="refresh"
      aria-label={showingLimits ? 'Refresh limits' : 'Refresh usage'}
      disabled={refreshing}
      onclick={refresh}
      data-testid="cliproxy-refresh"
    >
      <Icon name="update" size={14} />
    </button>
  </div>

  {#if loading}
    <p class="notice" data-testid="cliproxy-loading">Loading usage…</p>
  {:else if error}
    <p class="notice error" data-testid="cliproxy-error">{error}</p>
  {:else if showingLimits}
    <UsageLimitsView limits={data?.limits ?? null} error={data?.limitsError ?? null} now={limitsNow} />
  {:else}
    <div class="overview">
      <p class="sub dim">Local Codex transcripts · API-equivalent estimate, not a subscription bill. Limits cover the proxy account.</p>
      <section class="grid">
        <div class="left">
          <div class="figure-block">
            <span class="figure">{figure}</span>
            <span class="sub">
              {sessionLabel}{metric === 'cost' ? ' · API estimate' : ''}
            </span>
            {#if metric === 'cost' && unpricedShare > 0}
              <span class="sub dim">API estimate excludes {formatPercent(unpricedShare)} of tokens from unpriced models.</span>
            {/if}
          </div>
          <div class="provider">
            <div class="provider-line">
              <span class="provider-name">
                <span class="dot" aria-hidden="true"></span>
                Codex
                <span class="provider-sessions">{sessionLabel}</span>
              </span>
              <span class="provider-value">
                {metric === 'cost' ? formatUsd(windowMerged.totals.costUsd) : formatTokens(windowMerged.totals.totalTokens)}
              </span>
            </div>
            <span class="provider-meta">
              {metric === 'cost'
                ? `${formatPercent(1)} of cost · ${formatTokens(windowMerged.totals.totalTokens)} tokens`
                : `${formatPercent(1)} of tokens · ${formatUsd(windowMerged.totals.costUsd)}`}
            </span>
          </div>
        </div>
        <div class="right">
          <h2 class="heading">{chartHeading}</h2>
          <UsageChart
            periods={periods}
            totals={filled}
            metric={metric === 'tokens' ? 'tokens' : 'cost'}
            formatPeriod={(key) => (showingHourly ? formatHourShort(key) : formatDayShort(key))}
            formatTooltipPeriod={(key) => (showingHourly ? formatHourShort(key) : formatDayShort(key))}
            format={metric === 'tokens' ? formatTokens : formatUsd}
          />
          <span class="window-label">{chartTitle}</span>
        </div>
      </section>

      <section>
        <h2 class="heading">Totals</h2>
        <div class="totals">
          <div class="metric"><span class="metric-label">Processed tokens</span><span class="metric-value">{formatTokens(windowMerged.totals.totalTokens)}</span></div>
          <div class="metric"><span class="metric-label">Cached input</span><span class="metric-value">{formatTokens(windowMerged.totals.cachedInputTokens)}</span></div>
          <div class="metric"><span class="metric-label">Uncached input</span><span class="metric-value">{formatTokens(windowMerged.totals.uncachedInputTokens)}</span></div>
          <div class="metric"><span class="metric-label">Output</span><span class="metric-value">{formatTokens(windowMerged.totals.outputTokens)}</span></div>
          <div class="metric"><span class="metric-label">Cache savings</span><span class="metric-value">{formatUsd(windowMerged.totals.cacheSavingsUsd)}</span></div>
        </div>
      </section>

      {#if windowMerged.totals.totalTokens > 0}
        <section class="shares">
          <div class="share">
            <h3 class="heading">{shareHeading}</h3>
            <div
              class="share-bar"
              role="img"
              aria-label={`${shareHeading}: ${visibleSegments.map((segment) => `${segment.label} ${shareFormat(segment.value)}`).join(', ')}`}
            >
              {#each visibleSegments as segment (segment.label)}
                <div
                  class="segment"
                  style:flex={`${segment.value} 1 0`}
                  style:background={segment.color}
                  title={`${segment.label} · ${shareFormat(segment.value)}`}
                ></div>
              {/each}
            </div>
            <div class="legend">
              {#each visibleSegments as segment (segment.label)}
                <span class="legend-item">
                  <span class="swatch" style:background={segment.color}></span>
                  <span class="legend-label">{segment.label}</span>
                  <span class="legend-value">{shareFormat(segment.value)}</span>
                </span>
              {/each}
            </div>
          </div>
        </section>
      {/if}

      <section>
        <div class="breakdown-head">
          <h2 class="heading">Breakdown</h2>
          <div class="segmented" role="tablist" aria-label="Usage breakdown">
            <button
              type="button"
              role="tab"
              aria-selected={breakdown === 'model'}
              class:active={breakdown === 'model'}
              onclick={() => (breakdown = 'model')}
            >Model</button>
            <button
              type="button"
              role="tab"
              aria-selected={breakdown === 'time'}
              class:active={breakdown === 'time'}
              onclick={() => (breakdown = 'time')}
            >{showingHourly ? 'Hour' : 'Day'}</button>
          </div>
        </div>

        {#if breakdown === 'model'}
          <table class="table" data-testid="cliproxy-breakdown-model">
            <thead>
              <tr>
                <th class="num">#</th>
                <th class="wide">Model</th>
                <th class="num">Cost</th>
                <th class="num">Share</th>
                <th class="num">Tokens</th>
              </tr>
            </thead>
            <tbody>
              {#if windowMerged.models.length === 0}
                <tr><td colspan="5" class="empty">No activity in this window.</td></tr>
              {:else}
                {#each windowMerged.models as model, index (model.model)}
                  {@const value = metric === 'tokens' ? model.totalTokens : model.costUsd}
                  <tr>
                    <td class="num dim">{index + 1}</td>
                    <td class="wide">
                      <span class="model-name">{model.model}</span>
                      {#if value > 0 && breakdownPeak > 0}
                        <div class="model-bar" aria-hidden="true">
                          <div
                            class="model-bar-fill"
                            style:width={`max(8px, ${(value / breakdownPeak) * 100}%)`}
                          ></div>
                        </div>
                      {/if}
                    </td>
                    <td class="num">{model.unpriced && metric === 'cost' ? 'Unpriced' : formatUsd(model.costUsd)}</td>
                    <td class="num dim">
                      {formatPercent(metric === 'tokens' ? model.tokenShare : (model.costShare ?? 0))}
                    </td>
                    <td class="num dim">{formatTokens(model.totalTokens)}</td>
                  </tr>
                {/each}
              {/if}
            </tbody>
          </table>
        {:else}
          <table class="table" data-testid="cliproxy-breakdown-time">
            <thead>
              <tr>
                <th class="wide">{showingHourly ? 'Hour' : 'Day'}</th>
                <th class="num">Codex</th>
                <th class="num">Total</th>
                <th class="num">Tokens</th>
              </tr>
            </thead>
            <tbody>
              {#if breakdownPeriods.length === 0}
                <tr><td colspan="4" class="empty">No activity in this window.</td></tr>
              {:else}
                {#each breakdownPeriods as period (period.key)}
                  <tr>
                    <td class="wide">{showingHourly ? formatHourShort(period.key) : formatDayShort(period.key)}</td>
                    <td class="num dim">{formatUsd(period.costUsd)}</td>
                    <td class="num">{formatUsd(period.costUsd)}</td>
                    <td class="num dim">{formatTokens(period.totalTokens)}</td>
                  </tr>
                {/each}
              {/if}
            </tbody>
          </table>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .usage {
    display: flex;
    flex-direction: column;
    gap: var(--sp-6);
    padding: var(--sp-4) var(--sp-5) var(--sp-5);
    overflow-y: auto;
    height: 100%;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .spacer {
    flex: 1;
  }
  .segmented {
    display: inline-flex;
    border: var(--border-hair);
    border-radius: var(--radius);
    padding: 2px;
    gap: 2px;
    background: var(--bg-sunken);
  }
  .segmented button {
    min-height: 24px;
    padding: 0 var(--sp-3);
    font-size: var(--text-xs);
    background: transparent;
    border: 0;
    border-radius: var(--radius-sm);
    color: var(--fg-muted);
  }
  .segmented button:hover:not(:disabled) {
    background: var(--hover-fill);
    color: var(--fg);
    border: 0;
  }
  .segmented button.active {
    background: var(--selected);
    color: var(--fg);
  }
  .refresh {
    width: var(--control-h-sm);
    min-height: var(--control-h-sm);
    padding: 0;
  }
  .notice {
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .notice.error {
    color: var(--danger);
  }
  .overview {
    display: flex;
    flex-direction: column;
    gap: var(--sp-6);
  }
  .grid {
    display: grid;
    gap: var(--sp-6);
    grid-template-columns: minmax(0, 18rem) minmax(0, 1fr);
  }
  .left {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
    min-width: 0;
  }
  .figure-block {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
  }
  .figure {
    font-size: 34px;
    font-weight: 600;
    color: var(--fg);
    font-variant-numeric: tabular-nums;
    line-height: 1.15;
  }
  .sub {
    color: var(--fg-muted);
    font-size: var(--text-xs);
  }
  .sub.dim {
    color: var(--fg-dim);
  }
  .provider {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
  }
  .provider-line {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--sp-4);
  }
  .provider-name {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    font-size: var(--text-sm);
    color: var(--fg);
    min-width: 0;
  }
  .dot {
    width: 8px;
    height: 8px;
    flex-shrink: 0;
    border-radius: var(--radius-circle);
    background: var(--fg);
  }
  .provider-sessions {
    color: var(--fg-muted);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .provider-value {
    flex-shrink: 0;
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--fg);
    font-variant-numeric: tabular-nums;
  }
  .provider-meta {
    color: var(--fg-muted);
    font-size: var(--text-xs);
  }
  .right {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    min-width: 0;
  }
  .heading {
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--fg);
  }
  .window-label {
    color: var(--fg-dim);
    font-size: var(--text-xs);
  }
  .totals {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    column-gap: var(--sp-6);
    row-gap: var(--sp-4);
    padding-block: var(--sp-1);
  }
  .metric {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .metric-label {
    color: var(--fg-muted);
    font-size: var(--text-xs);
  }
  .metric-value {
    color: var(--fg);
    font-size: var(--text-lg);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .shares {
    display: grid;
    column-gap: var(--sp-6);
    row-gap: var(--sp-6);
    grid-template-columns: repeat(1, minmax(0, 1fr));
  }
  .share {
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
    min-width: 0;
  }
  .share-bar {
    display: flex;
    gap: 2px;
    height: 8px;
  }
  .segment {
    height: 100%;
    min-width: 4px;
    border-radius: 2px;
  }
  .segment:first-child {
    border-radius: var(--radius-pill) 2px 2px var(--radius-pill);
  }
  .segment:last-child {
    border-radius: 2px var(--radius-pill) var(--radius-pill) 2px;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    column-gap: var(--sp-4);
    row-gap: var(--sp-1);
    font-size: var(--text-xs);
  }
  .legend-item {
    display: flex;
    align-items: center;
    gap: var(--gap-tags);
  }
  .swatch {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .legend-label {
    color: var(--fg-muted);
  }
  .legend-value {
    color: var(--fg);
    font-variant-numeric: tabular-nums;
  }
  .breakdown-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
  }
  .table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }
  .table th {
    border-bottom: var(--border);
    color: var(--fg-muted);
    font-size: var(--text-xs);
    font-weight: 400;
    padding: var(--sp-2) 0;
    text-align: right;
  }
  .table th.wide,
  .table td.wide {
    text-align: left;
    width: 100%;
  }
  .table th.num {
    text-align: right;
    padding-left: var(--sp-6);
  }
  .table td {
    border-bottom: 1px solid var(--line-hair);
    padding: 10px 0;
    text-align: right;
    white-space: nowrap;
    color: var(--fg-muted);
    font-variant-numeric: tabular-nums;
  }
  .table td.num {
    padding-left: var(--sp-6);
  }
  .table td.dim {
    color: var(--fg-muted);
  }
  .table td.wide {
    white-space: normal;
  }
  .model-name {
    color: var(--fg);
  }
  .model-bar {
    margin-top: var(--gap-tags);
    height: 2px;
    max-width: 192px;
  }
  .model-bar-fill {
    height: 100%;
    border-radius: var(--radius-pill);
    background: var(--fg);
  }
  .empty {
    padding: var(--sp-6) 0;
    text-align: center;
    color: var(--fg-muted);
  }
  @media (max-width: 900px) {
    .grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media (min-width: 760px) {
    .totals {
      grid-template-columns: repeat(5, minmax(0, 1fr));
    }
    .shares {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
