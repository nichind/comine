import {
  history,
  type FilterType,
  type SortType,
  type HistoryItem,
  type HistoryPlaylistGroup,
  isPlaylistGroup,
} from '$lib/stores/history';
import { formatSize } from '$lib/utils/format';
import { type QueueItem, type PlaylistGroup } from '$lib/stores/queue';
import { settings, updateSettings } from '$lib/stores/settings';
import { get } from 'svelte/store';
import { calculateMatchScore } from '$lib/utils/search';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import {
  extractDominantColor,
  generateColorVars,
  getCachedColor,
  getCachedColorAsync,
  setColorInCache,
  type RGB,
} from '$lib/utils/color';
import { stat } from '@tauri-apps/plugin-fs';
import { LRUCache } from '$lib/utils/LRUCache';
import { translate } from '$lib/i18n';

export type SortDirection = 'asc' | 'desc';

export interface UnifiedDownloadItem {
  id: string;
  url: string;
  title: string;
  author: string;
  authorUrl?: string;
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
  isFavourite?: boolean;
  status?:
    | 'pending'
    | 'paused'
    | 'fetching-info'
    | 'downloading'
    | 'processing'
    | 'converting'
    | 'completed'
    | 'failed';
  statusMessage?: string;
  progress?: number;
  speed?: string;
  eta?: string;
  error?: string;
  priority?: number;
  convertedFormat?: string;
  source?: 'ytdlp' | 'file' | 'convert';
  downloadSource?: string;
}

export type VirtualListItem =
  | { kind: 'date'; label: string; id: string }
  | { kind: 'single'; item: UnifiedDownloadItem; dateLabel: string; id: string }
  | {
      kind: 'playlist-header';
      groupKey: string;
      playlistTitle: string;
      childCount: number;
      isExpanded: boolean;
      totalSize: number;
      totalDuration: number;
      dateLabel: string;
      id: string;
    }
  | {
      kind: 'playlist-child';
      item: UnifiedDownloadItem;
      groupKey: string;
      isLast: boolean;
      id: string;
    }
  | { kind: 'grid-row'; items: UnifiedDownloadItem[]; id: string }
  | {
      kind: 'grid-playlist-header';
      groupKey: string;
      playlistTitle: string;
      itemCount: number;
      isExpanded: boolean;
      id: string;
    };

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
    authorUrl: item.authorUrl,
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
    isFavourite: item.isFavourite,
    status: 'completed',
    progress: 100,
    convertedFormat: item.convertedFormat,
    downloadSource: item.downloadSource,
  };
}

