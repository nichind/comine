<script lang="ts">
  import { getContext } from 'svelte';
  import { DOWNLOADS_CONTEXT_KEY, type DownloadsContext } from '$lib/stores/downloadsContext';
  import type { UnifiedDownloadItem, DownloadsState } from '$lib/stores/downloadsState.svelte';
  import { queue } from '$lib/stores/queue';
  import Icon from '$lib/components/Icon.svelte';
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
  
  let cardState = $derived.by(() => ({
    isHovered: dState.hoveredItemId === item.id,
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

  function handleImageLoad() {
    dState.extractItemColor(cardState.thumbnailSrc);
    if (!item.isActive && item.filePath) {
      dState.checkFileExists(item.id, item.filePath);
    }
  }
  
  function handleImageError() {
    dState.markThumbnailFailed(item.id);
  }
</script>

<div
  class="grid-card"
  class:file-missing={cardState.fileMissing}
  class:active-download={isActiveDownload}
  class:paused={isPaused}
  class:failed={isFailed}
  style="{cardState.colorStyle} --progress: {displayProgress}%;"
  role="button"
  tabindex="0"
  onmouseenter={() => dState.setHoveredItem(item.id)}
  onmouseleave={() => dState.setHoveredItem(null)}
  onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); if (!item.isActive) ctx.openItem(item as any); } }}
