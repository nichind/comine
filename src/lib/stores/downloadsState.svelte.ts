import {
  history,
  type FilterType,
  type SortType,
  type HistoryItem,
  type HistoryPlaylistGroup,
  isPlaylistGroup,
} from '$lib/stores/history';
import { formatSize } from '$lib/utils/format';
import {
  type QueueItem,
  type PlaylistGroup,
} from '$lib/stores/queue';
import { settings, updateSettings } from '$lib/stores/settings';
import { get } from 'svelte/store';
import { calculateMatchScore } from '$lib/utils/search';
import { convertFileSrc } from '@tauri-apps/api/core';
import { extractDominantColor, generateColorVars, getCachedColor, getCachedColorAsync, type RGB } from '$lib/utils/color';
import { stat } from '@tauri-apps/plugin-fs';
import { LRUCache } from '$lib/utils/LRUCache';
import { translate } from '$lib/i18n';

export type SortDirection = 'asc' | 'desc';

export interface UnifiedDownloadItem {
  id: string;
  url: string;
  title: string;
  author: string;
  thumbnail?: string;
  extension: string;
  size: number;
  duration: number;
  filePath?: string;
  addedAt: number;
  type: 'video' | 'audio' | 'image' | 'file';
  playlistId?: string;
  playlistTitle?: string;
  playlistIndex?: number;
  isActive: boolean;
  status?: 'pending' | 'paused' | 'fetching-info' | 'downloading' | 'processing' | 'completed' | 'failed';
  progress?: number;
  speed?: string;
  eta?: string;
  error?: string;
  priority?: number;
}

export type VirtualListItem =
  | { kind: 'date'; label: string; id: string }
  | { kind: 'single'; item: UnifiedDownloadItem; dateLabel: string; id: string }
  | { kind: 'playlist'; group: HistoryPlaylistGroup; dateLabel: string; id: string }
  | { kind: 'playlist-child'; item: UnifiedDownloadItem; playlistId: string; isLast: boolean; dateLabel: string; id: string }
  | { kind: 'grid-row'; items: UnifiedDownloadItem[]; id: string };

interface HistoryDateGroup {
  label: string;
  items: (HistoryItem | HistoryPlaylistGroup)[];
}

interface ActiveDownloadData {
  groups: PlaylistGroup[];
  singles: QueueItem[];
}

function historyToUnified(item: HistoryItem): UnifiedDownloadItem {
  return {
    id: item.id,
    url: item.url,
    title: item.title,
    author: item.author,
    thumbnail: item.thumbnail,
    extension: item.extension,
    size: item.size,
    duration: item.duration,
    filePath: item.filePath,
    addedAt: item.downloadedAt,
    type: item.type,
    playlistId: item.playlistId,
    playlistTitle: item.playlistTitle,
    playlistIndex: item.playlistIndex,
    isActive: false,
    status: 'completed',
    progress: 100,
  };
}

function queueToUnified(item: QueueItem): UnifiedDownloadItem {
  return {
    id: item.id,
    url: item.url,
    title: item.title || 'Loading...',
    author: item.author || '',
    thumbnail: item.thumbnail,
    extension: item.extension || '',
    size: item.filesize || 0,
    duration: item.duration || 0,
    filePath: item.filePath,
    addedAt: item.addedAt,
    type: item.type || 'video',
    playlistId: item.playlistId,
    playlistTitle: item.playlistTitle,
    playlistIndex: item.playlistIndex,
    isActive: true,
    status: item.status,
    progress: item.progress || 0,
    speed: item.speed,
    eta: item.eta,
    error: item.error,
    priority: item.priority,
  };
}

const CACHE_SIZES = {
  colors: 200,
  thumbnails: 300,
  playlistThumbs: 50,
  failedThumbnails: 100,
} as const;

const GRID_CONFIG = {
  minColumnWidth: 160,
  gap: 10,
} as const;

export const VIRTUALIZATION_HEIGHTS = {
  listItem: 56,
  gridRow: 232,
  dateHeader: 40,
  playlistHeader: 60,
} as const;

