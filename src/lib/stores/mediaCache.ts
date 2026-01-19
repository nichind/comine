import { writable, derived } from 'svelte/store';
import { load, type Store } from '@tauri-apps/plugin-store';
import { LRUCache } from '$lib/utils/LRUCache';

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

export interface Chapter {
  title: string;
  start_time: number;
  end_time: number;
}

export interface Storyboard {
  url: string;
  width: number;
  height: number;
  cols: number;
  rows: number;
  fragment_count: number;
  fragment_duration: number;
}

export interface SponsorBlockSegment {
  category: string;
  segment: [number, number];
  UUID?: string;
  actionType?: string;
}

export interface VideoInfo {
  title: string;
  author: string | null;
  thumbnail: string | null;
  duration: number | null;
  viewCount: number | null;
  likeCount: number | null;
  uploadDate: string | null;
  description: string | null;
  channelUrl: string | null;
  channelId: string | null;
  chapters: Chapter[] | null;
  storyboards: Storyboard[] | null;
  sponsorSegments: SponsorBlockSegment[] | null;
}

export interface VideoFormat {
  formatId: string;
  ext: string;
  resolution: string | null;
  fps: number | null;
  vcodec: string | null;
  acodec: string | null;
  filesize: number | null;
  filesizeApprox: number | null;
  tbr: number | null;
  vbr: number | null;
  abr: number | null;
  asr: number | null;
  formatNote: string | null;
  hasVideo: boolean;
  hasAudio: boolean;
}

export interface PlaylistEntry {
  id: string;
  url: string;
  title: string;
  duration: number | null;
  thumbnail: string | null;
  uploader: string | null;
  isMusic: boolean;
}

export interface PlaylistInfo {
  isPlaylist: boolean;
  id: string | null;
  title: string;
  uploader: string | null;
  thumbnail: string | null;
  totalCount: number;
  entries: PlaylistEntry[];
  hasMore: boolean;
}

export interface ChannelEntry {
  id: string;
  url: string;
  title: string;
  duration: number | null;
  thumbnail: string | null;
  viewCount: number | null;
  uploadDate: string | null;
  isShort: boolean;
  isLive: boolean;
}

export interface ChannelInfo {
  id: string | null;
  name: string;
  handle: string | null;
  description: string | null;
  thumbnail: string | null;
  banner: string | null;
  subscriberCount: number | null;
  totalCount: number;
  entries: ChannelEntry[];
  hasMore: boolean;
}

export type UIState = {
  scrollTop: number;
  selectedMode?: 'all' | 'some';
  selectedIds: string[];
  deselectedIds?: string[];
  perItemSettings: Record<string, unknown>;
  viewMode: 'list' | 'grid';
  searchQuery: string;
  selectedVideo: string;
  selectedAudio: string;
  channelTab?: 'videos' | 'shorts' | 'live';
};

export interface CacheEntry {
  url: string;
  type: 'video' | 'playlist' | 'channel' | 'unknown';
  preview: MediaPreview | null;
  videoInfo: VideoInfo | null;
  formats: VideoFormat[] | null;
  playlistInfo: PlaylistInfo | null;
  channelInfo: ChannelInfo | null;
  uiState: UIState | null;
  createdAt: number;
  lastAccessedAt: number;
  previewUpdatedAt: number | null;
  videoInfoUpdatedAt: number | null;
  formatsUpdatedAt: number | null;
  playlistInfoUpdatedAt: number | null;
  channelInfoUpdatedAt: number | null;
}

const CONFIG = {
  maxMetadataEntries: 30,
  maxHeavyDataEntries: 3,
  maxFormats: 10,
  ttl: {
    preview: 30 * 60 * 1000,
    videoInfo: 20 * 60 * 1000,
    formats: 10 * 60 * 1000,
    playlistInfo: 15 * 60 * 1000,
    channelInfo: 15 * 60 * 1000,
    uiState: 30 * 60 * 1000,
  },
  debug: false,
};

const DEFAULT_UI_STATE: UIState = {
  scrollTop: 0,
  selectedMode: 'some',
  selectedIds: [],
  deselectedIds: [],
  perItemSettings: {},
  viewMode: 'list',
  searchQuery: '',
  selectedVideo: 'best',
  selectedAudio: 'best',
};

