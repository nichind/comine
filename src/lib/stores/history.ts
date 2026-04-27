import { writable, derived, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { translate, locale } from '$lib/i18n';
import { safeDuration } from '$lib/utils/duration';
import { logs } from './logs';
import type { HistoryItem as BindingHistoryItem, HistoryStats } from '$lib/bindings';

export type HistoryItem = Omit<
  BindingHistoryItem,
  | 'type'
  | 'authorUrl'
  | 'playlistId'
  | 'playlistTitle'
  | 'playlistIndex'
  | 'convertedFormat'
  | 'downloadSource'
  | 'isFavourite'
  | 'isDirectory'
  | 'fileCount'
  | 'podcastPath'
  | 'podcastSubtitlePath'
  | 'podcastStatus'
> & {
  type: 'video' | 'audio' | 'image' | 'file' | 'gallery' | 'torrent';
  authorUrl?: string;
  playlistId?: string;
  playlistTitle?: string;
  playlistIndex?: number;
  convertedFormat?: string;
  downloadSource?: string;
  isFavourite?: boolean;
  isDirectory?: boolean;
  fileCount?: number;
  podcastPath?: string;
  podcastSubtitlePath?: string;
  podcastStatus?: string;
  podcastProgress?: number;
  podcastStep?: string;
};

export type FilterType =
  | 'all'
  | 'video'
  | 'audio'
  | 'image'
  | 'file'
  | 'gallery'
  | 'torrent'
  | 'favourites';
export type SortType = 'date' | 'name' | 'size' | 'duration' | 'format';

interface HistoryState {
  items: HistoryItem[];
  searchQuery: string;
  filter: FilterType;
  sort: SortType;
}

/**
 * Normalize a backend HistoryItem into the frontend shape.
 * The backend already sends camelCase (via `#[serde(rename_all = "camelCase")]`),
 * so this mainly applies defaults for missing fields and sanitizes duration.
 */
function normalizeItem(raw: BindingHistoryItem): HistoryItem {
  return {
    id: raw.id,
    url: raw.url,
    title: raw.title ?? '',
    author: raw.author ?? '',
    authorUrl: raw.authorUrl ?? undefined,
    thumbnail: raw.thumbnail ?? '',
    extension: raw.extension ?? '',
    size: raw.size ?? 0,
    duration: safeDuration(raw.duration),
    filePath: raw.filePath ?? '',
    downloadedAt: raw.downloadedAt ?? 0,
    type: (raw.type as HistoryItem['type']) ?? 'video',
    playlistId: raw.playlistId ?? undefined,
    playlistTitle: raw.playlistTitle ?? undefined,
    playlistIndex: raw.playlistIndex ?? undefined,
    convertedFormat: raw.convertedFormat ?? undefined,
    downloadSource: raw.downloadSource ?? undefined,
    isFavourite: raw.isFavourite ?? false,
    isDirectory: raw.isDirectory ?? false,
    fileCount: raw.fileCount ?? undefined,
    podcastPath: raw.podcastPath ?? undefined,
    podcastSubtitlePath: raw.podcastSubtitlePath ?? undefined,
    podcastStatus: raw.podcastStatus ?? undefined,
  };
}

function toBackendItem(item: HistoryItem): BindingHistoryItem {
  return {
    id: item.id,
    url: item.url,
    title: item.title,
    author: item.author,
    authorUrl: item.authorUrl ?? null,
    thumbnail: item.thumbnail,
    extension: item.extension,
    size: item.size,
    duration: item.duration,
    filePath: item.filePath,
    downloadedAt: item.downloadedAt,
    type: item.type,
    playlistId: item.playlistId ?? null,
    playlistTitle: item.playlistTitle ?? null,
    playlistIndex: item.playlistIndex ?? null,
    convertedFormat: item.convertedFormat ?? null,
    downloadSource: item.downloadSource ?? null,
    isFavourite: item.isFavourite ?? false,
    isDirectory: item.isDirectory ?? false,
    fileCount: item.fileCount ?? null,
    podcastPath: item.podcastPath ?? null,
    podcastSubtitlePath: item.podcastSubtitlePath ?? null,
    podcastStatus: item.podcastStatus ?? null,
  };
}

function createHistoryStore() {
  const { subscribe, set, update } = writable<HistoryState>({
    items: [],
    searchQuery: '',
    filter: 'all',
    sort: 'date',
  });

  let initialized = false;
  let unlistenHistoryItem: (() => void) | null = null;

  async function migrateFromFrontendStore() {
    try {
      const { load } = await import('@tauri-apps/plugin-store');
      const oldStore = await load('history.json', { autoSave: false, defaults: {} });
      const oldItems = ((await oldStore.get('items')) as HistoryItem[]) || [];
      if (oldItems.length > 0) {
        logs.info('history', `Migrating ${oldItems.length} items from frontend store`);
        const backendItems = oldItems.map(toBackendItem);
        await invoke('restore_history_from_frontend', { items: backendItems });
        update((state) => ({ ...state, items: oldItems }));
      }
    } catch {}
  }

  return {
    subscribe,

    async init() {
      if (initialized) return;
      initialized = true;

      try {
        const rawItems = await invoke<BindingHistoryItem[]>('get_history');
        const items = rawItems.map(normalizeItem);
        update((state) => ({ ...state, items }));
        logs.info('history', `Loaded ${items.length} items from backend`);

        if (items.length === 0) {
          await migrateFromFrontendStore();
        }
      } catch (e) {
        console.error('[History] Failed to load from backend:', e);
        await migrateFromFrontendStore();
      }

      const unlisten = await listen<BindingHistoryItem>('history-item-added', (event) => {
        const item = normalizeItem(event.payload);
        update((state) => ({
          ...state,
          items: [item, ...state.items],
        }));
      });
      unlistenHistoryItem = unlisten;

      // Update history items when podcast status changes
      await listen<{ historyId: string; podcastPath?: string; subtitlePath?: string }>(
        'podcast-generation-completed',
        (event) => {
          const { historyId, podcastPath } = event.payload;
          update((state) => ({
            ...state,
            items: state.items.map((item) =>
              item.id === historyId
                ? { ...item, podcastStatus: 'completed', podcastPath: podcastPath ?? item.podcastPath, podcastProgress: undefined, podcastStep: undefined }
                : item
            ),
          }));
        }
      );

      await listen<{ historyId: string }>(
        'podcast-generation-started',
        (event) => {
          update((state) => ({
            ...state,
            items: state.items.map((item) =>
              item.id === event.payload.historyId
                ? { ...item, podcastStatus: 'generating', podcastProgress: 0, podcastStep: 'starting' }
                : item
            ),
          }));
        }
      );

      await listen<{ historyId: string; step: string; progress: number; error: string | null }>(
        'podcast-generation-progress',
        (event) => {
          const { historyId, step, progress } = event.payload;
          update((state) => ({
            ...state,
            items: state.items.map((item) =>
              item.id === historyId
                ? { ...item, podcastProgress: Math.round(progress * 100), podcastStep: step }
                : item
            ),
          }));
        }
      );

      await listen<{ historyId: string; error: string }>(
        'podcast-generation-failed',
        (event) => {
          update((state) => ({
            ...state,
            items: state.items.map((item) =>
              item.id === event.payload.historyId
                ? { ...item, podcastStatus: 'failed', podcastProgress: undefined, podcastStep: undefined }
                : item
            ),
          }));
        }
      );
    },

    async add(item: Omit<HistoryItem, 'id' | 'downloadedAt'>) {
      const newItem: HistoryItem = {
        ...item,
        duration: safeDuration(item.duration),
        id: crypto.randomUUID(),
        downloadedAt: Date.now(),
      };

      update((state) => ({ ...state, items: [newItem, ...state.items] }));

      try {
        await invoke('import_history', { items: [toBackendItem(newItem)] });
      } catch {}

      return newItem;
    },

    async restore(newItems: HistoryItem[]) {
      const backendItems = newItems.map(toBackendItem);
      try {
        await invoke('import_history', { items: backendItems });
        const rawItems = await invoke<BindingHistoryItem[]>('get_history');
        const items = rawItems.map(normalizeItem);
        update((state) => ({ ...state, items }));
      } catch (e) {
        console.error('[History] Failed to restore:', e);
      }
    },

    remove(id: string) {
      update((state) => ({
        ...state,
        items: state.items.filter((item) => item.id !== id),
      }));
      invoke('remove_history_item', { id }).catch((e) =>
        logs.error('history', `Failed to remove item ${id}: ${e}`)
      );
    },

    async clear() {
      update((state) => ({ ...state, items: [] }));
      await invoke('clear_history').catch((e) =>
        logs.error('history', `Failed to clear history: ${e}`)
      );
    },

    setSearch(query: string) {
      update((state) => ({ ...state, searchQuery: query }));
    },

    setFilter(filter: FilterType) {
      update((state) => ({ ...state, filter }));
    },

    setSort(sort: SortType) {
      update((state) => ({ ...state, sort }));
    },

    toggleFavourite(id: string) {
      update((state) => ({
        ...state,
        items: state.items.map((item) =>
          item.id === id ? { ...item, isFavourite: !item.isFavourite } : item
        ),
      }));
      invoke('toggle_history_favourite', { id }).catch((e) =>
        logs.error('history', `Failed to toggle favourite ${id}: ${e}`)
      );
    },

    async getItems(): Promise<HistoryItem[]> {
      return get({ subscribe }).items;
    },

    cleanup() {
      if (unlistenHistoryItem) {
        unlistenHistoryItem();
        unlistenHistoryItem = null;
      }
    },

    updateDuration(id: string, duration: number) {
      update((state) => ({
        ...state,
        items: state.items.map((item) => (item.id === id ? { ...item, duration } : item)),
      }));
      invoke('update_history_duration', { id, duration }).catch((e) =>
        logs.error('history', `Failed to update duration ${id}: ${e}`)
      );
    },
  };
}

export const history = createHistoryStore();

const filteredHistory = derived(history, ($history) => {
  let items = [...$history.items];

  if ($history.searchQuery) {
    const query = $history.searchQuery.toLowerCase();
    items = items.filter(
      (item) =>
        item.title.toLowerCase().includes(query) ||
        item.author.toLowerCase().includes(query) ||
        item.extension.toLowerCase().includes(query)
    );
  }

  if ($history.filter === 'favourites') {
    items = items.filter((item) => item.isFavourite);
  } else if ($history.filter !== 'all') {
    items = items.filter((item) => item.type === $history.filter);
  }

  switch ($history.sort) {
    case 'date':
      items.sort((a, b) => b.downloadedAt - a.downloadedAt);
      break;
    case 'name':
      items.sort((a, b) => a.title.localeCompare(b.title));
      break;
    case 'size':
      items.sort((a, b) => b.size - a.size);
      break;
  }

  return items;
});

export interface HistoryPlaylistGroup {
  playlistId: string;
  playlistTitle: string;
  items: HistoryItem[];
  totalSize: number;
  totalDuration: number;
  downloadedAt: number;
  isExpanded: boolean;
}

export const playlistGroupedHistory = derived([filteredHistory, locale], ([$items, _locale]) => {
  const groups: { label: string; items: (HistoryItem | HistoryPlaylistGroup)[] }[] = [];
  // Get sort type from the history store without subscribing to it directly —
  // filteredHistory already depends on history, so subscribing to both would
  // re-run this expensive derivation twice per history change.
  const sortType = get(history).sort;

  if (sortType !== 'date') {
    const playlistMap = new Map<string, HistoryItem[]>();
    const singles: HistoryItem[] = [];

    for (const item of $items) {
      if (item.playlistId) {
        const existing = playlistMap.get(item.playlistId) || [];
        existing.push(item);
        playlistMap.set(item.playlistId, existing);
      } else {
        singles.push(item);
      }
    }

    const groupedItems: (HistoryItem | HistoryPlaylistGroup)[] = [];

    playlistMap.forEach((items, playlistId) => {
      items.sort((a, b) => (a.playlistIndex || 0) - (b.playlistIndex || 0));

      const totalSize = items.reduce((sum, i) => sum + (i.size || 0), 0);
      const totalDuration = items.reduce((sum, i) => sum + (i.duration || 0), 0);
      const downloadedAt = Math.max(...items.map((i) => i.downloadedAt));

      groupedItems.push({
        playlistId,
        playlistTitle: items[0]?.playlistTitle || 'Playlist',
        items,
        totalSize,
        totalDuration,
        downloadedAt,
        isExpanded: false,
      });
    });

    groupedItems.push(...singles);

    if (sortType === 'name') {
      groupedItems.sort((a, b) => {
        const aName = 'playlistTitle' in a ? a.playlistTitle || '' : a.title;
        const bName = 'playlistTitle' in b ? b.playlistTitle || '' : b.title;
        return aName.localeCompare(bName);
      });
    } else if (sortType === 'size') {
      groupedItems.sort((a, b) => {
        const aSize = 'totalSize' in a ? a.totalSize : a.size;
        const bSize = 'totalSize' in b ? b.totalSize : b.size;
        return bSize - aSize;
      });
    }

    if (groupedItems.length > 0) {
      groups.push({
        label:
          sortType === 'name'
            ? translate('downloads.sortedBy.name')
            : translate('downloads.sortedBy.size'),
        items: groupedItems,
      });
    }

    return groups;
  }

  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const yesterday = today - 86400000;
  const thisWeek = today - 7 * 86400000;
  const thisMonth = today - 30 * 86400000;

  const dateGroups: { label: string; items: HistoryItem[]; startTime: number }[] = [
    { label: translate('downloads.dateGroups.today'), items: [], startTime: today },
    { label: translate('downloads.dateGroups.yesterday'), items: [], startTime: yesterday },
    { label: translate('downloads.dateGroups.thisWeek'), items: [], startTime: thisWeek },
    { label: translate('downloads.dateGroups.thisMonth'), items: [], startTime: thisMonth },
    { label: translate('downloads.dateGroups.older'), items: [], startTime: 0 },
  ];

  for (const item of $items) {
    if (item.downloadedAt >= today) {
      dateGroups[0].items.push(item);
    } else if (item.downloadedAt >= yesterday) {
      dateGroups[1].items.push(item);
    } else if (item.downloadedAt >= thisWeek) {
      dateGroups[2].items.push(item);
    } else if (item.downloadedAt >= thisMonth) {
      dateGroups[3].items.push(item);
    } else {
      dateGroups[4].items.push(item);
    }
  }

  for (const dateGroup of dateGroups) {
    if (dateGroup.items.length === 0) continue;

    const playlistMap = new Map<string, HistoryItem[]>();
    const singles: HistoryItem[] = [];

    for (const item of dateGroup.items) {
      if (item.playlistId) {
        const existing = playlistMap.get(item.playlistId) || [];
        existing.push(item);
        playlistMap.set(item.playlistId, existing);
      } else {
        singles.push(item);
      }
    }

    const groupedItems: (HistoryItem | HistoryPlaylistGroup)[] = [];

    playlistMap.forEach((items, playlistId) => {
      items.sort((a, b) => (a.playlistIndex || 0) - (b.playlistIndex || 0));

      const totalSize = items.reduce((sum, i) => sum + (i.size || 0), 0);
      const totalDuration = items.reduce((sum, i) => sum + (i.duration || 0), 0);
      const downloadedAt = Math.max(...items.map((i) => i.downloadedAt));

      groupedItems.push({
        playlistId,
        playlistTitle: items[0]?.playlistTitle || 'Playlist',
        items,
        totalSize,
        totalDuration,
        downloadedAt,
        isExpanded: false,
      });
    });

    groupedItems.push(...singles);

    groupedItems.sort((a, b) => {
      const aTime = 'downloadedAt' in a ? a.downloadedAt : (a as HistoryPlaylistGroup).downloadedAt;
      const bTime = 'downloadedAt' in b ? b.downloadedAt : (b as HistoryPlaylistGroup).downloadedAt;
      return bTime - aTime;
    });

    groups.push({ label: dateGroup.label, items: groupedItems });
  }

  return groups;
});

export function isPlaylistGroup(
  item: HistoryItem | HistoryPlaylistGroup
): item is HistoryPlaylistGroup {
  return (
    'playlistId' in item && 'items' in item && Array.isArray((item as HistoryPlaylistGroup).items)
  );
}

export const historyStats = writable<{
  totalDownloads: number;
  totalSize: number;
  totalDuration: number;
  formatCounts: Record<string, number>;
  favouritesCount: number;
}>({
  totalDownloads: 0,
  totalSize: 0,
  totalDuration: 0,
  formatCounts: {},
  favouritesCount: 0,
});

function applyHistoryStats(stats: HistoryStats) {
  historyStats.set({
    totalDownloads: stats.totalDownloads,
    totalSize: Number(stats.totalSize),
    totalDuration: stats.totalDuration,
    formatCounts: Object.fromEntries(
      Object.entries(stats.formatCounts).filter((e): e is [string, number] => e[1] != null)
    ),
    favouritesCount: stats.favouritesCount,
  });
}

invoke<HistoryStats>('get_history_stats')
  .then(applyHistoryStats)
  .catch((e) => logs.error('history', `Failed to load history stats: ${e}`));

listen<HistoryStats>('history-stats-changed', (event) => {
  applyHistoryStats(event.payload);
});