export class DownloadsState {
  searchQuery = $state('');
  activeFilter = $state<FilterType>('all');
  sortType = $state<SortType>('date');
  sortDirection = $state<SortDirection>('desc');
  viewMode = $state<'list' | 'grid'>('list');
  gridItemSize = $state(200);
  hoveredItemId = $state<string | null>(null);
  containerWidth = $state(800);
  
  selectedItemIds = $state(new Set<string>());
  lastSelectedItemId = $state<string | null>(null);
  isSelectionMode = $derived(this.selectedItemIds.size > 0);

  private _collapsedPlaylists = $state(new Set<string>());
  private _collapsedHistoryPlaylists = $state(new Set<string>());
  private missingFiles = $state(new Set<string>());
  private failedThumbnails = $state(new Set<string>());

  get collapsedPlaylists(): ReadonlySet<string> {
    return this._collapsedPlaylists;
  }

  get collapsedHistoryPlaylists(): ReadonlySet<string> {
    return this._collapsedHistoryPlaylists;
  }
  
  private readonly colorCache = new LRUCache<string, RGB>(CACHE_SIZES.colors);
  private readonly thumbnailSrcCache = new LRUCache<string, string>(CACHE_SIZES.thumbnails);
  private readonly playlistThumbCache = new LRUCache<string, string[]>(CACHE_SIZES.playlistThumbs);
  
  historyGroups = $state<HistoryDateGroup[]>([]);
  activeDownloadData = $state<ActiveDownloadData>({ groups: [], singles: [] });
  
  private cleanupEffects: (() => void) | null = null;

  constructor() {
    this.initFromSettings();
    this.setupEffects();
  }

  private initFromSettings(): void {
    const currentSettings = get(settings);
    this.viewMode = currentSettings.downloadsViewMode ?? 'list';
    this.sortType = currentSettings.downloadsSortType ?? 'date';
    this.sortDirection = currentSettings.downloadsSortDirection ?? 'desc';
    this.gridItemSize = currentSettings.gridItemSize ?? 200;
  }

  private setupEffects(): void {
    this.cleanupEffects = $effect.root(() => {
      $effect(() => { history.setSort(this.sortType); });
      $effect(() => { history.setFilter(this.activeFilter); });
      $effect(() => { history.setSearch(this.searchQuery); });
      
      $effect(() => { 
        updateSettings({
          downloadsViewMode: this.viewMode,
          downloadsSortType: this.sortType,
          downloadsSortDirection: this.sortDirection,
          gridItemSize: this.gridItemSize
        }); 
      });
    });
  }

  destroy(): void {
    this.cleanupEffects?.();
    this.cleanupEffects = null;
    if (this.fileCheckDebounceTimer) clearTimeout(this.fileCheckDebounceTimer);
    this.pendingFileChecks.clear();
    this.colorCache.clear();
    this.thumbnailSrcCache.clear();
    this.playlistThumbCache.clear();
  }

  setFilter(filter: FilterType): void {
    this.activeFilter = filter;
  }

  setSort(sort: SortType): void {
    this.sortType = sort;
  }

  setSortWithDirection(sort: SortType, direction: SortDirection): void {
    this.sortType = sort;
    this.sortDirection = direction;
  }

  toggleSortDirection(): void {
    this.sortDirection = this.sortDirection === 'asc' ? 'desc' : 'asc';
  }

  setViewMode(mode: 'list' | 'grid'): void {
    this.viewMode = mode;
  }

  increaseGridSize(): void {
    this.gridItemSize = Math.min(400, this.gridItemSize + 40);
  }

  decreaseGridSize(): void {
    this.gridItemSize = Math.max(120, this.gridItemSize - 40);
  }

  resetGridSize(): void {
    this.gridItemSize = 200;
  }

  setHoveredItem(id: string | null): void {
    this.hoveredItemId = id;
  }

