<script lang="ts">
  import { slide } from 'svelte/transition';
  import { getContext } from 'svelte';
  import { navigation } from '$lib/stores/navigation';
  import { t } from '$lib/i18n';
  import { activeDownloads, queue, isQueuePaused, type QueueItem } from '$lib/stores/queue';
  import Icon from '$lib/components/Icon.svelte';
  import HighlightText from '$lib/components/HighlightText.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { DownloadsState } from '$lib/stores/downloadsState.svelte';
  import { DOWNLOADS_CONTEXT_KEY, type DownloadsContext } from '$lib/stores/downloadsContext';
  
  interface Props {
      state: DownloadsState;
  }
  
  let { state }: Props = $props();
  const ctx = getContext<DownloadsContext>(DOWNLOADS_CONTEXT_KEY);
  
  // Use state.activeDownloadGroups which is filtered
  let grouped = $derived(state.activeDownloadGroups);
  
  // Local state for color (helper from state) is needed for items
  // Queue items are transient, maybe we don't cache their colors heavily or use the same state cache
  
  function handleImageLoad(src: string) {
       state.extractItemColor(src);
  }
  
  function getThumb(url: string) {
      return state.getThumbnailSrc(url);
  }
  
  function getColor(url: string) {
      return state.getItemColorStyle(url);
  }

  import type { IconName } from '$lib/components/Icon.svelte';

  function getTypeIcon(item: QueueItem): IconName {
    if (!item.type) return 'download';
    switch (item.type) {
      case 'video': return 'video';
      case 'audio': return 'music';
      case 'image': return 'image';
      case 'file': return 'file_text';
      default: return 'download';
    }
  }

  function handleOpenQueueItem(item: QueueItem) {
      // Create a temporary history-like item structure or adapt based on QueueItem
      // Context expects HistoryItem usually, but we can pass necessary fields
      const minimalItem = {
          id: item.id,
          url: item.url,
          title: item.title,
          author: item.author,
          thumbnail: item.thumbnail,
          type: item.type,
          playlistId: item.playlistId || '',
          playlistTitle: item.playlistTitle,
          filePath: item.filePath,
          // Add default wrappers for missing properties if strictly required
      };
      
      // If the context expects exact HistoryItem type, we might need a cast or different method
      // For now, we reuse openItem if it mimics valid structure, or we call navigation directly
      if (item.type === 'video' || item.type === 'audio') {
           navigation.openVideo(item.url, { title: item.title, author: item.author, thumbnail: item.thumbnail });
      } else {
           // For files/images fallback to context which handles file opening
           // We need to cast partially
           // However context openItem checks isFileMissing(item.id) which might fail for queue items not in history list yet?
           // Actually active items are not in history list 'missingFiles' set usually.
           if (item.filePath) {
               ctx.openFileLocation(item.filePath);
           }
      }
  }

</script>

