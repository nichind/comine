<script lang="ts">
  import { onMount, setContext, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { t } from '$lib/i18n';
  import {
    history,
    playlistGroupedHistory,
    historyStats,
  } from '$lib/stores/history';
  import { formatDuration, formatSize } from '$lib/utils/format';
  import { groupedDownloads } from '$lib/stores/queue';
  import { settings, updateSetting } from '$lib/stores/settings';
  import { navigation } from '$lib/stores/navigation';
  import { invoke } from '@tauri-apps/api/core';
  import { revealItemInDir, openPath, openUrl } from '@tauri-apps/plugin-opener';
  import { isAndroid } from '$lib/utils/android';
  import { DownloadsState, VIRTUALIZATION_HEIGHTS, type VirtualListItem } from '$lib/stores/downloadsState.svelte';
  import { DOWNLOADS_CONTEXT_KEY, type DownloadsContext } from '$lib/stores/downloadsContext';
  import { fly } from 'svelte/transition';

  import VirtualList from '$lib/components/VirtualList.svelte';
  import TableHeader from './components/TableHeader.svelte';
  import HistoryItemRow from './components/HistoryItemRow.svelte';
  import HistoryGridItem from './components/HistoryGridItem.svelte';

  import Icon from '$lib/components/Icon.svelte';
  import Chip from '$lib/components/Chip.svelte';
  import Select from '$lib/components/Select.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  const dState = new DownloadsState();

  $effect(() => {
    dState.historyGroups = $playlistGroupedHistory;
    dState.activeDownloadData = $groupedDownloads;
  });

  let containerEl: HTMLDivElement;
  let showStatsPanel = $state(false);
  let searchInputRef: HTMLInputElement | null = $state(null);
  let searchExpanded = $state(false);

  setContext<DownloadsContext>(DOWNLOADS_CONTEXT_KEY, {
    openItem: async (item) => {
      if (dState.isFileMissing(item.id)) return;
      if (item.type === 'video' || item.type === 'audio') {
        navigation.openVideo(item.url, {
          title: item.title,
          thumbnail: item.thumbnail,
          author: item.author,
        });
        await goto('/');
      } else {
        try {
            await openPath(item.filePath);
        } catch (e) {
            console.error('Failed to open file:', e);
        }
      }
    },
    openAuthor: async (item) => {
      if (item.type === 'video' || item.type === 'audio') {
        navigation.openVideo(item.url, {
          title: item.title,
          thumbnail: item.thumbnail,
          author: item.author,
        });
        await goto('/');
      }
    },
    playItem: async (item) => {
      if (dState.isFileMissing(item.id)) return;
      try {
        await openPath(item.filePath);
      } catch (e) {
        console.error('Failed to play file:', e);
      }
    },
    deleteItem: async (id) => {
      history.remove(id);
    },
    redownloadItem: async (url) => {
      navigation.openVideo(url);
      await goto('/');
    },
    openLink: async (url) => {
      try {
          await openUrl(url);
      } catch (e) {
          console.error('Failed to open link:', e);
      }
    },
    openFileLocation: async (path) => {
      if (isAndroid()) return;
      try {
        await revealItemInDir(path);
      } catch (err) {
        console.error('Failed to reveal file:', err);
      }
    },
  });

  onMount(() => {
    history.init();

    (async () => {
      const items = await history.getItems();
      const itemsToFix = items.filter(
        (item) =>
          item.duration === 0 && item.filePath && (item.type === 'video' || item.type === 'audio')
      );

      if (itemsToFix.length === 0) return;

      for (const item of itemsToFix) {
        try {
          const duration = await invoke<number>('get_media_duration', { filePath: item.filePath });
          if (duration > 0) {
            history.updateDuration(item.id, Math.floor(duration));
          }
        } catch (err) {}
      }
    })();

    let resizeDebounceTimer: ReturnType<typeof setTimeout> | null = null;
    const resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const newWidth = entry.contentRect.width;
        if (Math.abs(newWidth - dState.containerWidth) > 2) {
          if (resizeDebounceTimer) clearTimeout(resizeDebounceTimer);
          resizeDebounceTimer = setTimeout(() => {
            dState.containerWidth = newWidth;
          }, 100);
        }
      }
    });

    if (containerEl) {
      resizeObserver.observe(containerEl);
      dState.containerWidth = containerEl.offsetWidth;
    }

    return () => {
      if (resizeDebounceTimer) clearTimeout(resizeDebounceTimer);
      resizeObserver.disconnect();
      dState.destroy();
    };
  });

  $effect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return;
      if (isTypingTarget(document.activeElement)) return;

      if ((e.ctrlKey || e.metaKey) && (e.key === 'f' || e.key === 'F')) {
        e.preventDefault();
        searchExpanded = true;
        tick().then(() => searchInputRef?.focus());
        return;
      }

      if (e.ctrlKey || e.metaKey || e.altKey) return;

      if (e.key === ' ' || e.code === 'Space' || e.key === 'Spacebar') return;

      if (e.key === 'Escape') {
        if (dState.searchQuery.trim() || searchExpanded) {
          dState.searchQuery = '';
          searchExpanded = false;
          e.preventDefault();
        }
        return;
      }

      if (e.key.length === 1 && e.key !== '\n' && e.key !== '\r' && e.key !== '\t') {
        if (!dState.searchQuery.trim() && e.key.trim() === '') return;
        e.preventDefault();
        void openSearch(e.key);
      }
    };

    window.addEventListener('keydown', onKeyDown, true);
    return () => {
      window.removeEventListener('keydown', onKeyDown, true);
    };
  });

  function isTypingTarget(el: Element | null): boolean {
    if (!el) return false;
    const node = el as HTMLElement;
    const tag = node.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
    if (node.isContentEditable) return true;
    return false;
  }

  async function openSearch(initialText?: string) {
    searchExpanded = true;
    if (typeof initialText === 'string' && initialText.length) {
      dState.searchQuery = (dState.searchQuery ?? '') + initialText;
    }
    await tick();
    searchInputRef?.focus();
    try {
      const len = dState.searchQuery.length;
      searchInputRef?.setSelectionRange(len, len);
    } catch {
      // setSelectionRange may not be supported on all input types
    }
  }

  function collapseSearchIfEmpty() {
    if (!dState.searchQuery.trim()) {
      searchExpanded = false;
    }
  }

  let typeCounts = $derived.by(() => {
    const items = $history.items;
    const counts = { video: 0, audio: 0, image: 0, file: 0 };
    for (const item of items) {
      if (item.type in counts) {
        counts[item.type as keyof typeof counts]++;
      }
    }
    return counts;
  });

  let topFormats = $derived.by(() => {
    const formatCounts = $historyStats.formatCounts;
    return Object.entries(formatCounts)
      .sort((a, b) => b[1] - a[1])
      .slice(0, 5);
  });
