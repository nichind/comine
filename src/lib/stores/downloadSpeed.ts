import { derived, get, writable } from 'svelte/store';
import { activeDownloads, queue, type QueueItem } from './queue';

export type SpeedPoint = { t: number; bps: number };

const SAMPLE_MS = 500;
const WINDOW_MS = 120_000;

function isTransferActive(status: QueueItem['status']): boolean {
  // Only statuses where we expect bytes to move (or just moved).
  return status === 'downloading' || status === 'processing' || status === 'converting';
}

function sumBackendSpeedBps(items: QueueItem[]): number {
  let sum = 0;
  for (const item of items) {
    const bps = item.speedBps;
    if (typeof bps === 'number' && Number.isFinite(bps) && bps > 0) sum += bps;
  }
  return sum;
}

export const isDownloadSpeedRunning = derived(queue, ($q) =>
  $q.items.some((i) => isTransferActive(i.status))
);

function createSpeedHistory() {
  const points = writable<SpeedPoint[]>([]);

  let timer: ReturnType<typeof setInterval> | null = null;
  let prevT = 0;
  let prevBytesById = new Map<string, number>();

  const reset = () => {
    points.set([]);
    prevT = 0;
    prevBytesById = new Map<string, number>();
  };

  const sample = () => {
    const now = Date.now();
    const items = get(activeDownloads).filter((i) => isTransferActive(i.status));

    // Prefer backend-reported speed when available (more stable); fallback to deltas.
    let bps = sumBackendSpeedBps(items);

    if (!(bps > 0)) {
      if (prevT === 0) {
        prevT = now;
        for (const it of items) prevBytesById.set(it.id, it.downloadedBytes ?? 0);
        bps = 0;
      } else {
        const dt = now - prevT;
        prevT = now;

        let deltaBytes = 0;
        for (const it of items) {
          const cur = it.downloadedBytes ?? 0;
          const prev = prevBytesById.get(it.id);
          if (typeof prev === 'number' && cur >= prev) deltaBytes += cur - prev;
          prevBytesById.set(it.id, cur);
        }

        bps = dt > 0 ? (deltaBytes / dt) * 1000 : 0;
      }
    }

    points.update((arr) => {
      const next = [...arr, { t: now, bps: Math.max(0, bps) }];
      const cutoff = now - WINDOW_MS;
      // Keep a bit of pre-roll for nicer left edge.
      return next.filter((p) => p.t >= cutoff - SAMPLE_MS * 2);
    });
  };

  const start = () => {
    if (timer) return;
    reset();
    sample();
    timer = setInterval(sample, SAMPLE_MS);
  };

  const stop = () => {
    if (timer) clearInterval(timer);
    timer = null;
    reset();
  };

  let prevRunning = false;
  const unsub = isDownloadSpeedRunning.subscribe((running) => {
    if (running && !prevRunning) start();
    if (!running && prevRunning) stop();
    prevRunning = running;
  });

  return {
    subscribe: points.subscribe,
    cleanup: () => {
      unsub();
      stop();
    },
  };
}

export const downloadSpeedPoints = createSpeedHistory();

export const downloadSpeedNow = derived(downloadSpeedPoints, ($p) =>
  $p.length ? $p[$p.length - 1].bps : 0
);

export const downloadSpeedMax = derived(downloadSpeedPoints, ($p) => {
  let max = 0;
  for (const pt of $p) if (pt.bps > max) max = pt.bps;
  return max;
});
