<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '$lib/i18n';
  import { tooltip } from '$lib/actions/tooltip';
  import { settings, type DownloadMode, getProxyConfig, getSettings } from '$lib/stores/settings';
  import ThumbnailGlow from './ThumbnailGlow.svelte';
  import { deps } from '$lib/stores/deps';
  import { formatDuration, formatSize, getDisplayThumbnailUrl } from '$lib/utils/format';
  import Icon from './Icon.svelte';
  import Checkbox from './Checkbox.svelte';
  import Select from './Select.svelte';
  import CollapsibleBlock from './CollapsibleBlock.svelte';
  import MediaGrid, {
    type ViewMode,
    type MediaItemData,
    type MediaItemSettings,
  } from './MediaGrid.svelte';
  import {
    mediaCache,
    type PlaylistInfo as UnifiedPlaylistInfo,
    type PlaylistEntry as UnifiedPlaylistEntry,
  } from '$lib/stores/mediaCache';
  import { resolveUrl, convertProxyConfig } from '$lib/backend/mediaBackend';

  const CACHE_TTL = 10 * 60 * 1000;

  function getTotalDuration(entries: UnifiedPlaylistEntry[] | undefined): number | null {
    if (!entries || entries.length === 0) return null;
    let total = 0;
    let hasAny = false;
    for (const e of entries) {
      if (e.duration) {
        total += e.duration;
        hasAny = true;
      }
    }
    return hasAny ? total : null;
  }

  export type PlaylistEntry = UnifiedPlaylistEntry;

  type PlaylistInfo = UnifiedPlaylistInfo;

  export interface EntrySettings {
    downloadMode: DownloadMode;
    skipSponsors?: boolean;
    skipIntros?: boolean;
    skipSelfPromo?: boolean;
    skipInteraction?: boolean;
    embedChapters?: boolean;
    embedThumbnail?: boolean;
    embedMetadata?: boolean;
    embedSubs?: boolean;
    subLangs?: string;
  }

  export interface PlaylistSelection {
    entries: SelectedEntry[];
    playlistInfo: { id: string; title: string; usePlaylistFolder: boolean };
  }

  export interface SelectedEntry {
    entry: PlaylistEntry;
    settings: EntrySettings;
  }

  export interface PrefetchedPlaylistInfo {
    title?: string;
    thumbnail?: string;
    author?: string;
    entryCount?: number;
  }

  export interface DefaultSettings {
    sponsorBlock?: boolean;
    sponsorBlockSkipSponsors?: boolean;
    sponsorBlockSkipIntros?: boolean;
    sponsorBlockSkipSelfPromo?: boolean;
    sponsorBlockSkipInteraction?: boolean;
    chapters?: boolean;
    embedSubtitles?: boolean;
    subtitleLanguages?: string;
    embedThumbnail?: boolean;
    clearMetadata?: boolean;
  }

  interface Props {
    url: string;
    cookiesFromBrowser?: string;
    customCookies?: string;
    defaultDownloadMode?: DownloadMode;
    defaults?: DefaultSettings;
    ondownload?: (selection: PlaylistSelection) => void;
    onback?: () => void;
    onopenitem?: (entry: PlaylistEntry) => void;
    showHeader?: boolean;
    backLabel?: string;
    prefetchedInfo?: PrefetchedPlaylistInfo;
  }

  let {
    url,
    cookiesFromBrowser = '',
    customCookies = '',
    defaultDownloadMode = 'auto',
    defaults,
    ondownload,
    onback,
    onopenitem,
    showHeader = false,
    backLabel,
    prefetchedInfo,
  }: Props = $props();

  function getInitialState() {
    const uiState = mediaCache.getUIState(url);
    const playlistInfo = mediaCache.getPlaylistInfo(url);
    const hasFullData = !!playlistInfo;

    if (uiState) {
      const rawSelectedMode = uiState.selectedMode;
      const rawSelectedIds = uiState.selectedIds ?? [];
      const rawDeselectedIds = uiState.deselectedIds ?? [];

      const inferredMode: 'all' | 'some' = (() => {
        if (rawSelectedMode) return rawSelectedMode;
        if (!playlistInfo) return rawSelectedIds.length > 0 ? 'some' : 'all';
        return rawSelectedIds.length >= (playlistInfo.totalCount ?? playlistInfo.entries.length)
          ? 'all'
          : 'some';
      })();

      return {
        playlistInfo,
        selectedMode: inferredMode,
        selectedIds: inferredMode === 'some' ? rawSelectedIds : [],
        deselectedIds: inferredMode === 'all' ? rawDeselectedIds : [],
        perItemSettingsObj: (uiState.perItemSettings ?? {}) as Record<
          string,
          Partial<MediaItemSettings>
        >,
        scrollTop: uiState.scrollTop ?? 0,
        viewMode: (uiState.viewMode ?? 'list') as ViewMode,
        searchQuery: uiState.searchQuery ?? '',
        loading: !hasFullData,
        lastLoadedUrl: hasFullData ? url : '',
        fromCache: hasFullData,
      };
    }

    return {
      playlistInfo,
      selectedMode: 'all' as const,
      selectedIds: [] as string[],
      deselectedIds: [] as string[],
      perItemSettingsObj: {} as Record<string, Partial<MediaItemSettings>>,
      scrollTop: 0,
      viewMode: 'list' as const,
      searchQuery: '',
      loading: !hasFullData,
      lastLoadedUrl: hasFullData ? url : '',
      fromCache: hasFullData,
    };
  }

  const initialState = getInitialState();
  let playlistInfo = $state<PlaylistInfo | null>(initialState.playlistInfo);
  let loading = $state(initialState.loading);
  let error = $state<string | null>(null);
  let lastLoadedUrl = $state(initialState.lastLoadedUrl);

  let playlistInfoCached = $state<boolean>(initialState.fromCache);

  let destroyed = false;
  let thumbnailError = $state(false);

  let lastThumbnailSrc = $state<string | null>(null);
  $effect(() => {
    const next = displayThumbnail ?? null;
    if (next !== lastThumbnailSrc) {
      lastThumbnailSrc = next;
      thumbnailError = false;
    }
  });

  let selectedMode = $state<'all' | 'some'>(initialState.selectedMode);
  let selectedSomeIds = $state<Set<string>>(new Set(initialState.selectedIds));
  let deselectedIds = $state<Set<string>>(new Set(initialState.deselectedIds ?? []));

  function isSelected(id: string): boolean {
    return selectedMode === 'all' ? !deselectedIds.has(id) : selectedSomeIds.has(id);
  }

  let perItemSettingsObj = $state<Record<string, Partial<MediaItemSettings>>>(
    initialState.perItemSettingsObj
  );

  let currentScrollTop = $state(initialState.scrollTop);

  let cardEl = $state<HTMLDivElement | null>(null);
  let entriesContainerEl = $state<HTMLDivElement | null>(null);
  let entriesContainerHeight = $state<number | null>(null);

  function recomputeEntriesHeight() {
    if (!showHeader || !showEntries) {
      entriesContainerHeight = null;
      return;
    }
    if (!cardEl || !entriesContainerEl) return;

    const cardRect = cardEl.getBoundingClientRect();
    const containerRect = entriesContainerEl.getBoundingClientRect();
    const paddingBottom = 12;
    const available = Math.floor(cardRect.bottom - containerRect.top - paddingBottom);
    entriesContainerHeight = Math.max(160, available);
  }

  $effect(() => {
    if (!showHeader || !showEntries) {
      entriesContainerHeight = null;
      return;
    }

    let raf = requestAnimationFrame(recomputeEntriesHeight);
    const ro = new ResizeObserver(() => recomputeEntriesHeight());
    if (cardEl) ro.observe(cardEl);
    if (entriesContainerEl) ro.observe(entriesContainerEl);

    const onWindowResize = () => recomputeEntriesHeight();
    window.addEventListener('resize', onWindowResize, { passive: true });

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      window.removeEventListener('resize', onWindowResize);
    };
  });

  let searchQuery = $state(initialState.searchQuery);
  let showEntries = $state(true);
  let viewMode = $state<ViewMode>(initialState.viewMode);
  let showMoreOptions = $state(false);

  type DownloadOrder = 'queue' | 'reverse' | 'shuffle';
  let downloadOrder = $state<DownloadOrder>('queue');
  let usePlaylistFolder = $state(true);

  let globalVideoQuality = $state($settings.defaultVideoQuality ?? 'max');
  let globalAudioQuality = $state($settings.defaultAudioQuality ?? 'best');

  const videoQualityOptions = [
    { value: 'max', label: $t('download.quality.best') },
    { value: '4k', label: '4K (2160p)' },
    { value: '1440p', label: '1440p' },
    { value: '1080p', label: '1080p' },
    { value: '720p', label: '720p' },
    { value: '480p', label: '480p' },
    { value: '360p', label: '360p' },
  ];

  const audioQualityOptions = [
    { value: 'best', label: $t('download.quality.best') },
    { value: '320', label: '320 kbps' },
    { value: '256', label: '256 kbps' },
    { value: '192', label: '192 kbps' },
    { value: '128', label: '128 kbps' },
  ];

  const initialDefaults = untrack(() => ({
    sponsorBlock: defaults?.sponsorBlock ?? false,
    sponsorBlockSkipSponsors: defaults?.sponsorBlockSkipSponsors ?? true,
    sponsorBlockSkipIntros: defaults?.sponsorBlockSkipIntros ?? false,
    sponsorBlockSkipSelfPromo: defaults?.sponsorBlockSkipSelfPromo ?? false,
    sponsorBlockSkipInteraction: defaults?.sponsorBlockSkipInteraction ?? false,
    chapters: defaults?.chapters ?? true,
    embedThumbnail: defaults?.embedThumbnail ?? true,
    clearMetadata: defaults?.clearMetadata ?? false,
    embedSubtitles: defaults?.embedSubtitles ?? false,
    subtitleLanguages: defaults?.subtitleLanguages ?? 'en,ru',
  }));

  let globalSkipSponsors = $state(
    initialDefaults.sponsorBlock ? initialDefaults.sponsorBlockSkipSponsors : false
  );
  let globalSkipIntros = $state(
    initialDefaults.sponsorBlock ? initialDefaults.sponsorBlockSkipIntros : false
  );
  let globalSkipSelfPromo = $state(
    initialDefaults.sponsorBlock ? initialDefaults.sponsorBlockSkipSelfPromo : false
  );
  let globalSkipInteraction = $state(
    initialDefaults.sponsorBlock ? initialDefaults.sponsorBlockSkipInteraction : false
  );
  let globalEmbedChapters = $state(initialDefaults.chapters);
  let globalEmbedThumbnail = $state(initialDefaults.embedThumbnail);
  let globalEmbedMetadata = $state(!initialDefaults.clearMetadata);
  let globalEmbedSubs = $state(initialDefaults.embedSubtitles);
  let globalSubLangs = $state(initialDefaults.subtitleLanguages);

  let isYouTubeMusicUrl = $derived(url.toLowerCase().includes('music.youtube.com'));
  let bulkMode = $state<DownloadMode | null>(null);

  let displayTitle = $derived(playlistInfo?.title ?? prefetchedInfo?.title ?? '');
  let displayThumbnail = $derived.by(() => {
    const raw = playlistInfo?.thumbnail ?? prefetchedInfo?.thumbnail ?? null;
    return raw ? (getDisplayThumbnailUrl(url, raw, 'mq') ?? raw) : '';
  });
  let displayAuthor = $derived(playlistInfo?.uploader ?? prefetchedInfo?.author ?? '');
  let displayCount = $derived(playlistInfo?.totalCount ?? prefetchedInfo?.entryCount ?? null);
  let hasPrefetchedInfo = $derived(!!prefetchedInfo?.title || !!prefetchedInfo?.thumbnail);

  function applyModeToAll(mode: DownloadMode) {
    bulkMode = mode;
    perItemSettingsObj = {};
  }

  let searchQueryDebounced = $state('');
  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    if (!searchQuery) {
      searchQueryDebounced = '';
      return;
    }
    searchDebounceTimer = setTimeout(() => {
      searchQueryDebounced = searchQuery;
    }, 150);
  });

  let filteredEntries = $derived.by(() => {
    const entries = playlistInfo?.entries ?? [];
    if (!searchQueryDebounced) return entries;
    const q = searchQueryDebounced.toLowerCase();
    return entries.filter(
      (e) => e.title.toLowerCase().includes(q) || e.uploader?.toLowerCase().includes(q)
    );
  });

  let selectedCount = $derived.by(() => {
    const total = playlistInfo?.entries.length ?? 0;
    return selectedMode === 'all' ? Math.max(0, total - deselectedIds.size) : selectedSomeIds.size;
  });
  let allSelected = $derived.by(() => {
    if (filteredEntries.length === 0) return false;
    for (const e of filteredEntries) {
      if (!isSelected(e.id)) return false;
    }
    return true;
  });

  let estimatedSizeDebounced = $state<string | null>(null);
  let sizeCalcTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    const _ = [selectedCount, Object.keys(perItemSettingsObj).length, playlistInfo?.entries.length];

    if (sizeCalcTimer) clearTimeout(sizeCalcTimer);
    sizeCalcTimer = setTimeout(() => {
      estimatedSizeDebounced = calculateEstimatedSize();
    }, 200);
  });

  function calculateEstimatedSize(): string | null {
    if (!playlistInfo || selectedCount === 0) return null;

    let totalSeconds = 0;
    let audioSeconds = 0;
    let videoSeconds = 0;

    for (const entry of playlistInfo.entries) {
      if (!isSelected(entry.id) || !entry.duration) continue;

      const settings = getEntrySettings(entry);
      const mode = settings.downloadMode;

      if (mode === 'audio') {
        audioSeconds += entry.duration;
      } else {
        videoSeconds += entry.duration;
      }
      totalSeconds += entry.duration;
    }

    if (totalSeconds === 0) return null;

    const audioBytes = audioSeconds * 16 * 1024; // 128kbps
    const videoBytes = videoSeconds * 312 * 1024; // ~2.5Mbps for 720p

    const totalBytes = audioBytes + videoBytes;
    return totalBytes > 0 ? formatSize(totalBytes) : null;
  }

  let estimatedSize = $derived(() => estimatedSizeDebounced);

  function getDefaultSettings(isMusic: boolean): EntrySettings {
    const mode: DownloadMode = bulkMode ?? (isYouTubeMusicUrl || isMusic ? 'audio' : 'auto');
    return {
      downloadMode: mode,
      skipSponsors: globalSkipSponsors,
      skipIntros: globalSkipIntros,
      skipSelfPromo: globalSkipSelfPromo,
      skipInteraction: globalSkipInteraction,
      embedChapters: globalEmbedChapters,
      embedThumbnail: globalEmbedThumbnail,
      embedMetadata: globalEmbedMetadata,
      embedSubs: globalEmbedSubs,
      subLangs: globalSubLangs,
    };
  }

  function getEntrySettings(entry: PlaylistEntry): EntrySettings {
    const base = getDefaultSettings(entry.isMusic);
    const override = perItemSettingsObj[entry.id] ?? {};
    return { ...base, ...override };
  }

  function handleOpenItem(mediaItem: MediaItemData) {
    const entry = playlistInfo?.entries.find((e) => e.id === mediaItem.id);
    if (entry && onopenitem) {
      onopenitem(entry);
    }
  }

  function saveToCache() {
    if (url) {
      mediaCache.setUIState(url, {
        selectedMode,
        selectedIds: selectedMode === 'some' ? [...selectedSomeIds] : [],
        deselectedIds: selectedMode === 'all' ? [...deselectedIds] : [],
        perItemSettings: { ...perItemSettingsObj },
        scrollTop: currentScrollTop,
        viewMode,
        searchQuery,
      });
    }
  }

  $effect(() => {
    if (url && url !== lastLoadedUrl && !playlistInfo) {
      loadPlaylist();
    }
  });

  $effect(() => {
    return () => {
      destroyed = true;
      saveToCache();
      playlistInfo = null;
      perItemSettingsObj = {};
      selectedSomeIds = new Set();
      deselectedIds = new Set();
    };
  });

  async function loadPlaylist() {
    if (destroyed) return;
    loading = true;
    error = null;
    playlistInfo = null;
    selectedMode = 'all';
    selectedSomeIds = new Set();
    deselectedIds = new Set();
    perItemSettingsObj = {};
    lastLoadedUrl = url;
    usePlaylistFolder = $settings.usePlaylistFolders ?? true;

    try {
      const currentSettings = getSettings();
      const resolveResult = await resolveUrl(url, {
        cookies_from_browser: cookiesFromBrowser || null,
        custom_cookies: customCookies || null,
        proxy: convertProxyConfig(getProxyConfig()),
        youtube_player_client: currentSettings.usePlayerClientForExtraction
          ? currentSettings.youtubePlayerClient || null
          : null,
        flat_playlist: true,
      });

      if (destroyed) return;

      const info = resolveResult.info;

      const mappedEntries = (info.entries || []).map((e) => ({
        id: e.id,
        url: e.url,
        title: e.title || '',
        duration: typeof e.duration === 'bigint' ? Number(e.duration) : (e.duration ?? null),
        thumbnail: e.thumbnail ?? null,
        uploader: e.uploader ?? null,
        isMusic: e.isMusic ?? false,
      }));

      const seen = new Set<string>();
      const uniqueEntries = mappedEntries.filter((e) => {
        if (seen.has(e.id)) return false;
        seen.add(e.id);
        return true;
      });

      const unified: PlaylistInfo = {
        isPlaylist: true,
        id: null,
        title: info.title || $t('common.playlist'),
        uploader: info.uploader || null,
        thumbnail: info.thumbnail || null,
        totalCount: uniqueEntries.length,
        entries: uniqueEntries,
        hasMore: false,
      };

      playlistInfo = unified;
      selectedMode = 'all';
      selectedSomeIds = new Set();
      deselectedIds = new Set();
      perItemSettingsObj = {};

      if (!playlistInfoCached) {
        mediaCache.setPlaylistInfo(url, unified);
        playlistInfoCached = true;
      }

      saveToCache();
    } catch (e) {
      if (destroyed) return;
      error = String(e);
    } finally {
      if (!destroyed) {
        loading = false;
      }
    }
  }

  function toggleSelectAll() {
    if (allSelected) {
      if (selectedMode === 'all') {
        const next = new Set(deselectedIds);
        for (const e of filteredEntries) next.add(e.id);
        deselectedIds = next;
      } else {
        const next = new Set(selectedSomeIds);
        for (const e of filteredEntries) next.delete(e.id);
        selectedSomeIds = next;
      }
    } else {
      if (selectedMode === 'all') {
        const next = new Set(deselectedIds);
        for (const e of filteredEntries) next.delete(e.id);
        deselectedIds = next;
      } else {
        const next = new Set(selectedSomeIds);
        for (const e of filteredEntries) next.add(e.id);
        selectedSomeIds = next;
      }
    }
  }

  function toggleEntry(id: string) {
    if (isSelected(id)) {
      if (selectedMode === 'all') {
        const next = new Set(deselectedIds);
        next.add(id);
        deselectedIds = next;
      } else {
        const next = new Set(selectedSomeIds);
        next.delete(id);
        selectedSomeIds = next;
      }
    } else {
      if (selectedMode === 'all') {
        const next = new Set(deselectedIds);
        next.delete(id);
        deselectedIds = next;
      } else {
        const next = new Set(selectedSomeIds);
        next.add(id);
        selectedSomeIds = next;
      }
    }
  }

  function updateEntrySettings(id: string, newSettings: Partial<MediaItemSettings>) {
    const current = perItemSettingsObj[id] ?? {};
    const updated = { ...current, ...newSettings } as Partial<MediaItemSettings>;
    perItemSettingsObj = { ...perItemSettingsObj, [id]: updated };
  }

  const downloadOrderOptions = [
    { value: 'queue', label: $t('playlist.order.queue') },
    { value: 'reverse', label: $t('playlist.order.reverse') },
    { value: 'shuffle', label: $t('playlist.order.shuffle') },
  ];

  function handleDownload() {
    if (!playlistInfo || selectedCount === 0) return;

    let entries = playlistInfo.entries
      .filter((e) => isSelected(e.id))
      .map((e) => ({
        entry: e,
        settings: getEntrySettings(e),
      }));

    switch (downloadOrder) {
      case 'reverse':
        entries = entries.reverse();
        break;
      case 'shuffle':
        entries = entries.sort(() => Math.random() - 0.5);
        break;
    }

    const selection: PlaylistSelection = {
      entries,
      playlistInfo: {
        id: playlistInfo?.id ?? '',
        title: playlistInfo?.title ?? $t('common.playlist'),
        usePlaylistFolder,
      },
    };

    ondownload?.(selection);
  }
