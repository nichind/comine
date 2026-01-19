<script lang="ts">
  import { getContext } from 'svelte';
  import { DOWNLOADS_CONTEXT_KEY, type DownloadsContext } from '$lib/stores/downloadsContext';
  import type { UnifiedDownloadItem, DownloadsState } from '$lib/stores/downloadsState.svelte';
  import { queue } from '$lib/stores/queue';
  import Icon, { type IconName } from '$lib/components/Icon.svelte';
  import HighlightText from '$lib/components/HighlightText.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { t } from '$lib/i18n';
  import { formatDuration } from '$lib/utils/format';

  interface Props {
    item: UnifiedDownloadItem;
    dState: DownloadsState;
  }
  
  let { item, dState }: Props = $props();
  const ctx = getContext<DownloadsContext>(DOWNLOADS_CONTEXT_KEY);
  
  let rowState = $derived.by(() => ({
    isHovered: dState.hoveredItemId === item.id,
    isSelected: dState.isItemSelected(item.id),
    fileMissing: !item.isActive && dState.isFileMissing(item.id),
    thumbnailSrc: dState.getThumbnailSrc(item.thumbnail),
    colorStyle: dState.getItemColorStyle(item.thumbnail),
    isFailed: dState.isThumbnailFailed(item.id),
  }));

  let isActiveDownload = $derived(item.isActive);
  let displayProgress = $derived(Math.max(0, Math.min(100, Math.round(item.progress ?? 0))));
  let isPending = $derived(item.status === 'pending');
  let isPaused = $derived(item.status === 'paused');
  let isDownloading = $derived(item.status === 'downloading' || item.status === 'processing' || item.status === 'fetching-info');
  let isFailed = $derived(item.status === 'failed');
  
  let subtitle = $derived(dState.getItemSubtitle(item));

  function handleRowClick(e: MouseEvent) {
    if (e.ctrlKey || e.shiftKey || dState.isSelectionMode) {
      e.preventDefault();
      e.stopPropagation();
      dState.toggleSelection(item.id, e.ctrlKey || dState.isSelectionMode, e.shiftKey);
    } else {
      e.stopPropagation();
      if (!item.isActive && item.filePath) {
        ctx.openItem(item as any);
      }
    }
  }

  function handleContextMenu(e: MouseEvent) {
    if (!rowState.isSelected) {
      dState.toggleSelection(item.id, false, false);
    }
  }

  function getTypeIcon(type: string): IconName {
    switch (type) {
      case 'video': return 'video';
      case 'audio': return 'music';
      case 'image': return 'image';
      default: return 'file_text';
    }
  }

  function handleImageLoad() {
    dState.extractItemColor(item.id, rowState.thumbnailSrc);
    if (!item.isActive && item.filePath) {
      dState.checkFileExists(item.id, item.filePath);
    }
  }
  
  function handleImageError() {
    dState.markThumbnailFailed(item.id);
  }
</script>

