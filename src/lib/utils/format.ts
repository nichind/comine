import { get } from 'svelte/store';
import { settings } from '$lib/stores/settings';

let cachedSizeUnit: 'binary' | 'decimal' = 'decimal';
settings.subscribe((s) => {
  cachedSizeUnit = s.sizeUnit ?? 'decimal';
});

export function formatTime(
  seconds: number,
  options?: { forceHours?: boolean; showMs?: boolean }
): string {
  return formatDuration(seconds, {
    invalidFallback: '0:00',
    zeroFallback: '0:00',
    forceHours: options?.forceHours,
  });
}

export function parseTimeString(input: string): number | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  if (/^\d+(\.\d+)?$/.test(trimmed)) {
    return parseFloat(trimmed);
  }

  const parts = trimmed.split(':').map((p) => parseFloat(p.trim()));
  if (parts.some((p) => isNaN(p))) return null;

  if (parts.length === 2) {
    return parts[0] * 60 + parts[1];
  } else if (parts.length === 3) {
    return parts[0] * 3600 + parts[1] * 60 + parts[2];
  }
  return null;
}

export function formatSize(bytes: number): string {
  const base = cachedSizeUnit === 'binary' ? 1024 : 1000;
  const units =
    cachedSizeUnit === 'binary' ? ['B', 'KiB', 'MiB', 'GiB', 'TiB'] : ['B', 'kB', 'MB', 'GB', 'TB'];

  if (bytes === 0) return '0 B';

  const i = Math.floor(Math.log(bytes) / Math.log(base));
  const size = bytes / Math.pow(base, i);

  const decimals = i === 0 ? 0 : size < 10 ? 2 : 1;

  return `${size.toFixed(decimals)} ${units[i]}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  const base = cachedSizeUnit === 'binary' ? 1024 : 1000;
  const units =
    cachedSizeUnit === 'binary'
      ? ['B/s', 'KiB/s', 'MiB/s', 'GiB/s']
      : ['B/s', 'kB/s', 'MB/s', 'GB/s'];

  if (bytesPerSecond === 0) return '0 B/s';

  const i = Math.floor(Math.log(bytesPerSecond) / Math.log(base));
  const speed = bytesPerSecond / Math.pow(base, i);

  const decimals = speed < 10 ? 2 : 1;

  return `${speed.toFixed(decimals)} ${units[Math.min(i, units.length - 1)]}`;
}

export function formatDuration(
  seconds: number | null | undefined,
  options?: { invalidFallback?: string; zeroFallback?: string; forceHours?: boolean }
): string {
  const invalidFallback = options?.invalidFallback ?? '--:--';
  const zeroFallback = options?.zeroFallback ?? 'Live';

  if (seconds === null || seconds === undefined) return invalidFallback;
  if (typeof seconds !== 'number' || !Number.isFinite(seconds)) return invalidFallback;
  if (seconds <= 0) return zeroFallback;

  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);

  if (options?.forceHours || h > 0) {
    return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  }
  return `${m}:${s.toString().padStart(2, '0')}`;
}
