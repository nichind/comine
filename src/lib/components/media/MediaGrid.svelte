<script lang="ts">
  import { t } from '$lib/i18n';
  import { tooltip } from '$lib/actions/tooltip';
  import { skeleton } from '$lib/actions/skeleton';
  import { formatDuration } from '$lib/utils/format';
  import { normalizeYouTubeThumbnailUrl } from '$lib/utils/thumbnailUtils';
  import Icon, { type IconName } from '$lib/components/ui/Icon.svelte';
  import Checkbox from '$lib/components/ui/Checkbox.svelte';
  import VirtualGrid from '$lib/components/media/VirtualGrid.svelte';

  export type ViewMode = 'list' | 'grid';

  export interface MediaItemData {
    id: string;
    title: string;
    thumbnail: string | null;
    duration: number | null;
    author: string | null;
    isMusic?: boolean;
  }

  export interface MediaItemSettings {
    downloadMode: 'auto' | 'audio' | 'mute';
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

  const DEFAULT_SETTINGS: MediaItemSettings = {
    downloadMode: 'auto',
    skipSponsors: true,
    skipIntros: false,
    skipSelfPromo: false,
    skipInteraction: false,
    embedChapters: true,
    embedThumbnail: true,
    embedMetadata: true,
    embedSubs: false,
    subLangs: 'en',
  };

  interface Props {
    items: unknown[];
    mapItem?: (item: any) => MediaItemData;
    selectedIds: Set<string> | ((id: string) => boolean);
    viewMode?: ViewMode;
    perItemSettings:
      | Map<string, Partial<MediaItemSettings>>
      | Record<string, Partial<MediaItemSettings>>;
    getDefaultSettings?: (item: MediaItemData) => MediaItemSettings;
    loading?: boolean;
    totalItemCount?: number;
    ontoggle?: (id: string) => void;
    onupdatesettings?: (id: string, settings: Partial<MediaItemSettings>) => void;
    onopenitem?: (item: MediaItemData) => void;
    onscroll?: (scrollTop: number, scrollHeight: number, clientHeight: number) => void;
  }

  let {
    items,
    mapItem,
    selectedIds,
    viewMode = 'list',
    perItemSettings,
    getDefaultSettings,
    loading = false,
    totalItemCount,
    ontoggle,
    onupdatesettings,
    onopenitem,
    onscroll,
  }: Props = $props();

  let hoveredItemId = $state<string | null>(null);
  let failedThumbnails = $state(new Set<string>());

  let mappedItems = $derived.by(() => {
    if (!mapItem) return items as MediaItemData[];
    return items.map(mapItem);
  });

  function isItemSelected(id: string): boolean {
    if (typeof selectedIds === 'function') return selectedIds(id);
    return selectedIds.has(id);
  }

  function getSettings(item: MediaItemData): MediaItemSettings {
    const base = getDefaultSettings ? getDefaultSettings(item) : DEFAULT_SETTINGS;
    const override =
      perItemSettings instanceof Map
        ? (perItemSettings.get(item.id) ?? {})
        : (perItemSettings[item.id] ?? {});
    return { ...base, ...override };
  }

  const MODE_ICONS: Record<string, IconName> = { audio: 'music', mute: 'video', auto: 'download' };
  const MODE_LABELS_KEYS: Record<string, string> = {
    audio: 'download.mode.audio',
    mute: 'download.mode.mute',
    auto: 'download.mode.auto',
  };

  function getModeIcon(mode: string): IconName {
    return MODE_ICONS[mode] ?? 'download';
  }

  function getModeLabel(mode: string): string {
    return $t(MODE_LABELS_KEYS[mode] ?? 'download.mode.auto');
  }

  const MODES: Array<'auto' | 'audio' | 'mute'> = ['auto', 'audio', 'mute'];

  function cycleMode(item: MediaItemData) {
    const current = getSettings(item).downloadMode;
    const nextIndex = (MODES.indexOf(current) + 1) % 3;
    onupdatesettings?.(item.id, { downloadMode: MODES[nextIndex] });
  }
</script>

{#if loading}
  <div class="loading-container" class:grid-loading={viewMode === 'grid'}>
    {#each Array(viewMode === 'list' ? 5 : 6) as _, i (i)}
      {#if viewMode === 'list'}
        <div class="list-item skeleton-item">
          <div class="item-check">
            <div class="checkbox-skeleton" use:skeleton={{ loading: true }}></div>
          </div>
          <div class="item-thumb" use:skeleton={{ loading: true, radius: '6px' }}></div>
          <div class="item-info">
            <span
              class="item-title"
              use:skeleton={{ loading: true, minWidth: '150px', randomWidth: true }}>&nbsp;</span
            >
            <span class="item-author" use:skeleton={{ loading: true, minWidth: '80px' }}
              >&nbsp;</span
            >
          </div>
        </div>
      {:else}
        <div class="grid-card skeleton-card">
          <div class="card-thumb" use:skeleton={{ loading: true, radius: '8px' }}></div>
          <div class="card-info">
            <span
              class="card-title"
              use:skeleton={{ loading: true, minWidth: '100px', randomWidth: true }}>&nbsp;</span
            >
            <span class="card-author" use:skeleton={{ loading: true, minWidth: '60px' }}
              >&nbsp;</span
            >
          </div>
        </div>
      {/if}
    {/each}
  </div>
{:else}
  <VirtualGrid
    items={mappedItems}
    totalCount={totalItemCount}
    {viewMode}
    gap={10}
    listRowHeight={56}
    gridMinColumnWidth={150}
    getGridRowHeight={(w) => Math.round((w * 9) / 16 + 64)}
    overscan={5}
    {onscroll}
    getKey={(item) => item.id}
  >
    {#snippet children(item: MediaItemData | undefined, index: number, isPlaceholder: boolean)}
      {#if viewMode === 'list'}
        {@const isSelected = !isPlaceholder && item && isItemSelected(item.id)}
        {@const settings = item ? getSettings(item) : DEFAULT_SETTINGS}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="list-item"
          class:selected={isSelected}
          class:placeholder={isPlaceholder}
          onmouseenter={() => item && (hoveredItemId = item.id)}
          onmouseleave={() => (hoveredItemId = null)}
          onclick={() => !isPlaceholder && item && ontoggle?.(item.id)}
        >
          {#if isPlaceholder}
            <div class="item-check">
              <div class="checkbox-skeleton" use:skeleton={{ loading: true }}></div>
            </div>
            <div class="item-thumb" use:skeleton={{ loading: true, radius: '6px' }}></div>
            <div class="item-info">
              <span
                class="item-title"
                use:skeleton={{ loading: true, minWidth: '120px', randomWidth: [50, 90] }}
                >&nbsp;</span
              >
              <span
                class="item-author"
                use:skeleton={{ loading: true, minWidth: '60px', randomWidth: [40, 70] }}
                >&nbsp;</span
              >
            </div>
          {:else if item}
            <div class="item-check">
              <Checkbox checked={isSelected ?? false} />
            </div>
            <div class="item-thumb">
              {#if item.thumbnail && !failedThumbnails.has(item.id)}
                <img
                  src={item.thumbnail}
                  alt=""
                  loading="lazy"
                  decoding="async"
                  onerror={() => {
                    failedThumbnails = new Set([...failedThumbnails, item.id]);
                  }}
                />
              {:else}
                <div class="thumb-placeholder">
                  <Icon name="video" size={16} />
                </div>
              {/if}
              {#if item.duration}
                <span class="duration-badge">{formatDuration(item.duration)}</span>
              {/if}
              {#if hoveredItemId === item.id && onopenitem}
                <button
                  class="thumb-open-btn"
                  onclick={(e) => {
                    e.stopPropagation();
                    onopenitem(item);
                  }}
                >
                  <Icon name="maximize" size={12} />
                </button>
              {/if}
            </div>
            <div class="item-info">
              <span class="item-title" title={item.title}>{item.title}</span>
              <span class="item-author">{item.author ?? ''}</span>
            </div>
            <button
              class="mode-badge"
              class:audio={settings.downloadMode === 'audio'}
              class:mute={settings.downloadMode === 'mute'}
              onclick={(e) => {
                e.stopPropagation();
                cycleMode(item);
              }}
              use:tooltip={getModeLabel(settings.downloadMode)}
            >
              <Icon name={getModeIcon(settings.downloadMode)} size={12} />
            </button>
          {/if}
        </div>
      {:else}
        {@const isSelected = !isPlaceholder && item && isItemSelected(item.id)}
        {@const settings = item ? getSettings(item) : DEFAULT_SETTINGS}
        {@const isHovered = item && hoveredItemId === item.id}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="grid-card"
          class:selected={isSelected}
          class:placeholder={isPlaceholder}
          onmouseenter={() => item && (hoveredItemId = item.id)}
          onmouseleave={() => (hoveredItemId = null)}
          onclick={() => !isPlaceholder && item && ontoggle?.(item.id)}
        >
          {#if isPlaceholder}
            <div class="card-thumb" use:skeleton={{ loading: true, radius: '8px' }}></div>
            <div class="card-info">
              <span
                class="card-title"
                use:skeleton={{ loading: true, minWidth: '80px', randomWidth: [50, 90] }}
                >&nbsp;</span
              >
              <span
                class="card-author"
                use:skeleton={{ loading: true, minWidth: '50px', randomWidth: [40, 70] }}
                >&nbsp;</span
              >
            </div>
          {:else if item}
            <div class="card-thumb">
              {#if item.thumbnail && !failedThumbnails.has(item.id)}
                <img
                  src={normalizeYouTubeThumbnailUrl(item.thumbnail, 'mq')}
                  alt=""
                  loading="lazy"
                  decoding="async"
                  onerror={() => {
                    failedThumbnails = new Set([...failedThumbnails, item.id]);
                  }}
                />
              {:else}
                <div class="card-thumb-placeholder">
                  <Icon name="video" size={28} />
                </div>
              {/if}
              <div class="card-check">
                <Checkbox checked={isSelected ?? false} />
              </div>
              {#if item.duration}
                <span class="duration-badge">{formatDuration(item.duration)}</span>
              {/if}
              <button
                class="mode-indicator"
                class:audio={settings.downloadMode === 'audio'}
                class:mute={settings.downloadMode === 'mute'}
                onclick={(e) => {
                  e.stopPropagation();
                  cycleMode(item);
                }}
                title={getModeLabel(settings.downloadMode)}
              >
                <Icon name={getModeIcon(settings.downloadMode)} size={10} />
              </button>
              {#if isHovered && onopenitem}
                <button
                  class="card-open-btn"
                  onclick={(e) => {
                    e.stopPropagation();
                    onopenitem(item);
                  }}
                >
                  <Icon name="maximize" size={14} />
                </button>
              {/if}
            </div>
            <div class="card-info">
              <span class="card-title" title={item.title}>{item.title}</span>
              <span class="card-author">{item.author ?? ''}</span>
            </div>
          {/if}
        </div>
      {/if}
    {/snippet}

    {#snippet empty()}
      <div class="empty-state">
        <Icon name="search" size={24} />
        <span>{$t('playlist.noResults')}</span>
      </div>
    {/snippet}
  </VirtualGrid>
{/if}

<style>
  .loading-container {
    display: flex;
    flex-direction: column;
    padding: 8px;
  }

  .loading-container.grid-loading {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 10px;
  }

  .list-item {
    display: grid;
    grid-template-columns: 24px 56px 1fr auto;
    gap: 12px;
    padding: 8px 12px;
    align-items: center;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    background: transparent;
    transition: background 0.15s;
    user-select: none;
    height: 56px;
    box-sizing: border-box;
  }

  .list-item:hover {
    background: rgba(255, 255, 255, 0.04);
  }

  .list-item.selected {
    background: rgba(99, 102, 241, 0.1);
  }

  .list-item.placeholder {
    pointer-events: none;
  }

  .item-check {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .checkbox-skeleton {
    width: 16px;
    height: 16px;
    border-radius: 4px;
  }

  .item-thumb {
    position: relative;
    width: 56px;
    height: 32px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.2);
    overflow: hidden;
    flex-shrink: 0;
  }

  .item-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumb-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.2);
  }

  .duration-badge {
    position: absolute;
    bottom: 2px;
    right: 2px;
    background: rgba(0, 0, 0, 0.8);
    color: rgba(255, 255, 255, 0.9);
    font-size: 9px;
    padding: 1px 4px;
    border-radius: 2px;
    font-weight: 500;
  }

  .thumb-open-btn {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.7);
    color: white;
    border: none;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    backdrop-filter: blur(4px);
  }

  .item-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .item-title {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.9);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-author {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
  }

  .mode-badge {
    height: 24px;
    width: 24px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.4);
    border: none;
    cursor: pointer;
    transition: all 0.2s;
  }

  .mode-badge:hover {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.8);
  }

  .mode-badge.audio {
    color: #fca5a5;
    background: rgba(252, 165, 165, 0.1);
  }

  .mode-badge.mute {
    color: #93c5fd;
    background: rgba(147, 197, 253, 0.1);
  }

  .grid-card {
    display: flex;
    flex-direction: column;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 8px;
    overflow: hidden;
    transition: all 0.15s;
    cursor: pointer;
    position: relative;
    border: 1px solid transparent;
    height: 100%;
  }

  .grid-card:hover {
    background: rgba(255, 255, 255, 0.06);
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  }

  .grid-card.selected {
    border-color: rgba(99, 102, 241, 0.5);
    background: rgba(99, 102, 241, 0.08);
  }

  .grid-card.placeholder {
    pointer-events: none;
  }

  .card-thumb {
    position: relative;
    width: 100%;
    aspect-ratio: 16/9;
    background: rgba(0, 0, 0, 0.3);
  }

  .card-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .card-thumb-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.2);
  }

  .card-check {
    position: absolute;
    top: 6px;
    left: 6px;
    z-index: 2;
  }

  .mode-indicator {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 20px;
    height: 20px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.7);
    border: none;
    z-index: 2;
    cursor: pointer;
  }

  .mode-indicator.audio {
    color: #fca5a5;
  }

  .mode-indicator.mute {
    color: #93c5fd;
  }

  .card-open-btn {
    position: absolute;
    bottom: 6px;
    right: 6px;
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.7);
    border: none;
    border-radius: 5px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    transition: all 0.15s;
    z-index: 3;
  }

  .card-open-btn:hover {
    background: var(--accent, #6366f1);
    color: white;
  }

  .card-info {
    padding: 8px 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }

  .card-title {
    font-size: 11px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.9);
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    line-height: 1.3;
  }

  .card-author {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.4);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 32px;
    color: rgba(255, 255, 255, 0.4);
    font-size: 12px;
  }

  .skeleton-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
  }

  .skeleton-card {
    overflow: hidden;
  }

  @media (max-width: 560px) {
    .list-item {
      grid-template-columns: auto 48px 1fr auto;
      gap: 8px;
      padding: 6px 8px;
    }

    .item-thumb {
      width: 48px;
      height: 27px;
    }

    .item-title {
      font-size: 11px;
    }

    .mode-badge {
      width: 24px;
      height: 24px;
    }
  }
</style>