interface MetadataEntry {
  url: string;
  type: 'video' | 'playlist' | 'channel' | 'unknown';
  preview: MediaPreview | null;
  videoInfo: VideoInfo | null;
  formats: VideoFormat[] | null;
  uiState: UIState | null;
  createdAt: number;
  lastAccessedAt: number;
  previewUpdatedAt: number | null;
  videoInfoUpdatedAt: number | null;
  formatsUpdatedAt: number | null;
}

interface HeavyDataEntry {
  playlistInfo: PlaylistInfo | null;
  channelInfo: ChannelInfo | null;
  playlistInfoUpdatedAt: number | null;
  channelInfoUpdatedAt: number | null;
}

function normalizeUrl(url: string): string {
  if (!url) return '';
  try {
    const parsed = new URL(url);
    if (parsed.hostname.includes('youtube.com') || parsed.hostname.includes('youtu.be')) {
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
  metadata: Array<[string, MetadataEntry]>;
  heavyData: Array<[string, HeavyDataEntry]>;
  savedAt: number;
}

class UnifiedMediaCache {
  private metadata = new LRUCache<string, MetadataEntry>(CONFIG.maxMetadataEntries);
  private heavyData = new LRUCache<string, HeavyDataEntry>(CONFIG.maxHeavyDataEntries);
  private store = writable(0);
  private version = 0;
  private dirty = false;
  private flushTimer: ReturnType<typeof setTimeout> | null = null;
  private flushPromise: Promise<void> | null = null;
  private cleanupTimer: ReturnType<typeof setInterval> | null = null;
  private loaded = false;

  constructor() {
    this.metadata.setMutationCallback(() => this.scheduleDiskFlush());
    this.heavyData.setMutationCallback(() => this.scheduleDiskFlush());

    this.cleanupTimer = setInterval(
      () => {
        this.clearStale();
      },
      2 * 60 * 1000
    );
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
        metadata: this.metadata.toJSON(),
        heavyData: this.heavyData.toJSON(),
        savedAt: Date.now(),
      };

      const json = JSON.stringify(data);
      if (json.length > PERSISTENCE_CONFIG.maxDiskBytes) {
        const trimmed = this.trimForDiskLimit(data, json.length);
        await store.set('cache', trimmed);
      } else {
        await store.set('cache', data);
      }

      await store.save();
      this.dirty = false;
      this.log('FLUSH_TO_DISK', `${data.metadata.length}m/${data.heavyData.length}h`);
    } catch (e) {
      console.error('[MediaCache] Disk flush failed:', e);
    }
  }

  private trimForDiskLimit(data: DiskCacheData, currentSize: number): DiskCacheData {
    const ratio = PERSISTENCE_CONFIG.maxDiskBytes / currentSize;
    const metaKeep = Math.max(1, Math.floor(data.metadata.length * ratio * 0.9));
    const heavyKeep = Math.max(1, Math.floor(data.heavyData.length * ratio * 0.9));

    return {
      metadata: data.metadata.slice(-metaKeep),
      heavyData: data.heavyData.slice(-heavyKeep),
      savedAt: data.savedAt,
    };
  }

  private getOrCreateMetadata(url: string): MetadataEntry {
    const key = normalizeUrl(url);
    let entry = this.metadata.get(key);
    if (!entry) {
      entry = {
        url,
        type: detectType(url),
        preview: null,
        videoInfo: null,
        formats: null,
        uiState: null,
        createdAt: Date.now(),
        lastAccessedAt: Date.now(),
        previewUpdatedAt: null,
        videoInfoUpdatedAt: null,
        formatsUpdatedAt: null,
      };
      this.metadata.set(key, entry);
    }
    return entry;
  }

  get(url: string): CacheEntry | null {
    if (!this.loaded) {
      this.load();
      this.loaded = true;
    }

    const key = normalizeUrl(url);
    const meta = this.metadata.get(key);
    if (!meta) {
      this.log('MISS', url);
      return null;
    }
    meta.lastAccessedAt = Date.now();
    const heavy = this.heavyData.get(key);
    this.log('HIT', url);
    return {
      ...meta,
      playlistInfo: heavy?.playlistInfo ?? null,
      channelInfo: heavy?.channelInfo ?? null,
      playlistInfoUpdatedAt: heavy?.playlistInfoUpdatedAt ?? null,
      channelInfoUpdatedAt: heavy?.channelInfoUpdatedAt ?? null,
    };
  }

  has(url: string): boolean {
    return this.metadata.has(normalizeUrl(url));
  }

  hasHeavyData(url: string): boolean {
    return this.heavyData.has(normalizeUrl(url));
  }

  setPreview(url: string, preview: MediaPreview): void {
    const entry = this.getOrCreateMetadata(url);
    entry.preview = { ...entry.preview, ...preview };
    entry.previewUpdatedAt = Date.now();
    entry.lastAccessedAt = Date.now();
    if (preview.isPlaylist !== undefined) {
      entry.type = preview.isPlaylist ? 'playlist' : 'video';
    }
    this.log('SET_PREVIEW', url, preview);
    this.notify();
  }

  setVideoInfo(url: string, info: VideoInfo): void {
    const entry = this.getOrCreateMetadata(url);
    entry.videoInfo = info;
    entry.videoInfoUpdatedAt = Date.now();
    entry.lastAccessedAt = Date.now();
    entry.type = 'video';
    entry.preview = {
      ...entry.preview,
      title: info.title,
      thumbnail: info.thumbnail ?? entry.preview?.thumbnail,
      author: info.author ?? entry.preview?.author,
      duration: info.duration ?? entry.preview?.duration,
      isPlaylist: false,
    };
    entry.previewUpdatedAt = Date.now();
    this.log('SET_VIDEO_INFO', url);
    this.notify();
  }

  setFormats(url: string, formats: VideoFormat[]): void {
    const entry = this.getOrCreateMetadata(url);
    const sorted = [...formats].sort((a, b) => (b.tbr ?? 0) - (a.tbr ?? 0));
    entry.formats = sorted.slice(0, CONFIG.maxFormats);
    entry.formatsUpdatedAt = Date.now();
    entry.lastAccessedAt = Date.now();
    this.log('SET_FORMATS', url, `${formats.length} → ${entry.formats.length} formats`);
    this.notify();
  }

  setPlaylistInfo(url: string, info: PlaylistInfo): void {
    const key = normalizeUrl(url);
    const meta = this.getOrCreateMetadata(url);
    meta.type = 'playlist';
    meta.preview = {
      ...meta.preview,
      title: info.title,
      thumbnail: info.thumbnail ?? meta.preview?.thumbnail,
      author: info.uploader ?? meta.preview?.author,
      entryCount: info.totalCount,
      isPlaylist: true,
    };
    meta.previewUpdatedAt = Date.now();
    meta.lastAccessedAt = Date.now();

    const existing = this.heavyData.get(key);
    this.heavyData.set(key, {
      playlistInfo: info,
      channelInfo: existing?.channelInfo ?? null,
      playlistInfoUpdatedAt: Date.now(),
      channelInfoUpdatedAt: existing?.channelInfoUpdatedAt ?? null,
    });
    this.log('SET_PLAYLIST_INFO', url, `${info.totalCount} entries`);
    this.notify();
  }

  setChannelInfo(url: string, info: ChannelInfo): void {
    const key = normalizeUrl(url);
    const meta = this.getOrCreateMetadata(url);
    meta.type = 'channel';
    meta.preview = {
      ...meta.preview,
      title: info.name,
      thumbnail: info.thumbnail ?? meta.preview?.thumbnail,
      author: info.handle ?? meta.preview?.author,
      entryCount: info.totalCount,
      isPlaylist: false,
    };
    meta.previewUpdatedAt = Date.now();
    meta.lastAccessedAt = Date.now();

    const existing = this.heavyData.get(key);
    this.heavyData.set(key, {
      playlistInfo: existing?.playlistInfo ?? null,
      channelInfo: info,
      playlistInfoUpdatedAt: existing?.playlistInfoUpdatedAt ?? null,
      channelInfoUpdatedAt: Date.now(),
    });
    this.log('SET_CHANNEL_INFO', url, `${info.totalCount} videos`);
    this.notify();
  }

  setUIState(url: string, state: Partial<UIState>): void {
    const entry = this.getOrCreateMetadata(url);
    entry.uiState = { ...DEFAULT_UI_STATE, ...(entry.uiState ?? {}), ...state };
    entry.lastAccessedAt = Date.now();
    this.log('SET_UI_STATE', url);
    this.notify();
  }

  getPreview(url: string): MediaPreview | null {
    const key = normalizeUrl(url);
    const entry = this.metadata.get(key);
    if (!entry?.preview || isExpired(entry.previewUpdatedAt, CONFIG.ttl.preview)) {
      return null;
    }
    return entry.preview;
  }

  getVideoInfo(url: string): VideoInfo | null {
    const key = normalizeUrl(url);
    const entry = this.metadata.get(key);
    if (!entry?.videoInfo || isExpired(entry.videoInfoUpdatedAt, CONFIG.ttl.videoInfo)) {
      return null;
    }
    return entry.videoInfo;
  }

  getFormats(url: string): VideoFormat[] | null {
    const key = normalizeUrl(url);
    const entry = this.metadata.get(key);
    if (!entry?.formats || isExpired(entry.formatsUpdatedAt, CONFIG.ttl.formats)) {
      return null;
    }
    return entry.formats;
  }

  getPlaylistInfo(url: string): PlaylistInfo | null {
    const key = normalizeUrl(url);
    const heavy = this.heavyData.get(key);
    if (!heavy?.playlistInfo || isExpired(heavy.playlistInfoUpdatedAt, CONFIG.ttl.playlistInfo)) {
      return null;
    }
    return heavy.playlistInfo;
  }

  getChannelInfo(url: string): ChannelInfo | null {
    const key = normalizeUrl(url);
    const heavy = this.heavyData.get(key);
    if (!heavy?.channelInfo || isExpired(heavy.channelInfoUpdatedAt, CONFIG.ttl.channelInfo)) {
      return null;
    }
    return heavy.channelInfo;
  }

  getUIState(url: string): UIState | null {
    const key = normalizeUrl(url);
    const entry = this.metadata.get(key);
    return entry?.uiState ?? null;
  }

  hasAnyData(url: string): boolean {
    const entry = this.get(url);
    if (!entry) return false;
    return !!(
      entry.preview ||
      entry.videoInfo ||
      entry.formats ||
      entry.playlistInfo ||
      entry.channelInfo
    );
  }

  getBestPreview(url: string): MediaPreview | null {
    const entry = this.get(url);
    if (!entry) return null;
    const preview: MediaPreview = {};
    if (entry.videoInfo) {
      preview.title = entry.videoInfo.title;
      preview.thumbnail = entry.videoInfo.thumbnail ?? undefined;
      preview.author = entry.videoInfo.author ?? undefined;
      preview.duration = entry.videoInfo.duration ?? undefined;
      preview.isPlaylist = false;
    }
    if (entry.channelInfo) {
      preview.title = entry.channelInfo.name;
      preview.thumbnail = entry.channelInfo.thumbnail ?? undefined;
      preview.author = entry.channelInfo.handle ?? undefined;
      preview.entryCount = entry.channelInfo.totalCount;
      preview.isPlaylist = false;
    }
    if (entry.playlistInfo) {
      preview.title = entry.playlistInfo.title;
      preview.thumbnail = entry.playlistInfo.thumbnail ?? undefined;
      preview.author = entry.playlistInfo.uploader ?? undefined;
      preview.entryCount = entry.playlistInfo.totalCount;
      preview.isPlaylist = true;
    }
    if (entry.preview) {
      preview.title ??= entry.preview.title;
      preview.thumbnail ??= entry.preview.thumbnail;
      preview.author ??= entry.preview.author;
      preview.duration ??= entry.preview.duration;
      if (entry.preview.isMusic !== undefined) preview.isMusic = entry.preview.isMusic;
    }
    return Object.keys(preview).length > 0 ? preview : null;
  }

  delete(url: string): void {
    const key = normalizeUrl(url);
    this.metadata.delete(key);
    this.heavyData.delete(key);
    this.log('DELETE', url);
    this.notify();
  }

  clear(): void {
    this.metadata.clear();
    this.heavyData.clear();
    this.log('CLEAR', 'all');
    this.notify();
  }

  clearStale(): void {
    const now = Date.now();
    const toDelete: string[] = [];
    for (const [key, entry] of this.metadata.entries()) {
      const allStale =
        isExpired(entry.previewUpdatedAt, CONFIG.ttl.preview) &&
        isExpired(entry.videoInfoUpdatedAt, CONFIG.ttl.videoInfo) &&
        isExpired(entry.formatsUpdatedAt, CONFIG.ttl.formats);
      const heavy = this.heavyData.get(key);
      const heavyStale =
        !heavy ||
        (isExpired(heavy.playlistInfoUpdatedAt, CONFIG.ttl.playlistInfo) &&
          isExpired(heavy.channelInfoUpdatedAt, CONFIG.ttl.channelInfo));
      if (allStale && heavyStale && !entry.uiState) {
        toDelete.push(key);
      }
    }
    for (const key of toDelete) {
      this.metadata.delete(key);
      this.heavyData.delete(key);
    }
    if (toDelete.length > 0) {
      this.log('CLEAR_STALE', `${toDelete.length} entries`);
      this.notify();
    }
  }

  getStats(): {
    metadataSize: number;
    heavyDataSize: number;
    maxMetadata: number;
    maxHeavy: number;
  } {
    return {
      metadataSize: this.metadata.size,
      heavyDataSize: this.heavyData.size,
      maxMetadata: CONFIG.maxMetadataEntries,
      maxHeavy: CONFIG.maxHeavyDataEntries,
    };
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

    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }

    this.metadata.clear();
    this.heavyData.clear();
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

      const maxTTL = Math.max(
        CONFIG.ttl.preview,
        CONFIG.ttl.videoInfo,
        CONFIG.ttl.formats,
        CONFIG.ttl.playlistInfo,
        CONFIG.ttl.channelInfo
      );

      if (Date.now() - data.savedAt > maxTTL) {
        this.log('LOAD', 'disk data expired');
        await store.delete('cache');
        await store.save();
        return;
      }

      if (data.metadata?.length) {
        this.metadata.fromJSON(data.metadata);
      }
      if (data.heavyData?.length) {
        this.heavyData.fromJSON(data.heavyData);
      }

      this.dirty = false;
      this.log('LOAD', `${this.metadata.size}m/${this.heavyData.size}h from disk`);
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
      console.log(`[MediaCache] ${action}: ${url}`, data ?? '');
    }
  }
}

