import { invoke } from '@tauri-apps/api/core';
import { LRUCache } from '$lib/utils/LRUCache';

export interface RGB {
  r: number;
  g: number;
  b: number;
}

// LRU-bounded session memo — avoids repeat IPC for colors already seen this session.
const colorMemo = new LRUCache<string, RGB>(500);

function normalizeKey(url: string): string {
  if (!url) return url;
  try {
    const parsed = new URL(url);
    if (parsed.hostname.toLowerCase().includes('ytimg.com')) {
      const match = parsed.pathname.match(/\/vi(?:_webp)?\/([^/]+)\//);
      if (match) return `yt:${match[1]}`;
    }
  } catch {}
  return url;
}

function tupleToRgb(arr: [number, number, number]): RGB {
  return { r: arr[0], g: arr[1], b: arr[2] };
}

export async function extractDominantColor(imageUrl: string): Promise<RGB | null> {
  const key = normalizeKey(imageUrl);
  const memo = colorMemo.get(key);
  if (memo) return memo;

  try {
    const rustCached = await invoke<[number, number, number] | null>('get_cached_thumbnail_color', {
      url: imageUrl,
    });
    if (rustCached) {
      const color = tupleToRgb(rustCached);
      colorMemo.set(key, color);
      return color;
    }
  } catch {}

  const isLocalPath = /^[A-Z]:\\/i.test(imageUrl) || imageUrl.startsWith('/');

  try {
    const cmd = isLocalPath ? 'extract_local_thumbnail_color' : 'extract_thumbnail_color';
    const param = isLocalPath ? { path: imageUrl } : { url: imageUrl };
    const rustColor = await invoke<[number, number, number]>(cmd, param);
    const color = tupleToRgb(rustColor);
    colorMemo.set(key, color);
    return color;
  } catch {
    return null;
  }
}

export function rgbToRgba(color: RGB, alpha: number): string {
  return `rgba(${color.r}, ${color.g}, ${color.b}, ${alpha})`;
}

export function generateColorVars(color: RGB): string {
  return `--item-color: ${rgbToRgba(color, 1)}; --item-color-alpha: ${rgbToRgba(color, 0.25)}; --item-color-alpha-light: ${rgbToRgba(color, 0.15)}; --item-color-alpha-lighter: ${rgbToRgba(color, 0.08)}; --item-color-hover: ${rgbToRgba(color, 0.12)};`;
}

export function clearColorCache(): void {
  colorMemo.clear();
}

export function setColorInCache(url: string, color: RGB): void {
  colorMemo.set(normalizeKey(url), color);
}

export function getCachedColor(url: string): RGB | undefined {
  return colorMemo.get(normalizeKey(url));
}

export async function getCachedColorAsync(url: string): Promise<RGB | null> {
  return extractDominantColor(url).catch(() => null);
}

export function hslToHex(h: number, s: number, l: number): string {
  const hue2rgb = (p: number, q: number, t: number) => {
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const r = Math.round(hue2rgb(p, q, h + 1 / 3) * 255);
  const g = Math.round(hue2rgb(p, q, h) * 255);
  const b = Math.round(hue2rgb(p, q, h - 1 / 3) * 255);
  return `#${((1 << 24) | (r << 16) | (g << 8) | b).toString(16).slice(1)}`;
}

export function adjustBrightnessHex(hex: string, percent: number): string {
  const num = parseInt(hex.replace('#', ''), 16);
  const amt = Math.round(2.55 * percent);
  const r = Math.max(0, Math.min(255, (num >> 16) + amt));
  const g = Math.max(0, Math.min(255, ((num >> 8) & 0x00ff) + amt));
  const b = Math.max(0, Math.min(255, (num & 0x0000ff) + amt));
  return `#${((1 << 24) | (r << 16) | (g << 8) | b).toString(16).slice(1)}`;
}

export function hexToRgba(hex: string, alpha: number): string {
  const num = parseInt(hex.replace('#', ''), 16);
  return `rgba(${(num >> 16) & 0xff}, ${(num >> 8) & 0xff}, ${num & 0xff}, ${alpha})`;
}
