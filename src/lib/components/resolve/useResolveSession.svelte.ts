import { onDestroy } from 'svelte';
import {
  resolveUrl,
  resolveUrlStream,
  convertProxyConfig,
  type ResolveSettings,
  type StreamingResolveSession,
} from '$lib/backend/mediaBackend';
import { getSettings, getProxyConfig } from '$lib/stores/settings';
import { mediaCache, type MediaPreview } from '$lib/stores/mediaCache';
import type { UrlInfo } from '$lib/bindings/UrlInfo';
import type { ResolveEvent } from '$lib/bindings/ResolveEvent';

function createVideoShell(url: string, p?: MediaPreview): UrlInfo {
  return {
    url,
    title: p?.title ?? null,
    thumbnail: p?.thumbnail ?? null,
    uploader: p?.author ?? null,
    duration: p?.duration ?? null,
  } as UrlInfo;
}

function createPlaylistShell(url: string, p?: MediaPreview): UrlInfo {
  return {
    url,
    isPlaylist: true,
    contentType: 'Playlist',
    title: p?.title ?? null,
    thumbnail: p?.thumbnail ?? null,
    uploader: p?.author ?? null,
  } as UrlInfo;
}

export interface ResolveSessionState {
  readonly info: UrlInfo | null;
  readonly loading: boolean;
  readonly error: string | null;
  readonly fetchingMore: boolean;
  readonly streamingEntryCount: number;
  readonly backendName: string | null;
}

export interface ResolveSessionActions {
  load: () => Promise<void>;
  fetchMore: () => Promise<void>;
  cancel: () => void;
}

export interface ResolveSessionOptions {
  url: string;
  cookiesFromBrowser?: string;
  customCookies?: string;
  prefetchedInfo?: MediaPreview;
}