>
  {#if isActiveDownload && !isFailed}
    <div class="progress-bg"></div>
  {/if}

  <div class="card-thumbnail">
    {#if item.thumbnail && !cardState.isFailed}
      <img
        src={cardState.thumbnailSrc}
        alt=""
        class="thumbnail"
        loading="lazy"
        decoding="async"
        fetchpriority="low"
        onload={handleImageLoad}
        onerror={handleImageError}
      />
    {:else}
      <div class="card-thumb-placeholder">
        <Icon name="image" size={32} />
      </div>
    {/if}
    
    {#if item.duration > 0}
      <div class="duration-badge">{formatDuration(item.duration)}</div>
    {/if}

    {#if item.extension}
      <span class="type-badge">{item.extension.toUpperCase()}</span>
    {/if}

    {#if isDownloading}
      <div class="downloading-overlay">
        <div class="spinner"></div>
        <span class="progress-text">{displayProgress}%</span>
      </div>
    {:else if isPaused}
      <div class="paused-overlay">
        <Icon name="pause" size={24} />
      </div>
    {:else if cardState.fileMissing}
      <div class="missing-file-overlay" use:tooltip={$t('downloads.fileMissing')}>
        <Icon name="trash" size={24} />
      </div>
    {:else if cardState.isHovered}
      <div class="card-overlay">
        {#if isActiveDownload}
          <div class="card-actions-bar">
            {#if isPaused}
              <button class="card-action-btn" onclick={(e) => { e.stopPropagation(); queue.resumeItem(item.id); }} use:tooltip={$t('downloads.queue.resumeItem')}>
                <Icon name="play" size={14} />
              </button>
            {:else if isPending}
              <button class="card-action-btn" onclick={(e) => { e.stopPropagation(); queue.pauseItem(item.id); }} use:tooltip={$t('downloads.queue.pauseItem')}>
                <Icon name="pause" size={14} />
              </button>
            {/if}
            <button class="card-action-btn delete" onclick={(e) => { e.stopPropagation(); queue.cancel(item.id); }} use:tooltip={$t('common.cancel')}>
              <Icon name="close" size={14} />
            </button>
          </div>
        {:else}
          <button 
            class="play-overlay" 
            onclick={(e) => { e.stopPropagation(); ctx.playItem(item as any); }}
            use:tooltip={$t('downloads.play')}
          >
            <Icon name="play" size={24} />
          </button>
          
          <div class="card-actions-bar">
            <button class="card-action-btn" onclick={(e) => { e.stopPropagation(); if (item.filePath) ctx.openFileLocation(item.filePath); }} use:tooltip={$t('downloads.openFolder')}>
              <Icon name="folder" size={14} />
            </button>
            <button class="card-action-btn" onclick={(e) => { e.stopPropagation(); ctx.redownloadItem(item.url); }} use:tooltip={$t('downloads.redownload')}>
              <Icon name="download" size={14} />
            </button>
            <button class="card-action-btn" onclick={(e) => { e.stopPropagation(); ctx.openLink(item.url); }} use:tooltip={$t('downloads.openLink')}>
              <Icon name="link" size={14} />
            </button>
            <button class="card-action-btn delete" onclick={(e) => { e.stopPropagation(); ctx.deleteItem(item.id); }} use:tooltip={$t('downloads.delete')}>
              <Icon name="trash" size={14} />
            </button>
          </div>
        {/if}
      </div>
    {/if}
  </div>
  
  <div class="card-info">
    <button 
      type="button"
      class="card-title clickable" 
      onclick={(e) => { e.stopPropagation(); if (!item.isActive) ctx.openItem(item as any); }} 
      use:tooltip={item.isActive ? '' : $t('downloads.openInApp')}
      disabled={item.isActive}
    >
      <HighlightText text={item.title} highlight={dState.searchQuery} />
    </button>
    <div class="card-meta">
      {#if isFailed && item.error}
        <span class="card-error" use:tooltip={item.error}>
          <Icon name="warning" size={10} />
          Error
        </span>
      {:else if isActiveDownload && (isPending || isPaused)}
        <span class="card-status">
          {isPaused ? $t('downloads.queue.paused') : $t('downloads.queue.waiting')}
        </span>
      {:else}
        <button 
          type="button"
          class="card-author clickable" 
          onclick={(e) => { e.stopPropagation(); if (!item.isActive) ctx.openAuthor(item as any); }} 
          use:tooltip={item.isActive ? '' : $t('downloads.openAuthor')}
          disabled={item.isActive}
        >
          <HighlightText text={item.author} highlight={dState.searchQuery} />
        </button>
      {/if}
      <span class="card-size">{dState.getItemSizeDisplay(item)}</span>
    </div>
  </div>
</div>

<style>
  .grid-card {
    display: flex;
    flex-direction: column;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    overflow: hidden;
    transition: all 0.2s ease;
    height: 100%;
    position: relative;
    contain: paint layout;
    cursor: pointer;
  }

  .grid-card.active-download {
    border-color: var(--item-color-alpha, rgba(99, 102, 241, 0.3));
  }

  .grid-card.paused {
    opacity: 0.7;
  }

  .grid-card.failed {
    border-color: rgba(239, 68, 68, 0.4);
  }

  /* Progress background for active downloads */
  .progress-bg {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    height: var(--progress, 0%);
    background: linear-gradient(
      0deg,
      var(--item-color-alpha, rgba(99, 102, 241, 0.15)) 0%,
      transparent 100%
    );
    transition: height 0.3s ease-out;
    pointer-events: none;
    z-index: 0;
  }
  
  .grid-card:hover {
    background: var(--item-color-hover, rgba(255, 255, 255, 0.06));
    border-color: rgba(255, 255, 255, 0.1);
    transform: translateY(-1px);
    z-index: 1;
  }

  .grid-card:hover .thumbnail {
    transform: scale(1.05);
  }
  
  .card-thumbnail {
    position: relative;
    aspect-ratio: 16/9;
    background: rgba(0,0,0,0.3);
    overflow: hidden;
  }
  
  .thumbnail {
    width: 100%;
    height: 100%;
    object-fit: cover;
    transition: transform 0.3s ease;
  }
  
  .card-thumb-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, rgba(255,255,255,0.05) 0%, rgba(255,255,255,0.02) 100%);
    color: rgba(255,255,255,0.25);
  }
  
  .duration-badge {
    position: absolute;
    bottom: 6px;
    right: 6px;
    background: rgba(0,0,0,0.85);
    color: white;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 5px;
    border-radius: 4px;
    z-index: 2;
    letter-spacing: 0.3px;
    box-shadow: 0 1px 2px rgba(0,0,0,0.3);
  }

  .type-badge {
    position: absolute;
    top: 6px;
    left: 6px;
    background: var(--item-color, var(--accent, rgba(99, 102, 241, 0.9)));
    color: white;
    font-size: 9px;
    font-weight: 700;
    padding: 2px 5px;
    border-radius: 4px;
    letter-spacing: 0.3px;
    z-index: 2;
  }
  
  .play-overlay {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 42px;
    height: 42px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--item-color, var(--accent, #6366f1));
    border: none;
    border-radius: 50%;
    color: white;
    cursor: pointer;
    box-shadow: 0 4px 16px rgba(0,0,0,0.4);
    z-index: 10;
    transition: all 0.2s ease;
    animation: zoomIn 0.2s cubic-bezier(0.175, 0.885, 0.32, 1.275);
  }

  .play-overlay:hover {
    transform: translate(-50%, -50%) scale(1.1);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
  }
  
  .card-overlay {
    position: absolute;
    inset: 0;
    background: linear-gradient(180deg, rgba(0,0,0,0.1) 0%, rgba(0,0,0,0.4) 50%, rgba(0,0,0,0.5) 100%);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    animation: fadeIn 0.15s ease;
  }
  
  .card-actions-bar {
    display: flex;
    gap: 3px;
    padding: 8px;
    width: 100%;
    justify-content: center;
  }
  
  .card-action-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(40,40,40,0.95);
    border: 1px solid rgba(255,255,255,0.1);
    border-radius: 6px;
    color: rgba(255,255,255,0.9);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  
  .card-action-btn:hover {
    background: rgba(255,255,255,0.25);
    transform: scale(1.05);
  }
  
  .card-action-btn.delete:hover {
     background: rgba(239, 68, 68, 0.85);
  }
  
  .card-info {
    padding: 10px 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1; /* Match backup style slightly better */
  }
  
  .card-title {
    font-size: 12px;
    font-weight: 500;
    color: rgba(255,255,255,0.95);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    line-height: 1.3;
    background: none;
    border: none;
    padding: 0;
    text-align: left;
    width: 100%;
    cursor: pointer;
  }
  
  .card-title:disabled {
    cursor: default;
  }
  
  .card-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  
  .card-author {
    font-size: 11px;
    color: rgba(255,255,255,0.45);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    padding: 0;
    text-align: left;
    cursor: pointer;
  }
  
  .card-author:disabled {
    cursor: default;
  }

  .card-status {
    font-size: 11px;
    color: var(--item-color, var(--accent, #6366f1));
    font-weight: 500;
    flex: 1;
    min-width: 0;
  }

  .card-error {
    font-size: 11px;
    color: #f87171;
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }
  
  .card-size {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.35);
    flex-shrink: 0;
  }

  .clickable {
     transition: color 0.15s ease;
  }

  .clickable:hover {
      color: var(--item-color, var(--accent, #6366f1));
      cursor: pointer;
  }
  
  .grid-card.file-missing {
      opacity: 0.75;
  }
  
  .grid-card.file-missing .thumbnail {
      filter: grayscale(0.3) brightness(0.9);
  }
  
  .missing-file-overlay {
      position: absolute;
      top: 50%;
      left: 50%;
      transform: translate(-50%, -50%);
      width: 32px;
      height: 32px;
      background: rgba(239,68,68,0.7);
      border-radius: 50%;
      display: flex;
      align-items: center;
      justify-content: center;
      color: white;
      box-shadow: 0 2px 8px rgba(0,0,0,0.3);
      z-index: 5;
  }

  .downloading-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    background: rgba(0, 0, 0, 0.6);
    z-index: 5;
  }

  .downloading-overlay .spinner {
    width: 24px;
    height: 24px;
    border: 3px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .progress-text {
    font-size: 12px;
    font-weight: 600;
    color: white;
  }

  .paused-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
    color: rgba(255, 255, 255, 0.8);
    z-index: 5;
  }
  
  @keyframes zoomIn { from { transform: translate(-50%, -50%) scale(0); } to { transform: translate(-50%, -50%) scale(1); } }
  @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
