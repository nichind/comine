<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '$lib/i18n';
  import { tooltip } from '$lib/actions/tooltip';
  import { settings, type DownloadMode, getProxyConfig, getSettings } from '$lib/stores/settings';
  import ThumbnailGlow from './ThumbnailGlow.svelte';
  import { deps } from '$lib/stores/deps';
  import { formatDuration, getDisplayThumbnailUrl } from '$lib/utils/format';
  import Icon from './Icon.svelte';
  import Checkbox from './Checkbox.svelte';
  import Select from './Select.svelte';
  import CollapsibleBlock from './CollapsibleBlock.svelte';
  import MediaGrid, {
    type ViewMode,
    type MediaItemData,
    type MediaItemSettings,
  } from './MediaGrid.svelte';
  import { mediaCache, type ChannelInfo, type ChannelEntry } from '$lib/stores/mediaCache';
  import { resolveUrl, convertProxyConfig } from '$lib/backend/mediaBackend';
  import { navigation } from '$lib/stores/navigation';

  export type { ChannelEntry };

  function formatNumber(num: number | null | undefined): string {
    if (num == null) return '';
    if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
    if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
    return num.toString();
  }

  export interface ChannelSelection {
    entries: SelectedChannelEntry[];
    channelInfo: { id: string; name: string; useChannelFolder: boolean };
  }

  export interface SelectedChannelEntry {
    entry: ChannelEntry;
    settings: EntrySettings;
  }

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

  export interface PrefetchedChannelInfo {
    name?: string;
    thumbnail?: string;
    handle?: string;
    subscriberCount?: number;
  }

  export interface DefaultSettings {
    downloadMode?: 'auto' | 'audio' | 'mute';
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

  type ChannelTab = 'videos' | 'shorts' | 'live';

  interface Props {
    url: string;
    cookiesFromBrowser?: string;
    customCookies?: string;
    defaults?: DefaultSettings;
    ondownload?: (selection: ChannelSelection) => void;
    onback?: () => void;
    onopenitem?: (entry: ChannelEntry) => void;
    showHeader?: boolean;
    backLabel?: string;
    prefetchedInfo?: PrefetchedChannelInfo;
  }

  let {
    url,
    cookiesFromBrowser = '',
    customCookies = '',
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
    const cachedChannelInfo = mediaCache.getChannelInfo(url);

    let channelInfo: ChannelInfo | null = cachedChannelInfo;
    const hasFullData = !!channelInfo;

    if (uiState) {
      const rawSelectedMode = uiState.selectedMode;
      const rawSelectedIds = uiState.selectedIds ?? [];
      const rawDeselectedIds = uiState.deselectedIds ?? [];

      const inferredMode: 'all' | 'some' = (() => {
        if (rawSelectedMode) return rawSelectedMode;
        if (!channelInfo) return rawSelectedIds.length > 0 ? 'some' : 'all';
        return rawSelectedIds.length >= (channelInfo.totalCount ?? channelInfo.entries.length)
          ? 'all'
          : 'some';
      })();

      return {
        channelInfo,
        selectedMode: inferredMode,
        selectedIds: inferredMode === 'some' ? rawSelectedIds : [],
        deselectedIds: inferredMode === 'all' ? rawDeselectedIds : [],
        perItemSettingsObj: (uiState.perItemSettings ?? {}) as Record<
          string,
          Partial<MediaItemSettings>
        >,
        scrollTop: uiState.scrollTop ?? 0,
        viewMode: (uiState.viewMode ?? 'grid') as ViewMode,
        searchQuery: uiState.searchQuery ?? '',
        channelTab: (uiState.channelTab ?? 'videos') as ChannelTab,
        loading: !hasFullData,
        lastLoadedUrl: hasFullData ? url : '',
        fromCache: hasFullData,
      };
    }

    return {
      channelInfo,
      selectedMode: 'some' as const,
      selectedIds: [] as string[],
      deselectedIds: [] as string[],
      perItemSettingsObj: {} as Record<string, Partial<MediaItemSettings>>,
      scrollTop: 0,
      viewMode: 'grid' as ViewMode,
      searchQuery: '',
      channelTab: 'videos' as ChannelTab,
      loading: !hasFullData,
      lastLoadedUrl: hasFullData ? url : '',
      fromCache: hasFullData,
    };
  }

  const initialState = getInitialState();
  let channelInfo = $state<ChannelInfo | null>(initialState.channelInfo);
  let loading = $state(initialState.loading);
  let error = $state<string | null>(null);
  let lastLoadedUrl = $state(initialState.lastLoadedUrl);
  let channelInfoCached = $state<boolean>(initialState.fromCache);
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
  let viewMode = $state<ViewMode>(initialState.viewMode);
  let activeTab = $state<ChannelTab>(initialState.channelTab);
  let useChannelFolder = $state(true);

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
    downloadMode: defaults?.downloadMode ?? null,
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
  let bulkMode = $state<DownloadMode | null>(initialDefaults.downloadMode);

  let displayName = $derived(channelInfo?.name ?? prefetchedInfo?.name ?? '');
  let displayThumbnail = $derived.by(() => {
    const raw = channelInfo?.thumbnail ?? prefetchedInfo?.thumbnail ?? null;
    return raw ? (getDisplayThumbnailUrl(url, raw, 'mq') ?? raw) : '';
  });
  let displayHandle = $derived(channelInfo?.handle ?? prefetchedInfo?.handle ?? '');
  let displaySubscribers = $derived(
    channelInfo?.subscriberCount ?? prefetchedInfo?.subscriberCount ?? null
  );
  let displayCount = $derived(channelInfo?.totalCount ?? null);
  let hasPrefetchedInfo = $derived(!!prefetchedInfo?.name || !!prefetchedInfo?.thumbnail);

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

  let tabEntries = $derived.by(() => {
    const entries = channelInfo?.entries ?? [];
    switch (activeTab) {
      case 'videos':
        return entries.filter((e) => !e.isShort && !e.isLive);
      case 'shorts':
        return entries.filter((e) => e.isShort);
      case 'live':
        return entries.filter((e) => e.isLive);
      default:
        return entries;
    }
  });

  let filteredEntries = $derived.by(() => {
    if (!searchQueryDebounced) return tabEntries;
    const q = searchQueryDebounced.toLowerCase();
    return tabEntries.filter((e) => e.title.toLowerCase().includes(q));
  });

  let tabCounts = $derived.by(() => ({
    videos: (channelInfo?.entries ?? []).filter((e) => !e.isShort && !e.isLive).length,
    shorts: (channelInfo?.entries ?? []).filter((e) => e.isShort).length,
    live: (channelInfo?.entries ?? []).filter((e) => e.isLive).length,
  }));

  let selectedCount = $derived.by(() => {
    const entries = channelInfo?.entries ?? [];
    return selectedMode === 'all'
      ? Math.max(0, entries.length - deselectedIds.size)
      : selectedSomeIds.size;
  });

  let allSelected = $derived.by(() => {
    if (filteredEntries.length === 0) return false;
    for (const e of filteredEntries) {
      if (!isSelected(e.id)) return false;
    }
    return true;
  });

  function getDefaultSettings(): EntrySettings {
    const mode: DownloadMode = bulkMode ?? 'auto';
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

  function getEntrySettings(entry: ChannelEntry): EntrySettings {
    const base = getDefaultSettings();
    const override = perItemSettingsObj[entry.id] ?? {};
    return { ...base, ...override };
  }

  function handleOpenItem(mediaItem: MediaItemData) {
    const entry = channelInfo?.entries.find((e) => e.id === mediaItem.id);
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
        channelTab: activeTab,
      });
    }
  }

  $effect(() => {
    if (url && url !== lastLoadedUrl && !channelInfo) {
      loadChannel();
    }
  });

  $effect(() => {
    return () => {
      destroyed = true;
      saveToCache();
      channelInfo = null;
      perItemSettingsObj = {};
      selectedSomeIds = new Set();
      deselectedIds = new Set();
    };
  });

  async function loadChannel() {
    if (destroyed) return;
    loading = true;
    error = null;
    channelInfo = null;
    selectedMode = 'all';
    selectedSomeIds = new Set();
    deselectedIds = new Set();
    perItemSettingsObj = {};
    lastLoadedUrl = url;
    useChannelFolder = $settings.usePlaylistFolders ?? true;

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

      const mappedEntries: ChannelEntry[] = (info.entries || []).map((e) => ({
        id: e.id,
        url: e.url,
        title: e.title || '',
        duration: typeof e.duration === 'bigint' ? Number(e.duration) : (e.duration ?? null),
        thumbnail: e.thumbnail ?? null,
        viewCount: null,
        uploadDate: null,
        isShort: false,
        isLive: false,
      }));

      const unified: ChannelInfo = {
        id: info.channelId ?? null,
        name: info.channel || info.title || '',
        handle: null,
        description: info.description ?? null,
        thumbnail: info.thumbnail ?? null,
        banner: null,
        subscriberCount: null,
        totalCount: mappedEntries.length,
        entries: mappedEntries,
        hasMore: false,
      };

      channelInfo = unified;

      const videoIds = unified.entries.filter((e) => !e.isShort && !e.isLive).map((e) => e.id);
      selectedMode = 'some';
      selectedSomeIds = new Set(videoIds);
      deselectedIds = new Set();
      perItemSettingsObj = {};

      if (!channelInfoCached) {
        mediaCache.setChannelInfo(url, unified);
        channelInfoCached = true;
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

  function handleDownload() {
    if (!channelInfo || selectedCount === 0) return;

    const entries = channelInfo.entries
      .filter((e) => isSelected(e.id))
      .map((e) => ({
        entry: e,
        settings: getEntrySettings(e),
      }));

    const selection: ChannelSelection = {
      entries,
      channelInfo: {
        id: channelInfo.id ?? '',
        name: channelInfo.name ?? $t('common.channel'),
        useChannelFolder,
      },
    };

    ondownload?.(selection);
  }

  function mapEntryToMediaItem(e: ChannelEntry): MediaItemData {
    return {
      id: e.id,
      title: e.title,
      thumbnail: getDisplayThumbnailUrl(e.url, e.thumbnail, 'default'),
      duration: e.duration,
      author: e.viewCount ? formatNumber(e.viewCount) + ' views' : null,
      isMusic: false,
    };
  }

  function getTotalDuration(entries: ChannelEntry[]): number | null {
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

  let showEntries = $state(true);
  let showMoreOptions = $state(false);

  const downloadOrderOptions = [
    { value: 'queue', label: $t('playlist.order.queue') },
    { value: 'reverse', label: $t('playlist.order.reverse') },
    { value: 'shuffle', label: $t('playlist.order.shuffle') },
  ];
  let downloadOrder = $state<'queue' | 'reverse' | 'shuffle'>('queue');
</script>

{#if showHeader && $settings.builderThumbnailGlow}
  <ThumbnailGlow thumbnailUrl={displayThumbnail} enabled={showHeader} />
{/if}

<div class="channel-builder" class:full-bleed={showHeader} class:entries-open={showEntries}>
  {#if showHeader}
    <div class="view-header">
      <button class="back-btn" onclick={onback}>
        <Icon name="alt_arrow_rigth" size={16} class="rotate-180" />
        <span>{backLabel || $t('common.back')}</span>
      </button>
      <div class="header-badge channel">
        <Icon name="user" size={12} />
        <span>{$t('channel.title')}</span>
      </div>
      <div class="header-spacer"></div>
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
      <Icon name="user" size={14} />
      <span>{$t('common.youtubeChannel')}</span>
    </div>
  {/if}

  <div class="card" bind:this={cardEl}>
    {#if error}
      <div class="error-state">
        <Icon name="warning" size={18} />
        <span>{error}</span>
        <button class="retry-btn" onclick={loadChannel}>
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
            <div class="thumb empty"><Icon name="user" size={showHeader ? 32 : 24} /></div>
          {/if}
          <div class="info">
            {#if displayName || channelInfo}
              <span class="title-row">
                <span class="title">{displayName || $t('channel.title')}</span>
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
                {#if displayHandle}<span class="handle">{displayHandle}</span>{/if}
                {#if displaySubscribers}<span class="meta-item"
                    ><Icon name="user" size={12} />{formatNumber(displaySubscribers)} subscribers</span
                  >{/if}
                {#if displayCount}<span class="meta-item"
                    ><Icon name="video" size={12} />{displayCount}
                    {$t('channel.videos').toLowerCase()}</span
                  >{/if}
                {#if channelInfo && getTotalDuration(channelInfo.entries)}<span class="meta-item"
                    ><Icon name="clock" size={12} />{formatDuration(
                      getTotalDuration(channelInfo.entries)!
                    )}</span
                  >{/if}
              </span>
              {#if channelInfo?.description}
                <p class="description">{channelInfo.description}</p>
              {/if}
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
              class:active={bulkMode === 'auto' || !bulkMode}
              onclick={() => applyModeToAll('auto')}
              disabled={loading}
            >
              <Icon name="download" size={14} />
              <span>Auto</span>
            </button>
            <button
              class="mode-btn"
              class:active={bulkMode === 'audio'}
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
          checked={useChannelFolder}
          label={$t('playlist.createFolder')}
          disabled={loading}
          onchange={(v: boolean) => (useChannelFolder = v)}
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
          disabled={loading && !channelInfo}
        >
          <span class="selected-badge">{selectedCount}/{channelInfo?.totalCount ?? 0}</span>
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

      {#if showEntries && (channelInfo || loading)}
        <div class="entries-panel">
          {#if channelInfo}
            <div class="tab-bar">
              <button
                class="tab-btn"
                class:active={activeTab === 'videos'}
                onclick={() => (activeTab = 'videos')}
              >
                <Icon name="video" size={14} />
                <span>{$t('channel.videos')}</span>
                <span class="tab-count">{tabCounts.videos}</span>
              </button>
              <button
                class="tab-btn"
                class:active={activeTab === 'shorts'}
                onclick={() => (activeTab = 'shorts')}
              >
                <Icon name="play" size={14} />
                <span>{$t('channel.shorts')}</span>
                <span class="tab-count">{tabCounts.shorts}</span>
              </button>
              <button
                class="tab-btn"
                class:active={activeTab === 'live'}
                onclick={() => (activeTab = 'live')}
              >
                <Icon name="eye_line_duotone" size={14} />
                <span>{$t('channel.live')}</span>
                <span class="tab-count">{tabCounts.live}</span>
              </button>
            </div>
          {/if}

          <div class="entries-toolbar">
            <div class="search-box">
              <Icon name="search" size={14} />
              <input type="text" bind:value={searchQuery} placeholder={$t('channel.search')} />
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

              <button
                class="select-all-btn"
                onclick={toggleSelectAll}
                disabled={loading || !channelInfo}
              >
                <Checkbox checked={allSelected} disabled />
                <span>{allSelected ? $t('channel.deselectAll') : $t('channel.selectAll')}</span>
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
            {#if loading && !channelInfo}
              <div class="loading-state">
                <div class="spinner"></div>
                <span>{$t('channel.loading')}</span>
              </div>
            {:else if filteredEntries.length === 0}
              <div class="empty-state">
                <Icon name="video" size={32} />
                <span>
                  {#if searchQuery}
                    {$t('channel.noResults')}
                  {:else if activeTab === 'videos'}
                    {$t('channel.noVideos')}
                  {:else if activeTab === 'shorts'}
                    {$t('channel.noShorts')}
                  {:else}
                    {$t('channel.noLive')}
                  {/if}
                </span>
              </div>
            {:else}
              <MediaGrid
                items={filteredEntries}
                mapItem={mapEntryToMediaItem}
                selectedIds={isSelected}
                getDefaultSettings={() => getDefaultSettings()}
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
            {/if}
          </div>
        </div>
      {/if}

      {#if !showHeader && ondownload}
        <div class="footer-actions">
          <div class="spacer"></div>
          <button
            class="download-btn"
            onclick={handleDownload}
            disabled={loading || selectedCount === 0}
          >
            <Icon name="download" size={18} />
            <span>
              {$t('common.download')}
              {selectedCount > 0 ? `(${selectedCount})` : ''}
            </span>
          </button>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .channel-builder {
    display: flex;
    flex-direction: column;
    gap: 6px;
    animation: fadeIn 0.2s ease-out;
  }

  .channel-builder.full-bleed {
    padding: 0 8px 0 0;
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  @media (max-width: 480px) {
    .channel-builder.full-bleed {
      height: auto;
      min-height: 100%;
    }
  }

  .channel-builder.full-bleed .view-header {
    position: sticky;
    top: 0;
    z-index: 10;
    margin: 0;
    padding: 10px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .channel-builder.full-bleed .card {
    background: transparent;
    border: none;
    border-radius: 0;
    padding: 0;
    flex: 1;
    min-height: 0;
  }

  .channel-builder.full-bleed:not(.entries-open) .card {
    overflow-y: auto;
  }

  .channel-builder.full-bleed.entries-open .card {
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .channel-builder.full-bleed .entries-container {
    height: auto;
    max-height: none;
    border-radius: 0;
    background: transparent;
    padding: 0;
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
    padding: 10px 0;
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
    font-size: 13px;
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

  .yt-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: var(--radius, 8px);
    color: rgba(255, 255, 255, 0.6);
    font-size: 12px;
    font-weight: 500;
    margin-bottom: 4px;
  }

  .header-badge {
    display: none;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    background: rgba(99, 102, 241, 0.12);
    border-radius: var(--radius-sm, 6px);
    color: #818cf8;
    font-size: 11px;
    font-weight: 600;
  }

  @media (min-width: 400px) {
    .header-badge {
      display: inline-flex;
    }
  }

  .header-badge.channel {
    background: rgba(99, 102, 241, 0.12);
    color: #818cf8;
  }

  .header-spacer {
    flex: 1;
    min-width: 8px;
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
    font-size: 13px;
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

  .card {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: var(--radius-lg, 12px);
    padding: 12px;
  }

  .error-state {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px;
    background: rgba(239, 68, 68, 0.1);
    border-radius: var(--radius, 8px);
    color: #ef4444;
    font-size: 13px;
  }

  .retry-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 6px;
    background: rgba(255, 255, 255, 0.1);
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: white;
    cursor: pointer;
    margin-left: auto;
  }

  .retry-btn:hover {
    background: rgba(255, 255, 255, 0.15);
  }

  .main-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  .left {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .thumb {
    width: 64px;
    height: 64px;
    border-radius: 50%;
    object-fit: cover;
    background: rgba(255, 255, 255, 0.05);
    flex-shrink: 0;
  }

  .thumb.empty {
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.3);
  }

  .thumb.skeleton {
    animation: pulse 1.5s ease-in-out infinite;
  }

  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 4px;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .title {
    font-size: 15px;
    font-weight: 600;
    color: white;
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
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

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .meta .handle {
    color: rgba(255, 255, 255, 0.7);
    font-weight: 500;
  }

  .meta-item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .description {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.45);
    line-height: 1.4;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .title-skel {
    width: 180px;
    height: 18px;
    border-radius: var(--radius-sm, 4px);
  }

  .meta-skel {
    width: 120px;
    height: 14px;
    border-radius: var(--radius-sm, 4px);
  }

  .right {
    flex-shrink: 0;
  }

  .mode-selector {
    display: flex;
    gap: 3px;
    background: rgba(255, 255, 255, 0.03);
    padding: 3px;
    border-radius: var(--radius, 8px);
  }

  .mode-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.5);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }

  .mode-btn:hover:not(:disabled) {
    color: white;
    background: rgba(255, 255, 255, 0.05);
  }

  .mode-btn.active {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .mode-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .extras-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.04);
  }

  .spacer {
    flex: 1;
  }

  .more-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 10px;
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.6);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }

  .more-btn:hover,
  .more-btn.active {
    background: rgba(255, 255, 255, 0.05);
    color: white;
  }

  .entries-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }

  .entries-toggle:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.06);
    color: white;
  }

  .entries-toggle:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .selected-badge {
    padding: 2px 8px;
    background: var(--accent, #6366f1);
    border-radius: var(--radius, 10px);
    font-size: 11px;
    font-weight: 600;
    color: white;
  }

  .more-options {
    margin-top: 10px;
    padding: 10px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.04);
    border-radius: var(--radius, 8px);
    display: flex;
    flex-direction: column;
    gap: 12px;
    animation: fadeIn 0.15s ease-out;
  }

  .options-sections {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .options-row {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .option-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 140px;
  }

  .option-group.compact {
    min-width: 120px;
    flex: 1;
  }

  .group-label {
    font-size: 11px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.5);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .option-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
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

  .tab-bar {
    display: flex;
    gap: 3px;
    padding: 3px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: var(--radius, 8px);
    overflow-x: auto;
    margin-bottom: 8px;
  }

  .tab-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.5);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
  }

  .tab-btn:hover {
    color: white;
    background: rgba(255, 255, 255, 0.05);
  }

  .tab-btn.active {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .tab-count {
    padding: 2px 6px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 10px);
    font-size: 11px;
    color: rgba(255, 255, 255, 0.7);
  }

  .tab-btn.active .tab-count {
    background: rgba(255, 255, 255, 0.15);
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

  .channel-builder.full-bleed.entries-open .entries-panel {
    flex: 1;
  }

  .entries-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 8px;
  }

  .search-box {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 140px;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius, 8px);
  }

  .search-box input {
    flex: 1;
    background: none;
    border: none;
    color: white;
    font-size: 13px;
    outline: none;
    min-width: 80px;
  }

  .search-box input::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }

  .clear-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 4px);
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
  }

  .clear-btn:hover {
    color: white;
    background: rgba(255, 255, 255, 0.1);
  }

  .toolbar-right {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .view-toggle {
    display: flex;
    gap: 2px;
    background: rgba(255, 255, 255, 0.03);
    padding: 3px;
    border-radius: var(--radius-sm, 6px);
  }

  .view-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 6px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 4px);
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    transition: all 0.15s;
  }

  .view-btn:hover {
    color: white;
    background: rgba(255, 255, 255, 0.05);
  }

  .view-btn.active {
    color: white;
    background: rgba(255, 255, 255, 0.1);
  }

  .select-all-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius, 8px);
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .select-all-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .select-all-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
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

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 40px 24px;
    color: rgba(255, 255, 255, 0.4);
    font-size: 14px;
  }

  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 40px 24px;
    color: rgba(255, 255, 255, 0.5);
    font-size: 14px;
  }

  .spinner {
    width: 28px;
    height: 28px;
    border: 3px solid rgba(255, 255, 255, 0.1);
    border-top-color: var(--accent, #6366f1);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 0.4;
    }
    50% {
      opacity: 0.7;
    }
  }

  .skeleton {
    background: rgba(255, 255, 255, 0.08);
    animation: pulse 1.5s ease-in-out infinite;
  }

  .footer-actions {
    margin-top: 8px;
    padding-top: 10px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
    display: flex;
    align-items: center;
    gap: 12px;
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
    font-size: 14px;
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
    transform: none;
  }

  @media (max-width: 480px) {
    .main-row {
      flex-direction: column;
      gap: 12px;
    }

    .left {
      width: 100%;
    }

    .thumb {
      width: 56px;
      height: 56px;
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

    .options-row {
      flex-direction: column;
    }

    .option-group.compact {
      width: 100%;
    }
  }
</style>
