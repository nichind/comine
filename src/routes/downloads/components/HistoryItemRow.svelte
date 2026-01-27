<script lang="ts">
  import { getContext } from 'svelte';
  import { DOWNLOADS_CONTEXT_KEY, type DownloadsContext } from '$lib/stores/downloadsContext';
  import type { UnifiedDownloadItem, DownloadsState } from '$lib/stores/downloadsState.svelte';
  import { queue } from '$lib/stores/queue';
  import Icon, { type IconName } from '$lib/components/Icon.svelte';
  import HighlightText from '$lib/components/HighlightText.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/ContextMenu.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { t } from '$lib/i18n';
  import { formatDuration } from '$lib/utils/format';
  import { getConversionFormats } from '$lib/utils/conversion';
  import {
    buildContextMenuItems,
    buildConvertMenuItems,
    handleContextMenuAction,
  } from '$lib/utils/contextMenuActions';

  export interface Props {
    item: UnifiedDownloadItem;
    dState: DownloadsState;
    showSeparator?: boolean;
  }

  let { item, dState, showSeparator = true }: Props = $props();
  const ctx = getContext<DownloadsContext>(DOWNLOADS_CONTEXT_KEY);

  let contextMenuOpen = $state(false);
  let contextMenuX = $state(0);
  let contextMenuY = $state(0);

  let convertMenuOpen = $state(false);
  let convertMenuX = $state(0);
  let convertMenuY = $state(0);

  let gridTemplateColumns = $derived.by(() => {
    const cols = ['56px', '1fr'];
    if (dState.visibleColumns.includes('format')) cols.push('50px');
    if (dState.visibleColumns.includes('size')) cols.push('70px');
    if (dState.visibleColumns.includes('duration')) cols.push('60px');
    return cols.join(' ');
  });

  let rowState = $derived.by(() => ({
    isHovered: dState.hoveredItemId === item.id,
    isSelected: dState.isItemSelected(item.id),
    fileMissing: !item.isActive && dState.isFileMissing(item.id),
    thumbnailSrc: dState.getThumbnailSrc(item.thumbnail),
    colorStyle: dState.getItemColorStyle(item.thumbnail),
    isFailed: dState.isThumbnailFailed(item.id),
    localThumbnail: dState.getLocalThumbnail(item.id),
  }));

  let isActiveDownload = $derived(item.isActive);
  let displayProgress = $derived(Math.max(0, Math.min(100, Math.round(item.progress ?? 0))));
  let isPending = $derived(item.status === 'pending');
  let isPaused = $derived(item.status === 'paused');
  let isDownloading = $derived(
    item.status === 'downloading' ||
      item.status === 'processing' ||
      item.status === 'fetching-info' ||
      item.status === 'converting'
  );
  let isFailed = $derived(item.status === 'failed');

  let subtitle = $derived(dState.getItemSubtitle(item));

  let conversionFormats = $derived(getConversionFormats(item.extension));

  let convertMenuItems = $derived.by((): MenuItem[] => {
    return buildConvertMenuItems(conversionFormats);
  });

  function handleRowClick(e: MouseEvent) {
    if (e.ctrlKey || e.shiftKey || dState.isSelectionMode) {
      e.preventDefault();
      e.stopPropagation();
      dState.toggleSelection(item.id, e.ctrlKey || dState.isSelectionMode, e.shiftKey);
    }
  }

  function handleRowDoubleClick(e: MouseEvent) {
    if (e.ctrlKey || e.shiftKey || dState.isSelectionMode) return;
    if (!item.isActive && item.filePath && !rowState.fileMissing) {
      ctx.playItem(item as any);
    }
  }

  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    contextMenuX = e.clientX;
    contextMenuY = e.clientY;
    contextMenuOpen = true;
  }

  function handleTap(e: MouseEvent) {
    if (window.matchMedia('(hover: hover)').matches) return;
    if (e.ctrlKey || e.shiftKey || dState.isSelectionMode) return;

    e.preventDefault();
    e.stopPropagation();

    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    contextMenuX = rect.left + rect.width / 2;
    contextMenuY = rect.top + rect.height / 2;
    contextMenuOpen = true;
  }

  let contextMenuItems = $derived.by((): MenuItem[] => {
    const items = buildContextMenuItems(item, rowState.fileMissing, $t);
    const selectItem: MenuItem = rowState.isSelected
      ? { id: 'deselect', label: $t('downloads.deselectItem'), icon: 'check' }
      : { id: 'select', label: $t('downloads.selectItem'), icon: 'check' };
    return [selectItem, { id: 'divider-select', label: '', divider: true }, ...items];
  });

  function handleContextMenuSelect(id: string) {
    if (id === 'select' || id === 'deselect') {
      dState.toggleSelection(item.id, true, false);
      return;
    }
    handleContextMenuAction(id, item, ctx, () => {
      convertMenuX = contextMenuX;
      convertMenuY = contextMenuY;
      convertMenuOpen = true;
    });
  }

  function getTypeIcon(type: string): IconName {
    switch (type) {
      case 'video':
        return 'video';
      case 'audio':
        return 'music';
      case 'image':
        return 'image';
      default:
        return 'file_text';
    }
  }

  $effect(() => {
    if (!item.isActive && item.filePath) {
      dState.checkFileExists(item.id, item.filePath);
    }
  });

  function handleImageLoad() {
    dState.extractItemColor(rowState.thumbnailSrc);
  }

  async function handleImageError() {
    dState.markThumbnailFailed(item.id);
    if (item.filePath && !rowState.localThumbnail) {
      await dState.generateLocalThumbnail(item.id, item.filePath);
    }
  }