  toggleSelection(id: string, multi: boolean, range: boolean): void {
    const next = new Set(this.selectedItemIds);
    
    if (range && this.lastSelectedItemId) {
      const flatItems = this.flatRows.map(r => 'id' in r && r.kind === 'single' ? r.item.id : null).filter(Boolean) as string[];
      const allIds = flatItems.length ? flatItems : this.getAllItemIds();
      
      const startIdx = allIds.indexOf(this.lastSelectedItemId);
      const endIdx = allIds.indexOf(id);
      
      if (startIdx !== -1 && endIdx !== -1) {
        const [lower, upper] = [Math.min(startIdx, endIdx), Math.max(startIdx, endIdx)];
        const rangeIds = allIds.slice(lower, upper + 1);
        for (const rid of rangeIds) next.add(rid);
      }
    } else if (multi) {
      if (next.has(id)) next.delete(id);
      else next.add(id);
      this.lastSelectedItemId = id;
    } else {
       next.clear();
       next.add(id);
       this.lastSelectedItemId = id;
    }
    
    this.selectedItemIds = next;
  }

  isItemSelected(id: string): boolean {
    return this.selectedItemIds.has(id);
  }

  clearSelection(): void {
    if (this.selectedItemIds.size > 0) {
      this.selectedItemIds = new Set();
      this.lastSelectedItemId = null;
    }
  }

  selectAll(): void {
    const allIds = this.getAllItemIds();
    this.selectedItemIds = new Set(allIds);
  }
  
  private getAllItemIds(): string[] {
      const ids: string[] = [];
      for (const group of this.historyGroups) {
          for (const item of group.items) {
              if (isPlaylistGroup(item)) {
                 ids.push(...item.items.map((i: HistoryItem) => i.id));
              } else {
                 ids.push(item.id);
              }
          }
      }
      return ids;
  }

  togglePlaylist(playlistId: string, isHistory: boolean): void {
    const target = isHistory ? this._collapsedHistoryPlaylists : this._collapsedPlaylists;
    const next = new Set(target);
    
    if (next.has(playlistId)) {
      next.delete(playlistId);
    } else {
      next.add(playlistId);
    }

    if (isHistory) {
      this._collapsedHistoryPlaylists = next;
    } else {
      this._collapsedPlaylists = next;
    }
  }

  private pendingFileChecks = new Set<string>();
  private fileCheckDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  async checkFileExists(id: string, filePath: string | undefined): Promise<void> {
    if (!filePath) return;
    if (this.missingFiles.has(id) || this.pendingFileChecks.has(id)) return;
    
    this.pendingFileChecks.add(id);
    
    if (this.fileCheckDebounceTimer) clearTimeout(this.fileCheckDebounceTimer);
    this.fileCheckDebounceTimer = setTimeout(async () => {
      const checksToRun = [...this.pendingFileChecks];
      this.pendingFileChecks.clear();
      
      const BATCH_SIZE = 10;
      for (let i = 0; i < checksToRun.length; i += BATCH_SIZE) {
        const batch = checksToRun.slice(i, i + BATCH_SIZE);
        await Promise.all(batch.map(async (checkId) => {
          // Find the item's file path from history
          const item = this.unifiedItems.find(u => u.id === checkId);
          if (!item?.filePath) return;
          
          try {
            await stat(item.filePath);
          } catch {
            this.updateSet('missingFiles', checkId, 'add');
          }
        }));
      }
    }, 150);
  }

  isFileMissing(id: string): boolean {
    return this.missingFiles.has(id);
  }

  getThumbnailSrc(thumbnail: string | undefined): string | undefined {
    if (!thumbnail) return undefined;

    const cached = this.thumbnailSrcCache.get(thumbnail);
    if (cached) return cached;

    const isLocalPath = /^[A-Z]:\\/i.test(thumbnail) || thumbnail.startsWith('/');
    const result = isLocalPath ? convertFileSrc(thumbnail) : thumbnail;

    this.thumbnailSrcCache.set(thumbnail, result);
    return result;
  }

  markThumbnailFailed(id: string): void {
    this.updateSet('failedThumbnails', id, 'add', CACHE_SIZES.failedThumbnails);
  }

  isThumbnailFailed(id: string): boolean {
    return this.failedThumbnails.has(id);
  }

  getPlaylistGridThumbs(playlistId: string, items: { thumbnail?: string }[]): string[] {
    const cached = this.playlistThumbCache.get(playlistId);
    if (cached) return cached;

    const thumbs = items
      .filter((i): i is { thumbnail: string } => !!i.thumbnail)
      .slice(0, 4)
      .map((i) => this.getThumbnailSrc(i.thumbnail) ?? i.thumbnail);

    this.playlistThumbCache.set(playlistId, thumbs);
    return thumbs;
  }

