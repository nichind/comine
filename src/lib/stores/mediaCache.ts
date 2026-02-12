import { writable } from 'svelte/store';
import { load, type Store } from '@tauri-apps/plugin-store';
import { LRUCache } from '$lib/utils/LRUCache';
import { isYouTubeUrl } from '$lib/utils/urlUtils';
import { logs } from './logs';
import type { UrlInfo } from '$lib/bindings/UrlInfo';

const PERSISTENCE_CONFIG = {
  storeFile: 'media-cache.json',
  flushDebounceMs: 3000,
  maxDiskBytes: 2 * 1024 * 1024,
};

let sharedStore: Store | null = null;

async function getStore(): Promise<Store> {
  if (!sharedStore) {
    sharedStore = await load(PERSISTENCE_CONFIG.storeFile, { autoSave: false, defaults: {} });
  }
  return sharedStore;
}

export interface MediaPreview {
  title?: string;
  thumbnail?: string;
  author?: string;
  duration?: number;
  entryCount?: number;
  isPlaylist?: boolean;
  isMusic?: boolean;
}

export interface SponsorBlockSegment {
  category: string;
  segment: [number, number];
  UUID?: string;
  actionType?: string;
}

const CONFIG = {
  maxEntries: 30,
  ttl: {
    preview: 30 * 60 * 1000,
    resolveResult: 10 * 60 * 1000,
  },
  debug: false,
};

interface CacheEntry {
  url: string;
  type: 'video' | 'playlist' | 'channel' | 'unknown';
  preview: MediaPreview | null;
  resolveResult: UrlInfo | null;
  createdAt: number;
  lastAccessedAt: number;
  previewUpdatedAt: number | null;
  resolveResultUpdatedAt: number | null;
}

function normalizeUrl(url: string): string {
  if (!url) return '';
  try {
    const parsed = new URL(url);
    if (isYouTubeUrl(parsed.hostname)) {
      if (parsed.hostname.includes('youtu.be')) {
        const videoId = parsed.pathname.slice(1);
        const playlistId = parsed.searchParams.get('list');
        return playlistId ? `youtube:playlist:${playlistId}` : `youtube:video:${videoId}`;
      }
      const videoId = parsed.searchParams.get('v');
      const playlistId = parsed.searchParams.get('list');
      if (parsed.pathname.includes('/playlist') && playlistId) {
        return `youtube:playlist:${playlistId}`;
      }
      if (videoId) return `youtube:video:${videoId}`;
      if (parsed.pathname.includes('/@') || parsed.pathname.includes('/channel/')) {
        const channelPart = parsed.pathname.split('/').filter(Boolean).slice(0, 2).join('/');
        return `youtube:channel:${channelPart}`;
      }
    }
    parsed.hash = '';
    return parsed.toString();
  } catch {
    return url;
  }
}

function detectType(url: string): 'video' | 'playlist' | 'channel' | 'unknown' {
  const lower = url.toLowerCase();
  if (
    lower.includes('/@') ||
    lower.includes('/channel/') ||
    lower.includes('/c/') ||
    lower.includes('/user/')
  ) {
    return 'channel';
  }
  if (
    lower.includes('/playlist') ||
    (lower.includes('list=') && !lower.includes('v=')) ||
    lower.includes('/albums/') ||
    lower.includes('/sets/')
  ) {
    return 'playlist';
  }
  if (
    lower.includes('youtube.com/watch') ||
    lower.includes('youtu.be/') ||
    lower.includes('youtube.com/shorts/') ||
    lower.includes('music.youtube.com/watch')
  ) {
    return 'video';
  }
  return 'unknown';
}

function isExpired(timestamp: number | null, ttl: number): boolean {
  return !timestamp || Date.now() - timestamp > ttl;
}

interface DiskCacheData {
  entries: Array<[string, CacheEntry]>;
  savedAt: number;
}

class UnifiedMediaCache {
  private cache = new LRUCache<string, CacheEntry>(CONFIG.maxEntries);
  private store = writable(0);
  private version = 0;
  private dirty = false;
  private flushTimer: ReturnType<typeof setTimeout> | null = null;
  private flushPromise: Promise<void> | null = null;
  private loaded = false;
  private lastCleanup = Date.now();

  constructor() {
    this.cache.setMutationCallback(() => this.scheduleDiskFlush());
  }

  private scheduleDiskFlush(): void {
    this.dirty = true;
    if (this.flushTimer) return;
    this.flushTimer = setTimeout(() => {
      this.flushTimer = null;
      this.flushToDisk();
    }, PERSISTENCE_CONFIG.flushDebounceMs);
  }

  private async flushToDisk(): Promise<void> {
    if (!this.dirty) return;
    if (this.flushPromise) {
      await this.flushPromise;
      return;
    }
    this.flushPromise = this.doFlush();
    await this.flushPromise;
    this.flushPromise = null;
  }

  private async doFlush(): Promise<void> {
    try {
      const store = await getStore();
      const data: DiskCacheData = {
        entries: this.cache.toJSON(),
        savedAt: Date.now(),
      };
      const json = JSON.stringify(data);
      if (json.length > PERSISTENCE_CONFIG.maxDiskBytes) {
        const ratio = PERSISTENCE_CONFIG.maxDiskBytes / json.length;
        const keep = Math.max(1, Math.floor(data.entries.length * ratio * 0.9));
        data.entries = data.entries.slice(-keep);
      }
      await store.set('cache', data);
      await store.save();
      this.dirty = false;
      this.log('FLUSH_TO_DISK', `${data.entries.length} entries`);
    } catch (e) {
      console.error('[MediaCache] Disk flush failed:', e);
    }
  }

