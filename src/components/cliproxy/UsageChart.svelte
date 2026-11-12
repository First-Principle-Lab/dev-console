<script lang="ts">
  /* The single-provider area chart from T3's UsageProviderChart: a 960x260
   * plot, monotone cubic line, 12% area fill, nice tick gridlines, a hover
   * crosshair with a glass tooltip, and first/middle/last x labels. */
  import {
    PLOT_TOP,
    VIEW_HEIGHT,
    VIEW_WIDTH,
    areaPath,
    chartScale,
    curvePath,
    smoothCurve,
    valueToY,
    type PeriodTotals
  } from '../../lib/usage';

  let {
    periods,
    totals,
    metric,
    formatPeriod,
    formatTooltipPeriod,
    format
  }: {
    /** Period keys, oldest first. */
    periods: readonly string[];
    totals: readonly PeriodTotals[];
    metric: 'cost' | 'tokens';
    formatPeriod: (key: string) => string;
    formatTooltipPeriod: (key: string) => string;
    format: (value: number) => string;
  } = $props();

  let hoverIndex = $state<number | null>(null);
  let plot: HTMLElement | null = $state(null);
  let tooltip: HTMLElement | null = $state(null);
  let hoverPosition = $state<{ x: number; y: number } | null>(null);

  const values = $derived(totals.map((entry) => (metric === 'tokens' ? entry.totalTokens : entry.costUsd)));
  const scale = $derived(chartScale(values));
  const stepX = $derived(periods.length < 2 ? 0 : VIEW_WIDTH / (periods.length - 1));
  const points = $derived(
    values.map((value, index) => ({ x: index * stepX, y: valueToY(value, scale.max) }))
  );
  const line = $derived(curvePath(smoothCurve(points)));
  const area = $derived(areaPath(line));
  const yFor = (value: number) => valueToY(value, scale.max);

  function positionTooltip(): void {
    if (plot === null || tooltip === null || hoverPosition === null) return;
    const gap = 12;
    const tooltipWidth = tooltip.offsetWidth;
    const tooltipHeight = tooltip.offsetHeight;
    const plotWidth = plot.clientWidth;
    const plotHeight = plot.clientHeight;
    const preferredLeft =
      hoverPosition.x + gap + tooltipWidth <= plotWidth
        ? hoverPosition.x + gap
        : hoverPosition.x - gap - tooltipWidth;
    const preferredTop =
      hoverPosition.y + gap + tooltipHeight <= plotHeight
        ? hoverPosition.y + gap
        : hoverPosition.y - gap - tooltipHeight;
    const left = Math.min(Math.max(0, preferredLeft), Math.max(0, plotWidth - tooltipWidth));
    const top = Math.min(Math.max(0, preferredTop), Math.max(0, plotHeight - tooltipHeight));
    plot.style.setProperty('--usage-tooltip-left', `${left}px`);
    plot.style.setProperty('--usage-tooltip-top', `${top}px`);
  }

  $effect(() => {
    if (hoverIndex === null) return;
    positionTooltip();
  });

  function onMove(event: MouseEvent): void {
    if (plot === null || periods.length === 0) return;
    const bounds = plot.getBoundingClientRect();
    if (bounds.width === 0) return;
    const localX = Math.min(bounds.width, Math.max(0, event.clientX - bounds.left));
    const localY = Math.min(bounds.height, Math.max(0, event.clientY - bounds.top));
    const index = Math.round((localX / bounds.width) * (periods.length - 1));
    hoverPosition = { x: localX, y: localY };
    hoverIndex = Math.min(periods.length - 1, Math.max(0, index));
  }

  function onLeave(): void {
    hoverPosition = null;
    hoverIndex = null;
  }

  const hovered = $derived(hoverIndex === null ? undefined : totals[hoverIndex]);
  const hoveredKey = $derived(hoverIndex === null ? undefined : periods[hoverIndex]);
</script>