  async extractItemColor(thumbnailUrl: string | undefined): Promise<void> {
    const currentSettings = get(settings);
    if (!currentSettings.thumbnailTheming || !thumbnailUrl) return;
    if (this.colorCache.has(thumbnailUrl)) return;

    const cachedColor = await getCachedColorAsync(thumbnailUrl);
    if (cachedColor) {
      this.colorCache.set(thumbnailUrl, cachedColor);
      return;
    }

    const color = await extractDominantColor(thumbnailUrl);
    if (color) {
      this.colorCache.set(thumbnailUrl, color);
    }
  }

  getItemColorStyle(thumbnailUrl: string | undefined): string {
    const currentSettings = get(settings);
    if (!currentSettings.thumbnailTheming || !thumbnailUrl) return '';
    
    const color = this.colorCache.get(thumbnailUrl) ?? getCachedColor(thumbnailUrl);
    return color ? generateColorVars(color) : '';
  }
  
  getItemSizeDisplay(item: UnifiedDownloadItem): string {
    if (item.isActive && (item.status === 'downloading' || item.status === 'processing' || item.status === 'fetching-info')) {
      const parts: string[] = [];
      if (item.speed && !['na', 'unknown', 'n/a', '~', ''].includes(item.speed.toLowerCase())) {
        parts.push(item.speed);
      }
      if (item.eta && !['na', 'unknown', 'n/a', '~', ''].includes(item.eta.toLowerCase())) {
        parts.push(item.eta);
      }
      if (parts.length > 0) {
        return parts.join(' • ');
      }
    }
    return formatSize(item.size);
  }

  getItemSubtitle(item: UnifiedDownloadItem): { text: string; type: 'author' | 'status' | 'error' } {
    if (item.status === 'failed' && item.error) {
      return { text: item.error.slice(0, 60), type: 'error' };
    }
    if (item.isActive) {
      const progress = Math.max(0, Math.min(100, Math.round(item.progress ?? 0)));
      if (item.status === 'paused') return { text: translate('downloads.queue.paused'), type: 'status' };
      if (item.status === 'pending') return { text: translate('downloads.queue.waiting'), type: 'status' };
      if (item.status === 'downloading' || item.status === 'processing' || item.status === 'fetching-info') return { text: `${progress}%`, type: 'status' };
      return { text: item.author, type: 'author' };
    }
    return { text: item.author, type: 'author' };
  }

  private updateSet(
    field: 'missingFiles' | 'failedThumbnails',
    id: string,
    action: 'add' | 'delete',
    maxSize?: number
  ): void {
    const current = this[field];
    const next = new Set(current);

    if (action === 'add') {
      next.add(id);
      // Evict oldest if over limit
      if (maxSize && next.size > maxSize) {
        const iterator = next.values();
        const toRemove = Math.min(20, next.size - maxSize);
        for (let i = 0; i < toRemove; i++) {
          const oldest = iterator.next().value;
          if (oldest) next.delete(oldest);
        }
      }
    } else {
      next.delete(id);
    }

    this[field] = next;
  }

  itemsPerRow = $derived.by(() => {
    const minColumnWidth = this.gridItemSize;
    const { gap } = GRID_CONFIG;
    const width = this.containerWidth;
    
    if (!Number.isFinite(width) || width <= 0) return 1;
    return Math.max(1, Math.floor((width + gap) / (minColumnWidth + gap)));
  });

  private unifiedHistory = $derived.by<UnifiedDownloadItem[]>(() => {
    const historyItems: UnifiedDownloadItem[] = [];
    for (const group of this.historyGroups) {
      for (const item of group.items) {
        if (isPlaylistGroup(item)) {
          for (const child of item.items) {
            historyItems.push(historyToUnified(child));
          }
        } else {
          historyItems.push(historyToUnified(item));
        }
      }
    }
    return historyItems;
  });

