/**
 * Download speed tracking — driven by job-event progress updates,
 * no polling/intervals. Speed points are pushed from eventHandler.ts.
 */
import { derived, writable } from 'svelte/store';

export type SpeedPoint = { t: number; bps: number };

const MAX_POINTS = 240;
const WINDOW_MS = 120_000;

const points = writable<SpeedPoint[]>([]);

/** Called from eventHandler.ts on every progress event */
export function pushSpeedPoint(bps: number): void {
  const now = Date.now();
  points.update((arr) => {
    const cutoff = now - WINDOW_MS;
    // Filter stale + append in one pass
    const next: SpeedPoint[] = [];
    for (const p of arr) {
      if (p.t >= cutoff) next.push(p);
    }
    next.push({ t: now, bps: Math.max(0, bps) });
    if (next.length > MAX_POINTS) next.splice(0, next.length - MAX_POINTS);
    return next;
  });
}

/** Called when downloads stop to reset the graph */
export function clearSpeedPoints(): void {
  points.set([]);
}

export const downloadSpeedPoints = { subscribe: points.subscribe };

export const downloadSpeedNow = derived(points, ($p) => ($p.length ? $p[$p.length - 1].bps : 0));