export function useResolveSession(getOptions: () => ResolveSessionOptions) {
  // Use $state.raw to avoid deep-proxying the massive UrlInfo object (60+ fields,
  // nested arrays of formats/entries/chapters). Reassignment triggers reactivity.
  let info = $state.raw<UrlInfo | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let fetchingMore = $state(false);
  let streamingEntryCount = $state(0);
  let backendName = $state<string | null>(null);

  let activeSession: StreamingResolveSession | null = null;

  onDestroy(() => {
    if (activeSession) {
      activeSession.cancel();
      activeSession = null;
    }
  });

  function cacheResult(url: string, resolvedInfo: UrlInfo) {
    mediaCache.setResolveResult(url, resolvedInfo);
  }

  function tryLoadFromCache(url: string): UrlInfo | null {
    const cached = mediaCache.getResolveResult(url);
    if (cached) {
      info = cached;
      return cached;
    }
    return null;
  }

  let _selectedIds: Set<string> = new Set();
  let _onSelectionUpdate: ((ids: Set<string>) => void) | null = null;

  function notifySelectionAdd(id: string) {
    _selectedIds.add(id);
    _onSelectionUpdate?.(_selectedIds);
  }

  function notifySelectionBulk(ids: string[]) {
    for (const id of ids) _selectedIds.add(id);
    _onSelectionUpdate?.(_selectedIds);
  }

  function notifySelectionReset(ids: string[]) {
    _selectedIds = new Set(ids);
    _onSelectionUpdate?.(_selectedIds);
  }

  function handleEvent(event: ResolveEvent, url: string, prefetchedInfo?: MediaPreview) {
    switch (event.type) {
      case 'started':
        backendName = event.backend;
        break;

      case 'metadataUpdate': {
        if (!info) {
          info = createVideoShell(url, prefetchedInfo);
        }
        if (event.title != null) info.title = event.title;
        if (event.description != null) info.description = event.description;
        if (event.thumbnail != null) info.thumbnail = event.thumbnail;
        if (event.uploader != null) info.uploader = event.uploader;
        if (event.channel != null) info.channel = event.channel;
        if (event.channelUrl != null) info.channelUrl = event.channelUrl;
        if (event.playlistId != null) info.playlistId = event.playlistId;
        if (event.playlistTitle != null) info.playlistTitle = event.playlistTitle;
        if (event.playlistCount != null) info.playlistCount = event.playlistCount;
        if (event.contentType != null) info.contentType = event.contentType;
        if (event.contentType === 'Playlist') info.isPlaylist = true;
        info = info; // Trigger reactivity
        break;
      }

      case 'entryDiscovered': {
        if (!info) {
          info = createPlaylistShell(url, prefetchedInfo);
        }
        if (!info.entries) info.entries = [];
        info.entries.push(event.entry);
        streamingEntryCount++;
        info = info;
        notifySelectionAdd(event.entry.id);
        break;
      }

      case 'entriesBatch': {
        if (!info) {
          info = createPlaylistShell(url, prefetchedInfo);
        }
        if (!info.entries) info.entries = [];
        const newEntries = event.entries.map(([_, entry]) => entry);
        for (const entry of newEntries) info.entries.push(entry);
        streamingEntryCount += newEntries.length;
        info = info;
        notifySelectionBulk(newEntries.map((e) => e.id));
        break;
      }

      case 'complete': {
        const completeInfo = event.info;
        const streamedEntries = info?.entries;

        if (streamedEntries && streamedEntries.length > 0) {
          info = { ...completeInfo, entries: streamedEntries };
        } else {
          info = completeInfo;
          if (info.entries && info.entries.length > 0) {
            notifySelectionReset(info.entries.map((e) => e.id));
          }
        }

        activeSession = null;
        loading = false;
        cacheResult(url, info);
        break;
      }

      case 'error':
        error = event.message;
        activeSession = null;
        loading = false;
        break;

      case 'cancelled':
        activeSession = null;
        loading = false;
        break;
    }
  }

  function buildResolveSettings(
    opts: ResolveSessionOptions,
    overrides?: Partial<ResolveSettings>
  ): ResolveSettings {
    const appSettings = getSettings();
    return {
      cookies_from_browser: opts.cookiesFromBrowser || appSettings.cookiesFromBrowser || null,
      custom_cookies: opts.customCookies || appSettings.customCookies || null,
      proxy: convertProxyConfig(getProxyConfig()),
      youtube_player_client: appSettings.usePlayerClientForExtraction
        ? appSettings.youtubePlayerClient || null
        : null,
      flat_playlist: true,
      page_size: null,
      ...overrides,
    };
  }

  async function load() {
    const opts = getOptions();
    const url = opts.url;
    if (!url) return;

    if (activeSession) {
      await activeSession.cancel();
      activeSession = null;
    }

    loading = true;
    error = null;
    info = null;
    streamingEntryCount = 0;
    backendName = null;

    try {
      const cached = tryLoadFromCache(url);
      if (cached) {
        loading = false;
        if (cached.entries && cached.entries.length > 0) {
          notifySelectionReset(cached.entries.map((e) => e.id));
        }
        return;
      }
    } catch (e) {
      console.warn('Failed to load from cache:', e);
    }

    try {
      streamingEntryCount = 0;

      activeSession = await resolveUrlStream(
        url,
        buildResolveSettings(opts),
        (event: ResolveEvent) => {
          handleEvent(event, url, opts.prefetchedInfo);
        }
      );
    } catch (e) {
      console.error('Resolve error:', e);
      error = String(e);
      loading = false;
    }
  }

  async function fetchMore() {
    const opts = getOptions();
    const url = opts.url;
    const hasMore = info?.pagination?.hasMore ?? false;
    const nextCursor = info?.pagination?.nextCursor ?? null;

    if (fetchingMore || !hasMore || !nextCursor || !info) return;

    fetchingMore = true;
    try {
      const res = await resolveUrl(
        url,
        buildResolveSettings(opts, { page_size: 50, cursor: nextCursor })
      );

      if (res && res.info && res.info.entries) {
        const existingIds = new Set(info.entries?.map((e) => e.id) ?? []);
        const newEntries = res.info.entries.filter((e) => !existingIds.has(e.id));

        if (!info.entries) info.entries = [];
        for (const entry of newEntries) info.entries.push(entry);
        info.pagination = res.info.pagination;
        info = info;
      }
    } catch (e) {
      console.error('Fetch more error:', e);
    } finally {
      fetchingMore = false;
    }
  }

  function cancel() {
    if (activeSession) {
      activeSession.cancel();
      activeSession = null;
    }
  }

  function onSelectionUpdate(callback: (ids: Set<string>) => void) {
    _onSelectionUpdate = callback;
  }

  return {
    get info() {
      return info;
    },
    set info(v: UrlInfo | null) {
      info = v;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    get fetchingMore() {
      return fetchingMore;
    },
    get streamingEntryCount() {
      return streamingEntryCount;
    },
    get backendName() {
      return backendName;
    },
    load,
    fetchMore,
    cancel,
    onSelectionUpdate,
  };
}