<div class="chart">
  <div class="row">
    <div class="y-axis" aria-hidden="true">
      {#each scale.ticks as tick (tick)}
        <span class="tick" style:top={`${(yFor(tick) / VIEW_HEIGHT) * 100}%`}>
          {tick === 0 ? '0' : format(tick)}
        </span>
      {/each}
    </div>
    <div
      class="plot"
      bind:this={plot}
      role="img"
      aria-label={`${metric === 'tokens' ? 'Daily processed tokens' : 'Daily cost'} by provider`}
      onmousemove={onMove}
      onmouseleave={onLeave}
    >
      <svg viewBox={`0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}`} preserveAspectRatio="none">
        {#each scale.ticks as tick (tick)}
          <line x1="0" x2={VIEW_WIDTH} y1={yFor(tick)} y2={yFor(tick)} class="gridline" vector-effect="non-scaling-stroke" />
        {/each}
        {#if area}
          <path d={area} class="series-area" />
        {/if}
        {#if line}
          <path d={line} class="series-line" vector-effect="non-scaling-stroke" />
        {/if}
        {#if hoverIndex !== null}
          <line
            x1={hoverIndex * stepX}
            x2={hoverIndex * stepX}
            y1={PLOT_TOP}
            y2={VIEW_HEIGHT}
            class="crosshair"
            vector-effect="non-scaling-stroke"
          />
        {/if}
      </svg>
      {#if hoverIndex !== null && hovered !== undefined && hoveredKey !== undefined}
        <div class="tooltip" bind:this={tooltip}>
          <div class="tooltip-label">{formatTooltipPeriod(hoveredKey)}</div>
          <div class="tooltip-total">
            <span>Total</span>
            <span>{format(metric === 'tokens' ? hovered.totalTokens : hovered.costUsd)}</span>
          </div>
        </div>
      {/if}
    </div>
  </div>
  <div class="x-axis">
    <span>{periods[0] === undefined ? '' : formatPeriod(periods[0])}</span>
    <span>{periods[Math.floor(periods.length / 2)] === undefined ? '' : formatPeriod(periods[Math.floor(periods.length / 2)])}</span>
    <span>{periods[periods.length - 1] === undefined ? '' : formatPeriod(periods[periods.length - 1])}</span>
  </div>
</div>

<style>
  .chart {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    min-width: 0;
  }
  .row {
    display: flex;
    gap: var(--sp-2);
  }
  .y-axis {
    position: relative;
    height: 224px;
    width: 56px;
    flex-shrink: 0;
  }
  .tick {
    position: absolute;
    right: 0;
    transform: translateY(-50%);
    font-size: 10px;
    color: var(--fg-dim);
    font-variant-numeric: tabular-nums;
  }
  .plot {
    position: relative;
    height: 224px;
    flex: 1;
    min-width: 0;
  }
  svg {
    height: 100%;
    width: 100%;
    display: block;
    color: var(--line);
  }
  .gridline {
    stroke: currentColor;
    stroke-width: 1;
  }
  .series-area {
    fill: var(--fg);
    fill-opacity: 0.12;
    stroke: none;
  }
  .series-line {
    fill: none;
    stroke: var(--fg);
    stroke-width: 2;
  }
  .crosshair {
    stroke: var(--fg-muted);
    stroke-width: 1;
  }
  .tooltip {
    pointer-events: none;
    position: absolute;
    z-index: 10;
    min-width: 9rem;
    max-width: 100%;
    border-radius: var(--radius-lg);
    border: var(--border-hair);
    background: var(--bg-raised);
    box-shadow: var(--shadow-raised);
    padding: var(--sp-2) 10px;
    font-size: var(--text-xs);
    left: var(--usage-tooltip-left, 0px);
    top: var(--usage-tooltip-top, 0px);
  }
  .tooltip-label {
    color: var(--fg-muted);
    margin-bottom: var(--sp-1);
  }
  .tooltip-total {
    margin-top: var(--sp-1);
    padding-top: var(--sp-1);
    border-top: var(--border-hair);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
  }
  .tooltip-total span:last-child {
    color: var(--fg);
    font-variant-numeric: tabular-nums;
  }
  .tooltip-total span:first-child {
    color: var(--fg-muted);
  }
  .x-axis {
    display: flex;
    justify-content: space-between;
    padding-left: 64px;
    font-size: 10px;
    color: var(--fg-dim);
    text-transform: uppercase;
  }
</style>