  private unifiedQueue = $derived.by<UnifiedDownloadItem[]>(() => {
    const { groups, singles } = this.activeDownloadData;
    const queueItems: UnifiedDownloadItem[] = [];
    
    for (const item of singles) {
      if (item.status !== 'completed') {
        queueItems.push(queueToUnified(item));
      }
    }
    
    for (const group of groups) {
      for (const item of group.items) {
        if (item.status !== 'completed') {
          queueItems.push(queueToUnified(item));
        }
      }
    }
    return queueItems;
  });

  private unifiedItems = $derived.by<UnifiedDownloadItem[]>(() => {
    let allItems = [...this.unifiedQueue, ...this.unifiedHistory];
    
    if (this.activeFilter !== 'all') {
      allItems = allItems.filter(item => item.type === this.activeFilter);
    }

    const query = this.searchQuery.trim().toLowerCase();
    if (query) {
      allItems = allItems.filter(item => {
        const text = `${item.title} ${item.author} ${item.url}`.toLowerCase();
        return calculateMatchScore(text, query) > 0;
      });
    }

    const direction = this.sortDirection === 'asc' ? 1 : -1;
    switch (this.sortType) {
      case 'date':
        allItems.sort((a, b) => direction * (a.addedAt - b.addedAt));
        break;
      case 'name':
        allItems.sort((a, b) => direction * a.title.localeCompare(b.title));
        break;
      case 'size':
        allItems.sort((a, b) => direction * (a.size - b.size));
        break;
      case 'duration':
        allItems.sort((a, b) => direction * (a.duration - b.duration));
        break;
      case 'format':
        allItems.sort((a, b) => direction * a.extension.localeCompare(b.extension));
        break;
    }

    return allItems;
  });

  private flatRows = $derived.by<VirtualListItem[]>(() => {
    if (this.viewMode === 'grid') return [];

    const rows: VirtualListItem[] = [];
    const items = this.unifiedItems;

    if (this.sortType === 'date') {
      let lastDateLabel = '';
      
      for (const item of items) {
        const date = new Date(item.addedAt);
        const label = this.getDateLabel(date);

        if (label !== lastDateLabel) {
          rows.push({ kind: 'date', label: label, id: `date-${label}` });
          lastDateLabel = label;
        }

        rows.push({
          kind: 'single',
          item,
          dateLabel: label,
          id: `s-${label}-${item.id}`,
        });
      }
    } else {
      for (const item of items) {
        rows.push({
          kind: 'single',
          item,
          dateLabel: '',
          id: `s-${item.id}`,
        });
      }
    }

    return rows;
  });

  private getDateLabel(date: Date): string {
    const now = new Date();
    
    if (isNaN(date.getTime())) {
       return 'Unknown Date';
    }

    if (date.toDateString() === now.toDateString()) {
      return 'Today';
    }
    
    const yesterday = new Date(now);
    yesterday.setDate(yesterday.getDate() - 1);
    if (date.toDateString() === yesterday.toDateString()) {
      return 'Yesterday';
    }
    
    return date.toLocaleDateString(undefined, { 
      year: 'numeric', 
      month: 'long', 
      day: 'numeric' 
    });
  }

  private gridRows = $derived.by<VirtualListItem[]>(() => {
    if (this.viewMode !== 'grid') return [];

    const allItems = this.unifiedItems;
    const rows: VirtualListItem[] = [];
    const perRow = this.itemsPerRow;

    for (let i = 0; i < allItems.length; i += perRow) {
      rows.push({
        kind: 'grid-row',
        items: allItems.slice(i, i + perRow),
        id: `grid-row-${i}`,
      });
    }

    return rows;
  });

  displayItems = $derived.by(() => {
    return this.viewMode === 'grid' ? this.gridRows : this.flatRows;
  });

  activeDownloadGroups = $derived.by(() => {
    const { groups, singles } = this.activeDownloadData;
    const query = this.searchQuery.trim();

    if (!query) return { groups, singles };

    const matchesQuery = (item: QueueItem): boolean => {
      const text = `${item.title} ${item.author} ${item.url}`;
      return calculateMatchScore(text, query) > 0;
    };

    return {
      groups: groups
        .map((g) => ({ ...g, items: g.items.filter(matchesQuery) }))
        .filter((g) => g.items.length > 0),
      singles: singles.filter(matchesQuery),
    };
  });
}
