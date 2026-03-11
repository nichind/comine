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
import { logs } from '$lib/stores/logs';

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
  type: 'video' | 'audio' | 'image' | 'file' | 'gallery' | 'torrent';
  isDirectory?: boolean;
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
  convertedFormat?: string;
  source?: 'ytdlp' | 'file' | 'convert';
  downloadSource?: string;
  podcastPath?: string;
  podcastStatus?: string;
  podcastProgress?: number;
  podcastStep?: string;
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
  | { kind: 'grid-row'; items: UnifiedDownloadItem[]; id: string };

interface HistoryDateGroup {
  label: string;
  items: (HistoryItem | HistoryPlaylistGroup)[];
}

interface ActiveDownloadData {
  groups: PlaylistGroup[];
  singles: QueueItem[];
}

function matchesFilter(
  item: { type: string; isFavourite?: boolean; id: string },
  filter: FilterType,
  hideMissing: boolean,
  missingFiles: ReadonlySet<string>,
  isActive = false
): boolean {
  if (filter !== 'all' && filter !== 'favourites' && item.type !== filter) return false;
  if (filter === 'favourites' && !item.isFavourite) return false;
  if (hideMissing && !isActive && missingFiles.has(item.id)) return false;
  return true;
}

function matchesSearch(
  item: { title: string; author: string; url: string },
  query: string
): boolean {
  if (!query) return true;
  const text = `${item.title} ${item.author} ${item.url}`.toLowerCase();
  return calculateMatchScore(text, query) > 0;
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
    isDirectory: item.isDirectory,
    playlistId: item.playlistId,
    playlistTitle: item.playlistTitle,
    playlistIndex: item.playlistIndex,
    isActive: false,
    isFavourite: item.isFavourite,
    status: 'completed',
    progress: 100,
    convertedFormat: item.convertedFormat,
    downloadSource: item.downloadSource,
    podcastPath: item.podcastPath ?? undefined,
    podcastStatus: item.podcastStatus ?? undefined,
    podcastProgress: item.podcastProgress,
    podcastStep: item.podcastStep,
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
    source: item.source,
  };
}

const GRID_GAP = 10;

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

export function getListGridTemplate(
  visibleColumns: ColumnKey[],
  listItemSize: number = 56
): string {
  const thumbHeight = listItemSize - 16;
  const thumbWidth = Math.round(thumbHeight * (16 / 9));
  const cols = [`${thumbWidth}px`, '1fr'];
  if (visibleColumns.includes('format')) cols.push('50px');
  if (visibleColumns.includes('size')) cols.push('70px');
  if (visibleColumns.includes('duration')) cols.push('60px');
  return cols.join(' ');
}

export class DownloadsState {
  searchQuery = $state('');
  activeFilter = $state<FilterType>('all');
  sortType = $state<SortType>('date');
  sortDirection = $state<SortDirection>('desc');
  viewMode = $state<'list' | 'grid'>('list');
  gridItemSize = $state(200);
  listItemSize = $state(56);
  hoveredItemId = $state<string | null>(null);
  containerWidth = $state(800);
  visibleColumns = $state<ColumnKey[]>(['format', 'size', 'duration']);
  hideMissingFiles = $state(false);
  showSourceTags = $state(true);
  ungroupPlaylistsOnSort = $state(false);

  selectedItemIds = $state.raw(new Set<string>());
  lastSelectedItemId = $state<string | null>(null);
  isSelectionMode = $derived(this.selectedItemIds.size > 0);

  private _collapsedHistoryPlaylists = $state.raw(new Set<string>());
  private missingFiles = $state.raw(new Set<string>());
  private failedThumbnails = $state.raw(new Set<string>());

  private readonly localThumbnailCache = new LRUCache<string, string>(300);
  private thumbnailPromises = new LRUCache<string, Promise<string | null>>(300);

