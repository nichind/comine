import { invoke } from '@tauri-apps/api/core';

import { LRUCache } from '$lib/utils/LRUCache';

export interface RGB {
  r: number;
  g: number;
  b: number;
}

const COLOR_CACHE_MAX_SIZE = 500;
const colorCache = new LRUCache<string, RGB>(COLOR_CACHE_MAX_SIZE);

let sharedCanvas: HTMLCanvasElement | null = null;
let sharedCtx: CanvasRenderingContext2D | null = null;
let canvasIdleTimer: ReturnType<typeof setTimeout> | null = null;
const CANVAS_IDLE_TIMEOUT = 30000;
const SAMPLE_SIZE = 50;

function normalizeThumbnailUrlForCache(url: string): string {
  if (!url) return url;

  try {
    const parsed = new URL(url);
    const host = parsed.hostname.toLowerCase();
    if (host.includes('ytimg.com')) {
      const match = parsed.pathname.match(/\/vi(?:_webp)?\/([^/]+)\//);
      if (match) {
        return `yt:${match[1]}`;
      }
    }
    return url;
  } catch {
    return url;
  }
}

function releaseCanvas(): void {
  if (sharedCanvas) {
    sharedCtx = null;
    sharedCanvas.width = 0;
    sharedCanvas.height = 0;
    sharedCanvas = null;
  }
}

function scheduleCanvasRelease(): void {
  if (canvasIdleTimer) clearTimeout(canvasIdleTimer);
  canvasIdleTimer = setTimeout(releaseCanvas, CANVAS_IDLE_TIMEOUT);
}

function getSharedCanvas(): { canvas: HTMLCanvasElement; ctx: CanvasRenderingContext2D } | null {
  if (canvasIdleTimer) clearTimeout(canvasIdleTimer);

  if (!sharedCanvas) {
    sharedCanvas = document.createElement('canvas');
    sharedCanvas.width = SAMPLE_SIZE;
    sharedCanvas.height = SAMPLE_SIZE;
    sharedCtx = sharedCanvas.getContext('2d', { willReadFrequently: true });
  }
  if (!sharedCtx) return null;

  scheduleCanvasRelease();
  return { canvas: sharedCanvas, ctx: sharedCtx };
}

function setCacheEntry(key: string, value: RGB): void {
  const normalizedKey = normalizeThumbnailUrlForCache(key);
  colorCache.set(normalizedKey, value);
}

function getCacheEntry(key: string): RGB | undefined {
  const normalizedKey = normalizeThumbnailUrlForCache(key);
  return colorCache.get(normalizedKey);
}

export async function extractDominantColor(imageUrl: string): Promise<RGB | null> {
  const cached = getCacheEntry(imageUrl);
  if (cached) return cached;

  try {
    const rustCached = await invoke<[number, number, number] | null>('get_cached_thumbnail_color', {
      url: imageUrl,
    });
    if (rustCached) {
      const color: RGB = { r: rustCached[0], g: rustCached[1], b: rustCached[2] };
      setCacheEntry(imageUrl, color);
      return color;
    }
  } catch {}

  // Local file paths (e.g. generated thumbnails from downloaded files).
  // Canvas extraction can fail/taint in some cases, so prefer the backend.
  const isLocalPath = /^[A-Z]:\\/i.test(imageUrl) || imageUrl.startsWith('/');
  if (isLocalPath) {
    try {
      const rustColor = await invoke<[number, number, number]>('extract_local_thumbnail_color', {
        path: imageUrl,
      });
      const color: RGB = { r: rustColor[0], g: rustColor[1], b: rustColor[2] };
      setCacheEntry(imageUrl, color);
      return color;
    } catch {
      // Fall back to canvas below.
    }
  }

  const isYouTubeThumbnail =
    imageUrl.includes('ytimg.com') ||
    imageUrl.includes('ggpht.com') ||
    imageUrl.includes('googleusercontent.com');

  if (isYouTubeThumbnail) {
    try {
      const rustColor = await invoke<[number, number, number]>('extract_thumbnail_color', {
        url: imageUrl,
      });
      if (rustColor) {
        const color: RGB = { r: rustColor[0], g: rustColor[1], b: rustColor[2] };
        setCacheEntry(imageUrl, color);
        return color;
      }
    } catch {}
    return null;
  }

  return new Promise((resolve) => {
    const img = new Image();
    img.crossOrigin = 'anonymous';

    const timeout = setTimeout(() => {
      img.onload = null;
      img.onerror = null;
      img.src = '';
      resolve(null);
    }, 5000);

    img.onload = () => {
      clearTimeout(timeout);
      try {
        const result = extractFromImage(img);
        img.onload = null;
        img.onerror = null;
        img.src = '';
        if (result) {
          setCacheEntry(imageUrl, result);
        }
        resolve(result);
      } catch {
        resolve(null);
      }
    };

    img.onerror = () => {
      clearTimeout(timeout);
      img.onload = null;
      img.onerror = null;
      img.src = '';
      resolve(null);
    };

    img.src = imageUrl;
  });
}

async function extractFromBlobUrl(blobUrl: string): Promise<RGB | null> {
  return new Promise((resolve) => {
    const img = new Image();

    img.onload = () => {
      const result = extractFromImage(img);
      img.onload = null;
      img.onerror = null;
      resolve(result);
    };

    img.onerror = () => {
      img.onload = null;
      img.onerror = null;
      resolve(null);
    };

    img.src = blobUrl;
  });
}

function extractFromImage(img: HTMLImageElement): RGB | null {
  try {
    const shared = getSharedCanvas();
    if (!shared) {
      return null;
    }
    const { ctx } = shared;

    // Clear and draw
    ctx.clearRect(0, 0, SAMPLE_SIZE, SAMPLE_SIZE);
    ctx.drawImage(img, 0, 0, SAMPLE_SIZE, SAMPLE_SIZE);

    const imageData = ctx.getImageData(0, 0, SAMPLE_SIZE, SAMPLE_SIZE);
    const pixels = imageData.data;

    let bestColor: RGB = { r: 99, g: 102, b: 241 };
    let bestScore = 0;

    for (let i = 0; i < pixels.length; i += 16) {
      const r = pixels[i];
      const g = pixels[i + 1];
      const b = pixels[i + 2];
      const a = pixels[i + 3];

      if (a < 128) continue;

      const max = Math.max(r, g, b);
      const min = Math.min(r, g, b);
      const lightness = (max + min) / 2 / 255;
      const saturation = max === min ? 0 : (max - min) / (1 - Math.abs(2 * lightness - 1)) / 255;

      const lightnessScore = 1 - Math.abs(lightness - 0.5) * 2;
      const score = saturation * lightnessScore * (1 - Math.abs(lightness - 0.4));

      if (score > bestScore && saturation > 0.2) {
        bestScore = score;
        bestColor = { r, g, b };
      }
    }

    if (bestScore < 0.1) {
      for (let i = 0; i < pixels.length; i += 16) {
        const r = pixels[i];
        const g = pixels[i + 1];
        const b = pixels[i + 2];
        const a = pixels[i + 3];

        if (a < 128) continue;

        const max = Math.max(r, g, b);
        const min = Math.min(r, g, b);
        if (max - min > 30) {
          bestColor = { r, g, b };
          break;
        }
      }
    }

    return boostSaturation(bestColor, 1.2);
  } catch {
    return null;
  }
}

function boostSaturation(color: RGB, factor: number): RGB {
  const r = color.r / 255;
  const g = color.g / 255;
  const b = color.b / 255;

  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  let h = 0;
  let s = 0;
  const l = (max + min) / 2;

  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);

    switch (max) {
      case r:
        h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
        break;
      case g:
        h = ((b - r) / d + 2) / 6;
        break;
      case b:
        h = ((r - g) / d + 4) / 6;
        break;
    }
  }

  s = Math.min(1, s * factor);

  if (s === 0) {
    const gray = Math.round(l * 255);
    return { r: gray, g: gray, b: gray };
  }

  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;

  const hue2rgb = (p: number, q: number, t: number) => {
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };

  return {
    r: Math.round(hue2rgb(p, q, h + 1 / 3) * 255),
    g: Math.round(hue2rgb(p, q, h) * 255),
    b: Math.round(hue2rgb(p, q, h - 1 / 3) * 255),
  };
}