export const mediaCache = new UnifiedMediaCache();

export function getCacheEntry(url: string) {
  return derived(mediaCache, ($cache) => {
    return mediaCache.get(url);
  });
}

export function getCachedPreview(url: string) {
  return derived(mediaCache, () => {
    return mediaCache.getBestPreview(url);
  });
}

export function convertBackendVideoInfo(backend: {
  title: string;
  uploader?: string | null;
  channel?: string | null;
  creator?: string | null;
  thumbnail?: string | null;
  duration?: number | null;
  channel_url?: string | null;
  channel_id?: string | null;
  chapters?: Chapter[] | null;
  storyboards?: Storyboard[] | null;
}): VideoInfo {
  return {
    title: backend.title,
    author: backend.uploader ?? backend.channel ?? backend.creator ?? null,
    thumbnail: backend.thumbnail ?? null,
    duration: backend.duration ?? null,
    viewCount: null,
    likeCount: null,
    uploadDate: null,
    description: null,
    channelUrl: backend.channel_url ?? null,
    channelId: backend.channel_id ?? null,
    chapters: backend.chapters ?? null,
    storyboards: backend.storyboards ?? null,
    sponsorSegments: null,
  };
}

export function convertBackendFormats(backend: {
  title: string;
  author?: string | null;
  thumbnail?: string | null;
  duration?: number | null;
  formats: Array<{
    format_id: string;
    ext: string;
    resolution?: string | null;
    fps?: number | null;
    vcodec?: string | null;
    acodec?: string | null;
    filesize?: number | null;
    filesize_approx?: number | null;
    tbr?: number | null;
    vbr?: number | null;
    abr?: number | null;
    asr?: number | null;
    format_note?: string | null;
    has_video: boolean;
    has_audio: boolean;
  }>;
  view_count?: number | null;
  like_count?: number | null;
  description?: string | null;
  upload_date?: string | null;
  channel_url?: string | null;
  channel_id?: string | null;
  chapters?: Chapter[] | null;
  storyboards?: Storyboard[] | null;
}): { info: VideoInfo; formats: VideoFormat[] } {
  return {
    info: {
      title: backend.title,
      author: backend.author ?? null,
      thumbnail: backend.thumbnail ?? null,
      duration: backend.duration ?? null,
      viewCount: backend.view_count ?? null,
      likeCount: backend.like_count ?? null,
      uploadDate: backend.upload_date ?? null,
      description: backend.description ?? null,
      channelUrl: backend.channel_url ?? null,
      channelId: backend.channel_id ?? null,
      chapters: backend.chapters ?? null,
      storyboards: backend.storyboards ?? null,
      sponsorSegments: null,
    },
    formats: backend.formats.map((f) => ({
      formatId: f.format_id,
      ext: f.ext,
      resolution: f.resolution ?? null,
      fps: f.fps ?? null,
      vcodec: f.vcodec ?? null,
      acodec: f.acodec ?? null,
      filesize: f.filesize ?? null,
      filesizeApprox: f.filesize_approx ?? null,
      tbr: f.tbr ?? null,
      vbr: f.vbr ?? null,
      abr: f.abr ?? null,
      asr: f.asr ?? null,
      formatNote: f.format_note ?? null,
      hasVideo: f.has_video,
      hasAudio: f.has_audio,
    })),
  };
}