  private getOrCreate(url: string): CacheEntry {
    const key = normalizeUrl(url);
    let entry = this.cache.get(key);
    if (!entry) {
      entry = {
        url,
        type: detectType(url),
        preview: null,
        resolveResult: null,
        createdAt: Date.now(),
        lastAccessedAt: Date.now(),
        previewUpdatedAt: null,
        resolveResultUpdatedAt: null,
      };
      this.cache.set(key, entry);
    }
    return entry;
  }

  get(url: string): CacheEntry | null {
    if (!this.loaded) {
      this.load();
      this.loaded = true;
    }
    const now = Date.now();
    if (now - this.lastCleanup > 2 * 60 * 1000) {
      this.lastCleanup = now;
      this.clearStale();
    }
    const key = normalizeUrl(url);
    const entry = this.cache.get(key);
    if (!entry) {
      this.log('MISS', url);
      return null;
    }
    entry.lastAccessedAt = Date.now();
    this.log('HIT', url);
    return entry;
  }

  getPreview(url: string): MediaPreview | null {
    return this.get(url)?.preview ?? null;
  }

  setPreview(url: string, preview: MediaPreview): void {
    const entry = this.getOrCreate(url);
    entry.preview = { ...entry.preview, ...preview };
    entry.previewUpdatedAt = Date.now();
    entry.lastAccessedAt = Date.now();
    if (preview.isPlaylist !== undefined) {
      entry.type = preview.isPlaylist ? 'playlist' : 'video';
    }
    this.log('SET_PREVIEW', url, preview);
    this.notify();
  }

  setResolveResult(url: string, result: UrlInfo): void {
    const entry = this.getOrCreate(url);
    entry.resolveResult = result;
    entry.resolveResultUpdatedAt = Date.now();
    entry.lastAccessedAt = Date.now();
    entry.preview = {
      ...entry.preview,
      title: result.title ?? entry.preview?.title,
      thumbnail: result.thumbnail ?? entry.preview?.thumbnail,
      author: result.uploader ?? entry.preview?.author,
      duration: result.duration ? Number(result.duration) : entry.preview?.duration,
      isPlaylist: result.isPlaylist ?? entry.preview?.isPlaylist,
      entryCount: result.playlistCount ?? result.entries?.length ?? entry.preview?.entryCount,
    };
    entry.previewUpdatedAt = Date.now();
    entry.type = result.isPlaylist ? 'playlist' : 'video';
    this.log('SET_RESOLVE_RESULT', url);
    this.notify();
  }

  getResolveResult(url: string): UrlInfo | null {
    const key = normalizeUrl(url);
    const entry = this.cache.get(key);
    if (
      !entry?.resolveResult ||
      isExpired(entry.resolveResultUpdatedAt, CONFIG.ttl.resolveResult)
    ) {
      return null;
    }
    return entry.resolveResult;
  }

  getBestPreview(url: string): MediaPreview | null {
    const entry = this.get(url);
    if (!entry?.preview) return null;
    return entry.preview;
  }

  delete(url: string): void {
    const key = normalizeUrl(url);
    this.cache.delete(key);
    this.log('DELETE', url);
    this.notify();
  }

  clear(): void {
    this.cache.clear();
    this.log('CLEAR', 'all');
    this.notify();
  }

  clearStale(): void {
    const toDelete: string[] = [];
    for (const [key, entry] of this.cache.entries()) {
      const stale =
        isExpired(entry.previewUpdatedAt, CONFIG.ttl.preview) &&
        isExpired(entry.resolveResultUpdatedAt, CONFIG.ttl.resolveResult);
      if (stale) {
        toDelete.push(key);
      }
    }
    for (const key of toDelete) {
      this.cache.delete(key);
    }
    if (toDelete.length > 0) {
      this.log('CLEAR_STALE', `${toDelete.length} entries`);
      this.notify();
    }
  }

  async flush(): Promise<void> {
    if (this.flushTimer) {
      clearTimeout(this.flushTimer);
      this.flushTimer = null;
    }
    this.dirty = true;
    await this.flushToDisk();
  }

  async unload(): Promise<void> {
    await this.flush();
    this.cache.clear();
    this.log('UNLOAD', 'memory cleared, disk persisted');
    this.notify();
  }

  load(): void {
    this.loadAsync().catch((e) => {
      console.error('[MediaCache] Load failed:', e);
    });
  }

  private async loadAsync(): Promise<void> {
    try {
      const store = await getStore();
      const data = await store.get<DiskCacheData>('cache');
      if (!data) {
        this.log('LOAD', 'no disk data');
        return;
      }
      const maxTTL = Math.max(CONFIG.ttl.preview, CONFIG.ttl.resolveResult);
      if (Date.now() - data.savedAt > maxTTL) {
        this.log('LOAD', 'disk data expired');
        await store.delete('cache');
        await store.save();
        return;
      }
      if (data.entries?.length) {
        this.cache.fromJSON(data.entries);
      }
      this.dirty = false;
      this.log('LOAD', `${this.cache.size} entries from disk`);
      this.notify();
    } catch (e) {
      console.error('[MediaCache] Load async failed:', e);
    }
  }

  subscribe = this.store.subscribe;

  private notify(): void {
    this.store.set(++this.version);
  }

  private log(action: string, url: string, data?: unknown): void {
    if (CONFIG.debug) {
      logs.debug('MediaCache', `${action}: ${url}${data ? ` ${JSON.stringify(data)}` : ''}`);
    }
  }
}

export const mediaCache = new UnifiedMediaCache();