{#if $activeDownloads.length > 0}
  <div class="active-downloads-section">
      <div class="section-header">
        <div class="header-left">
            <Icon name="queue" size={18} />
            <span>{$t('downloads.active')}</span>
            <span class="count-badge">{$activeDownloads.length}</span>
        </div>

        <div class="queue-controls">
          <button
            class="queue-btn"
            class:paused={$isQueuePaused}
            onclick={() => queue.togglePause()}
            use:tooltip={$isQueuePaused ? $t('downloads.queue.resume') : $t('downloads.queue.pause')}
          >
            <Icon name={$isQueuePaused ? 'play' : 'pause'} size={14} />
          </button>
          <button
            class="queue-btn clear"
            onclick={() => queue.clearFinished()}
            use:tooltip={$t('downloads.queue.clearFinished')}
          >
            <Icon name="trash" size={14} />
          </button>
        </div>
      </div>

      {#if $isQueuePaused}
        <div class="queue-paused-banner" transition:slide>
          <Icon name="pause" size={16} />
          <span>{$t('downloads.queue.pausedMessage')}</span>
          <button onclick={() => queue.resume()}>{$t('downloads.queue.resumeBtn')}</button>
        </div>
      {/if}

      <div class="downloads-list">
          <!-- Groups -->
          {#each grouped.groups as group (group.playlistId)}
              {@const isExpanded = !state.collapsedPlaylists.has(group.playlistId)}
              <div class="playlist-group" class:collapsed={!isExpanded}>
                  <div class="playlist-header-row">
                      <button class="playlist-header" onclick={() => state.togglePlaylist(group.playlistId, false)}>
                          <div style="transition: transform 0.2s" style:transform={isExpanded ? 'rotate(0deg)' : 'rotate(-90deg)'}>
                              <Icon name="chevron_down" size={16} />
                          </div>
                          <Icon name="playlist" size={18} />
                          <span class="playlist-title">{group.playlistTitle}</span>
                          <span class="playlist-progress">{group.items.length} items</span>
                      </button>
                      <div class="playlist-controls">
                          <button class="ctrl-btn" onclick={() => queue.pausePlaylist(group.playlistId)} use:tooltip={$t('downloads.queue.pauseAll')}>
                             <Icon name="pause" size={12} />
                          </button>
                          <button class="ctrl-btn" onclick={() => queue.resumePlaylist(group.playlistId)} use:tooltip={$t('downloads.queue.resumeAll')}>
                             <Icon name="play" size={12} />
                          </button>
                          <button class="ctrl-btn" onclick={() => queue.cancelPlaylist(group.playlistId)} use:tooltip={$t('downloads.queue.cancelAll')}>
                             <Icon name="close" size={12} />
                          </button>
                      </div>
                  </div>
                  
                  {#if isExpanded}
                      <div class="playlist-items" transition:slide>
                          {#each group.items as item (item.id)}
                              {@const displayProgress = Math.max(0, Math.round(item.progress))}
                              {@const isPending = item.status === 'pending' || item.status === 'paused'}
                              <div 
                                class="active-item playlist-child" 
                                class:paused={item.status === 'paused'}
                                style="--progress: {displayProgress}%; {getColor(item.thumbnail)}"
                              >
                                  <div class="progress-bg"></div>
                                  <div class="item-content">
                                      <div class="item-thumb">
                                        {#if item.thumbnail && !state.isThumbnailFailed(item.id)}
                                            {@const thumbSrc = getThumb(item.thumbnail)}
                                           <img 
                                              src={thumbSrc} 
                                              alt="" 
                                              loading="lazy"
                                              onload={() => handleImageLoad(thumbSrc || '')}
                                              onerror={() => state.markThumbnailFailed(item.id)}
                                           />
                                        {:else}
                                           <div class="thumb-placeholder">
                                              <Icon name={getTypeIcon(item)} size={20} />
                                           </div>
                                        {/if}
                                        
                                        {#if item.status === 'downloading' || item.status === 'processing' || item.status === 'fetching-info'}
                                          <div class="spinner-overlay">
                                            <div class="spinner"></div>
                                          </div>
                                        {/if}
                                        {#if item.status === 'paused'}
                                          <div class="paused-overlay">
                                            <Icon name="pause" size={14} />
                                          </div>
                                        {/if}
                                      </div>
                                      
                                      <div class="item-info">
                                         <button type="button" class="item-title clickable" onclick={() => handleOpenQueueItem(item)}><HighlightText text={item.title} highlight={state.searchQuery} /></button>
                                         <div class="item-status">
                                            {#if item.status === 'paused'}
                                                <span class="status-paused">{$t('downloads.queue.paused')}</span>
                                            {:else if item.status === 'pending'}
                                                <span class="status-pending">{$t('downloads.queue.waiting')}</span>
                                            {:else}
                                                <span class="status-message">{item.statusMessage || item.status}</span>
                                                {#if item.speed && item.status === 'downloading' && !['na', 'unknown', 'n/a', '~'].includes(item.speed.toLowerCase())}
                                                   <span class="status-speed">• {item.speed}</span>
                                                {/if}
                                                {#if item.eta && item.status === 'downloading' && !['na', 'unknown', 'n/a', '~'].includes(item.eta.toLowerCase())}
                                                    <span>• {item.eta}</span>
                                                {/if}
                                            {/if}
                                         </div>
                                      </div>
                                      
                                      <span class="active-progress-text">{displayProgress}%</span>
                                      
                                      <div class="item-actions">
                                          {#if isPending}
                                              <button onclick={() => queue.moveToTop(item.id)} use:tooltip={$t('downloads.queue.moveToTop')}>
                                                  <Icon name="chevron_up" size={14} />
                                              </button>
                                              
                                              {#if item.status === 'paused'}
                                                  <button onclick={() => queue.resumeItem(item.id)} use:tooltip={$t('downloads.queue.resumeItem')}>
                                                      <Icon name="play" size={14} />
                                                  </button>
                                              {:else}
                                                  <button onclick={() => queue.pauseItem(item.id)} use:tooltip={$t('downloads.queue.pauseItem')}>
                                                      <Icon name="pause" size={14} />
                                                  </button>
                                              {/if}
                                          {/if}
                                          
                                          <button class="cancel-btn" onclick={() => queue.cancel(item.id)} use:tooltip={$t('common.cancel')}>
                                              <Icon name="close" size={14} />
                                          </button>
                                      </div>
                                  </div>
                              </div>
                          {/each}
                      </div>
                  {/if}
              </div>
          {/each}

          <!-- Singles -->
          {#each grouped.singles as item (item.id)}
             {@const displayProgress = Math.max(0, Math.round(item.progress))}
             {@const isPending = item.status === 'pending' || item.status === 'paused'}
             
             <div 
                class="active-item single"
                class:paused={item.status === 'paused'}
                style="--progress: {displayProgress}%; {getColor(item.thumbnail)}"
             >
                  <div class="progress-bg"></div>
                  <div class="item-content">
                      {#if isPending && item.priority > 0}
                        <span class="priority-badge">#{item.priority}</span>
                      {/if}
                  
                      <div class="item-thumb">
                        {#if item.thumbnail && !state.isThumbnailFailed(item.id)}
                            {@const thumbSrc = getThumb(item.thumbnail)}
                           <img 
                              src={thumbSrc} 
                              alt=""
                              loading="lazy" 
                              onload={() => handleImageLoad(thumbSrc || '')}
                              onerror={() => state.markThumbnailFailed(item.id)}
                           />
                        {:else}
                           <div class="thumb-placeholder">
                               <Icon name={getTypeIcon(item)} size={20} />
                           </div>
                        {/if}
                        
                        {#if item.status === 'downloading' || item.status === 'processing' || item.status === 'fetching-info'}
                          <div class="spinner-overlay">
                            <div class="spinner"></div>
                          </div>
                        {/if}
                        {#if item.status === 'paused'}
                          <div class="paused-overlay">
                            <Icon name="pause" size={14} />
                          </div>
                        {/if}
                      </div>
                      
                      <div class="item-info">
                         <button type="button" class="item-title clickable" onclick={() => handleOpenQueueItem(item)}><HighlightText text={item.title} highlight={state.searchQuery} /></button>
                         <div class="item-status">
                            {#if item.status === 'paused'}
                                <span class="status-paused">{$t('downloads.queue.paused')}</span>
                            {:else if item.status === 'pending'}
                                <span class="status-pending">{$t('downloads.queue.waiting')}</span>
                            {:else}
                                <span class="status-message">{item.statusMessage || item.status}</span>
                                {#if item.speed && item.status === 'downloading' && !['na', 'unknown', 'n/a', '~'].includes(item.speed.toLowerCase())}
                                   <span class="status-speed">• {item.speed}</span>
                                {/if}
                                {#if item.eta && item.status === 'downloading' && !['na', 'unknown', 'n/a', '~'].includes(item.eta.toLowerCase())}
                                    <span>• {item.eta}</span>
                                {/if}
                            {/if}
                         </div>
                      </div>
                      
                      <span class="active-progress-text">{displayProgress}%</span>
                      
                      <div class="item-actions">
                          {#if isPending}
                              <button onclick={() => queue.moveToTop(item.id)} use:tooltip={$t('downloads.queue.moveToTop')}>
                                  <Icon name="chevron_up" size={14} />
                              </button>
                          
                              {#if item.status === 'paused'}
                                  <button onclick={() => queue.resumeItem(item.id)} use:tooltip={$t('downloads.queue.resumeItem')}>
                                      <Icon name="play" size={14} />
                                  </button>
                              {:else}
                                  <button onclick={() => queue.pauseItem(item.id)} use:tooltip={$t('downloads.queue.pauseItem')}>
                                      <Icon name="pause" size={14} />
                                  </button>
                              {/if}
                          {/if}
                          <button class="cancel-btn" onclick={() => queue.cancel(item.id)} use:tooltip={$t('common.cancel')}>
                              <Icon name="close" size={14} />
                          </button>
                      </div>
                  </div>
             </div>
          {/each}
      </div>
  </div>
{/if}

<style>
  .active-downloads-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 20px;
    border: none;
    background: transparent;
  }
  
  .section-header {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 4px;
      font-size: 13px;
      font-weight: 600;
      color: rgba(255, 255, 255, 0.7);
      background: transparent;
      border: none;
      padding: 0;
  }
  
  .header-left {
      display: flex;
      align-items: center;
      gap: 8px;
  }
  
  .count-badge {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.4));
    color: white;
    font-size: 11px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 10px;
  }
  
  .queue-controls {
    display: flex;
    gap: 4px;
    margin-left: auto;
  }
  
  .queue-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 6px;
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    transition: all 0.15s;
    padding: 0;
  }
  
  .queue-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    color: white;
  }
  
  .queue-btn.paused {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.3));
    color: var(--accent, rgba(99, 102, 241, 1));
  }
  
  .queue-btn.clear:hover {
      background: rgba(239, 68, 68, 0.2);
      color: rgb(239, 68, 68);
  }
  
  .queue-paused-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: rgba(251, 191, 36, 0.15);
    border: 1px solid rgba(251, 191, 36, 0.3);
    border-radius: 8px;
    margin-bottom: 12px;
    font-size: 13px;
    color: rgba(251, 191, 36, 0.9);
  }
  
  .queue-paused-banner button {
      margin-left: auto;
      padding: 4px 12px;
      background: rgba(251, 191, 36, 0.2);
      border: 1px solid rgba(251, 191, 36, 0.4);
      border-radius: 6px;
      color: rgba(251, 191, 36, 1);
      font-size: 12px;
      font-weight: 500;
      cursor: pointer;
      transition: all 0.15s;
  }
  
  .queue-paused-banner button:hover {
      background: rgba(251, 191, 36, 0.3);
  }

  .downloads-list {
      display: flex;
      flex-direction: column;
      gap: 8px;
  }
  
  .active-item {
    position: relative;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    overflow: hidden;
  }
  
  .active-item.paused {
      opacity: 0.7;
  }

  /* Progress gradient background */
  .progress-bg {
    position: absolute;
    inset: 0;
    width: 100%;
    background: linear-gradient(
      90deg,
      var(--item-color-alpha, var(--accent-alpha, rgba(99, 102, 241, 0.25))) 0%,
      var(--item-color-alpha-light, var(--accent-alpha-light, rgba(99, 102, 241, 0.15)))
        calc(var(--progress) - 5%),
      var(--item-color-alpha-lighter, var(--accent-alpha-lighter, rgba(99, 102, 241, 0.08)))
        var(--progress),
      transparent calc(var(--progress) + 2%)
    );
    pointer-events: none;
    transition: width 0.3s linear;
  }
  
  .item-content {
      position: relative;
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 12px 16px;
      z-index: 1;
  }
  
  .priority-badge {
    position: absolute;
    top: 4px;
    left: 4px;
    background: var(--accent, rgba(99, 102, 241, 0.8));
    color: white;
    font-size: 9px;
    font-weight: 700;
    padding: 2px 5px;
    border-radius: 4px;
    z-index: 2;
  }

  .item-thumb {
    position: relative;
    width: 48px;
    height: 36px;
    border-radius: 6px;
    overflow: hidden;
    flex-shrink: 0;
    background: rgba(255, 255, 255, 0.08);
  }
  
  .item-thumb img {
      width: 100%; height: 100%; object-fit: cover;
  }
  
  .thumb-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.4);
  }

  /* Spinner overlay on thumbnail */
  .spinner-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
  }
  
  .spinner {
    width: 18px;
    height: 18px;
    border: 2px solid rgba(255, 255, 255, 0.2);
    border-top-color: var(--accent, rgba(99, 102, 241, 1));
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
    color: rgba(251, 191, 36, 0.9);
  }
  
  .item-info {
      flex: 1;
      display: flex;
      flex-direction: column;
      gap: 2px;
      min-width: 0;
  }
  
  .item-title {
    font-size: 14px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.9);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    background: none;
    border: none;
    padding: 0;
    text-align: left;
    width: 100%;
  }
  
  .item-title.clickable {
      cursor: pointer;
      transition: color 0.15s;
  }
  
  .item-title.clickable:hover {
      color: var(--accent, #6366f1);
  }

  .item-status {
      display: flex;
      gap: 8px;
      font-size: 12px;
      color: rgba(255, 255, 255, 0.5);
  }
  
  /* Status Colors */
  .status-paused { color: rgba(251, 191, 36, 0.9); }
  .status-pending { color: rgba(255, 255, 255, 0.4); }
  .status-message { color: rgba(255, 255, 255, 0.7); }
  .status-speed { color: rgba(255, 255, 255, 0.5); }
  
  /* Progress numbers */
  .active-progress-text {
    font-size: 14px;
    font-weight: 600;
    color: var(--item-color, var(--accent, rgba(99, 102, 241, 1)));
    min-width: 45px;
    text-align: right;
  }

  .item-actions {
      display: flex;
      gap: 4px;
      margin-left: 8px;
  }
  
  .item-actions button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 4px;
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    transition: all 0.15s;
    padding: 0;
  }
  
  .item-actions button:hover {
      background: rgba(255, 255, 255, 0.15);
      color: white;
  }
  
  .item-actions button.cancel-btn {
      width: 28px;
      height: 28px;
      background: rgba(239, 68, 68, 0.15);
      border-radius: 6px;
      color: rgba(239, 68, 68, 0.8);
      margin-left: 4px;
  }
  
  .item-actions button.cancel-btn:hover {
      background: rgba(239, 68, 68, 0.3);
      color: rgba(239, 68, 68, 1);
  }

  /* Playlist grouping */
  .playlist-group {
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    overflow: hidden;
  }
  
  .playlist-group.collapsed {
    background: rgba(255, 255, 255, 0.03);
  }

  .playlist-header-row {
    display: flex;
    align-items: center;
    padding: 10px 12px;
    background: rgba(255, 255, 255, 0.04);
    width: 100%;
    color: white;
    box-sizing: border-box;
  }
  
  .playlist-header-row:hover {
      background: rgba(255, 255, 255, 0.08);
  }
  
  .playlist-header {
      display: flex;
      align-items: center;
      gap: 8px;
      flex: 1;
      background: none;
      border: none;
      color: white;
      cursor: pointer;
      padding: 0;
      font-size: 13px;
      font-weight: 600;
  }
  
  .playlist-title {
      font-weight: 500;
      color: rgba(255,255,255,0.9);
  }

  .playlist-progress {
      font-size: 12px;
      color: rgba(255,255,255,0.5);
      margin-left: 4px;
      font-weight: 400;
  }
  
  .playlist-controls {
      display: flex;
      gap: 4px;
  }
  
  .ctrl-btn {
      width: 24px; height: 24px;
      background: rgba(255,255,255,0.1);
      border: none;
      border-radius: 4px;
      color: rgba(255,255,255,0.8);
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 0;
  }
  
  .ctrl-btn:hover {
      background: rgba(255,255,255,0.2);
      color: white;
  }
  
  .playlist-child {
      border-top: 1px solid rgba(255,255,255,0.05);
      border-radius: 0;
      border-left: none;
      border-right: none;
      background: transparent;
  }
  
  .playlist-child:first-child {
      border-top: none;
  }
  
  .playlist-child .item-content {
      padding-left: 32px;
  }

</style>
