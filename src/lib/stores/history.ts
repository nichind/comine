import { writable, derived, get } from 'svelte/store';
import { settings } from './settings';
import { translate, locale } from '$lib/i18n';
import { load, type Store } from '@tauri-apps/plugin-store';

export interface HistoryItem {
  id: string;
  url: string;
  title: string;
  author: string;
  authorUrl?: string;
  thumbnail: string;
  extension: string;
  size: number;
  duration: number;
  filePath: string;
  downloadedAt: number;
  type: 'video' | 'audio' | 'image' | 'file';
  playlistId?: string;
  playlistTitle?: string;
  playlistIndex?: number;
  convertedFormat?: string;
  downloadSource?: string;
  isFavourite?: boolean;
}

export type FilterType = 'all' | 'video' | 'audio' | 'image' | 'file' | 'favourites';
export type SortType = 'date' | 'name' | 'size' | 'duration' | 'format';

interface HistoryState {
  items: HistoryItem[];
  searchQuery: string;
  filter: FilterType;
  sort: SortType;
}

let store: Store | null = null;
let saveTimeout: ReturnType<typeof setTimeout> | null = null;

function debouncedSave(items: HistoryItem[]) {
  if (saveTimeout) {
    clearTimeout(saveTimeout);
  }
  saveTimeout = setTimeout(async () => {
    if (store) {
      try {
        await store.set('items', items);
        await store.save();
      } catch (e) {
        console.error('[History] Failed to save:', e);
      }
    }
  }, 300);
}

const MAX_HISTORY_ITEMS = 1000;

async function forceSave(items: HistoryItem[]) {
  if (saveTimeout) {
    clearTimeout(saveTimeout);
    saveTimeout = null;
  }
  if (store) {
    try {
      await store.set('items', items);
      await store.save();
    } catch (e) {
      console.error('[History] Failed to force save:', e);
    }
  }
}

function createHistoryStore() {
  const { subscribe, set, update } = writable<HistoryState>({
    items: [],
    searchQuery: '',
    filter: 'all',
    sort: 'date',
  });

  let initialized = false;
  let initPromise: Promise<void> | null = null;

  async function ensureInitialized() {
    if (initialized) return;

    if (initPromise) {
      await initPromise;
      return;
    }

    initPromise = (async () => {
      try {
        store = await load('history.json', { autoSave: false, defaults: {} });
        const items = ((await store.get('items')) as HistoryItem[]) || [];
        update((state) => ({ ...state, items }));
        console.log(`[History] Lazy loaded ${items.length} items`);
        initialized = true;
      } catch (e) {
        console.error('[History] Failed to initialize:', e);
        initialized = true; // Mark as initialized even on error to prevent loops
      }
    })();

    await initPromise;
  }

  return {
    subscribe,

    async init() {
      await ensureInitialized();
    },

    async add(item: Omit<HistoryItem, 'id' | 'downloadedAt'>) {
      await ensureInitialized();

      if (typeof item.duration !== 'number' || !Number.isFinite(item.duration)) {
        console.warn(
          '[History] Invalid duration received:',
          item.duration,
          'type:',
          typeof item.duration
        );
      }

      const newItem: HistoryItem = {
        ...item,
        duration:
          typeof item.duration === 'number' && Number.isFinite(item.duration) ? item.duration : 0,
        id: crypto.randomUUID(),
        downloadedAt: Date.now(),
      };

      update((state) => {
        const items = [newItem, ...state.items].slice(0, MAX_HISTORY_ITEMS);
        debouncedSave(items);
        return { ...state, items };
      });

      return newItem;
    },

    async restore(newItems: HistoryItem[]) {
      await ensureInitialized();

      update((state) => {
        const currentIds = new Set(state.items.map((i) => i.id));
        const uniqueNew = newItems.filter((i) => !currentIds.has(i.id));

        const items = [...uniqueNew, ...state.items]
          .sort((a, b) => b.downloadedAt - a.downloadedAt)
          .slice(0, MAX_HISTORY_ITEMS);

        debouncedSave(items);
        return { ...state, items };
      });
    },

    remove(id: string) {
      update((state) => {
        const items = state.items.filter((item) => item.id !== id);
        debouncedSave(items);
        return { ...state, items };
      });
    },

    async clear() {
      update((state) => ({ ...state, items: [] }));
      await forceSave([]);
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
      update((state) => {
        const items = state.items.map((item) =>
          item.id === id ? { ...item, isFavourite: !item.isFavourite } : item
        );
        debouncedSave(items);
        return { ...state, items };
      });
    },

    setFavourite(id: string, value: boolean) {
      update((state) => {
        const items = state.items.map((item) =>
          item.id === id ? { ...item, isFavourite: value } : item
        );
        debouncedSave(items);
        return { ...state, items };
      });
    },

    bulkSetFavourite(ids: string[], value: boolean) {
      update((state) => {
        const idSet = new Set(ids);
        const items = state.items.map((item) =>
          idSet.has(item.id) ? { ...item, isFavourite: value } : item
        );
        debouncedSave(items);
        return { ...state, items };
      });
    },

    async exportData(): Promise<string> {
      await ensureInitialized();
      const state = get({ subscribe });
      return JSON.stringify(state.items, null, 2);
    },

    async importData(jsonData: string): Promise<boolean> {
      try {
        const items = JSON.parse(jsonData) as HistoryItem[];
        if (!Array.isArray(items)) {
          throw new Error('Invalid data format');
        }
        for (const item of items) {
          if (!item.id || !item.url || !item.title) {
            throw new Error('Invalid item structure');
          }
        }
        update((state) => ({ ...state, items }));
        await forceSave(items);
        return true;
      } catch (e) {
        console.error('[History] Failed to import:', e);
        return false;
      }
    },

    async getItems(): Promise<HistoryItem[]> {
      await ensureInitialized();
      return get({ subscribe }).items;
    },

    updateDuration(id: string, duration: number) {
      update((state) => {
        const items = state.items.map((item) => (item.id === id ? { ...item, duration } : item));
        debouncedSave(items);
        return { ...state, items };
      });
    },
  };
}

