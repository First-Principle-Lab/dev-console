<script lang="ts">
  /* The Limits view: one section for Codex, one card per rate-limit window,
   * matching T3's pooled limit cards — a label column, a big remaining
   * percentage with a pace icon, and a rounded bar with the pace tick cut out. */
  import {
    PACE_LABEL,
    formatDuration,
    formatResetsIn,
    paceOf,
    paceTickPercent,
    remainingPercent,
    type CliproxyLimits,
    type LimitWindow
  } from '../../lib/usage';

  let { limits, error, now }: { limits: CliproxyLimits | null; error: string | null; now: number } = $props();

  const windows = $derived(limits?.windows ?? []);

  function paceMark(timeLeft: number | null): { left: string; gap: string } | null {
    if (timeLeft === null) return null;
    const at = `clamp(3px, ${timeLeft}%, calc(100% - 3px))`;
    const left = `calc(${at} - 3px)`;
    const right = `calc(${at} + 3px)`;
    const near = '-1px';
    const far = 'calc(100% + 1px)';
    return {
      left: at,
      gap: `polygon(${near} ${near}, ${left} ${near}, ${left} ${far}, ${right} ${far}, ${right} ${near}, ${far} ${near}, ${far} ${far}, ${near} ${far})`
    };
  }

  function summary(window: LimitWindow): string {
    const remaining = remainingPercent(window);
    const timeLeft = paceTickPercent(window, now);
    const resetsIn = formatResetsIn(window, now);
    return `${window.label}: ${remaining}% left${
      timeLeft === null ? '' : `, ${timeLeft}% of the window left`
    }${resetsIn ? `, ${resetsIn}` : ''}`;
  }

  function resetsChip(window: LimitWindow): string {
    return formatResetsIn(window, now)?.replace('resets in ', '↻ ') ?? '';
  }

  const credits = $derived(limits?.resetCredits ?? null);
  const creditsSummary = $derived.by(() => {
    if (credits === null || credits.availableCount <= 0) return '';
    let tail = '';
    if (credits.nextExpiresAt !== undefined) {
      const at = Date.parse(credits.nextExpiresAt);
      if (Number.isFinite(at)) tail = ` · next expires in ${formatDuration(at - now)}`;
    }
    return `${credits.availableCount} ${credits.availableCount === 1 ? 'reset credit' : 'reset credits'} banked${tail}`;
  });
</script>

<div class="limits" data-testid="cliproxy-limits">
  {#if error}
    <p class="notice">{error}</p>
  {:else if windows.length === 0}
    <p class="notice">No provider reports subscription limits.</p>
  {:else}
    <section class="pool">
      <h2 class="provider">
        <span class="dot" aria-hidden="true"></span>
        Codex
        {#if limits?.plan}
          <span class="plan">{limits.plan}</span>
        {/if}
      </h2>
      {#each windows as window (window.id)}
        {@const remaining = remainingPercent(window)}
        {@const timeLeft = paceTickPercent(window, now)}
        {@const mark = paceMark(timeLeft)}
        {@const pace = paceOf(window, now)}
        <div class="card">
          <div class="head">
            <span class="label">{window.label}</span>
            <span class="figure">
              <span class="big">{remaining}%</span>
              <span class="left">left</span>
              {#if pace}
                <span class="pace" role="img" aria-label={PACE_LABEL[pace]} title={PACE_LABEL[pace]}>
                  {pace === 'ahead' ? '▲' : pace === 'under' ? '▼' : '◆'}
                </span>
              {/if}
            </span>
          </div>
          <div class="bar-holder">
            <div
              class="bar"
              role="img"
              aria-label={summary(window)}
              style:clip-path={mark ? mark.gap : undefined}
            >
              <div class="track"></div>
              {#if remaining > 0}
                <div class="fill" style:width={`${remaining}%`}></div>
              {/if}
            </div>
            {#if mark}
              <span class="tick" style:left={mark.left}></span>
            {/if}
            <span class="bar-text">
              <span class="bar-name">{window.label}</span>
              <span class="bar-percent">{remaining}%</span>
              <span class="bar-resets">{resetsChip(window)}</span>
            </span>
          </div>
        </div>
      {/each}
      {#if creditsSummary}
        <p class="credits" data-testid="cliproxy-credits">{creditsSummary}</p>
      {/if}
    </section>
  {/if}
</div>

<style>
  .limits {
    display: flex;
    flex-direction: column;
    gap: var(--sp-6);
  }
  .notice {
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .pool {
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }
  .provider {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--fg);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-circle);
    background: var(--fg);
  }
  .plan {
    color: var(--fg-muted);
    font-weight: 400;
    text-transform: capitalize;
  }
  .card {
    display: grid;
    grid-template-columns: 11rem minmax(0, 1fr);
    align-items: center;
    column-gap: var(--sp-6);
    row-gap: var(--sp-3);
    border: var(--border-hair);
    border-radius: var(--radius-lg);
    padding: var(--sp-4);
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
  }
  .label {
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--fg);
  }
  .figure {
    display: inline-flex;
    align-items: baseline;
    gap: var(--sp-2);
  }
  .big {
    font-size: 30px;
    font-weight: 600;
    color: var(--fg);
    font-variant-numeric: tabular-nums;
    line-height: 1.1;
  }
  .left {
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .pace {
    color: var(--fg-muted);
    font-size: 10px;
  }
  .bar-holder {
    position: relative;
    height: 32px;
    min-width: 0;
  }
  .bar {
    position: absolute;
    inset: 0;
    cursor: default;
    border-radius: var(--radius);
  }
  .track {
    position: absolute;
    inset: 0;
    border-radius: var(--radius);
    background: var(--bg-active);
  }
  .fill {
    position: absolute;
    inset-block: 0;
    left: 0;
    border-radius: var(--radius);
    background: var(--fg);
    opacity: 0.35;
  }
  .tick {
    position: absolute;
    inset-block: 2px;
    width: 2px;
    transform: translateX(-50%);
    border-radius: var(--edge-w);
    background: var(--fg);
  }
  .bar-text {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    overflow: hidden;
    border-radius: var(--radius);
    padding: 0 var(--sp-2);
    font-size: var(--text-xs);
  }
  .bar-name {
    color: var(--fg);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar-percent {
    color: var(--fg);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .bar-resets {
    margin-left: auto;
    color: var(--fg);
    background: var(--bg-panel);
    border-radius: var(--radius-sm);
    padding: 2px var(--gap-tags);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .credits {
    color: var(--fg-muted);
    font-size: var(--text-xs);
  }
  @media (max-width: 720px) {
    .card {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