function queueToUnified(item: QueueItem): UnifiedDownloadItem {
  return {
    id: item.id,
    url: item.url,
    title: item.title || 'Loading...',
    author: item.author || '',
    authorUrl: item.authorUrl,
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
    statusMessage: item.statusMessage,
    progress: item.progress || 0,
    speed: item.speed,
    eta: item.eta,
    error: item.error,
    priority: item.priority,
    source: item.source,
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
  playlistHeader: 56,
  playlistChild: 56,
} as const;

export function computeGridRowHeight(containerWidth: number, itemsPerRow: number): number {
  const gap = 10;
  const columnWidth = (containerWidth - gap * (itemsPerRow - 1)) / itemsPerRow;
  const thumbnailHeight = columnWidth * (9 / 16);
  const infoHeight = 58;
  const rowGap = 10;
  return Math.ceil(thumbnailHeight + infoHeight + rowGap);
}

export type ColumnKey = 'format' | 'size' | 'duration';

export class DownloadsState {
  searchQuery = $state('');
  activeFilter = $state<FilterType>('all');
  sortType = $state<SortType>('date');
  sortDirection = $state<SortDirection>('desc');
  viewMode = $state<'list' | 'grid'>('list');
  gridItemSize = $state(200);
  hoveredItemId = $state<string | null>(null);
  containerWidth = $state(800);
  visibleColumns = $state<ColumnKey[]>(['format', 'size', 'duration']);
  hideMissingFiles = $state(false);
  showSourceTags = $state(true);
  ungroupPlaylistsOnSort = $state(false);

  selectedItemIds = $state(new Set<string>());
  lastSelectedItemId = $state<string | null>(null);
  isSelectionMode = $derived(this.selectedItemIds.size > 0);

  private _collapsedHistoryPlaylists = $state(new Set<string>());
  private missingFiles = $state(new Set<string>());
  private failedThumbnails = $state(new Set<string>());

  get collapsedHistoryPlaylists(): ReadonlySet<string> {
    return this._collapsedHistoryPlaylists;
  }

  private readonly colorCache = new LRUCache<string, RGB>(CACHE_SIZES.colors);
  private readonly thumbnailSrcCache = new LRUCache<string, string>(CACHE_SIZES.thumbnails);
  private readonly playlistThumbCache = new LRUCache<string, string[]>(CACHE_SIZES.playlistThumbs);
  private readonly localThumbnailCache = new LRUCache<string, string>(CACHE_SIZES.thumbnails);
  private localThumbnailPending = new Set<string>();

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

    const isMobile =
      typeof window !== 'undefined' && window.matchMedia('(max-width: 700px)').matches;
    const defaultColumns: ColumnKey[] = isMobile ? ['size'] : ['format', 'size', 'duration'];
    this.visibleColumns = currentSettings.downloadsVisibleColumns ?? defaultColumns;

    this.hideMissingFiles = currentSettings.hideMissingFiles ?? false;
    this.showSourceTags = currentSettings.showSourceTags ?? true;
    this.ungroupPlaylistsOnSort = currentSettings.downloadsUngroupPlaylistsOnSort ?? false;
  }

  private setupEffects(): void {
    this.cleanupEffects = $effect.root(() => {
      $effect(() => {
        history.setSort(this.sortType);
      });
      $effect(() => {
        const filterForHistory = this.activeFilter === 'favourites' ? 'all' : this.activeFilter;
        history.setFilter(filterForHistory);
      });
      $effect(() => {
        history.setSearch(this.searchQuery);
      });

      $effect(() => {
        updateSettings({
          downloadsViewMode: this.viewMode,
          downloadsSortType: this.sortType,
          downloadsSortDirection: this.sortDirection,
          gridItemSize: this.gridItemSize,
          downloadsVisibleColumns: this.visibleColumns,
          hideMissingFiles: this.hideMissingFiles,
          showSourceTags: this.showSourceTags,
          downloadsUngroupPlaylistsOnSort: this.ungroupPlaylistsOnSort,
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
    this.localThumbnailCache.clear();
    this.localThumbnailPending.clear();
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

  isColumnVisible(column: ColumnKey): boolean {
    return this.visibleColumns.includes(column);
  }

  toggleColumn(column: ColumnKey): void {
    if (this.visibleColumns.includes(column)) {
      this.visibleColumns = this.visibleColumns.filter((c) => c !== column);
    } else {
      const order: ColumnKey[] = ['format', 'size', 'duration'];
      const newColumns = order.filter((c) => c === column || this.visibleColumns.includes(c));
      this.visibleColumns = newColumns;
    }
  }

  toggleUngroupPlaylistsOnSort(): void {
    this.ungroupPlaylistsOnSort = !this.ungroupPlaylistsOnSort;
  }

  toggleHideMissingFiles(): void {
    this.hideMissingFiles = !this.hideMissingFiles;
  }

  toggleShowSourceTags(): void {
    this.showSourceTags = !this.showSourceTags;
  }

  setHoveredItem(id: string | null): void {
    this.hoveredItemId = id;
  }

  toggleSelection(id: string, multi: boolean, range: boolean): void {
    const next = new Set(this.selectedItemIds);

    if (range && this.lastSelectedItemId) {
      const flatItems = this.flatRows
        .map((r) => ('id' in r && r.kind === 'single' ? r.item.id : null))
        .filter(Boolean) as string[];
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

  getItemById(id: string): UnifiedDownloadItem | undefined {
    return this.unifiedItems.find((item) => item.id === id);
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

  togglePlaylist(groupKey: string): void {
    const next = new Set(this._collapsedHistoryPlaylists);

    if (next.has(groupKey)) {
      next.delete(groupKey);
    } else {
      next.add(groupKey);
    }

    this._collapsedHistoryPlaylists = next;
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
        await Promise.all(
          batch.map(async (checkId) => {
            // Find the item's file path from history
            const item = this.unifiedItems.find((u) => u.id === checkId);
            if (!item?.filePath) return;

            try {
              await stat(item.filePath);
            } catch {
              this.updateSet('missingFiles', checkId, 'add');
            }
          })
        );
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

  getLocalThumbnail(id: string): string | undefined {
    return this.localThumbnailCache.get(id);
  }

  async generateLocalThumbnail(id: string, filePath: string | undefined): Promise<string | null> {
    if (!filePath) return null;
    if (this.localThumbnailPending.has(id)) return null;

    // Check cache first
    const cached = this.localThumbnailCache.get(id);
    if (cached) return cached;

    this.localThumbnailPending.add(id);

    try {
      const result = await invoke<string>('generate_local_thumbnail', {
        filePath,
        itemId: id,
      });

      // Convert file:// URL to convertFileSrc format for Tauri
      const thumbnailPath = result.replace('file://', '');
      const srcUrl = convertFileSrc(thumbnailPath);

      // Precompute dominant color for local thumbnails. This avoids relying on canvas,
      // and it works even though srcUrl is an asset://... URL.
      try {
        const rgbArr = await invoke<[number, number, number]>('extract_local_thumbnail_color', {
          path: thumbnailPath,
        });
        const color: RGB = { r: rgbArr[0], g: rgbArr[1], b: rgbArr[2] };
        this.colorCache.set(srcUrl, color);
        setColorInCache(srcUrl, color);
      } catch {}

      this.localThumbnailCache.set(id, srcUrl);
      return srcUrl;
    } catch (e) {
      console.debug('Failed to generate local thumbnail:', e);
      return null;
    } finally {
      this.localThumbnailPending.delete(id);
    }
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
    if (
      item.isActive &&
      (item.status === 'downloading' ||
        item.status === 'processing' ||
        item.status === 'converting' ||
        item.status === 'fetching-info')
    ) {
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

  getItemSubtitle(item: UnifiedDownloadItem): {
    text: string;
    type: 'author' | 'status' | 'error';
  } {
    if (item.status === 'failed' && item.error) {
      return { text: item.error.slice(0, 60), type: 'error' };
    }
    if (item.isActive) {
      const progress = Math.max(0, Math.min(100, Math.round(item.progress ?? 0)));
      if (item.status === 'paused')
        return { text: translate('downloads.queue.paused'), type: 'status' };
      if (item.status === 'pending')
        return { text: translate('downloads.queue.waiting'), type: 'status' };
      if (
        item.status === 'converting' ||
        item.status === 'downloading' ||
        item.status === 'processing' ||
        item.status === 'fetching-info'
      ) {
        const translatedStatus =
          item.status === 'fetching-info'
            ? translate('downloads.status.fetchingInfo')
            : translate(`downloads.status.${item.status}`);
        const label =
          item.statusMessage ||
          translatedStatus ||
          (item.status === 'fetching-info' ? 'Fetching info' : item.status);
        return {
          text: label ? `${label} ${progress}%` : `${progress}%`,
          type: 'status',
        };
      }
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

    // Type filter (video, audio, image, file)
    if (this.activeFilter !== 'all' && this.activeFilter !== 'favourites') {
      allItems = allItems.filter((item) => item.type === this.activeFilter);
    }

    // Favourites filter
    if (this.activeFilter === 'favourites') {
      allItems = allItems.filter((item) => item.isFavourite);
    }

    // Filter out missing files if enabled
    if (this.hideMissingFiles) {
      allItems = allItems.filter((item) => item.isActive || !this.missingFiles.has(item.id));
    }

    const query = this.searchQuery.trim().toLowerCase();
    if (query) {
      allItems = allItems.filter((item) => {
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

  private getUnifiedSortKey(item: UnifiedDownloadItem): number | string {
    switch (this.sortType) {
      case 'date':
        return item.addedAt;
      case 'name':
        return item.title.toLowerCase();
      case 'size':
        return item.size;
      case 'duration':
        return item.duration;
      case 'format':
        return (item.extension ?? '').toLowerCase();
    }
  }

  private flatRows = $derived.by<VirtualListItem[]>(() => {
    if (this.viewMode === 'grid') return [];

    const shouldUngroup = this.ungroupPlaylistsOnSort && this.sortType !== 'date';
    if (shouldUngroup) {
      return this.unifiedItems.map((item) => ({
        kind: 'single',
        item,
        dateLabel: this.getDateLabel(new Date(item.addedAt)),
        id: `s-${item.id}`,
      }));
    }

    const { groups: activeGroups, singles: activeSingles } = this.activeDownloadData;

    // Build a unified list of all items (active + history)
    // Each item will have its sort key for proper ordering
    type UnifiedEntry =
      | { type: 'single'; item: UnifiedDownloadItem; sortKey: number | string }
      | {
          type: 'playlist';
          playlistId: string;
          playlistTitle: string;
          items: UnifiedDownloadItem[];
          sortKey: number | string;
        };

    const entries: UnifiedEntry[] = [];
    const playlistMap = new Map<string, { playlistTitle: string; items: UnifiedDownloadItem[] }>();

    // Add active singles
    for (const qItem of activeSingles) {
      const item = queueToUnified(qItem);

      // Apply filters
      if (this.activeFilter !== 'all' && this.activeFilter !== 'favourites') {
        if (item.type !== this.activeFilter) continue;
      }

      const query = this.searchQuery.trim().toLowerCase();
      if (query) {
        const text = `${item.title} ${item.author} ${item.url}`.toLowerCase();
        if (calculateMatchScore(text, query) === 0) continue;
      }

      entries.push({
        type: 'single',
        item,
        sortKey: this.getUnifiedSortKey(item),
      });
    }

    // Add active playlist items to playlistMap
    for (const group of activeGroups) {
      const items = group.items.map(queueToUnified);
      playlistMap.set(group.playlistId, {
        playlistTitle: group.playlistTitle,
        items,
      });
    }

    // Process history items
    for (const dateGroup of this.historyGroups) {
      for (const entry of dateGroup.items) {
        if (isPlaylistGroup(entry)) {
          // Get or create playlist entry
          let playlistEntry = playlistMap.get(entry.playlistId);
          if (!playlistEntry) {
            playlistEntry = { playlistTitle: entry.playlistTitle, items: [] };
            playlistMap.set(entry.playlistId, playlistEntry);
          }

          // Apply filters to history items and add them
          let filteredItems = entry.items;

          if (this.activeFilter !== 'all' && this.activeFilter !== 'favourites') {
            filteredItems = filteredItems.filter((item) => item.type === this.activeFilter);
          }
          if (this.activeFilter === 'favourites') {
            filteredItems = filteredItems.filter((item) => item.isFavourite);
          }
          if (this.hideMissingFiles) {
            filteredItems = filteredItems.filter((item) => !this.missingFiles.has(item.id));
          }

          const query = this.searchQuery.trim().toLowerCase();
          if (query) {
            filteredItems = filteredItems.filter((item) => {
              const text = `${item.title} ${item.author} ${item.url}`.toLowerCase();
              return calculateMatchScore(text, query) > 0;
            });
          }

          // Append history items to playlist (active items are already there)
          playlistEntry.items.push(...filteredItems.map(historyToUnified));
        } else {
          // Single history item
          const item = entry as HistoryItem;

          if (this.activeFilter !== 'all' && this.activeFilter !== 'favourites') {
            if (item.type !== this.activeFilter) continue;
          }
          if (this.activeFilter === 'favourites' && !item.isFavourite) continue;
          if (this.hideMissingFiles && this.missingFiles.has(item.id)) continue;

          const query = this.searchQuery.trim().toLowerCase();
          if (query) {
            const text = `${item.title} ${item.author} ${item.url}`.toLowerCase();
            if (calculateMatchScore(text, query) === 0) continue;
          }

          const unified = historyToUnified(item);
          entries.push({
            type: 'single',
            item: unified,
            sortKey: this.getUnifiedSortKey(unified),
          });
        }
      }
    }

    // Convert playlistMap to entries
    for (const [playlistId, playlist] of playlistMap) {
      if (playlist.items.length === 0) continue;

      // Sort items within playlist by playlistIndex, then by addedAt
      playlist.items.sort((a, b) => {
        if (a.playlistIndex !== undefined && b.playlistIndex !== undefined) {
          return a.playlistIndex - b.playlistIndex;
        }
        return b.addedAt - a.addedAt;
      });

      // Use the most recent item's timestamp for sorting the playlist
      const latestTime = Math.max(...playlist.items.map((i) => i.addedAt));
      const totalSize = playlist.items.reduce((sum, i) => sum + (i.size || 0), 0);
      const totalDuration = playlist.items.reduce((sum, i) => sum + (i.duration || 0), 0);

      const formatKey =
        playlist.items
          .map((i) => (i.extension ?? '').toLowerCase())
          .filter(Boolean)
          .sort()[0] ?? '';

      entries.push({
        type: 'playlist',
        playlistId,
        playlistTitle: playlist.playlistTitle,
        items: playlist.items,
        sortKey:
          this.sortType === 'date'
            ? latestTime
            : this.sortType === 'name'
              ? playlist.playlistTitle.toLowerCase()
              : this.sortType === 'size'
                ? totalSize
                : this.sortType === 'duration'
                  ? totalDuration
                  : formatKey,
      });
    }

    // Sort all entries with stable secondary sort
    entries.sort((a, b) => {
      const aKey = a.sortKey;
      const bKey = b.sortKey;

      let cmp = 0;
      if (typeof aKey === 'number' && typeof bKey === 'number') {
        cmp = this.sortDirection === 'desc' ? bKey - aKey : aKey - bKey;
      } else if (typeof aKey === 'string' && typeof bKey === 'string') {
        const strCmp = aKey.localeCompare(bKey);
        cmp = this.sortDirection === 'desc' ? -strCmp : strCmp;
      }

      // Stable secondary sort by ID
      if (cmp === 0) {
        const aId = a.type === 'single' ? a.item.id : a.playlistId;
        const bId = b.type === 'single' ? b.item.id : b.playlistId;
        cmp = aId.localeCompare(bId);
      }

      return cmp;
    });

    // Build rows with date headers if sorting by date
    const rows: VirtualListItem[] = [];
    let currentDateLabel = '';

    for (const entry of entries) {
      if (entry.type === 'single') {
        const item = entry.item;
        const dateLabel = this.getDateLabel(new Date(item.addedAt));

        if (this.sortType === 'date' && dateLabel !== currentDateLabel) {
          rows.push({ kind: 'date', label: dateLabel, id: `date-${dateLabel}` });
          currentDateLabel = dateLabel;
        }

        rows.push({
          kind: 'single',
          item,
          dateLabel,
          id: `s-${item.id}`,
        });
      } else {
        // Playlist - FLATTENED: header + individual children
        const latestTime = Math.max(...entry.items.map((i) => i.addedAt));
        const dateLabel = this.getDateLabel(new Date(latestTime));

        if (this.sortType === 'date' && dateLabel !== currentDateLabel) {
          rows.push({ kind: 'date', label: dateLabel, id: `date-${dateLabel}` });
          currentDateLabel = dateLabel;
        }

        const groupKey = entry.playlistId;
        const isExpanded = !this._collapsedHistoryPlaylists.has(groupKey);
        const totalSize = entry.items.reduce((sum, i) => sum + (i.size || 0), 0);
        const totalDuration = entry.items.reduce((sum, i) => sum + (i.duration || 0), 0);

        rows.push({
          kind: 'playlist-header',
          groupKey,
          playlistTitle: entry.playlistTitle,
          childCount: entry.items.length,
          isExpanded,
          totalSize,
          totalDuration,
          dateLabel,
          id: `playlist-${groupKey}`,
        });

        if (isExpanded) {
          entry.items.forEach((item, idx) => {
            rows.push({
              kind: 'playlist-child',
              item,
              groupKey,
              isLast: idx === entry.items.length - 1,
              id: `pc-${item.id}`,
            });
          });
        }
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
      day: 'numeric',
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