</script>

<div
  class="history-item"
  class:with-separator={showSeparator}
  class:file-missing={rowState.fileMissing}
  class:selected={rowState.isSelected}
  class:active-download={isActiveDownload}
  class:paused={isPaused}
  class:failed={isFailed}
  class:hovered={rowState.isHovered}
  style="{rowState.colorStyle} --progress: {displayProgress}%; grid-template-columns: {gridTemplateColumns};"
  role="button"
  tabindex="0"
  onmouseenter={() => dState.setHoveredItem(item.id)}
  onmouseleave={() => dState.setHoveredItem(null)}
  onclick={(e) => {
    handleRowClick(e);
    handleTap(e);
  }}
  ondblclick={handleRowDoubleClick}
  onkeydown={(e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      handleRowDoubleClick(e as unknown as MouseEvent);
    } else if (e.key === ' ') {
      e.preventDefault();
      dState.toggleSelection(item.id, true, false);
    } else if (e.key === 'Delete') {
      e.preventDefault();
      if (item.isActive) {
        queue.cancel(item.id);
      } else {
        ctx.deleteItem(item.id);
      }
    }
  }}
  oncontextmenu={handleContextMenu}
>
  {#if isActiveDownload && !isFailed}
    <div class="progress-bg"></div>
  {/if}

  <div class="col-thumb">
    {#if rowState.isSelected}
      <div class="thumb-selected-overlay">
        <Icon name="check" size={16} />
      </div>
    {:else if rowState.localThumbnail}
      <img src={rowState.localThumbnail} alt="" class="thumbnail" loading="lazy" decoding="async" />
    {:else if item.thumbnail && !rowState.isFailed}
      <img
        src={rowState.thumbnailSrc}
        alt=""
        class="thumbnail"
        loading="lazy"
        decoding="async"
        onload={handleImageLoad}
        onerror={handleImageError}
      />
    {:else if !item.thumbnail && item.filePath}
      {#await dState.generateLocalThumbnail(item.id, item.filePath)}
        <div class="thumbnail-placeholder">
          <Icon name={getTypeIcon(item.type)} size={20} />
        </div>
      {:then localThumb}
        {#if localThumb}
          <img src={localThumb} alt="" class="thumbnail" loading="lazy" decoding="async" />
        {:else}
          <div class="thumbnail-placeholder">
            <Icon name={getTypeIcon(item.type)} size={20} />
          </div>
        {/if}
      {:catch}
        <div class="thumbnail-placeholder">
          <Icon name={getTypeIcon(item.type)} size={20} />
        </div>
      {/await}
    {:else}
      <div class="thumbnail-placeholder">
        <Icon name={getTypeIcon(item.type)} size={20} />
      </div>
    {/if}

    {#if isDownloading}
      <div class="spinner-overlay">
        <div class="spinner"></div>
      </div>
    {:else if isPaused}
      <div class="paused-overlay">
        <Icon name="pause" size={14} />
      </div>
    {:else if rowState.fileMissing}
      <div class="thumb-missing-indicator" use:tooltip={$t('downloads.fileMissing')}>
        <Icon name="trash" size={12} />
      </div>
    {:else if rowState.isHovered && !isActiveDownload}
      <button
        class="thumb-play-overlay"
        onclick={(e) => {
          e.stopPropagation();
          ctx.playItem(item as any);
        }}
        use:tooltip={$t('downloads.play')}
      >
        <Icon name="play" size={16} />
      </button>
    {/if}
  </div>

  <div class="col-metadata">
    <div class="item-title-row">
      {#if (item.type === 'video' || item.type === 'audio') && !item.isActive}
        <button
          type="button"
          class="item-title clickable"
          onclick={(e) => {
            e.stopPropagation();
            ctx.openItem(item as any);
          }}
          use:tooltip={$t('downloads.openInApp')}
        >
          <HighlightText text={item.title} highlight={dState.searchQuery} />
        </button>
      {:else}
        <span class="item-title" use:tooltip={item.title}>
          <HighlightText text={item.title} highlight={dState.searchQuery} />
        </span>
      {/if}
      {#if item.convertedFormat}
        <span
          class="converted-tag"
          use:tooltip={$t('downloads.convertedFrom', { format: item.convertedFormat })}
          >{$t('downloads.converted')}</span
        >
      {/if}
      {#if dState.showSourceTags && item.downloadSource}
        <span class="source-tag"
          >{$t(`downloads.source.${item.downloadSource}`, { default: item.downloadSource })}</span
        >
      {/if}
    </div>
    {#if subtitle.type === 'author' && item.authorUrl}
      <button
        type="button"
        class="item-subtitle clickable"
        onclick={(e) => {
          e.stopPropagation();
          ctx.openAuthor(item as any);
        }}
        use:tooltip={$t('downloads.viewChannel')}
      >
        <HighlightText text={subtitle.text} highlight={dState.searchQuery} />
      </button>
    {:else}
      <span
        class="item-subtitle"
        class:status={subtitle.type === 'status'}
        class:error={subtitle.type === 'error'}
        use:tooltip={subtitle.type === 'error' ? item.error : item.author}
      >
        {#if subtitle.type === 'error'}
          <Icon name="warning" size={10} />
        {/if}
        <HighlightText
          text={subtitle.text}
          highlight={subtitle.type === 'author' ? dState.searchQuery : ''}
        />
      </span>
    {/if}
  </div>

  {#if isFailed && item.isActive}
    <div class="retry-button-container">
      <button
        class="retry-btn"
        onclick={(e) => {
          e.stopPropagation();
          queue.retry(item.id);
        }}
        use:tooltip={$t('downloads.retry')}
      >
        <Icon name="refresh" size={14} />
        <span>{$t('downloads.retry')}</span>
      </button>
    </div>
  {/if}

  <div class="floating-action-bar" class:visible={rowState.isHovered}>
    {#if isActiveDownload}
      {#if isFailed}
        <button
          class="action-btn"
          onclick={(e) => {
            e.stopPropagation();
            queue.retry(item.id);
          }}
          use:tooltip={$t('downloads.retry')}
        >
          <Icon name="refresh" size={15} />
        </button>
      {:else if item.source !== 'convert'}
        {#if isPaused}
          <button
            class="action-btn"
            onclick={(e) => {
              e.stopPropagation();
              queue.resumeItem(item.id);
            }}
            use:tooltip={$t('downloads.queue.resumeItem')}
          >
            <Icon name="play" size={15} />
          </button>
        {:else if isPending || isDownloading}
          <button
            class="action-btn"
            onclick={(e) => {
              e.stopPropagation();
              queue.pauseItem(item.id);
            }}
            use:tooltip={$t('downloads.queue.pauseItem')}
          >
            <Icon name="pause" size={15} />
          </button>
        {/if}
      {/if}
      <button
        class="action-btn danger"
        onclick={(e) => {
          e.stopPropagation();
          queue.cancel(item.id);
        }}
        use:tooltip={$t('common.cancel')}
      >
        <Icon name="close" size={15} />
      </button>
    {:else}
      <button
        class="action-btn"
        onclick={(e) => {
          e.stopPropagation();
          if (item.filePath) ctx.openFileLocation(item.filePath);
        }}
        use:tooltip={$t('downloads.openFolder')}
        disabled={rowState.fileMissing}
      >
        <Icon name="folder" size={15} />
      </button>
      {#if conversionFormats.length > 0 && !rowState.fileMissing}
        <button
          class="action-btn"
          onclick={(e) => {
            e.stopPropagation();
            const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
            convertMenuX = rect.left;
            convertMenuY = rect.bottom + 4;
            convertMenuOpen = true;
          }}
          use:tooltip={$t('downloads.convert')}
        >
          <Icon name="refresh" size={15} />
        </button>
      {/if}
      <button
        class="action-btn danger"
        onclick={(e) => {
          e.stopPropagation();
          ctx.deleteItem(item.id);
        }}
        use:tooltip={$t('downloads.delete')}
      >
        <Icon name="trash" size={15} />
      </button>
      <button
        class="action-btn kebab"
        onclick={(e) => {
          e.stopPropagation();
          contextMenuX = e.clientX;
          contextMenuY = e.clientY;
          contextMenuOpen = true;
        }}
        use:tooltip={$t('common.more')}
      >
        <Icon name="dots" size={15} />
      </button>
    {/if}
  </div>

  {#if dState.visibleColumns.includes('format')}
    <div class="col-ext">
      {#if item.extension}
        <span class="ext-badge">{item.extension.toUpperCase()}</span>
      {:else}
        <span class="ext-badge">—</span>
      {/if}
    </div>
  {/if}
  {#if dState.visibleColumns.includes('size')}
    <div class="col-size">{dState.getItemSizeDisplay(item)}</div>
  {/if}
  {#if dState.visibleColumns.includes('duration')}
    <div class="col-length">
      {#if item.duration > 0 || item.type === 'video' || item.type === 'audio'}
        {formatDuration(item.duration)}
      {:else}
        —
      {/if}
    </div>
  {/if}
</div>

<ContextMenu
  bind:open={contextMenuOpen}
  x={contextMenuX}
  y={contextMenuY}
  items={contextMenuItems}
  onclose={() => (contextMenuOpen = false)}
  onselect={handleContextMenuSelect}
/>

<ContextMenu
  bind:open={convertMenuOpen}
  x={convertMenuX}
  y={convertMenuY}
  items={convertMenuItems}
  onclose={() => (convertMenuOpen = false)}
  onselect={handleContextMenuSelect}
/>

<style>
  .history-item {
    display: grid;
    gap: 12px;
    align-items: center;
    padding: 8px 16px;
    cursor: default;
    background: transparent;
    border-radius: var(--radius, 10px);
    transition: background-color 0.15s ease;
    height: 56px;
    contain: layout style;
    position: relative;
    overflow: visible;
  }

  .history-item.with-separator::before {
    content: '';
    position: absolute;
    left: 16px;
    right: 16px;
    top: 0;
    height: 1px;
    background: color-mix(
      in srgb,
      var(--border-subtle, var(--surface-border, rgba(255, 255, 255, 1))) 20%,
      transparent
    );
    pointer-events: none;
  }

  .history-item:hover,
  .history-item.hovered {
    background: var(--item-color-hover, rgba(255, 255, 255, 0.03));
  }

  .history-item.selected {
    background: rgba(255, 255, 255, 0.08);
  }

  .history-item.active-download {
    background: rgba(255, 255, 255, 0.01);
  }

  .history-item.paused {
    opacity: 0.7;
  }

  .history-item.failed {
    background: rgba(239, 68, 68, 0.08);
  }

  .progress-bg {
    position: absolute;
    inset: 0;
    right: auto;
    width: var(--progress, 0%);
    border-radius: inherit;
    background: linear-gradient(
      90deg,
      var(--item-color-alpha, rgba(99, 102, 241, 0.15)) 0%,
      var(--item-color-alpha, rgba(99, 102, 241, 0.08)) 100%
    );
    transition: width 0.3s ease-out;
    pointer-events: none;
  }

  .col-thumb {
    position: relative;
    width: 56px;
    height: 32px;
    border-radius: var(--radius-sm, 4px);
    overflow: hidden;
    background: rgba(0, 0, 0, 0.3);
    flex-shrink: 0;
    z-index: 1;
  }

  .thumb-selected-overlay {
    position: absolute;
    inset: 0;
    background: var(--accent, #6366f1);
    display: flex;
    align-items: center;
    justify-content: center;
    color: white;
  }

  .thumbnail {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumbnail-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.2);
  }

  .thumb-play-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(2px);
    border: none;
    color: white;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.2s;
  }

  .col-thumb:hover .thumb-play-overlay,
  .thumb-play-overlay:focus {
    opacity: 1;
  }

  .spinner-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
  }

  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .paused-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
    color: rgba(255, 255, 255, 0.8);
  }

  .thumb-missing-indicator {
    position: absolute;
    inset: 0;
    background: rgba(239, 68, 68, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    color: white;
  }

  .col-metadata {
    display: flex;
    flex-direction: column;
    justify-content: center;
    min-width: 0;
    gap: 2px;
    z-index: 1;
    padding: 0 4px 0 0;
  }

  .item-title {
    font-size: var(--text-base, 13px);
    font-weight: 500;
    color: rgba(255, 255, 255, 0.9);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: color 0.15s;
    background: none;
    border: none;
    padding: 0;
    text-align: left;
  }

  .item-title.clickable {
    cursor: pointer;
  }

  .item-title.clickable:hover {
    color: var(--item-color, var(--accent, #6366f1));
  }

  .item-title-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .converted-tag {
    flex-shrink: 0;
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    padding: 2px 5px;
    border-radius: var(--radius-sm, 4px);
    background: var(--item-color, var(--accent, #6366f1));
    color: white;
    opacity: 0.85;
  }

  .source-tag {
    flex-shrink: 0;
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    padding: 2px 5px;
    border-radius: var(--radius-sm, 4px);
    background: rgba(255, 255, 255, 0.15);
    color: rgba(255, 255, 255, 0.7);
  }

  .item-subtitle {
    font-size: var(--text-xs, 11px);
    color: rgba(255, 255, 255, 0.5);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    display: flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    padding: 0;
    text-align: left;
  }

  .item-subtitle.clickable {
    cursor: pointer;
    transition: color 0.15s;
  }

  .item-subtitle.clickable:hover {
    color: var(--item-color, var(--accent, #6366f1));
  }

  .item-subtitle.status {
    color: var(--item-color, var(--accent, #6366f1));
    font-weight: 500;
  }

  .item-subtitle.error {
    color: #f87171;
  }

  .floating-action-bar {
    position: absolute;
    right: 12px;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 3px;
    background: var(--surface-bg, rgba(18, 18, 18, 0.75));
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 8px);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
    opacity: 0;
    pointer-events: none;
    z-index: 10;
    transition: opacity 0.15s ease;
  }

  @media (hover: hover) {
    .floating-action-bar.visible {
      opacity: 1;
      pointer-events: auto;
    }
  }

  @media (hover: none) {
    .floating-action-bar {
      display: none;
    }

    .history-item {
      -webkit-tap-highlight-color: rgba(var(--accent-rgb, 99, 102, 241), 0.15);
    }
  }

  .action-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.06);
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .action-btn:hover {
    background: rgba(255, 255, 255, 0.15);
    color: white;
  }

  .action-btn:active {
    transform: scale(0.96);
  }

  .action-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
    pointer-events: none;
  }

  .action-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.15);
    color: #f87171;
  }

  .action-btn.kebab:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .col-ext,
  .col-size,
  .col-length {
    font-size: var(--text-xs, 11px);
    color: rgba(255, 255, 255, 0.4);
    white-space: nowrap;
    text-align: center;
    justify-self: center;
    z-index: 1;
  }

  .ext-badge {
    background: rgba(255, 255, 255, 0.06);
    padding: 2px 6px;
    border-radius: var(--radius-sm, 4px);
    font-size: 10px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.6);
  }

  .history-item.file-missing {
    opacity: 0.6;
  }

  .retry-button-container {
    position: absolute;
    right: 12px;
    top: 50%;
    transform: translateY(-50%);
    z-index: 5;
  }

  .retry-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: rgba(99, 102, 241, 0.15);
    border: 1px solid rgba(99, 102, 241, 0.3);
    border-radius: var(--radius, 8px);
    color: var(--accent, #6366f1);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .retry-btn:hover {
    background: rgba(99, 102, 241, 0.25);
    border-color: rgba(99, 102, 241, 0.5);
  }

  .retry-btn:active {
    transform: scale(0.97);
  }

  .history-item:hover .retry-button-container,
  .history-item.hovered .retry-button-container {
    opacity: 0;
    pointer-events: none;
  }

  @media (max-width: 700px) {
    .history-item {
      gap: 10px;
      padding: 8px 4px;
    }

    .col-thumb {
      width: 48px;
      height: 27px;
    }
  }
</style>