export const history = createHistoryStore();
export const historyReady = writable(false);

const dateRefreshTrigger = writable(0);
export function refreshDateGroups() {
  dateRefreshTrigger.update((n) => n + 1);
}

export async function initHistory(): Promise<void> {
  await history.init();
  historyReady.set(true);
}

export const filteredHistory = derived(history, ($history) => {
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

  if ($history.filter !== 'all') {
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

export const groupedHistory = derived(
  [filteredHistory, locale, dateRefreshTrigger],
  ([$items, _locale, _refresh]) => {
    const groups: { label: string; items: HistoryItem[] }[] = [];
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    const yesterday = today - 86400000;
    const thisWeek = today - 7 * 86400000;
    const thisMonth = today - 30 * 86400000;

    const todayItems: HistoryItem[] = [];
    const yesterdayItems: HistoryItem[] = [];
    const thisWeekItems: HistoryItem[] = [];
    const thisMonthItems: HistoryItem[] = [];
    const olderItems: HistoryItem[] = [];

    for (const item of $items) {
      if (item.downloadedAt >= today) {
        todayItems.push(item);
      } else if (item.downloadedAt >= yesterday) {
        yesterdayItems.push(item);
      } else if (item.downloadedAt >= thisWeek) {
        thisWeekItems.push(item);
      } else if (item.downloadedAt >= thisMonth) {
        thisMonthItems.push(item);
      } else {
        olderItems.push(item);
      }
    }

    if (todayItems.length)
      groups.push({ label: translate('downloads.dateGroups.today'), items: todayItems });
    if (yesterdayItems.length)
      groups.push({ label: translate('downloads.dateGroups.yesterday'), items: yesterdayItems });
    if (thisWeekItems.length)
      groups.push({ label: translate('downloads.dateGroups.thisWeek'), items: thisWeekItems });
    if (thisMonthItems.length)
      groups.push({ label: translate('downloads.dateGroups.thisMonth'), items: thisMonthItems });
    if (olderItems.length)
      groups.push({ label: translate('downloads.dateGroups.older'), items: olderItems });

    return groups;
  }
);

export interface HistoryPlaylistGroup {
  playlistId: string;
  playlistTitle: string;
  items: HistoryItem[];
  totalSize: number;
  totalDuration: number;
  downloadedAt: number;
  isExpanded: boolean;
}

export const playlistGroupedHistory = derived(
  [filteredHistory, locale, history, dateRefreshTrigger],
  ([$items, _locale, $history, _refresh]) => {
    const groups: { label: string; items: (HistoryItem | HistoryPlaylistGroup)[] }[] = [];
    const sortType = $history.sort;

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
        const aTime =
          'downloadedAt' in a ? a.downloadedAt : (a as HistoryPlaylistGroup).downloadedAt;
        const bTime =
          'downloadedAt' in b ? b.downloadedAt : (b as HistoryPlaylistGroup).downloadedAt;
        return bTime - aTime;
      });

      groups.push({ label: dateGroup.label, items: groupedItems });
    }

    return groups;
  }
);

export function isPlaylistGroup(
  item: HistoryItem | HistoryPlaylistGroup
): item is HistoryPlaylistGroup {
  return (
    'playlistId' in item && 'items' in item && Array.isArray((item as HistoryPlaylistGroup).items)
  );
}

export const historyStats = derived(history, ($history) => {
  const items = $history.items;
  const totalDownloads = items.length;
  const totalSize = items.reduce((sum: number, item: HistoryItem) => sum + (item.size || 0), 0);
  const totalDuration = items.reduce(
    (sum: number, item: HistoryItem) => sum + (item.duration || 0),
    0
  );
  const favouritesCount = items.filter((item: HistoryItem) => item.isFavourite).length;

  const formatCounts: Record<string, number> = {};
  for (const item of items) {
    const ext = item.extension || 'unknown';
    formatCounts[ext] = (formatCounts[ext] || 0) + 1;
  }

  const mostCommonFormat =
    Object.entries(formatCounts).sort((a, b) => b[1] - a[1])[0]?.[0] || 'N/A';

  return {
    totalDownloads,
    totalSize,
    totalDuration,
    formatCounts,
    mostCommonFormat,
    favouritesCount,
  };
});