</script>

{#if showHeader && $settings.builderThumbnailGlow}
  <ThumbnailGlow thumbnailUrl={displayThumbnail} enabled={showHeader} />
{/if}

<div class="playlist-builder" class:full-bleed={showHeader} class:entries-open={showEntries}>
  {#if showHeader}
    <div class="view-header">
      <button class="back-btn" onclick={onback}>
        <Icon name="alt_arrow_rigth" size={16} class="rotate-180" />
        <span>{backLabel || $t('common.back')}</span>
      </button>
      <div class="header-badge playlist">
        <Icon name="playlist" size={12} />
        <span>{$t('common.playlist')}</span>
      </div>
      <div class="header-spacer"></div>
      {#if estimatedSize()}
        <span class="size-estimate">~{estimatedSize()}</span>
      {/if}
      <button
        class="header-download-btn"
        onclick={handleDownload}
        disabled={loading || selectedCount === 0}
      >
        <Icon name="download" size={16} />
        <span>{$t('common.download')} {selectedCount > 0 ? `(${selectedCount})` : ''}</span>
      </button>
    </div>
  {:else}
    <div class="yt-badge">
      <Icon name="playlist" size={14} />
      <span>{$t('common.youtubePlaylist')}</span>
    </div>
  {/if}

  <div class="card" bind:this={cardEl}>
    {#if error}
      <div class="error-state">
        <Icon name="warning" size={18} />
        <span>{error}</span>
        <button class="retry-btn" onclick={loadPlaylist}>
          <Icon name="restart" size={14} />
        </button>
      </div>
    {:else}
      <div class="main-row">
        <div class="left">
          {#if displayThumbnail && !thumbnailError}
            <img
              src={displayThumbnail}
              alt=""
              class="thumb"
              decoding="async"
              onload={(e) => {
                const src = (e.currentTarget as HTMLImageElement).getAttribute('src');
                if (src && src === displayThumbnail) thumbnailError = false;
              }}
              onerror={(e) => {
                const src = (e.currentTarget as HTMLImageElement).getAttribute('src');
                if (src && src === displayThumbnail) thumbnailError = true;
              }}
            />
          {:else if loading && !hasPrefetchedInfo}
            <div class="thumb skeleton"></div>
          {:else}
            <div class="thumb empty"><Icon name="playlist" size={showHeader ? 32 : 20} /></div>
          {/if}
          <div class="info">
            {#if displayTitle || playlistInfo}
              <span class="title-row">
                <span class="title">{displayTitle || $t('common.playlist')}</span>
                <button
                  class="copy-link-btn"
                  onclick={() => {
                    navigator.clipboard.writeText(url);
                    import('$lib/components/Toast.svelte').then((m) =>
                      m.toast.success($t('common.copied'))
                    );
                  }}
                  title={$t('common.copyLink')}
                >
                  <Icon name="link" size={12} />
                </button>
              </span>
              <span class="meta">
                {#if displayAuthor}<span class="author"
                    ><Icon name="user" size={12} />{displayAuthor}</span
                  >{/if}
                {#if displayCount}<span class="meta-item"
                    ><Icon name="video" size={12} />{displayCount}
                    {$t('playlist.videos')}</span
                  >{/if}
                {#if playlistInfo && getTotalDuration(playlistInfo.entries)}<span class="meta-item"
                    ><Icon name="clock" size={12} />{formatDuration(
                      getTotalDuration(playlistInfo.entries)!
                    )}</span
                  >{/if}
              </span>
            {:else if loading}
              <div class="title-skel skeleton"></div>
              <div class="meta-skel skeleton"></div>
            {/if}
          </div>
        </div>

        <div class="right">
          <div class="mode-selector">
            <button
              class="mode-btn"
              class:active={bulkMode === 'auto' || (!bulkMode && !isYouTubeMusicUrl)}
              onclick={() => applyModeToAll('auto')}
              disabled={loading}
            >
              <Icon name="download" size={14} />
              <span>Auto</span>
            </button>
            <button
              class="mode-btn"
              class:active={bulkMode === 'audio' || (!bulkMode && isYouTubeMusicUrl)}
              onclick={() => applyModeToAll('audio')}
              disabled={loading}
            >
              <Icon name="music" size={14} />
              <span>Audio</span>
            </button>
            <button
              class="mode-btn"
              class:active={bulkMode === 'mute'}
              onclick={() => applyModeToAll('mute')}
              disabled={loading}
            >
              <Icon name="video2" size={14} />
              <span>No Audio</span>
            </button>
          </div>
        </div>
      </div>

      <div class="extras-row">
        <Checkbox
          checked={usePlaylistFolder}
          label={$t('playlist.createFolder')}
          disabled={loading}
          onchange={(v: boolean) => (usePlaylistFolder = v)}
        />
        <div class="spacer"></div>
        <button
          class="more-btn"
          class:active={showMoreOptions}
          onclick={() => (showMoreOptions = !showMoreOptions)}
        >
          <Icon name={showMoreOptions ? 'chevron_up' : 'chevron_down'} size={14} />
          <span>More</span>
        </button>
        <button
          class="entries-toggle"
          onclick={() => (showEntries = !showEntries)}
          disabled={loading}
        >
          <span class="selected-badge">{selectedCount}/{playlistInfo?.totalCount ?? 0}</span>
          <Icon name={showEntries ? 'chevron_up' : 'chevron_down'} size={14} />
          <span>videos</span>
        </button>
      </div>

      {#if showMoreOptions}
        <div class="more-options">
          <div class="options-row">
            <div class="option-group compact">
              <span class="group-label">{$t('download.videoQuality')}</span>
              <Select
                bind:value={globalVideoQuality}
                options={videoQualityOptions}
                disabled={loading}
              />
            </div>
            <div class="option-group compact">
              <span class="group-label">{$t('download.audioQuality')}</span>
              <Select
                bind:value={globalAudioQuality}
                options={audioQualityOptions}
                disabled={loading}
              />
            </div>
            <div class="option-group compact">
              <span class="group-label">{$t('playlist.downloadOrder')}</span>
              <Select
                bind:value={downloadOrder}
                options={downloadOrderOptions}
                disabled={loading}
              />
            </div>
          </div>

          <div class="options-sections">
            <CollapsibleBlock title="SponsorBlock" expanded={true}>
              <div class="option-grid">
                <Checkbox
                  bind:checked={globalSkipSponsors}
                  label={$t('download.tracks.skipSponsors')}
                />
                <Checkbox
                  bind:checked={globalSkipIntros}
                  label={$t('download.tracks.skipIntros')}
                />
                <Checkbox
                  bind:checked={globalSkipSelfPromo}
                  label={$t('download.tracks.skipSelfPromo')}
                />
                <Checkbox
                  bind:checked={globalSkipInteraction}
                  label={$t('download.tracks.skipInteraction')}
                />
              </div>
            </CollapsibleBlock>

            <CollapsibleBlock title={$t('download.tracks.embedOptions')} expanded={true}>
              <div class="option-grid">
                <Checkbox
                  bind:checked={globalEmbedChapters}
                  label={$t('download.tracks.embedChapters')}
                />
                <Checkbox
                  bind:checked={globalEmbedThumbnail}
                  label={$t('download.tracks.embedThumbnail')}
                />
                <Checkbox
                  bind:checked={globalEmbedMetadata}
                  label={$t('download.tracks.embedMetadata')}
                />
              </div>
            </CollapsibleBlock>

            <CollapsibleBlock title={$t('download.tracks.subtitles')} expanded={true}>
              <div class="subs-row">
                <Checkbox bind:checked={globalEmbedSubs} label={$t('download.tracks.embedSubs')} />
                {#if globalEmbedSubs}
                  <input type="text" class="lang-input" bind:value={globalSubLangs} />
                {/if}
              </div>
            </CollapsibleBlock>
          </div>
        </div>
      {/if}

      {#if showEntries && playlistInfo}
        <div class="entries-panel">
          <div class="entries-toolbar">
            <div class="search-box">
              <Icon name="search" size={14} />
              <input type="text" bind:value={searchQuery} placeholder={$t('playlist.search')} />
              {#if searchQuery}
                <button class="clear-btn" onclick={() => (searchQuery = '')}>
                  <Icon name="close" size={12} />
                </button>
              {/if}
            </div>

            <div class="toolbar-right">
              <div class="view-toggle">
                <button
                  class="view-btn"
                  class:active={viewMode === 'list'}
                  onclick={() => (viewMode = 'list')}
                  use:tooltip={$t('downloads.views.list')}
                >
                  <Icon name="checklist" size={16} />
                </button>
                <button
                  class="view-btn"
                  class:active={viewMode === 'grid'}
                  onclick={() => (viewMode = 'grid')}
                  use:tooltip={$t('downloads.views.grid')}
                >
                  <Icon name="gallery" size={16} />
                </button>
              </div>

              <button class="select-all-btn" onclick={toggleSelectAll}>
                <Checkbox checked={allSelected} disabled />
                <span>{allSelected ? $t('playlist.deselectAll') : $t('playlist.selectAll')}</span>
              </button>
            </div>
          </div>

          <div
            class="entries-container"
            bind:this={entriesContainerEl}
            style={showHeader && showEntries && entriesContainerHeight
              ? `height: ${entriesContainerHeight}px;`
              : undefined}
          >
            <MediaGrid
              items={filteredEntries}
              mapItem={(e: PlaylistEntry) => ({
                id: e.id,
                title: e.title,
                thumbnail: getDisplayThumbnailUrl(e.url, e.thumbnail, 'default'),
                duration: e.duration,
                author: e.uploader,
                isMusic: e.isMusic,
              })}
              selectedIds={isSelected}
              getDefaultSettings={(item) => getDefaultSettings(!!item.isMusic)}
              {viewMode}
              perItemSettings={perItemSettingsObj}
              {loading}
              initialScrollTop={currentScrollTop}
              ontoggle={toggleEntry}
              onupdatesettings={updateEntrySettings}
              onopenitem={onopenitem ? handleOpenItem : undefined}
              onscroll={(pos) => {
                currentScrollTop = pos;
              }}
            />
          </div>
        </div>
      {/if}

      {#if !showHeader && ondownload}
        <div class="footer-actions">
          {#if estimatedSize()}
            <span class="size-estimate">~{estimatedSize()}</span>
          {/if}
          <button
            class="download-btn footer-download"
            onclick={handleDownload}
            disabled={loading || selectedCount === 0}
          >
            <Icon name="download" size={18} />
            <span
              >{$t('common.download')}
              {selectedCount > 0 ? `(${selectedCount} ${$t('playlist.videos')})` : ''}</span
            >
          </button>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .playlist-builder {
    display: flex;
    flex-direction: column;
    gap: 6px;
    animation: fadeIn 0.2s ease-out;
  }

  .playlist-builder.full-bleed {
    padding: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  @media (max-width: 480px) {
    .playlist-builder.full-bleed {
      height: auto;
      min-height: 100%;
    }
  }

  .playlist-builder.full-bleed .view-header {
    position: sticky;
    top: 0;
    z-index: 10;
    margin: 0;
    padding: 10px 0 10px 0;
    margin-right: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .playlist-builder.full-bleed .card {
    background: transparent;
    border: none;
    border-radius: 0;
    padding: 0 8px 0 0;
    flex: 1;
    min-height: 0;
  }

  .playlist-builder.full-bleed:not(.entries-open) .card {
    overflow-y: auto;
  }

  .playlist-builder.full-bleed.entries-open .card {
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .playlist-builder.full-bleed .yt-badge {
    display: none;
  }

  .playlist-builder.full-bleed .entries-container {
    height: auto;
    max-height: none;
    border-radius: 0;
    background: transparent;
    margin: 0 -16px;
    padding: 6px 16px 0 16px;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .view-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    flex-shrink: 0;
    flex-wrap: wrap;
    min-width: 0;
  }

  .back-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.6);
    font-size: var(--text-base, 13px);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    min-width: 0;
    flex-shrink: 1;
  }

  .back-btn span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 120px;
  }

  .back-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .header-badge {
    display: none;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    background: rgba(255, 0, 0, 0.12);
    border-radius: var(--radius-sm, 6px);
    color: #ff6b6b;
    font-size: var(--text-xs, 11px);
    font-weight: 600;
  }

  @media (min-width: 400px) {
    .header-badge {
      display: inline-flex;
    }
  }

  .header-badge.playlist {
    background: rgba(255, 165, 0, 0.12);
    color: #ffa500;
  }

  .header-spacer {
    flex: 1;
    min-width: 8px;
  }

  .size-estimate {
    font-size: var(--text-sm, 12px);
    color: rgba(255, 255, 255, 0.5);
    font-weight: 500;
    flex-shrink: 0;
  }

  .header-download-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius, 8px);
    color: white;
    font-size: var(--text-base, 13px);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    flex-shrink: 0;
  }

  .header-download-btn span {
    display: none;
  }

  @media (min-width: 360px) {
    .header-download-btn span {
      display: inline;
    }
  }

  .header-download-btn:hover:not(:disabled) {
    background: var(--accent, #6366f1);
    border-color: var(--accent, #6366f1);
  }

  .header-download-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .download-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 20px;
    background: var(--accent, #6366f1);
    border: none;
    border-radius: var(--radius, 8px);
    color: white;
    font-size: var(--text-md, 14px);
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .download-btn:hover:not(:disabled) {
    background: var(--accent-hover, #5558e3);
    transform: translateY(-1px);
  }

  .download-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .footer-actions {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
    display: flex;
    justify-content: flex-end;
  }

  .footer-download {
    padding: 12px 24px;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .yt-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: rgba(255, 0, 0, 0.15);
    border-radius: var(--radius-sm, 6px);
    color: #ff6b6b;
    font-size: var(--text-sm, 12px);
    font-weight: 600;
    width: fit-content;
  }

  .card {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: var(--radius-lg, 12px);
    padding: 12px;
  }

  .error-state {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px;
    color: rgba(239, 68, 68, 0.9);
  }

  .retry-btn {
    margin-left: auto;
    padding: 6px;
    background: rgba(255, 255, 255, 0.06);
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
  }

  .retry-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .main-row {
    display: flex;
    gap: 14px;
    padding: 4px 0;
  }

  .playlist-builder.full-bleed .main-row {
    gap: 16px;
    padding: 8px 0;
  }

  .left {
    display: flex;
    gap: 12px;
    flex: 1;
    min-width: 0;
  }

  .thumb {
    width: 72px;
    height: 72px;
    border-radius: var(--radius, 8px);
    object-fit: cover;
    flex-shrink: 0;
    background: rgba(255, 255, 255, 0.04);
  }

  .playlist-builder.full-bleed .thumb {
    width: 120px;
    height: 120px;
    border-radius: var(--radius, 10px);
  }

  .thumb.empty {
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.2);
  }

  .info {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }

  .playlist-builder.full-bleed .info {
    gap: 6px;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .title {
    font-size: var(--text-base, 13px);
    font-weight: 600;
    color: white;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    line-height: 1.3;
  }

  .copy-link-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    background: rgba(255, 255, 255, 0.08);
    border: none;
    border-radius: var(--radius-sm, 4px);
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .copy-link-btn:hover {
    background: rgba(255, 255, 255, 0.15);
    color: rgba(255, 255, 255, 0.9);
  }

  .playlist-builder.full-bleed .title {
    font-size: 16px;
    line-height: 1.35;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.45);
  }

  .playlist-builder.full-bleed .meta {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.5);
  }

  .meta .author,
  .meta .meta-item {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .meta .author {
    font-weight: 500;
    color: rgba(255, 255, 255, 0.6);
  }

  .meta .author::after {
    content: '•';
    margin-left: 5px;
    color: rgba(255, 255, 255, 0.3);
  }

  .meta .meta-item::after {
    content: '•';
    margin-left: 5px;
    color: rgba(255, 255, 255, 0.3);
  }

  .meta .meta-item:last-child::after {
    display: none;
  }

  .playlist-builder.full-bleed .meta .author {
    color: rgba(255, 255, 255, 0.7);
  }

  .right {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
    align-items: center;
  }

  .mode-selector {
    display: flex;
    gap: 2px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: var(--radius, 8px);
    padding: 3px;
  }

  .mode-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 12px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.5);
    font-size: var(--text-sm, 12px);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
  }

  .mode-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.8);
  }

  .mode-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .mode-btn.active {
    background: var(--accent, #6366f1);
    color: white;
  }

  .extras-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }

  .spacer {
    flex: 1;
  }

  .entries-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: rgba(255, 255, 255, 0.06);
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.7);
    font-size: var(--text-sm, 12px);
    cursor: pointer;
    transition: all 0.15s;
  }

  .entries-toggle:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .entries-toggle:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .selected-badge {
    padding: 2px 6px;
    background: var(--accent, #6366f1);
    border-radius: var(--radius-sm, 4px);
    font-size: 11px;
    font-weight: 600;
    color: white;
  }

  .entries-panel {
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .playlist-builder.full-bleed.entries-open .entries-panel {
    flex: 1;
  }

  .entries-toolbar {
    display: flex;
    gap: 8px;
    margin-bottom: 8px;
    align-items: center;
  }

  .search-box {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-sm, 6px);
  }

  .search-box :global(svg) {
    color: rgba(255, 255, 255, 0.4);
    flex-shrink: 0;
  }

  .search-box input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: white;
    font-size: var(--text-sm, 12px);
    min-width: 0;
  }

  .search-box input::placeholder {
    color: rgba(255, 255, 255, 0.4);
  }

  .clear-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    background: rgba(255, 255, 255, 0.1);
    border: none;
    border-radius: 50%;
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
  }

  .view-toggle {
    display: flex;
    background: rgba(255, 255, 255, 0.06);
    border-radius: var(--radius-sm, 6px);
    padding: 3px;
    gap: 2px;
  }

  .view-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 4px);
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    transition: all 0.15s;
  }

  .view-btn:hover {
    color: rgba(255, 255, 255, 0.7);
    background: rgba(255, 255, 255, 0.06);
  }

  .view-btn.active {
    background: rgba(255, 255, 255, 0.12);
    color: white;
  }

  .select-all-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.7);
    font-size: var(--text-xs, 11px);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .select-all-btn:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .toolbar-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .entries-container {
    height: 400px;
    max-height: 400px;
    overflow: hidden;
    border-radius: var(--radius, 8px);
    background: rgba(0, 0, 0, 0.2);
    padding: 6px;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .more-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.6);
    font-size: var(--text-xs, 11px);
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
  }

  .more-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.8);
  }

  .more-btn.active {
    color: var(--accent, #6366f1);
    border-color: var(--accent, #6366f1);
    background: rgba(99, 102, 241, 0.1);
  }

  .more-options {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    background: rgba(0, 0, 0, 0.15);
    border-radius: var(--radius, 8px);
    margin-top: 10px;
  }

  .options-sections {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .options-row {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }

  .option-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 120px;
  }

  .option-group.compact {
    min-width: 100px;
  }

  .group-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-weight: 500;
  }

  .option-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
  }

  .subs-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .lang-input {
    flex: 1;
    max-width: 200px;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-sm, 6px);
    color: white;
    font-size: 12px;
    font-family: inherit;
    outline: none;
    transition: border-color 0.15s;
  }

  .lang-input::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }

  .lang-input:focus {
    border-color: var(--accent, #6366f1);
  }

  .lang-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .skeleton {
    background: linear-gradient(
      90deg,
      rgba(255, 255, 255, 0.04) 0%,
      rgba(255, 255, 255, 0.08) 50%,
      rgba(255, 255, 255, 0.04) 100%
    );
    background-size: 200% 100%;
    animation: shimmer 1.5s infinite;
    border-radius: var(--radius-sm, 4px);
  }

  @keyframes shimmer {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }

  .title-skel {
    height: 14px;
    width: 90%;
  }

  .meta-skel {
    height: 10px;
    width: 50%;
  }

  @media (max-width: 560px) {
    .main-row {
      flex-direction: column;
      gap: 12px;
    }

    .thumb {
      width: 64px;
      height: 64px;
    }

    .right {
      width: 100%;
    }

    .mode-selector {
      width: 100%;
      justify-content: center;
    }

    .mode-btn {
      flex: 1;
      justify-content: center;
    }

    .extras-row {
      flex-wrap: wrap;
      gap: 6px;
    }

    .entries-toolbar {
      flex-wrap: wrap;
    }

    .search-box {
      width: 100%;
      order: 1;
    }

    .view-toggle {
      order: 2;
    }

    .select-all-btn {
      order: 3;
      flex: 1;
      justify-content: center;
    }

    .entries-container {
      max-height: 300px;
    }

    .playlist-builder.full-bleed .entries-container {
      max-height: none;
    }
  }

  :global(.rotate-180) {
    transform: rotate(180deg);
  }
</style>