export function convertBackendPlaylistInfo(backend: {
  is_playlist: boolean;
  id?: string | null;
  title: string;
  uploader?: string | null;
  thumbnail?: string | null;
  total_count: number;
  entries: Array<{
    id: string;
    url: string;
    title: string;
    duration?: number | null;
    thumbnail?: string | null;
    uploader?: string | null;
    is_music: boolean;
  }>;
  has_more: boolean;
}): PlaylistInfo {
  return {
    isPlaylist: backend.is_playlist,
    id: backend.id ?? null,
    title: backend.title,
    uploader: backend.uploader ?? null,
    thumbnail: backend.thumbnail ?? null,
    totalCount: backend.total_count,
    entries: backend.entries.map((e) => ({
      id: e.id,
      url: e.url,
      title: e.title,
      duration: e.duration ?? null,
      thumbnail: e.thumbnail ?? null,
      uploader: e.uploader ?? null,
      isMusic: e.is_music,
    })),
    hasMore: backend.has_more,
  };
}

export function convertBackendChannelInfo(backend: {
  id?: string | null;
  title?: string | null;
  channel?: string | null;
  uploader?: string | null;
  uploader_id?: string | null;
  channel_id?: string | null;
  description?: string | null;
  thumbnail?: string | null;
  thumbnails?: Array<{ url?: string }> | null;
  channel_follower_count?: number | null;
  entries?: Array<{
    id: string;
    url?: string;
    title?: string;
    duration?: number | null;
    thumbnail?: string | null;
    thumbnails?: Array<{ url?: string }> | null;
    view_count?: number | null;
    upload_date?: string | null;
    original_url?: string | null;
  }>;
  playlist_count?: number | null;
}): ChannelInfo {
  const entries = backend.entries ?? [];
  const name = backend.channel ?? backend.uploader ?? backend.title ?? 'Unknown Channel';
  const handle = backend.uploader_id ?? null;
  const thumbnail = backend.thumbnail ?? backend.thumbnails?.[0]?.url ?? null;

  return {
    id: backend.channel_id ?? backend.id ?? null,
    name,
    handle,
    description: backend.description ?? null,
    thumbnail,
    banner: null,
    subscriberCount: backend.channel_follower_count ?? null,
    totalCount: backend.playlist_count ?? entries.length,
    entries: entries.map((e) => {
      const videoUrl =
        e.url ?? e.original_url ?? (e.id ? `https://www.youtube.com/watch?v=${e.id}` : '');
      const isShort = videoUrl.includes('/shorts/') || (e.duration != null && e.duration < 61);
      const isLive = e.duration === null && e.view_count != null;

      return {
        id: e.id,
        url: videoUrl,
        title: e.title ?? 'Untitled',
        duration: e.duration ?? null,
        thumbnail: e.thumbnail ?? e.thumbnails?.[0]?.url ?? null,
        viewCount: e.view_count ?? null,
        uploadDate: e.upload_date ?? null,
        isShort,
        isLive,
      };
    }),
    hasMore: false,
  };
}