  // Use $state.raw to avoid deep-proxying these large nested structures.
  // historyGroups contains ALL history items nested in date/playlist groups —
  // with $state, every HistoryItem (19 fields) gets its own Proxy wrapper.
  // These are replaced wholesale, so fine-grained reactivity isn't needed.
  historyGroups = $state.raw<HistoryDateGroup[]>([]);
  activeDownloadData = $state.raw<ActiveDownloadData>({ groups: [], singles: [] });

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
    this.listItemSize = currentSettings.listItemSize ?? 56;

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
          listItemSize: this.listItemSize,
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
    this.localThumbnailCache.clear();
    this.thumbnailPromises.clear();
  }

  toggleSelection(id: string, multi: boolean, range: boolean): void {
    const next = new Set(this.selectedItemIds);

    if (range && this.lastSelectedItemId) {
      const flatItems = this.flatRows
        .map((r) => {
          if (r.kind === 'single') return r.item.id;
          if (r.kind === 'playlist-child') return r.item.id;
          return null;
        })
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

  toggleGroupSelection(groupKey: string): void {
    const childIds = this.flatRows
      .filter((r) => r.kind === 'playlist-child' && r.groupKey === groupKey)
      .map((r) => (r as { kind: 'playlist-child'; item: UnifiedDownloadItem }).item.id);

    if (childIds.length === 0) return;

    const next = new Set(this.selectedItemIds);
    const allSelected = childIds.every((id) => next.has(id));
    for (const id of childIds) {
      if (allSelected) next.delete(id);
      else next.add(id);
    }
    this.selectedItemIds = next;
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
    const isLocalPath = /^[A-Z]:\\/i.test(thumbnail) || thumbnail.startsWith('/');
    return isLocalPath ? convertFileSrc(thumbnail) : thumbnail;
  }

  markThumbnailFailed(id: string): void {
    this.updateSet('failedThumbnails', id, 'add', 100);
  }

  isThumbnailFailed(id: string): boolean {
    return this.failedThumbnails.has(id);
  }

  getLocalThumbnail(id: string): string | undefined {
    return this.localThumbnailCache.get(id);
  }

  generateLocalThumbnail(id: string, filePath: string | undefined): Promise<string | null> {
    if (!filePath) return Promise.resolve(null);
    return this.cachedThumbnailOp(id, () => this._doGenerateLocalThumbnail(id, filePath));
  }

  private async _doGenerateLocalThumbnail(id: string, filePath: string): Promise<string | null> {
    try {
      const result = await invoke<string>('generate_local_thumbnail', {
        filePath,
        itemId: id,
      });

      const thumbnailPath = result.replace('file://', '');
      const srcUrl = convertFileSrc(thumbnailPath);

      try {
        const rgbArr = await invoke<[number, number, number]>('extract_local_thumbnail_color', {
          path: thumbnailPath,
        });
        const color: RGB = { r: rgbArr[0], g: rgbArr[1], b: rgbArr[2] };
        setColorInCache(srcUrl, color);
      } catch {}

      this.localThumbnailCache.set(id, srcUrl);
      return srcUrl;
    } catch (e) {
      logs.debug('downloads', `Failed to generate local thumbnail: ${e}`);
      return null;
    }
  }

  ensureCachedThumbnail(id: string, url: string): Promise<string | null> {
    return this.cachedThumbnailOp(id, () => this._doEnsureCachedThumbnail(id, url));
  }

  private async _doEnsureCachedThumbnail(id: string, url: string): Promise<string | null> {
    try {
      const localPath = await invoke<string>('get_or_cache_thumbnail', {
        url,
        itemId: id,
      });

      const srcUrl = convertFileSrc(localPath);
      this.localThumbnailCache.set(id, srcUrl);
      return srcUrl;
    } catch (e) {
      logs.debug('downloads', `Failed to cache thumbnail: ${e}`);
      return null;
    }
  }

  async extractItemColor(thumbnailUrl: string | undefined): Promise<void> {
    const currentSettings = get(settings);
    if (!currentSettings.thumbnailTheming || !thumbnailUrl) return;
    if (getCachedColor(thumbnailUrl)) return;

    const cachedColor = await getCachedColorAsync(thumbnailUrl);
    if (cachedColor) return;

    await extractDominantColor(thumbnailUrl);
  }

  getItemColorStyle(thumbnailUrl: string | undefined): string {
    const currentSettings = get(settings);
    if (!currentSettings.thumbnailTheming || !thumbnailUrl) return '';

    const color = getCachedColor(thumbnailUrl);
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

  private cachedThumbnailOp(id: string, op: () => Promise<string | null>): Promise<string | null> {
    const existing = this.thumbnailPromises.get(id);
    if (existing) return existing;

    const cached = this.localThumbnailCache.get(id);
    if (cached) {
      const resolved = Promise.resolve(cached);
      this.thumbnailPromises.set(id, resolved);
      return resolved;
    }

    const promise = op();
    this.thumbnailPromises.set(id, promise);
    return promise;
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
    const width = this.containerWidth;

    if (!Number.isFinite(width) || width <= 0) return 1;
    return Math.max(1, Math.floor((width + GRID_GAP) / (minColumnWidth + GRID_GAP)));
  });

  private unifiedItems = $derived.by<UnifiedDownloadItem[]>(() => {
    const query = this.searchQuery.trim().toLowerCase();
    const filter = this.activeFilter;
    const hideMissing = this.hideMissingFiles;
    const missing = this.missingFiles;

    const allItems: UnifiedDownloadItem[] = [];

    const { groups, singles } = this.activeDownloadData;
    for (const item of singles) {
      if (item.status !== 'completed') {
        const unified = queueToUnified(item);
        if (
          matchesFilter(unified, filter, hideMissing, missing, true) &&
          matchesSearch(unified, query)
        ) {
          allItems.push(unified);
        }
      }
    }
    for (const group of groups) {
      for (const item of group.items) {
        if (item.status !== 'completed') {
          const unified = queueToUnified(item);
          if (
            matchesFilter(unified, filter, hideMissing, missing, true) &&
            matchesSearch(unified, query)
          ) {
            allItems.push(unified);
          }
        }
      }
    }

    for (const group of this.historyGroups) {
      for (const item of group.items) {
        if (isPlaylistGroup(item)) {
          for (const child of item.items) {
            const unified = historyToUnified(child);
            if (
              matchesFilter(unified, filter, hideMissing, missing, false) &&
              matchesSearch(unified, query)
            ) {
              allItems.push(unified);
            }
          }
        } else {
          const unified = historyToUnified(item);
          if (
            matchesFilter(unified, filter, hideMissing, missing, false) &&
            matchesSearch(unified, query)
          ) {
            allItems.push(unified);
          }
        }
      }
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
    const shouldUngroup = this.ungroupPlaylistsOnSort && this.sortType !== 'date';
    if (shouldUngroup) {
      return this.unifiedItems.map((item) => ({
        kind: 'single' as const,
        item,
        dateLabel: this.getDateLabel(new Date(item.addedAt)),
        id: `s-${item.id}`,
      }));
    }

    type UnifiedEntry =
      | { type: 'single'; item: UnifiedDownloadItem; sortKey: number | string }
      | {
          type: 'playlist';
          playlistId: string;
          playlistTitle: string;
          items: UnifiedDownloadItem[];
          totalSize: number;
          totalDuration: number;
          sortKey: number | string;
        };

    const entries: UnifiedEntry[] = [];
    const playlistMap = new Map<string, { playlistTitle: string; items: UnifiedDownloadItem[] }>();

    for (const item of this.unifiedItems) {
      if (item.playlistId) {
        let playlistEntry = playlistMap.get(item.playlistId);
        if (!playlistEntry) {
          playlistEntry = { playlistTitle: item.playlistTitle ?? item.playlistId, items: [] };
          playlistMap.set(item.playlistId, playlistEntry);
        }
        playlistEntry.items.push(item);
      } else {
        entries.push({
          type: 'single',
          item,
          sortKey: this.getUnifiedSortKey(item),
        });
      }
    }

    for (const [playlistId, playlist] of playlistMap) {
      if (playlist.items.length === 0) continue;

      playlist.items.sort((a, b) => {
        if (a.playlistIndex !== undefined && b.playlistIndex !== undefined) {
          return a.playlistIndex - b.playlistIndex;
        }
        return b.addedAt - a.addedAt;
      });

      let totalSize = 0;
      let totalDuration = 0;
      let latestTime = 0;
      for (const i of playlist.items) {
        totalSize += i.size || 0;
        totalDuration += i.duration || 0;
        if (i.addedAt > latestTime) latestTime = i.addedAt;
      }

      const formatKey =
        this.sortType === 'format'
          ? (playlist.items
              .map((i) => (i.extension ?? '').toLowerCase())
              .filter(Boolean)
              .sort()[0] ?? '')
          : '';

      entries.push({
        type: 'playlist',
        playlistId,
        playlistTitle: playlist.playlistTitle,
        items: playlist.items,
        totalSize,
        totalDuration,
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

      if (cmp === 0) {
        const aId = a.type === 'single' ? a.item.id : a.playlistId;
        const bId = b.type === 'single' ? b.item.id : b.playlistId;
        cmp = aId.localeCompare(bId);
      }

      return cmp;
    });

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
        const latestTime = Math.max(...entry.items.map((i) => i.addedAt));
        const dateLabel = this.getDateLabel(new Date(latestTime));

        if (this.sortType === 'date' && dateLabel !== currentDateLabel) {
          rows.push({ kind: 'date', label: dateLabel, id: `date-${dateLabel}` });
          currentDateLabel = dateLabel;
        }

        const groupKey = entry.playlistId;
        const isExpanded = !this._collapsedHistoryPlaylists.has(groupKey);

        rows.push({
          kind: 'playlist-header',
          groupKey,
          playlistTitle: entry.playlistTitle,
          childCount: entry.items.length,
          isExpanded,
          totalSize: entry.totalSize,
          totalDuration: entry.totalDuration,
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

  displayItems = $derived.by<VirtualListItem[]>(() => {
    if (this.viewMode === 'list') return this.flatRows;

    const allItems = this.unifiedItems;
    const perRow = this.itemsPerRow;
    const result: VirtualListItem[] = [];
    let currentRowItems: UnifiedDownloadItem[] = [];

    const flushRow = () => {
      if (currentRowItems.length === 0) return;
      result.push({
        kind: 'grid-row',
        items: currentRowItems,
        id: `grid-${currentRowItems[0].id}`,
      });
      currentRowItems = [];
    };

    for (const item of allItems) {
      currentRowItems.push(item);
      if (currentRowItems.length >= perRow) flushRow();
    }
    flushRow();

    return result;
  });
}