</script>

<div class="page" bind:this={containerEl} style="--cols: {dState.itemsPerRow}">
  <!-- Toolbar -->
  <div class="toolbar" class:search-open={searchExpanded || dState.searchQuery.trim()}>
    <!-- Row 1: Search (expands from icon) -->
    <div class="search-container" class:expanded={searchExpanded || dState.searchQuery.trim()}>
      {#if searchExpanded || dState.searchQuery.trim()}
        <Icon name="search" size={18} />
        <input
          bind:this={searchInputRef}
          type="text"
          placeholder={$t('downloads.searchPlaceholder')}
          bind:value={dState.searchQuery}
          onfocus={() => (searchExpanded = true)}
          onblur={collapseSearchIfEmpty}
        />
        <button
          type="button"
          class="search-close"
          onclick={(e) => {
            e.stopPropagation();
            if (dState.searchQuery.trim()) {
              dState.searchQuery = '';
              searchInputRef?.focus();
            } else {
              searchExpanded = false;
            }
          }}
          aria-label={dState.searchQuery.trim() ? $t('common.clear') : $t('common.close')}
        >
          <Icon name="cross" size={16} />
        </button>
      {:else}
        <button
          class="search-icon-btn"
          onclick={() => openSearch()}
          use:tooltip={$t('downloads.search')}
        >
          <Icon name="search" size={18} />
        </button>
      {/if}
    </div>

    <!-- Row 2: Controls (slides down when search expands) -->
    <div class="controls-row">
      <div class="filters">
        <Chip
          selected={dState.activeFilter === 'all'}
          icon="date"
          onclick={() => dState.setFilter('all')}
        >
          {$t('downloads.filters.all')}
        </Chip>
        <Chip
          selected={dState.activeFilter === 'video'}
          icon="video"
          onclick={() => dState.setFilter('video')}
        >
          {$t('downloads.filters.video')}
        </Chip>
        <Chip
          selected={dState.activeFilter === 'audio'}
          icon="music"
          onclick={() => dState.setFilter('audio')}
        >
          {$t('downloads.filters.audio')}
        </Chip>
        <Chip
          selected={dState.activeFilter === 'image'}
          icon="image"
          onclick={() => dState.setFilter('image')}
        >
          {$t('downloads.filters.image')}
        </Chip>
        <Chip
          selected={dState.activeFilter === 'file'}
          icon="file_text"
          onclick={() => dState.setFilter('file')}
        >
          {$t('downloads.filters.file')}
        </Chip>
      </div>

      <div class="controls-right">
        <div class="sort-control">
          <span class="sort-label">{$t('downloads.sort.label')}:</span>
          <Select
            bind:value={dState.sortType}
            options={[
              { value: 'date', label: $t('downloads.sort.date') },
              { value: 'name', label: $t('downloads.sort.name') },
              { value: 'size', label: $t('downloads.sort.size') },
            ]}
            onchange={(v) => dState.setSort(v as any)}
          />
        </div>

        <div class="view-toggle">
          <button
            class="view-btn"
            class:active={dState.viewMode === 'list'}
            onclick={() => dState.setViewMode('list')}
            use:tooltip={$t('downloads.views.list')}
          >
            <Icon name="checklist" size={18} />
          </button>
          <button
            class="view-btn"
            class:active={dState.viewMode === 'grid'}
            onclick={() => dState.setViewMode('grid')}
            use:tooltip={$t('downloads.views.grid')}
          >
            <Icon name="gallery" size={18} />
          </button>
        </div>

        {#if $settings.showHistoryStats && $historyStats.totalDownloads > 0}
          <button
            class="toolbar-btn stats-btn"
            class:active={showStatsPanel}
            onclick={() => (showStatsPanel = !showStatsPanel)}
            use:tooltip={$t('downloads.stats.toggle')}
          >
            <Icon name="stats" size={18} />
          </button>
        {/if}
      </div>
    </div>
    
    {#if dState.isSelectionMode}
      <div class="selection-toolbar" transition:fly={{ y: -10, duration: 200 }}>
        <div class="selection-count">
             <span class="count-badge">{dState.selectedItemIds.size}</span>
             <span class="count-label">{$t('downloads.selected')}</span>
        </div>
        <div class="selection-actions">
           <button class="toolbar-btn" onclick={() => dState.clearSelection()} use:tooltip={$t('downloads.clearSelection')}>
               <Icon name="cross" size={18} />
           </button>
        </div>
      </div>
    {/if}
  </div>

  {#if $settings.showHistoryStats && showStatsPanel && $historyStats.totalDownloads > 0}
    <div class="stats-panel">
      <div class="stats-grid">
        <div class="stat-card">
          <Icon name="download" size={20} />
          <div class="stat-content">
            <span class="stat-value">{$historyStats.totalDownloads}</span>
            <span class="stat-label">{$t('downloads.stats.totalDownloads')}</span>
          </div>
        </div>
        <div class="stat-card">
          <Icon name="file_text" size={20} />
          <div class="stat-content">
            <span class="stat-value">{formatSize($historyStats.totalSize)}</span>
            <span class="stat-label">{$t('downloads.stats.totalSize')}</span>
          </div>
        </div>
        <div class="stat-card">
          <Icon name="clock" size={20} />
          <div class="stat-content">
            <span class="stat-value">{formatDuration($historyStats.totalDuration)}</span>
            <span class="stat-label">{$t('downloads.stats.totalDuration')}</span>
          </div>
        </div>
      </div>

      <div class="stats-breakdown">
        <div class="breakdown-section">
          <span class="breakdown-title">{$t('downloads.stats.byType')}</span>
          <div class="breakdown-items">
            {#if typeCounts.video > 0}
              <span class="breakdown-item"
                ><Icon name="video" size={14} />
                {typeCounts.video}
                {$t('downloads.filters.video')}</span
              >
            {/if}
            {#if typeCounts.audio > 0}
              <span class="breakdown-item"
                ><Icon name="music" size={14} />
                {typeCounts.audio}
                {$t('downloads.filters.audio')}</span
              >
            {/if}
            {#if typeCounts.image > 0}
              <span class="breakdown-item"
                ><Icon name="image" size={14} />
                {typeCounts.image}
                {$t('downloads.filters.image')}</span
              >
            {/if}
            {#if typeCounts.file > 0}
              <span class="breakdown-item"
                ><Icon name="file_text" size={14} />
                {typeCounts.file}
                {$t('downloads.filters.file')}</span
              >
            {/if}
          </div>
        </div>

        {#if topFormats.length > 0}
          <div class="breakdown-section">
            <span class="breakdown-title">{$t('downloads.stats.topFormats')}</span>
            <div class="breakdown-items format-items">
              {#each topFormats as [format, count]}
                <span class="format-badge"
                  >{format.toUpperCase()} <span class="format-count">{count}</span></span
                >
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <div class="list-container">
    <VirtualList
      items={dState.displayItems}
      estimatedItemHeight={dState.viewMode === 'list' ? VIRTUALIZATION_HEIGHTS.listItem : VIRTUALIZATION_HEIGHTS.gridRow}
      useFadeMask={true}
      useCustomScrollbar={true}
      getItemSize={(index, item) => {
        if (dState.viewMode === 'list') {
           return item.kind === 'date' ? VIRTUALIZATION_HEIGHTS.dateHeader : VIRTUALIZATION_HEIGHTS.listItem;
        }
        return undefined; // Grid mode is responsive/variable
      }}
    >
      {#snippet header()}
        {#if dState.viewMode === 'list'}
          <TableHeader 
            sortType={dState.sortType} 
            sortDirection={dState.sortDirection} 
            onSortChange={(type, direction) => dState.setSortWithDirection(type, direction)} 
          />
        {/if}
      {/snippet}

      {#snippet children(item: VirtualListItem, index: number)}
        {#if item.kind === 'date'}
          <div class="date-header">
            <span class="date-label">{item.label}</span>
          </div>
        {:else if item.kind === 'single'}
          <HistoryItemRow item={item.item} {dState} />
        {:else if item.kind === 'grid-row'}
          <div class="grid-row">
            {#each item.items as subItem (subItem.id)}
              <div class="grid-cell">
                <HistoryGridItem item={subItem} {dState} />
              </div>
            {/each}
          </div>
        {/if}
      {/snippet}

      {#snippet footer()}
        {#if dState.displayItems.length === 0}
          <div class="empty-state">
            <Icon name="download" size={48} />
            <p>{$t('downloads.empty')}</p>
          </div>
        {:else if dState.displayItems.length > 0}
           <p class="end-message">{$t('downloads.endMessage')}</p>
        {/if}
      {/snippet}
    </VirtualList>
  </div>
</div>

<style>
  .page {
    padding: 0 var(--page-padding-inline-compact) 0 var(--page-padding-inline);
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .list-container {
      flex: 1;
      min-height: 0;
      margin-right: 4px;
      margin-bottom: 4px;
  }

  .toolbar {
    position: relative;
    display: block;
    height: 36px;
    margin-bottom: 16px;
    transition: height 0.35s cubic-bezier(0.4, 0, 0.2, 1);
    will-change: height;
  }

  .toolbar.search-open {
    height: 84px;
  }

  .search-container {
    position: absolute;
    top: 0;
    left: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 36px;
    width: 36px;
    height: 36px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid transparent;
    border-radius: 8px;
    overflow: hidden;
    z-index: 20;
    transition: 
      width 0.35s cubic-bezier(0.4, 0, 0.2, 1),
      background 0.2s,
      border-color 0.2s,
      padding 0.35s cubic-bezier(0.4, 0, 0.2, 1);
    will-change: width, padding;
  }

  .search-container.expanded {
    width: 100%;
    justify-content: flex-start;
    background: rgba(255, 255, 255, 0.06);
    border-color: rgba(255, 255, 255, 0.1);
    padding: 0 8px 0 14px;
    gap: 10px;
  }

  .search-container.expanded:focus-within {
    border-color: var(--accent, rgba(99, 102, 241, 0.5));
    background: rgba(255, 255, 255, 0.08);
  }

  .search-container :global(.icon) {
    color: rgba(255, 255, 255, 0.4);
    flex-shrink: 0;
  }

  .search-icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    transition: color 0.15s;
  }

  .search-icon-btn:hover {
    color: white;
  }

  .search-container input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    font-size: 14px;
    color: rgba(255, 255, 255, 0.9);
    min-width: 0;
  }

  .search-container input::placeholder {
    color: rgba(255, 255, 255, 0.35);
  }

  .search-clear,
  .search-close {
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    flex-shrink: 0;
    transition: all 0.15s;
  }

  .search-clear {
    width: 22px;
    height: 22px;
    border-radius: 50%;
  }

  .search-close {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    background: transparent;
  }

  .search-clear:hover,
  .search-close:hover {
    background: rgba(255, 255, 255, 0.15);
    color: white;
  }

  .controls-row {
    position: absolute;
    top: 0;
    left: 48px;
    right: 0;
    height: 36px;
    display: flex;
    align-items: center;
    transition: 
      top 0.35s cubic-bezier(0.4, 0, 0.2, 1),
      left 0.35s cubic-bezier(0.4, 0, 0.2, 1);
    background: transparent;
  }

  .toolbar.search-open .controls-row {
     top: 48px;
     left: 0;
  }

  .filters {
    display: flex;
    gap: 8px;
    height: 36px;
    align-items: center;
    overflow-x: auto;
    overflow-y: hidden;
    white-space: nowrap;
    scrollbar-width: none;
    -ms-overflow-style: none;
    flex: 1;
    min-width: 0;
    mask-image: linear-gradient(to right, black calc(100% - 24px), transparent 100%);
    -webkit-mask-image: linear-gradient(to right, black calc(100% - 24px), transparent 100%);
    padding-right: 20px;
  }
  
  .filters::-webkit-scrollbar {
    display: none;
  }

  .controls-right {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: 8px;
    flex-shrink: 0;
  }

  .toolbar-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border: none;
    background: rgba(255, 255, 255, 0.05);
    border-radius: 8px;
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    transition: all 0.15s;
  }

  .toolbar-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .toolbar-btn.active {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.2));
    color: var(--accent, rgba(99, 102, 241, 1));
  }

  .sort-control {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .sort-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .view-toggle {
    display: flex;
    background: rgba(255, 255, 255, 0.05);
    border-radius: 8px;
    padding: 2px;
  }

  .view-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: none;
    background: transparent;
    border-radius: 6px;
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    transition: all 0.15s;
  }

  .view-btn:hover {
    color: white;
  }

  .view-btn.active {
    background: rgba(255, 255, 255, 0.1);
    color: white;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  }

  .stats-panel {
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 12px;
    padding: 16px;
    margin-bottom: 20px;
  }

  @keyframes slideDown {
    from {
      opacity: 0;
      transform: translateY(-10px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 12px;
    margin-bottom: 16px;
  }

  .stat-card {
    display: flex;
    align-items: center;
    gap: 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 8px;
    padding: 12px;
  }

  .stat-card :global(.icon) {
    color: var(--accent, rgba(99, 102, 241, 0.8));
    background: var(--accent-alpha-light, rgba(99, 102, 241, 0.1));
    padding: 8px;
    border-radius: 8px;
    box-sizing: content-box;
  }

  .stat-content {
    display: flex;
    flex-direction: column;
  }

  .stat-value {
    font-size: 16px;
    font-weight: 700;
    color: white;
  }

  .stat-label {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .stats-breakdown {
    display: flex;
    flex-wrap: wrap;
    gap: 24px;
    padding-top: 16px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .breakdown-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .breakdown-title {
    font-size: 11px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
  }

  .breakdown-items {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .breakdown-item {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.8);
    background: rgba(255, 255, 255, 0.05);
    padding: 4px 10px;
    border-radius: 100px;
  }

  .format-items {
    gap: 6px;
  }

  .format-badge {
    font-size: 11px;
    font-weight: 700;
    color: rgba(255, 255, 255, 0.6);
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid rgba(255, 255, 255, 0.05);
    padding: 2px 6px;
    border-radius: 4px;
  }

  .format-count {
    color: rgba(255, 255, 255, 0.3);
    margin-left: 2px;
    font-weight: 400;
  }
  
  .date-header {
      display: grid;
      grid-template-columns: 56px 1fr 60px 50px 70px 60px;
      gap: 12px;
      padding: 16px 16px 8px;
      align-items: center;
  }
  
  .date-label {
      grid-column: 1;
      
      font-size: 10px;
      font-weight: 600;
      color: rgba(255, 255, 255, 0.4);
      text-transform: uppercase;
      letter-spacing: 0.5px;
      white-space: nowrap;
      
      text-align: center;
      width: 100%;
      
      overflow: visible;
  }

  .grid-row {
     display: grid;
     grid-template-columns: repeat(var(--cols, 1), 1fr);
     gap: 10px;
     padding-bottom: 12px;
  }
  
  .grid-cell {
     min-width: 0;
     height: 100%;
  }
  
  .empty-state {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 60px 0;
      color: rgba(255,255,255,0.2);
  }
  
  .empty-state p {
      margin-top: 16px;
  }
  
  .end-message {
      text-align: center;
      padding: 40px 0;
      font-size: 13px;
      color: rgba(255,255,255,0.2);
  }

  @media (max-width: 700px) {
      .toolbar {
          height: 84px;
      }
      
      .toolbar.search-open {
          height: 132px;
      }

      .controls-row {
          left: 0 !important;
          width: 100%;
          height: auto;
          display: block;
      }

      .filters {
          width: 100%;
          padding-left: 48px;
          margin-bottom: 12px;
          height: 36px;
          box-sizing: border-box;
          transition: padding-left 0.35s cubic-bezier(0.4, 0, 0.2, 1);
      }

      .toolbar.search-open .filters {
          padding-left: 0;
      }

      .controls-right {
          width: 100%;
          margin-left: 0;
          height: 36px;
          justify-content: space-between;
          padding-right: 4px;
      }
  }
  
  .selection-toolbar {
      position: absolute;
      inset: 0;
      background: var(--bg-secondary, #1a1a1a);
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 16px;
      z-index: 30;
      border-radius: 8px;
  }
  
  .selection-count {
      display: flex;
      align-items: center;
      gap: 12px;
      color: white;
      font-weight: 600;
      font-size: 14px;
  }
  
  .count-badge {
      background: var(--accent, #6366f1);
      color: white;
      padding: 2px 8px;
      border-radius: 12px;
      font-size: 12px;
  }
  
  .selection-actions {
      display: flex;
      gap: 8px;
  }
</style>
