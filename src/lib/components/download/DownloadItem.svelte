<script lang="ts">
  import type { UnifiedDownloadItem, DownloadsState } from '$lib/stores/downloadsState.svelte';
  import { getListGridTemplate } from '$lib/stores/downloadsState.svelte';
  import { queue } from '$lib/stores/queue';
  import Icon, { type IconName } from '$lib/components/ui/Icon.svelte';
  import HighlightText from '$lib/components/ui/HighlightText.svelte';
  import ContextMenu from '$lib/components/ui/ContextMenu.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { t } from '$lib/i18n';
  import { formatDuration } from '$lib/utils/format';
  import { useDownloadItem } from './useDownloadItem.svelte';

  export interface Props {
    item: UnifiedDownloadItem;
    dState: DownloadsState;
    showSeparator?: boolean;
  }

  let { item, dState, showSeparator = true }: Props = $props();
  const dl = useDownloadItem(
    () => item,
    () => dState
  );

  let isGrid = $derived(dState.viewMode === 'grid');
  let isSelected = $derived(dState.isItemSelected(item.id));
  let gridTemplateColumns = $derived(
    getListGridTemplate(dState.visibleColumns, dState.listItemSize)
  );
  let thumbHeight = $derived(dState.listItemSize - 16);
  let thumbWidth = $derived(Math.round(thumbHeight * (16 / 9)));
  let subtitle = $derived(dState.getItemSubtitle(item));

  let progressLabel = $derived.by(() => {
    if (!item.isActive) return '';
    if (item.status === 'pending') return $t('downloads.queue.waiting');
    if (item.status === 'paused') return $t('downloads.queue.paused');
    return item.statusMessage || $t(`downloads.status.${item.status}`, { default: '' }) || '';
  });

  function handleRowClick(e: MouseEvent) {
    if (e.ctrlKey || e.shiftKey || dState.isSelectionMode) {
      e.preventDefault();
      e.stopPropagation();
      dState.toggleSelection(item.id, e.ctrlKey || dState.isSelectionMode, e.shiftKey);
    }
  }

  function handleRowDoubleClick(e: MouseEvent) {
    if (e.ctrlKey || e.shiftKey) return;
    if (!item.isActive && item.filePath && !dl.fileMissing) {
      dl.ctx.playItem(item);
    }
  }

  let longPressTimer: ReturnType<typeof setTimeout> | null = null;
  let longPressTriggered = false;

  function handlePointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    longPressTriggered = false;
    longPressTimer = setTimeout(() => {
      longPressTriggered = true;
      dState.toggleSelection(item.id, true, false);
    }, 500);
  }

  function handlePointerUp() {
    if (longPressTimer) {
      clearTimeout(longPressTimer);
      longPressTimer = null;
    }
  }

  function handlePointerCancel() {
    if (longPressTimer) {
      clearTimeout(longPressTimer);
      longPressTimer = null;
    }
    longPressTriggered = false;
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
</script>

{#snippet thumbnailContent(
  placeholderIcon: IconName,
  placeholderSize: number,
  placeholderClass: string
)}
  {#if dl.localThumbnail}
    <img src={dl.localThumbnail} alt="" class="thumbnail" loading="lazy" decoding="async" />
  {:else if item.thumbnail && !dl.isThumbnailFailed}
    {#await dState.ensureCachedThumbnail(item.id, item.thumbnail)}
      <img
        src={dl.thumbnailSrc}
        alt=""
        class="thumbnail"
        loading="lazy"
        decoding="async"
        onload={dl.handleImageLoad}
        onerror={dl.handleImageError}
      />
    {:then localThumb}
      {#if localThumb}
        <img src={localThumb} alt="" class="thumbnail" loading="lazy" decoding="async" />
      {:else}
        <img
          src={dl.thumbnailSrc}
          alt=""
          class="thumbnail"
          loading="lazy"
          decoding="async"
          onload={dl.handleImageLoad}
          onerror={dl.handleImageError}
        />
      {/if}
    {:catch}
      <img
        src={dl.thumbnailSrc}
        alt=""
        class="thumbnail"
        loading="lazy"
        decoding="async"
        onload={dl.handleImageLoad}
        onerror={dl.handleImageError}
      />
    {/await}
  {:else if !item.thumbnail && item.filePath}
    {#await dState.generateLocalThumbnail(item.id, item.filePath)}
      <div class={placeholderClass}>
        <Icon name={placeholderIcon} size={placeholderSize} />
      </div>
    {:then localThumb}
      {#if localThumb}
        <img src={localThumb} alt="" class="thumbnail" loading="lazy" decoding="async" />
      {:else}
        <div class={placeholderClass}>
          <Icon name={placeholderIcon} size={placeholderSize} />
        </div>
      {/if}
    {:catch}
      <div class={placeholderClass}>
        <Icon name={placeholderIcon} size={placeholderSize} />
      </div>
    {/await}
  {:else}
    <div class={placeholderClass}>
      <Icon name={placeholderIcon} size={placeholderSize} />
    </div>
  {/if}
{/snippet}

{#if isGrid}
  <div
    class="grid-card"
    class:file-missing={dl.fileMissing}
    class:selected={isSelected}
    class:active-download={dl.isActiveDownload}
    class:paused={dl.isPaused}
    class:failed={dl.isFailed}
    style="{dl.colorStyle} --progress: {dl.displayProgress}%;"
    role="button"
    tabindex="0"
    onmouseenter={() => (dState.hoveredItemId = item.id)}
    onmouseleave={() => (dState.hoveredItemId = null)}
    onpointerdown={handlePointerDown}
    onpointerup={handlePointerUp}
    onpointercancel={handlePointerCancel}
    onpointermove={handlePointerCancel}
    onclick={(e) => {
      if (longPressTriggered) {
        e.preventDefault();
        e.stopPropagation();
        longPressTriggered = false;
        return;
      }
      handleRowClick(e);
      if (!(e.ctrlKey || e.shiftKey || dState.isSelectionMode)) {
        dl.handleTap(e);
      }
    }}
    ondblclick={handleRowDoubleClick}
    oncontextmenu={dl.handleContextMenu}
    onkeydown={(e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        if (!item.isActive && !dl.fileMissing) dl.ctx.playItem(item);
      } else if (e.key === ' ') {
        e.preventDefault();
        dState.toggleSelection(item.id, true, false);
      } else if (e.key === 'Delete') {
        e.preventDefault();
        if (item.isActive) {
          queue.cancel(item.id);
        } else {
          dl.ctx.deleteItem(item.id);
        }
      }
    }}
  >
    {#if dl.isActiveDownload && !dl.isFailed}
      <div class="grid-progress-bg"></div>
    {/if}

    <div class="card-thumbnail">
      {@render thumbnailContent('image', 32, 'card-thumb-placeholder')}

      {#if !isSelected}
        {#if item.duration > 0}
          <div class="duration-badge">{formatDuration(item.duration)}</div>
        {/if}

        {#if item.extension}
          <span class="type-badge">{item.extension.toUpperCase()}</span>
        {/if}

        {#if dl.isDownloading}
          <div class="downloading-overlay">
            <div class="spinner"></div>
            <span class="progress-text"
              >{progressLabel
                ? `${progressLabel} ${dl.displayProgress}%`
                : `${dl.displayProgress}%`}</span
            >
            <div class="download-actions">
              {#if item.source !== 'convert'}
                <button
                  class="overlay-action-btn"
                  onclick={(e) => {
                    e.stopPropagation();
                    queue.pauseItem(item.id);
                  }}
                  use:tooltip={$t('downloads.queue.pauseItem')}
                >
                  <Icon name="pause" size={16} />
                </button>
              {/if}
              <button
                class="overlay-action-btn danger"
                onclick={(e) => {
                  e.stopPropagation();
                  queue.cancel(item.id);
                }}
                use:tooltip={$t('common.cancel')}
              >
                <Icon name="close" size={16} />
              </button>
            </div>
          </div>
        {:else if dl.isPaused}
          <div class="grid-paused-overlay">
            <Icon name="pause" size={24} />
            <div class="download-actions">
              {#if item.source !== 'convert'}
                <button
                  class="overlay-action-btn"
                  onclick={(e) => {
                    e.stopPropagation();
                    queue.resumeItem(item.id);
                  }}
                  use:tooltip={$t('downloads.queue.resumeItem')}
                >
                  <Icon name="play" size={16} />
                </button>
              {/if}
              <button
                class="overlay-action-btn danger"
                onclick={(e) => {
                  e.stopPropagation();
                  queue.cancel(item.id);
                }}
                use:tooltip={$t('common.cancel')}
              >
                <Icon name="close" size={16} />
              </button>
            </div>
          </div>
        {:else if dl.isPending && dl.isActiveDownload}
          <div class="pending-overlay">
            <span class="pending-text">{$t('downloads.queue.waiting')}</span>
            <div class="download-actions">
              <button
                class="overlay-action-btn danger"
                onclick={(e) => {
                  e.stopPropagation();
                  queue.cancel(item.id);
                }}
                use:tooltip={$t('common.cancel')}
              >
                <Icon name="close" size={16} />
              </button>
            </div>
          </div>
        {:else if dl.isFailed && dl.isActiveDownload}
          <div class="failed-overlay">
            <button
              class="retry-btn grid-retry"
              onclick={(e) => {
                e.stopPropagation();
                queue.retry(item.id);
              }}
              use:tooltip={$t('downloads.retry')}
            >
              <Icon name="refresh" size={18} />
              <span>{$t('downloads.retry')}</span>
            </button>
          </div>
        {:else if dl.fileMissing}
          <div class="missing-file-overlay" use:tooltip={$t('downloads.fileMissing')}>
            <Icon name="trash" size={24} />
          </div>
        {:else if dl.isHovered}
          <div class="card-overlay">
            {#if !dl.isActiveDownload}
              <button
                class="play-overlay"
                onclick={(e) => {
                  e.stopPropagation();
                  dl.ctx.playItem(item);
                }}
                use:tooltip={$t('downloads.play')}
              >
                <Icon name="play" size={24} />
              </button>

              <div class="card-actions-bar">
                <button
                  class="card-action-btn"
                  onclick={(e) => {
                    e.stopPropagation();
                    if (item.filePath) dl.ctx.openFileLocation(item.filePath);
                  }}
                  use:tooltip={$t('downloads.openFolder')}
                >
                  <Icon name="folder" size={14} />
                </button>
                <button
                  class="card-action-btn"
                  onclick={(e) => {
                    e.stopPropagation();
                    dl.ctx.redownloadItem(item.url);
                  }}
                  use:tooltip={$t('downloads.redownload')}
                >
                  <Icon name="download" size={14} />
                </button>
                <button
                  class="card-action-btn"
                  onclick={(e) => {
                    e.stopPropagation();
                    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
                    dl.openContextMenuAt(rect.left, rect.bottom + 4);
                  }}
                  use:tooltip={$t('common.more')}
                >
                  <Icon name="dots" size={14} />
                </button>
                <button
                  class="card-action-btn delete"
                  onclick={(e) => {
                    e.stopPropagation();
                    dl.ctx.deleteItem(item.id);
                  }}
                  use:tooltip={$t('downloads.delete')}
                >
                  <Icon name="trash" size={14} />
                </button>
              </div>
            {/if}
          </div>
        {/if}
      {/if}
      {#if isSelected}
        <div class="card-check-badge">
          <Icon name="check" size={12} />
        </div>
      {/if}
    </div>

    <div class="card-info">
      <div class="card-title-row">
        <button
          type="button"
          class="card-title clickable"
          onclick={(e) => {
            e.stopPropagation();
            if (e.ctrlKey || e.shiftKey || dState.isSelectionMode) {
              dState.toggleSelection(item.id, e.ctrlKey || dState.isSelectionMode, e.shiftKey);
            } else if (!item.isActive) {
              dl.ctx.openItem(item);
            }
          }}
          use:tooltip={item.isActive ? '' : $t('downloads.openInApp')}
          disabled={item.isActive}
        >
          <HighlightText text={item.title} highlight={dState.searchQuery} />
        </button>
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
      <div class="card-meta">
        {#if dl.isFailed && item.error}
          <span class="card-error" use:tooltip={item.error}>
            <Icon name="warning" size={10} />
            Error
          </span>
        {:else if dl.isActiveDownload && (dl.isPending || dl.isPaused)}
          <span class="card-status">
            {dl.isPaused ? $t('downloads.queue.paused') : $t('downloads.queue.waiting')}
          </span>
        {:else if item.authorUrl}
          <button
            type="button"
            class="card-author clickable"
            onclick={(e) => {
              e.stopPropagation();
              if (e.ctrlKey || e.shiftKey || dState.isSelectionMode) {
                dState.toggleSelection(item.id, e.ctrlKey || dState.isSelectionMode, e.shiftKey);
              } else {
                dl.ctx.openAuthor(item);
              }
            }}
            use:tooltip={$t('downloads.viewChannel')}
          >
            <HighlightText text={item.author} highlight={dState.searchQuery} />
          </button>
        {:else}
          <span class="card-author">
            <HighlightText text={item.author} highlight={dState.searchQuery} />
          </span>
        {/if}
        <span class="card-size">{dState.getItemSizeDisplay(item)}</span>
      </div>
    </div>
  </div>
{:else}
  <div
    class="history-item"
    class:with-separator={showSeparator}
    class:file-missing={dl.fileMissing}
    class:selected={isSelected}
    class:active-download={dl.isActiveDownload}
    class:paused={dl.isPaused}
    class:failed={dl.isFailed}
    class:hovered={dl.isHovered}
    style="{dl.colorStyle} --progress: {dl.displayProgress}%; --item-height: {dState.listItemSize}px; --thumb-w: {thumbWidth}px; --thumb-h: {thumbHeight}px; grid-template-columns: {gridTemplateColumns};"
    role="button"
    tabindex="0"
    onmouseenter={() => (dState.hoveredItemId = item.id)}
    onmouseleave={() => (dState.hoveredItemId = null)}
    onpointerdown={handlePointerDown}
    onpointerup={handlePointerUp}
    onpointercancel={handlePointerCancel}
    onpointermove={handlePointerCancel}
    onclick={(e) => {
      if (longPressTriggered) {
        e.preventDefault();
        e.stopPropagation();
        longPressTriggered = false;
        return;
      }
      handleRowClick(e);
      if (!(e.ctrlKey || e.shiftKey || dState.isSelectionMode)) {
        dl.handleTap(e);
      }
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
          dl.ctx.deleteItem(item.id);
        }
      }
    }}
    oncontextmenu={dl.handleContextMenu}
  >
    {#if dl.isActiveDownload && !dl.isFailed}
      <div class="progress-bg"></div>
    {/if}

    <div class="col-thumb">
      {@render thumbnailContent(getTypeIcon(item.type), 20, 'thumbnail-placeholder')}

      {#if dl.isDownloading}
        <div class="spinner-overlay">
          <div class="spinner"></div>
        </div>
      {:else if dl.isPaused}
        <div class="list-paused-overlay">
          <Icon name="pause" size={14} />
        </div>
      {:else if dl.fileMissing}
        <div class="thumb-missing-indicator" use:tooltip={$t('downloads.fileMissing')}>
          <Icon name="trash" size={12} />
        </div>
      {:else if dl.isHovered && !dl.isActiveDownload}
        <button
          class="thumb-play-overlay"
          onclick={(e) => {
            e.stopPropagation();
            dl.ctx.playItem(item);
          }}
          use:tooltip={$t('downloads.play')}
        >
          <Icon name="play" size={16} />
        </button>
      {/if}
      {#if isSelected}
        <div class="thumb-check-badge">
          <Icon name="check" size={10} />
        </div>
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
              if (e.ctrlKey || e.shiftKey || dState.isSelectionMode) {
                dState.toggleSelection(item.id, e.ctrlKey || dState.isSelectionMode, e.shiftKey);
              } else {
                dl.ctx.openItem(item);
              }
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
            if (e.ctrlKey || e.shiftKey || dState.isSelectionMode) {
              dState.toggleSelection(item.id, e.ctrlKey || dState.isSelectionMode, e.shiftKey);
            } else {
              dl.ctx.openAuthor(item);
            }
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

    {#if dl.isFailed && item.isActive}
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

    <div class="floating-action-bar" class:visible={dl.isHovered}>
      {#if dl.isActiveDownload}
        {#if dl.isFailed}
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
          {#if dl.isPaused}
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
          {:else if dl.isPending || dl.isDownloading}
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
            if (item.filePath) dl.ctx.openFileLocation(item.filePath);
          }}
          use:tooltip={$t('downloads.openFolder')}
          disabled={dl.fileMissing}
        >
          <Icon name="folder" size={15} />
        </button>
        {#if dl.conversionFormats.length > 0 && !dl.fileMissing}
          <button
            class="action-btn"
            onclick={(e) => {
              e.stopPropagation();
              const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
              dl.openConvertMenuAt(rect.left, rect.bottom + 4);
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
            dl.ctx.deleteItem(item.id);
          }}
          use:tooltip={$t('downloads.delete')}
        >
          <Icon name="trash" size={15} />
        </button>
        <button
          class="action-btn kebab"
          onclick={(e) => {
            e.stopPropagation();
            dl.openContextMenuAt(e.clientX, e.clientY);
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
{/if}

<ContextMenu
  bind:open={dl.contextMenuOpen}
  x={dl.contextMenuX}
  y={dl.contextMenuY}
  items={dl.contextMenuItems}
  onclose={() => (dl.contextMenuOpen = false)}
  onselect={dl.handleContextMenuSelect}
/>

<ContextMenu
  bind:open={dl.convertMenuOpen}
  x={dl.convertMenuX}
  y={dl.convertMenuY}
  items={dl.convertMenuItems}
  onclose={() => (dl.convertMenuOpen = false)}
  onselect={dl.handleContextMenuSelect}
/>

<style>
  .history-item {
    display: grid;
    gap: 10px;
    align-items: center;
    padding: 4px 12px;
    cursor: default;
    background: transparent;
    border-radius: var(--radius, 10px);
    transition: background-color 0.15s ease;
    height: var(--item-height, 56px);
    contain: layout style;
    position: relative;
    overflow: visible;
  }

  .history-item.with-separator::before {
    content: '';
    position: absolute;
    left: 12px;
    right: 12px;
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
    background: var(--accent-alpha, rgba(99, 102, 241, 0.15));
  }

  .history-item.selected .item-title {
    color: rgba(255, 255, 255, 0.95);
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
    width: var(--thumb-w, 56px);
    height: var(--thumb-h, 32px);
    border-radius: var(--radius-sm, 4px);
    overflow: hidden;
    background: rgba(0, 0, 0, 0.3);
    flex-shrink: 0;
    z-index: 1;
  }

  .thumb-check-badge {
    position: absolute;
    bottom: 2px;
    right: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--accent, #6366f1);
    display: flex;
    align-items: center;
    justify-content: center;
    color: white;
    z-index: 3;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
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

  .list-paused-overlay {
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

  .history-item .converted-tag {
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

  .history-item .source-tag {
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
    width: fit-content;
    max-width: 100%;
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
      gap: 8px;
      padding: 4px 4px;
    }
  }

  .grid-card {
    display: flex;
    flex-direction: column;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: var(--radius, 10px);
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

  .grid-card.selected {
    border-color: var(--accent, rgba(99, 102, 241, 0.7));
    border-width: 2px;
    background: rgba(99, 102, 241, 0.1);
  }

  .grid-progress-bg {
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
    background: rgba(0, 0, 0, 0.3);
    overflow: hidden;
  }

  .card-thumbnail .thumbnail {
    transition: transform 0.3s ease;
  }

  .card-check-badge {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent, #6366f1);
    display: flex;
    align-items: center;
    justify-content: center;
    color: white;
    z-index: 5;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.5);
  }

  .card-thumb-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(
      135deg,
      rgba(255, 255, 255, 0.05) 0%,
      rgba(255, 255, 255, 0.02) 100%
    );
    color: rgba(255, 255, 255, 0.25);
  }

  .duration-badge {
    position: absolute;
    bottom: 6px;
    left: 6px;
    background: rgba(0, 0, 0, 0.85);
    color: white;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 5px;
    border-radius: var(--radius-sm, 4px);
    z-index: 2;
    letter-spacing: 0.3px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
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
    border-radius: var(--radius-sm, 4px);
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
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
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
    background: linear-gradient(
      180deg,
      rgba(0, 0, 0, 0.1) 0%,
      rgba(0, 0, 0, 0.5) 60%,
      rgba(0, 0, 0, 0.7) 100%
    );
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    animation: fadeIn 0.12s ease;
  }

  .card-actions-bar {
    display: flex;
    gap: 4px;
    padding: 10px;
    width: 100%;
    justify-content: center;
    background: linear-gradient(0deg, rgba(0, 0, 0, 0.5) 0%, transparent 100%);
  }

  .card-action-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.85);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .card-action-btn:hover {
    background: rgba(255, 255, 255, 0.15);
    color: white;
  }

  .card-action-btn:active {
    transform: scale(0.95);
  }

  .card-action-btn.delete:hover {
    background: rgba(239, 68, 68, 0.2);
    border-color: rgba(239, 68, 68, 0.3);
    color: #f87171;
  }

  .card-info {
    padding: 10px 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }

  .card-title {
    font-size: var(--text-sm, 12px);
    font-weight: 500;
    color: rgba(255, 255, 255, 0.95);
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

  .card-title-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .grid-card .converted-tag {
    flex-shrink: 0;
    font-size: 8px;
    font-weight: 600;
    text-transform: uppercase;
    padding: 2px 4px;
    border-radius: 3px;
    background: var(--item-color, var(--accent, #6366f1));
    color: white;
    opacity: 0.85;
  }

  .grid-card .source-tag {
    flex-shrink: 0;
    font-size: 8px;
    font-weight: 600;
    text-transform: uppercase;
    padding: 2px 4px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.15);
    color: rgba(255, 255, 255, 0.7);
  }

  .card-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .card-author {
    font-size: var(--text-xs, 11px);
    color: rgba(255, 255, 255, 0.45);
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
    font-size: var(--text-xs, 11px);
    color: var(--item-color, var(--accent, #6366f1));
    font-weight: 500;
    flex: 1;
    min-width: 0;
  }

  .card-error {
    font-size: var(--text-xs, 11px);
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
    background: rgba(239, 68, 68, 0.7);
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: white;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
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

  .grid-paused-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    background: rgba(0, 0, 0, 0.6);
    color: rgba(255, 255, 255, 0.8);
    z-index: 5;
  }

  .pending-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    background: rgba(0, 0, 0, 0.5);
    z-index: 5;
  }

  .pending-text {
    font-size: 11px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.7);
  }

  .download-actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .overlay-action-btn {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.15);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: var(--radius, 8px);
    color: white;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .overlay-action-btn:hover {
    background: rgba(255, 255, 255, 0.25);
    transform: scale(1.05);
  }

  .overlay-action-btn:active {
    transform: scale(0.95);
  }

  .overlay-action-btn.danger:hover {
    background: rgba(239, 68, 68, 0.4);
    border-color: rgba(239, 68, 68, 0.5);
  }

  .failed-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(239, 68, 68, 0.2);
    z-index: 5;
  }

  .grid-retry {
    padding: 8px 14px;
    background: rgba(99, 102, 241, 0.9);
    border: none;
    border-radius: var(--radius, 8px);
    color: white;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  }

  .grid-retry:hover {
    background: var(--accent, #6366f1);
    transform: scale(1.05);
    border-color: transparent;
  }

  .grid-retry:active {
    transform: scale(0.98);
  }

  @keyframes zoomIn {
    from {
      transform: translate(-50%, -50%) scale(0);
    }
    to {
      transform: translate(-50%, -50%) scale(1);
    }
  }
  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
