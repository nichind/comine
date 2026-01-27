<script lang="ts">
  import { formatSpeed } from '$lib/utils/format';

  export type SpeedPoint = { t: number; bps: number };

  interface Props {
    points: SpeedPoint[];
    windowMs?: number;
    height?: number;
    showLabels?: boolean;
    variant?: 'card' | 'sparkline';
  }

  let {
    points,
    windowMs = 120_000,
    height = 120,
    showLabels = true,
    variant = 'card',
  }: Props = $props();

  let endT = $derived(points.length ? points[points.length - 1].t : Date.now());
  let startT = $derived(endT - windowMs);

  let rawWindowPoints = $derived(points.filter((p) => p.t >= startT));

  let windowPoints = $derived.by(() => {
    if (variant !== 'sparkline') return rawWindowPoints;
    if (rawWindowPoints.length <= 2) return rawWindowPoints;

    const alpha = 0.35; // EMA smoothing factor
    let ema = rawWindowPoints[0].bps;
    return rawWindowPoints.map((p, idx) => {
      if (idx === 0) return p;
      ema = alpha * p.bps + (1 - alpha) * ema;
      return { ...p, bps: ema };
    });
  });
  let maxBps = $derived.by(() => {
    let max = 0;
    for (const p of windowPoints) if (p.bps > max) max = p.bps;
    return Math.max(1, Math.ceil(max * 1.15));
  });

  let peakBps = $derived.by(() => {
    let max = 0;
    for (const p of windowPoints) if (p.bps > max) max = p.bps;
    return max;
  });

  let dLine = $derived.by(() => {
    if (windowPoints.length === 0) return '';

    const toX = (t: number) => ((t - startT) / windowMs) * 100;
    const toY = (bps: number) => 100 - (Math.max(0, bps) / maxBps) * 100;

    let d = '';
    for (let i = 0; i < windowPoints.length; i++) {
      const p = windowPoints[i];
      const x = toX(p.t);
      const y = toY(p.bps);
      d += i === 0 ? `M ${x.toFixed(3)} ${y.toFixed(3)}` : ` L ${x.toFixed(3)} ${y.toFixed(3)}`;
    }
    return d;
  });

  let dArea = $derived.by(() => {
    if (!dLine) return '';
    return `${dLine} L 100 100 L 0 100 Z`;
  });

  let currentBps = $derived(windowPoints.length ? windowPoints[windowPoints.length - 1].bps : 0);

  let lastPoint = $derived(windowPoints.length ? windowPoints[windowPoints.length - 1] : undefined);
  let lastX = $derived(lastPoint ? ((lastPoint.t - startT) / windowMs) * 100 : 100);
  let lastY = $derived(lastPoint ? 100 - (Math.max(0, lastPoint.bps) / maxBps) * 100 : 100);

  let isIdle = $derived(variant === 'sparkline' && windowPoints.length > 2 && currentBps < 1024);
</script>

<div
  class="graph"
  class:sparkline={variant === 'sparkline'}
  style="--h: {height}px"
  title={variant === 'sparkline'
    ? `Now: ${formatSpeed(currentBps)} • Peak: ${formatSpeed(peakBps)}`
    : ''}
>
  <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
    {#if variant !== 'sparkline'}
      <defs>
        <linearGradient id="speedFill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color="var(--accent, rgba(80, 160, 255, 1))" stop-opacity="0.35" />
          <stop offset="100%" stop-color="var(--accent, rgba(80, 160, 255, 1))" stop-opacity="0" />
        </linearGradient>
      </defs>

      <path class="grid" d="M 0 80 L 100 80" />
      <path class="grid" d="M 0 60 L 100 60" />
      <path class="grid" d="M 0 40 L 100 40" />
    {/if}

    {#if variant !== 'sparkline'}
      {#if dArea}
        <path class="area" d={dArea} />
      {/if}
    {/if}

    {#if dLine}
      <path
        class="line"
        d={dLine}
        vector-effect={variant === 'sparkline' ? 'non-scaling-stroke' : undefined}
      />
    {:else}
      <path
        class="line"
        d="M 0 100 L 100 100"
        vector-effect={variant === 'sparkline' ? 'non-scaling-stroke' : undefined}
      />
    {/if}

    {#if variant === 'sparkline' && windowPoints.length}
      <circle class="now-dot" class:idle={isIdle} cx={lastX} cy={lastY} r="1.6" />
    {/if}
  </svg>

  {#if showLabels && variant !== 'sparkline'}
    <div class="labels">
      <span class="label max">{formatSpeed(maxBps)}</span>
      <span class="label now">{formatSpeed(currentBps)}</span>
      <span class="label zero">0</span>
    </div>
  {/if}
</div>

<style>
  .graph {
    position: relative;
    height: var(--h);
    width: 100%;
    border-radius: var(--radius, 10px);
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--surface-border, rgba(255, 255, 255, 0.08));
    overflow: hidden;
  }

  .graph.sparkline {
    border-radius: 0;
    background: transparent;
    border: 0;
    overflow: visible;
  }

  svg {
    width: 100%;
    height: 100%;
    display: block;
  }

  .grid {
    fill: none;
    stroke: rgba(255, 255, 255, 0.06);
    stroke-width: 0.8;
  }

  .area {
    fill: url(#speedFill);
  }

  .line {
    fill: none;
    stroke: var(--accent, rgba(80, 160, 255, 1));
    stroke-width: 1.8;
    stroke-linejoin: round;
    stroke-linecap: round;
    filter: drop-shadow(0 6px 10px rgba(0, 0, 0, 0.35));
    shape-rendering: geometricPrecision;
  }

  .graph.sparkline .grid,
  .graph.sparkline .area {
    display: none;
  }

  .graph.sparkline .line {
    stroke-width: 1px;
    filter: drop-shadow(0 2px 6px rgba(0, 0, 0, 0.25));
    opacity: 0.78;
  }

  .now-dot {
    fill: var(--accent, rgba(80, 160, 255, 1));
    opacity: 0.9;
  }

  .now-dot.idle {
    fill: rgba(255, 195, 90, 0.95);
    opacity: 0.95;
  }

  .labels {
    position: absolute;
    inset: 8px;
    pointer-events: none;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.55);
  }

  .label.now {
    align-self: flex-end;
    color: rgba(255, 255, 255, 0.8);
  }

  .label.max {
    color: rgba(255, 255, 255, 0.55);
  }

  .label.zero {
    color: rgba(255, 255, 255, 0.35);
  }
</style>