export function rgbToRgba(color: RGB, alpha: number): string {
  return `rgba(${color.r}, ${color.g}, ${color.b}, ${alpha})`;
}

export function generateColorVars(color: RGB): string {
  return `--item-color: ${rgbToRgba(color, 1)}; --item-color-alpha: ${rgbToRgba(color, 0.25)}; --item-color-alpha-light: ${rgbToRgba(color, 0.15)}; --item-color-alpha-lighter: ${rgbToRgba(color, 0.08)}; --item-color-hover: ${rgbToRgba(color, 0.12)};`;
}

export function clearColorCache(): void {
  colorCache.clear();
  if (canvasIdleTimer) {
    clearTimeout(canvasIdleTimer);
    canvasIdleTimer = null;
  }
  releaseCanvas();
}

export function removeFromColorCache(url: string): void {
  const normalizedKey = normalizeThumbnailUrlForCache(url);
  colorCache.delete(normalizedKey);
}

export function setColorInCache(url: string, color: RGB): void {
  setCacheEntry(url, color);
}

export function getCachedColor(url: string): RGB | undefined {
  return getCacheEntry(url);
}

export async function getCachedColorAsync(url: string): Promise<RGB | null> {
  const jsCached = getCacheEntry(url);
  if (jsCached) return jsCached;

  try {
    const rustCached = await invoke<[number, number, number] | null>('get_cached_thumbnail_color', {
      url,
    });
    if (rustCached) {
      const color: RGB = { r: rustCached[0], g: rustCached[1], b: rustCached[2] };
      setCacheEntry(url, color);
      return color;
    }
  } catch {
    /* not available */
  }

  return null;
}

export function getThumbnailCacheKey(url: string): string {
  return normalizeThumbnailUrlForCache(url);
}