<div
  class="history-item"
  class:file-missing={rowState.fileMissing}
  class:selected={rowState.isSelected}
  class:active-download={isActiveDownload}
  class:paused={isPaused}
  class:failed={isFailed}
  style="{rowState.colorStyle} --progress: {displayProgress}%;"
  role="button"
  tabindex="0"
  onmouseenter={() => dState.setHoveredItem(item.id)}
  onmouseleave={() => dState.setHoveredItem(null)}
  onclick={handleRowClick}
  onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleRowClick(e as unknown as MouseEvent); } }}
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
        onclick={(e) => { e.stopPropagation(); ctx.playItem(item as any); }}
        use:tooltip={$t('downloads.play')}
      >
        <Icon name="play" size={16} />
      </button>
    {/if}
  </div>
  
  <div class="col-metadata">
    <span class="item-title" title={item.title}>
      <HighlightText text={item.title} highlight={dState.searchQuery} />
    </span>
    <span 
      class="item-subtitle"
      class:status={subtitle.type === 'status'}
      class:error={subtitle.type === 'error'}
      title={subtitle.type === 'error' ? item.error : item.author}
    >
      {#if subtitle.type === 'error'}
        <Icon name="warning" size={10} />
      {/if}
      <HighlightText text={subtitle.text} highlight={subtitle.type === 'author' ? dState.searchQuery : ''} />
    </span>
  </div>
  
  <div class="col-actions" class:visible={rowState.isHovered}>
    {#if isActiveDownload}
      {#if isPaused}
        <button
          class="action-btn"
          onclick={(e) => { e.stopPropagation(); queue.resumeItem(item.id); }}
          use:tooltip={$t('downloads.queue.resumeItem')}
        >
          <Icon name="play" size={15} />
        </button>
      {:else if isPending || isDownloading}
        <button
          class="action-btn"
          onclick={(e) => { e.stopPropagation(); queue.pauseItem(item.id); }}
          use:tooltip={$t('downloads.queue.pauseItem')}
        >
          <Icon name="pause" size={15} />
        </button>
      {/if}
      <button
        class="action-btn danger"
        onclick={(e) => { e.stopPropagation(); queue.cancel(item.id); }}
        use:tooltip={$t('common.cancel')}
      >
        <Icon name="close" size={15} />
      </button>
    {:else}
      <button
        class="action-btn"
        onclick={(e) => { e.stopPropagation(); if (item.filePath) ctx.openFileLocation(item.filePath); }}
        use:tooltip={$t('downloads.openFolder')}
      >
        <Icon name="folder" size={15} />
      </button>
      <button
        class="action-btn danger"
        onclick={(e) => { e.stopPropagation(); ctx.deleteItem(item.id); }}
        use:tooltip={$t('downloads.delete')}
      >
        <Icon name="trash" size={15} />
      </button>
    {/if}
  </div>

  <div class="col-ext">
    {#if item.extension}
      <span class="ext-badge">{item.extension.toUpperCase()}</span>
    {:else}
      <span class="ext-badge">—</span>
    {/if}
  </div>
  <div class="col-size">{dState.getItemSizeDisplay(item)}</div>
  <div class="col-length">{formatDuration(item.duration)}</div>
</div>

<style>
  .history-item {
    display: grid;
    grid-template-columns: 56px 1fr 60px 50px 70px 60px;
    gap: 12px;
    align-items: center;
    padding: 8px 16px;
    cursor: default;
    background: transparent;
    transition: background-color 0.15s ease;
    height: 56px;
    contain: layout style;
    position: relative;
    overflow: hidden;
  }

  .history-item:hover {
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

  /* Progress background for active downloads */
  .progress-bg {
    position: absolute;
    inset: 0;
    right: auto;
    width: var(--progress, 0%);
    background: linear-gradient(
      90deg,
      var(--item-color-alpha, rgba(99, 102, 241, 0.15)) 0%,
      var(--item-color-alpha, rgba(99, 102, 241, 0.08)) 100%
    );
    transition: width 0.3s ease-out;
    pointer-events: none;
  }

  /* Thumb Column */
  .col-thumb {
    position: relative;
    width: 56px;
    height: 32px;
    border-radius: 4px;
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
    to { transform: rotate(360deg); }
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

  /* Metadata Column */
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
    font-size: 13px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.9);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-subtitle {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .item-subtitle.status {
    color: var(--item-color, var(--accent, #6366f1));
    font-weight: 500;
  }

  .item-subtitle.error {
    color: #f87171;
  }

  .item-title {
    cursor: pointer;
    transition: color 0.15s;
  }

  .item-title:hover {
    color: var(--item-color, var(--accent, #6366f1));
  }

  /* Inline Actions Column */
  .col-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.12s;
    z-index: 1;
  }

  .col-actions.visible {
    opacity: 1;
    pointer-events: auto;
  }

  .action-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.06);
    border: none;
    border-radius: 6px;
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    transition: all 0.12s;
  }

  .action-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    color: white;
  }

  .action-btn.danger:hover {
    background: rgba(239, 68, 68, 0.2);
    color: #f87171;
  }

  /* Other Columns - Centered content */
  .col-ext, .col-size, .col-length {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
    white-space: nowrap;
    text-align: center;
    justify-self: center;
    z-index: 1;
  }
  
  .ext-badge {
    background: rgba(255, 255, 255, 0.06);
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.6);
  }

  /* Missing File */
  .history-item.file-missing {
    opacity: 0.6;
  }
  
  /* Mobile Responsiveness */
  @media (max-width: 700px) {
    .history-item {
      grid-template-columns: 48px 1fr auto;
      gap: 10px;
    }
    
    .col-ext, .col-size, .col-length {
      display: none;
    }
    
    .col-actions {
      opacity: 1;
      pointer-events: auto;
    }
  }
</style>
