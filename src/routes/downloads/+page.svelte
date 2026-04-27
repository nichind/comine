<script lang="ts">
  import { onMount, setContext, tick } from 'svelte';
  import { goto } from '$app/navigation';
  import { t } from '$lib/i18n';
  import {
    history,
    playlistGroupedHistory,
    historyStats,
    type SortType,
  } from '$lib/stores/history';
  import { formatDuration, formatSize, formatSpeed } from '$lib/utils/format';
  import { groupedDownloads, queue, activeDownloadsCount } from '$lib/stores/queue';
  import { downloadSpeedNow, downloadSpeedPoints } from '$lib/stores/downloadSpeed';
  import { initConversions, cleanupConversions, startConversion } from '$lib/stores/conversions';
  import { settings } from '$lib/stores/settings';
  import { navigation } from '$lib/stores/navigation';
  import { invoke } from '@tauri-apps/api/core';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { isAndroid } from '$lib/utils/android';
  import { openFile, revealFile, openFolder } from '$lib/utils/platform';
  import {
    DownloadsState,
    VIRTUALIZATION_HEIGHTS,
    computeGridRowHeight,
    type ColumnKey,
    type UnifiedDownloadItem,
    type VirtualListItem,
  } from '$lib/stores/downloadsState.svelte';
  import { DOWNLOADS_CONTEXT_KEY, type DownloadsContext } from '$lib/stores/downloadsContext';
  import { fade } from 'svelte/transition';
  import { toast } from '$lib/components/ui/Toast.svelte';

  import VirtualList from '$lib/components/media/VirtualList.svelte';
  import TableHeader from '$lib/components/download/TableHeader.svelte';
  import DownloadItem from '$lib/components/download/DownloadItem.svelte';
  import DownloadSpeedGraph from '$lib/components/download/DownloadSpeedGraph.svelte';
  import DownloadItemDetailsModal from '$lib/components/download/DownloadItemDetailsModal.svelte';

  import Icon from '$lib/components/ui/Icon.svelte';
  import Chip from '$lib/components/ui/Chip.svelte';
  import Select from '$lib/components/ui/Select.svelte';
  import Dropdown from '$lib/components/ui/Dropdown.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import PageShell from '$lib/components/layout/PageShell.svelte';
  import {
    hasOpenAriaModal,
    hasOpenAriaMenu,
    isPrintableKey,
    isTypingTarget,
    matchesShortcut,
  } from '$lib/utils/keyboard';

  const dState = new DownloadsState();

  $effect(() => {
    dState.historyGroups = $playlistGroupedHistory;
    dState.activeDownloadData = $groupedDownloads;
  });

  let containerEl: HTMLDivElement;
  let listRef: any = $state(null);
  let showStatsPanel = $state(false);
  let searchInputRef: HTMLInputElement | null = $state(null);
  let searchExpanded = $state(false);
  let columnsDropdownOpen = $state(false);
  let columnsDropdownAnchor: HTMLElement | null = $state(null);

  let detailsOpen = $state(false);
  let detailsItem = $state<UnifiedDownloadItem | null>(null);

  let speedGraphVisible = $derived(
    $queue.items.some(
      (i) => i.status === 'downloading' || i.status === 'processing' || i.status === 'converting'
    )
  );

  let prevFilter = $state<string | null>(null);
  let prevSearchQuery = $state<string | null>(null);

  $effect(() => {
    const currentFilter = dState.activeFilter;
    const currentQuery = dState.searchQuery;
    const list = listRef;

    if (prevFilter === null && prevSearchQuery === null) {
      prevFilter = currentFilter;
      prevSearchQuery = currentQuery;
      return;
    }

    const filterChanged = currentFilter !== prevFilter;
    const queryChanged = currentQuery !== prevSearchQuery;

    prevFilter = currentFilter;
    prevSearchQuery = currentQuery;

    if (!list || (!filterChanged && !queryChanged)) return;

    void (async () => {
      await tick();
      try {
        sessionStorage.removeItem('downloads-scroll');
      } catch {}
      list.scrollToTop?.();
      list.refresh?.();
    })();
  });

  setContext<DownloadsContext>(DOWNLOADS_CONTEXT_KEY, {
    openItem: async (item) => {
      if (dState.isFileMissing(item.id)) {
        toast.error($t('downloads.fileMissing'));
        return;
      }
      if (item.type === 'video' || item.type === 'audio') {
        navigation.openVideo(item.url, {
          title: item.title,
          thumbnail: item.thumbnail,
          author: item.author,
        });
        await goto('/');
      } else {
        if (!item.filePath) {
          toast.error($t('downloads.noFilePath'));
          return;
        }
        const success = item.isDirectory
          ? await openFolder(item.filePath)
          : await openFile(item.filePath);
        if (!success) {
          toast.error($t('downloads.openError'));
        }
      }
    },
    openAuthor: async (item) => {
      if (item.authorUrl) {
        navigation.openChannel(item.authorUrl, {
          title: item.author,
          author: item.author,
        });
        await goto('/');
      } else if (item.type === 'video' || item.type === 'audio') {
        navigation.openVideo(item.url, {
          title: item.title,
          thumbnail: item.thumbnail,
          author: item.author,
        });
        await goto('/');
      }
    },
    playItem: async (item) => {
      if (dState.isFileMissing(item.id)) {
        toast.error($t('downloads.fileMissing'));
        return;
      }
      if (!item.filePath) {
        toast.error($t('downloads.noFilePath'));
        return;
      }
      const success = await openFile(item.filePath);
      if (!success) {
        toast.error($t('downloads.openError'));
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
      const success = await revealFile(path);
      if (!success && !isAndroid()) {
        toast.error($t('downloads.revealError'));
      }
    },
    convertItem: async (item, targetFormat, audioOnly) => {
      const result = await startConversion(item as any, targetFormat, audioOnly);
      if (result) {
        toast.success(`Converted to ${targetFormat.toUpperCase()}`);
      } else {
        toast.error($t('downloads.conversionFailed'));
      }
    },
    showDetails: (item) => {
      detailsItem = item;
      detailsOpen = true;
    },
  });

  onMount(() => {
    void (async () => {
      await history.init();
      await tick();
      listRef?.refresh?.();
    })();
    initConversions();

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

    let rafId: number | null = null;

    const resizeObserver = new ResizeObserver((entries) => {
      if (rafId !== null) return;
      rafId = requestAnimationFrame(() => {
        rafId = null;
        dState.containerWidth = entries[0].contentRect.width;
      });
    });

    if (containerEl) {
      resizeObserver.observe(containerEl);
      dState.containerWidth = containerEl.offsetWidth;
    }

    return () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      resizeObserver.disconnect();
      cleanupConversions();
      dState.destroy();
    };
  });

  $effect(() => {
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;

      if (dState.viewMode === 'list') {
        e.preventDefault();
        if (e.deltaY < 0) {
          dState.listItemSize = Math.min(80, dState.listItemSize + 8);
        } else if (e.deltaY > 0) {
          dState.listItemSize = Math.max(40, dState.listItemSize - 8);
        }
        return;
      }

      if (dState.viewMode !== 'grid') return;

      e.preventDefault();
      if (e.deltaY < 0) {
        dState.gridItemSize = Math.min(400, dState.gridItemSize + 40);
      } else if (e.deltaY > 0) {
        dState.gridItemSize = Math.max(120, dState.gridItemSize - 40);
      }
    };

    window.addEventListener('wheel', onWheel, { passive: false });
    return () => window.removeEventListener('wheel', onWheel);
  });

  $effect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return;
      if (isTypingTarget(document.activeElement)) return;

      if (hasOpenAriaModal()) return;

      if (columnsDropdownOpen && e.key === 'Escape') {
        e.preventDefault();
        columnsDropdownOpen = false;
        return;
      }

      if (columnsDropdownOpen) return;
      if (hasOpenAriaMenu()) return;

      const bindings: Array<{
        match: (e: KeyboardEvent) => boolean;
        run: (e: KeyboardEvent) => void | Promise<void>;
        preventDefault?: boolean;
      }> = [
        {
          match: (e) => matchesShortcut(e, { key: 'f', mod: true }),
          run: async () => {
            searchExpanded = true;
            await tick();
            searchInputRef?.focus();
          },
        },
        {
          match: (e) => matchesShortcut(e, { key: 'a', mod: true, shift: false }),
          run: () => dState.selectAll(),
        },
        {
          match: (e) => matchesShortcut(e, { key: 'a', mod: true, shift: true }),
          run: () => {
            const activeIds = dState.displayItems
              .filter(
                (item): item is Extract<typeof item, { kind: 'single' }> =>
                  item.kind === 'single' && item.item.isActive
              )
              .map((item) => item.item.id);
            dState.selectedItemIds = new Set(activeIds);
          },
        },
        {
          match: (e) => matchesShortcut(e, { key: 'c', mod: true }) && dState.isSelectionMode,
          run: async () => {
            const urls = Array.from(dState.selectedItemIds)
              .map((id) => dState.getItemById(id)?.url)
              .filter(Boolean)
              .join('\n');

            if (!urls) return;

            try {
              await navigator.clipboard.writeText(urls);
              toast.success($t('common.copied'));
            } catch (err) {
              console.error('Clipboard write failed:', err);
            }
          },
        },
        {
          match: (e) => (e.ctrlKey || e.metaKey) && (e.key === '=' || e.key === '+'),
          run: () => {
            if (dState.viewMode === 'grid')
              dState.gridItemSize = Math.min(400, dState.gridItemSize + 40);
            else dState.listItemSize = Math.min(80, dState.listItemSize + 8);
          },
        },
        {
          match: (e) => (e.ctrlKey || e.metaKey) && (e.key === '-' || e.key === '_'),
          run: () => {
            if (dState.viewMode === 'grid')
              dState.gridItemSize = Math.max(120, dState.gridItemSize - 40);
            else dState.listItemSize = Math.max(40, dState.listItemSize - 8);
          },
        },
        {
          match: (e) => (e.ctrlKey || e.metaKey) && e.key === '0',
          run: () => {
            if (dState.viewMode === 'grid') dState.gridItemSize = 200;
            else dState.listItemSize = 56;
          },
        },
        {
          match: (e) => !e.ctrlKey && !e.metaKey && !e.altKey && e.key === 'Escape',
          run: () => {
            if (dState.isSelectionMode) {
              dState.clearSelection();
              return;
            }

            if (dState.searchQuery.trim() || searchExpanded) {
              dState.searchQuery = '';
              searchExpanded = false;
            }
          },
        },
        {
          match: (e) =>
            !e.ctrlKey &&
            !e.metaKey &&
            !e.altKey &&
            dState.isSelectionMode &&
            (e.key === 'Delete' || e.key === 'Backspace'),
          run: () => {
            const selectedIds = Array.from(dState.selectedItemIds);
            for (const id of selectedIds) {
              const item = dState.getItemById(id);
              if (item?.isActive) {
                queue.cancel(id);
              } else {
                history.remove(id);
              }
            }
            dState.clearSelection();
          },
        },
        {
          match: (e) =>
            !e.ctrlKey &&
            !e.metaKey &&
            !e.altKey &&
            e.key === 'Enter' &&
            dState.selectedItemIds.size >= 1,
          run: () => {
            for (const id of dState.selectedItemIds) {
              const item = dState.getItemById(id);
              if (item && !item.isActive && item.filePath && !dState.isFileMissing(id)) {
                void openFile(item.filePath);
              }
            }
          },
        },
        {
          match: (e) =>
            !e.ctrlKey &&
            !e.metaKey &&
            !e.altKey &&
            (e.key === ' ' || e.code === 'Space') &&
            dState.selectedItemIds.size >= 1,
          run: () => {
            for (const id of dState.selectedItemIds) {
              const item = dState.getItemById(id);
              if (!item?.isActive || item.status === 'failed') continue;

              if (item.status === 'paused') {
                queue.resumeItem(id);
              } else {
                queue.pauseItem(id);
              }
            }
          },
        },
        {
          match: (e) =>
            !e.ctrlKey &&
            !e.metaKey &&
            !e.altKey &&
            (e.key === 'r' || e.key === 'R') &&
            dState.selectedItemIds.size >= 1,
          run: () => {
            const selectedIds = Array.from(dState.selectedItemIds);
            const hasFailedItem = selectedIds.some((id) => {
              const item = dState.getItemById(id);
              return item?.isActive && item.status === 'failed';
            });
            if (!hasFailedItem) return;

            for (const id of selectedIds) {
              const item = dState.getItemById(id);
              if (item?.isActive && item.status === 'failed') {
                queue.retry(id);
              }
            }
          },
        },
        {
          match: (e) =>
            !e.ctrlKey &&
            !e.metaKey &&
            !e.altKey &&
            (e.key === 'f' || e.key === 'F') &&
            dState.selectedItemIds.size === 1,
          run: () => {
            const id = Array.from(dState.selectedItemIds)[0];
            const item = dState.getItemById(id);
            if (!item?.filePath) return;

            void revealFile(item.filePath);
          },
        },
        {
          match: (e) => isPrintableKey(e),
          run: (e) => {
            if (!dState.searchQuery.trim() && e.key.trim() === '') return;
            void openSearch(e.key);
          },
        },
      ];

      for (const binding of bindings) {
        if (!binding.match(e)) continue;
        if (binding.preventDefault !== false) e.preventDefault();
        void binding.run(e);
        return;
      }
    };

    window.addEventListener('keydown', onKeyDown, true);
    return () => {
      window.removeEventListener('keydown', onKeyDown, true);
    };
  });

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
    } catch {}
  }

  function collapseSearchIfEmpty() {
    if (!dState.searchQuery.trim()) {
      searchExpanded = false;
    }
  }

  function toggleCol(col: ColumnKey) {
    dState.visibleColumns = dState.visibleColumns.includes(col)
      ? dState.visibleColumns.filter((c) => c !== col)
      : [...dState.visibleColumns, col];
  }

  const COLUMNS: ColumnKey[] = ['format', 'size', 'duration'];

  const FILTER_CHIPS = [
    { value: 'all', icon: 'date' },
    { value: 'video', icon: 'video' },
    { value: 'audio', icon: 'music' },
    { value: 'image', icon: 'image' },
    { value: 'gallery', icon: 'gallery' },
    { value: 'file', icon: 'file_text' },
    { value: 'torrent', icon: 'folder' },
  ] as const;

  const TYPE_BREAKDOWN = [
    { key: 'video', icon: 'video' },
    { key: 'audio', icon: 'music' },
    { key: 'image', icon: 'image' },
    { key: 'gallery', icon: 'gallery' },
    { key: 'file', icon: 'file_text' },
    { key: 'torrent', icon: 'folder' },
  ] as const;

  let typeCounts = $derived.by(() => {
    const items = $history.items;
    const counts = { video: 0, audio: 0, image: 0, gallery: 0, file: 0, torrent: 0 };
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

  let selectedItems = $derived(
    Array.from(dState.selectedItemIds)
      .map((id) => dState.getItemById(id))
      .filter(Boolean) as UnifiedDownloadItem[]
  );
  let selectionHasFailed = $derived(
    selectedItems.some((item) => item.isActive && item.status === 'failed')
  );
  let selectionHasActive = $derived(
    selectedItems.some(
      (item) =>
        item.isActive &&
        (item.status === 'downloading' ||
          item.status === 'pending' ||
          item.status === 'paused' ||
          item.status === 'processing' ||
          item.status === 'converting')
    )
  );
  let selectionHasPaused = $derived(
    selectedItems.some((item) => item.isActive && item.status === 'paused')
  );
  let selectionHasCompleted = $derived(selectedItems.some((item) => !item.isActive));
</script>

<DownloadItemDetailsModal
  open={detailsOpen}
  item={detailsItem}
  onClose={() => {
    detailsOpen = false;
    detailsItem = null;
  }}
/>

<PageShell scrollMode="virtual-list">
  <div class="downloads-inner" bind:this={containerEl} style="--cols: {dState.itemsPerRow}">
    <div
      class="toolbar"
      class:search-open={searchExpanded || dState.searchQuery.trim()}
      class:selection-open={dState.isSelectionMode}
    >
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

      <div class="controls-row">
        <div class="filters">
          {#each FILTER_CHIPS as chip}
            <Chip
              selected={dState.activeFilter === chip.value}
              icon={chip.icon}
              onclick={() => (dState.activeFilter = chip.value)}
            >
              {$t(`downloads.filters.${chip.value}`)}
            </Chip>
          {/each}
          {#if $historyStats.favouritesCount > 0}
            <Chip
              selected={dState.activeFilter === 'favourites'}
              icon="star"
              onclick={() => (dState.activeFilter = 'favourites')}
            >
              {$t('downloads.filters.favourites')} ({$historyStats.favouritesCount})
            </Chip>
          {/if}
        </div>

        <div class="controls-right">
          {#if $activeDownloadsCount > 0}
            <div class="queue-controls">
              <button
                class="toolbar-btn"
                class:active={$queue.isPaused}
                onclick={() => queue.togglePause()}
                use:tooltip={$queue.isPaused
                  ? $t('downloads.queue.resumeAll')
                  : $t('downloads.queue.pauseAll')}
              >
                <Icon name={$queue.isPaused ? 'play' : 'pause'} size={18} />
              </button>
              <button
                class="toolbar-btn danger"
                onclick={() => {
                  if (
                    confirm(
                      $t('downloads.queue.cancelAllConfirm', { count: $activeDownloadsCount })
                    )
                  ) {
                    queue.cancelAll();
                  }
                }}
                use:tooltip={$t('downloads.queue.cancelAll')}
              >
                <Icon name="close" size={18} />
              </button>
              <span class="active-count">{$activeDownloadsCount}</span>
            </div>
          {/if}

          <div class="sort-control">
            <span class="sort-label">{$t('downloads.sort.label')}:</span>
            <Select
              bind:value={dState.sortType}
              options={[
                { value: 'date', label: $t('downloads.sort.date') },
                { value: 'name', label: $t('downloads.sort.name') },
                { value: 'size', label: $t('downloads.sort.size') },
              ]}
              onchange={(v) => (dState.sortType = v as SortType)}
            />
          </div>

          {#if dState.viewMode === 'list'}
            <button
              class="toolbar-btn"
              class:active={columnsDropdownOpen}
              bind:this={columnsDropdownAnchor}
              onclick={() => (columnsDropdownOpen = !columnsDropdownOpen)}
              use:tooltip={$t('downloads.columns.toggle')}
            >
              <Icon name="settings" size={18} />
            </button>
          {/if}

          <div class="view-toggle">
            <button
              class="view-btn"
              class:active={dState.viewMode === 'list'}
              onclick={() => (dState.viewMode = 'list')}
              use:tooltip={$t('downloads.views.list')}
            >
              <Icon name="checklist" size={18} />
            </button>
            <button
              class="view-btn"
              class:active={dState.viewMode === 'grid'}
              onclick={() => (dState.viewMode = 'grid')}
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
        <div class="selection-toolbar" transition:fade={{ duration: 150 }}>
          <div class="selection-left">
            <span class="count-badge">{dState.selectedItemIds.size}</span>
            <span class="count-label">{$t('downloads.selected')}</span>
            <button
              class="selection-action compact"
              onclick={() => dState.selectAll()}
              use:tooltip={$t('downloads.selectAll')}
            >
              <Icon name="check" size={14} />
              <span>{$t('downloads.selectAll')}</span>
            </button>
          </div>
          <div class="selection-actions">
            {#if selectionHasActive}
              {#if selectionHasPaused}
                <button
                  class="selection-action"
                  onclick={() => {
                    for (const item of selectedItems) {
                      if (item.isActive && item.status === 'paused') {
                        queue.resumeItem(item.id);
                      }
                    }
                  }}
                  use:tooltip={$t('downloads.queue.resumeItem')}
                >
                  <Icon name="play" size={16} />
                  <span>{$t('downloads.queue.resumeItem')}</span>
                </button>
              {:else}
                <button
                  class="selection-action"
                  onclick={() => {
                    for (const item of selectedItems) {
                      if (
                        item.isActive &&
                        (item.status === 'downloading' ||
                          item.status === 'pending' ||
                          item.status === 'processing' ||
                          item.status === 'converting')
                      ) {
                        queue.pauseItem(item.id);
                      }
                    }
                  }}
                  use:tooltip={$t('downloads.queue.pauseItem')}
                >
                  <Icon name="pause" size={16} />
                  <span>{$t('downloads.queue.pauseItem')}</span>
                </button>
              {/if}
            {/if}
            {#if selectionHasFailed}
              <button
                class="selection-action"
                onclick={() => {
                  for (const item of selectedItems) {
                    if (item.isActive && item.status === 'failed') {
                      queue.retry(item.id);
                    }
                  }
                }}
                use:tooltip={$t('downloads.retry')}
              >
                <Icon name="refresh" size={16} />
                <span>{$t('downloads.retry')}</span>
              </button>
            {/if}
            <button
              class="selection-action"
              onclick={() => {
                const urls = Array.from(dState.selectedItemIds)
                  .map((id) => dState.getItemById(id)?.url)
                  .filter(Boolean)
                  .join('\n');
                if (urls) {
                  navigator.clipboard.writeText(urls);
                  toast.success($t('common.copied'));
                }
              }}
              use:tooltip={$t('downloads.copyUrl')}
            >
              <Icon name="copy" size={16} />
              <span>{$t('downloads.copyUrl')}</span>
            </button>
            {#if selectionHasCompleted}
              <button
                class="selection-action"
                onclick={() => {
                  for (const id of dState.selectedItemIds) {
                    const item = dState.getItemById(id);
                    if (item && item.filePath && !dState.isFileMissing(id)) {
                      void openFile(item.filePath);
                    }
                  }
                  dState.clearSelection();
                }}
                use:tooltip={$t('downloads.openSelected')}
              >
                <Icon name="play" size={16} />
                <span>{$t('downloads.openSelected')}</span>
              </button>
              <button
                class="selection-action"
                onclick={async () => {
                  for (const item of selectedItems) {
                    if (!item.isActive && item.url) {
                      navigation.openVideo(item.url);
                      await goto('/');
                    }
                  }
                  dState.clearSelection();
                }}
                use:tooltip={$t('downloads.redownload')}
              >
                <Icon name="download" size={16} />
                <span>{$t('downloads.redownload')}</span>
              </button>
            {/if}
            <div class="selection-divider"></div>
            <button
              class="selection-action danger"
              onclick={() => {
                for (const id of dState.selectedItemIds) {
                  const item = dState.getItemById(id);
                  if (item?.isActive) {
                    queue.cancel(id);
                  } else {
                    history.remove(id);
                  }
                }
                dState.clearSelection();
              }}
              use:tooltip={$t('downloads.deleteSelected')}
            >
              <Icon name="trash" size={16} />
              <span>{$t('downloads.deleteSelected')}</span>
            </button>
            <button
              class="selection-action"
              onclick={() => dState.clearSelection()}
              use:tooltip={$t('downloads.clearSelection')}
            >
              <Icon name="cross" size={16} />
            </button>
          </div>
        </div>
      {/if}
    </div>
    <Dropdown
      open={columnsDropdownOpen}
      anchorEl={columnsDropdownAnchor}
      onclose={() => (columnsDropdownOpen = false)}
    >
      <span class="menu-section-title">{$t('downloads.columns.title')}</span>
      {#each COLUMNS as col}
        <button
          class="menu-option"
          class:selected={dState.visibleColumns.includes(col)}
          role="menuitemcheckbox"
          aria-checked={dState.visibleColumns.includes(col)}
          onclick={() => toggleCol(col)}
        >
          <span>{$t(`downloads.table.${col}`)}</span>
          {#if dState.visibleColumns.includes(col)}
            <Icon name="check" size={14} />
          {/if}
        </button>
      {/each}

      <div class="menu-divider"></div>

      <span class="menu-section-title">{$t('downloads.display.title')}</span>
      <button
        class="menu-option"
        class:selected={dState.hideMissingFiles}
        role="menuitemcheckbox"
        aria-checked={dState.hideMissingFiles}
        onclick={() => (dState.hideMissingFiles = !dState.hideMissingFiles)}
      >
        <span>{$t('downloads.display.hideMissing')}</span>
        {#if dState.hideMissingFiles}
          <Icon name="check" size={14} />
        {/if}
      </button>
      <button
        class="menu-option"
        class:selected={dState.showSourceTags}
        role="menuitemcheckbox"
        aria-checked={dState.showSourceTags}
        onclick={() => (dState.showSourceTags = !dState.showSourceTags)}
      >
        <span>{$t('downloads.display.showSourceTags')}</span>
        {#if dState.showSourceTags}
          <Icon name="check" size={14} />
        {/if}
      </button>

      <button
        class="menu-option"
        class:selected={dState.ungroupPlaylistsOnSort}
        role="menuitemcheckbox"
        aria-checked={dState.ungroupPlaylistsOnSort}
        onclick={() => (dState.ungroupPlaylistsOnSort = !dState.ungroupPlaylistsOnSort)}
      >
        <span>{$t('downloads.display.ungroupPlaylistsOnSort')}</span>
        {#if dState.ungroupPlaylistsOnSort}
          <Icon name="check" size={14} />
        {/if}
      </button>
    </Dropdown>

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
              {#each TYPE_BREAKDOWN as tb}
                {#if typeCounts[tb.key] > 0}
                  <span class="breakdown-item">
                    <Icon name={tb.icon} size={14} />
                    {typeCounts[tb.key]}
                    {$t(`downloads.filters.${tb.key}`)}
                  </span>
                {/if}
              {/each}
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
        bind:this={listRef}
        items={dState.displayItems}
        getKey={(item) => item.id}
        estimatedItemHeight={dState.viewMode === 'list'
          ? dState.listItemSize
          : computeGridRowHeight(dState.containerWidth, dState.itemsPerRow)}
        overscan={10}
        useFadeMask={true}
        useCustomScrollbar={true}
        preserveScrollKey="downloads-scroll"
        getItemSize={(index, item) => {
          if (item.kind === 'date') return VIRTUALIZATION_HEIGHTS.dateHeader;
          if (item.kind === 'playlist-header') return dState.listItemSize;
          if (item.kind === 'playlist-child') return dState.listItemSize;
          if (item.kind === 'grid-row')
            return computeGridRowHeight(dState.containerWidth, dState.itemsPerRow);
          return dState.listItemSize;
        }}
      >
        {#snippet header()}
          <div class="table-header-area">
            {#if dState.viewMode === 'list'}
              <TableHeader
                sortType={dState.sortType}
                sortDirection={dState.sortDirection}
                visibleColumns={dState.visibleColumns}
                listItemSize={dState.listItemSize}
                onSortChange={(type, direction) => {
                  dState.sortType = type;
                  dState.sortDirection = direction;
                }}
              />
            {/if}

            {#if dState.viewMode === 'list' && speedGraphVisible}
              <div class="speed-sparkline" aria-hidden="true">
                <DownloadSpeedGraph
                  points={$downloadSpeedPoints}
                  height={26}
                  showLabels={false}
                  variant="sparkline"
                />
                {#if $downloadSpeedNow > 0}
                  <div class="speed-sparkline-text">{formatSpeed($downloadSpeedNow)}</div>
                {/if}
              </div>
            {/if}
          </div>
        {/snippet}

        {#snippet children(item: VirtualListItem, index: number)}
          {#if item.kind === 'date'}
            <div class="date-header">
              <span class="date-label">{item.label}</span>
            </div>
          {:else if item.kind === 'playlist-header'}
            <button
              class="playlist-header"
              class:collapsed={!item.isExpanded}
              style="height: {dState.listItemSize}px;"
              onclick={(e) => {
                if (e.ctrlKey || e.shiftKey || dState.isSelectionMode) {
                  e.preventDefault();
                  e.stopPropagation();
                  dState.toggleGroupSelection(item.groupKey);
                } else {
                  dState.togglePlaylist(item.groupKey);
                }
              }}
              aria-expanded={item.isExpanded}
            >
              <Icon name="chevron_down" size={14} class="icon" />
              <div class="playlist-info">
                <span class="playlist-label">{item.playlistTitle}</span>
                <span class="playlist-meta">
                  <span class="playlist-count">{item.childCount} items</span>
                  {#if item.totalSize > 0}
                    <span class="playlist-size">{formatSize(item.totalSize)}</span>
                  {/if}
                  {#if item.totalDuration > 0}
                    <span class="playlist-duration">{formatDuration(item.totalDuration)}</span>
                  {/if}
                </span>
              </div>
            </button>
          {:else if item.kind === 'playlist-child'}
            <div class="playlist-item" class:last={item.isLast}>
              <DownloadItem
                item={item.item}
                {dState}
                showSeparator={index > 0 &&
                  dState.displayItems[index - 1]?.kind !== 'date' &&
                  dState.displayItems[index - 1]?.kind !== 'playlist-header'}
              />
            </div>
          {:else if item.kind === 'single'}
            <DownloadItem
              item={item.item}
              {dState}
              showSeparator={index > 0 &&
                dState.displayItems[index - 1]?.kind !== 'date' &&
                dState.displayItems[index - 1]?.kind !== 'playlist-header'}
            />
          {:else if item.kind === 'grid-row'}
            <div class="grid-row">
              {#each item.items as subItem (subItem.id)}
                <div class="grid-cell">
                  <DownloadItem item={subItem} {dState} />
                </div>
              {/each}
            </div>
          {/if}
        {/snippet}

        {#snippet footer()}
          {#if dState.displayItems.length === 0}
            <div class="empty-state">
              <div class="empty-icon">
                <Icon name="download" size={48} />
              </div>
              <h3 class="empty-title">{$t('downloads.empty')}</h3>
              <p class="empty-hint">{$t('downloads.startHint')}</p>
              <button class="empty-action" onclick={() => goto('/')}>
                <Icon name="add" size={16} />
                {$t('nav.download')}
              </button>
            </div>
          {:else if dState.displayItems.length > 0}
            <p class="end-message">{$t('downloads.endMessage')}</p>
          {/if}
        {/snippet}
      </VirtualList>
    </div>
  </div>
</PageShell>

<style>
  .downloads-inner {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .list-container {
    flex: 1;
    min-height: 0;
    margin-right: 0;
    margin-left: 0;
    margin-bottom: 4px;
  }

  .toolbar {
    position: relative;
    display: block;
    height: 36px;
    margin-bottom: 16px;
    margin-right: 4px;
    overflow: hidden;
    transition: height 0.35s cubic-bezier(0.4, 0, 0.2, 1);
    will-change: height;
  }

  .toolbar.search-open {
    height: 84px;
  }

  .toolbar.selection-open {
    overflow: visible;
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
    border-radius: var(--radius, 8px);
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
    font-size: var(--text-md, 14px);
    color: rgba(255, 255, 255, 0.9);
    min-width: 0;
  }

  .search-container input::placeholder {
    color: rgba(255, 255, 255, 0.35);
  }

  .search-close {
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    flex-shrink: 0;
    transition: all 0.15s;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm, 6px);
  }

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

  .queue-controls {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-right: 8px;
    margin-right: 4px;
    border-right: 1px solid rgba(255, 255, 255, 0.1);
  }

  .active-count {
    font-size: 11px;
    font-weight: 600;
    color: var(--accent, #6366f1);
    background: var(--accent-alpha, rgba(99, 102, 241, 0.15));
    padding: 2px 6px;
    border-radius: var(--radius-sm, 6px);
    min-width: 20px;
    text-align: center;
  }

  .toolbar-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border: none;
    background: rgba(255, 255, 255, 0.05);
    border-radius: var(--radius, 8px);
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

  .toolbar-btn.danger:hover {
    background: rgba(239, 68, 68, 0.15);
    color: #f87171;
  }

  .sort-control {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .sort-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    white-space: nowrap;
  }

  .view-toggle {
    display: flex;
    background: rgba(255, 255, 255, 0.05);
    border-radius: var(--radius, 8px);
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
    border-radius: var(--radius-sm, 6px);
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
    border-radius: var(--radius-lg, 12px);
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
    border-radius: var(--radius, 8px);
    padding: 12px;
  }

  .stat-card :global(.icon) {
    color: var(--accent, rgba(99, 102, 241, 0.8));
    background: var(--accent-alpha-light, rgba(99, 102, 241, 0.1));
    padding: 8px;
    border-radius: var(--radius, 8px);
    box-sizing: content-box;
  }

  .stat-content {
    display: flex;
    flex-direction: column;
  }

  .stat-value {
    font-size: var(--text-lg, 16px);
    font-weight: 700;
    color: white;
  }

  .stat-label {
    font-size: var(--text-xs, 11px);
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
    font-size: var(--text-xs, 11px);
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
    font-size: var(--text-sm, 12px);
    color: rgba(255, 255, 255, 0.8);
    background: rgba(255, 255, 255, 0.05);
    padding: 4px 10px;
    border-radius: 100px;
  }

  .format-items {
    gap: 6px;
  }

  .format-badge {
    font-size: var(--text-xs, 11px);
    font-weight: 700;
    color: rgba(255, 255, 255, 0.6);
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid rgba(255, 255, 255, 0.05);
    padding: 2px 6px;
    border-radius: var(--radius-sm, 4px);
  }

  .format-count {
    color: rgba(255, 255, 255, 0.3);
    margin-left: 2px;
    font-weight: 400;
  }

  .date-header {
    padding: 16px 16px 8px;
  }

  .date-label {
    font-size: 10px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    white-space: nowrap;
    text-align: start;
  }

  .grid-row {
    display: grid;
    grid-template-columns: repeat(var(--cols, 1), 1fr);
    gap: 10px;
  }

  .grid-cell {
    min-width: 0;
    height: 100%;
  }

  .playlist-header {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 100%;
    padding: 0 12px;
    background: rgba(255, 255, 255, 0.06);
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.7);
    font-size: 13px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .playlist-header:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.9);
  }

  .playlist-header :global(.icon) {
    display: block;
    opacity: 0.75;
    transform: rotate(0deg);
    transform-origin: 50% 50%;
    transition:
      opacity 0.15s ease,
      transform 0.15s ease;
    flex-shrink: 0;
  }

  .playlist-header:hover :global(.icon) {
    opacity: 0.9;
  }

  .playlist-header.collapsed :global(.icon) {
    transform: rotate(-90deg);
  }

  .playlist-info {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    min-width: 0;
  }

  .playlist-label {
    max-width: 400px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .playlist-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
  }

  .playlist-size,
  .playlist-duration {
    opacity: 0.8;
  }

  .playlist-meta span:not(:last-child)::after {
    content: '·';
    margin-left: 8px;
    opacity: 0.5;
  }

  .playlist-item {
    background: rgba(255, 255, 255, 0.03);
    border-radius: var(--radius-sm, 6px);
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 60px 20px;
    text-align: center;
  }

  .empty-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 96px;
    height: 96px;
    margin-bottom: 16px;
    border: 2px dashed rgba(255, 255, 255, 0.15);
    border-radius: 50%;
    color: rgba(255, 255, 255, 0.2);
  }

  .empty-title {
    margin: 0 0 8px;
    font-size: 18px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.7);
  }

  .empty-hint {
    margin: 0 0 20px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.4);
  }

  .empty-action {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 20px;
    border: none;
    border-radius: var(--radius, 8px);
    background: var(--accent, #6366f1);
    color: white;
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    transition:
      background 0.2s,
      transform 0.1s;
  }

  .empty-action:hover {
    background: var(--accent-hover, #4f46e5);
  }

  .empty-action:active {
    transform: scale(0.98);
  }

  .empty-state p {
    margin-top: 16px;
  }

  .end-message {
    text-align: center;
    padding: 40px 0;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.2);
  }

  .table-header-area {
    position: relative;
    z-index: 20;
  }

  .speed-sparkline {
    position: absolute;
    top: calc(100% + 2px);
    left: 0;
    right: 0;
    height: 26px;
    z-index: 24;
    pointer-events: none;
    opacity: 0.95;

    mask-image: linear-gradient(
      to right,
      transparent 0px,
      black 14px,
      black calc(100% - 14px),
      transparent 100%
    );
    -webkit-mask-image: linear-gradient(
      to right,
      transparent 0px,
      black 14px,
      black calc(100% - 14px),
      transparent 100%
    );
  }

  .speed-sparkline-text {
    position: absolute;
    top: 5px;
    right: 0;
    padding: 0 2px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.55);
    font-variant-numeric: tabular-nums;
    text-shadow: 0 1px 0 rgba(0, 0, 0, 0.35);
  }

  @media (max-width: 700px) {
    .toolbar {
      margin-inline: 8px;
      height: 84px;
    }

    .toolbar.search-open {
      height: 132px;
    }

    .toolbar.selection-open {
      overflow: visible;
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
      justify-content: flex-end;
      padding-right: 4px;
    }

    .grid-row {
      gap: 4px;
      padding-left: 4px;
      padding-right: 4px;
    }

    .speed-sparkline {
      left: 4px;
      right: 4px;
    }

    .stats-panel {
      margin-inline: 8px;
    }
  }

  .selection-toolbar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 36px;
    box-sizing: border-box;
    background: var(--surface-elevated-bg, rgba(20, 20, 22, 0.98));
    border: 1px solid var(--surface-border, rgba(255, 255, 255, 0.08));
    box-shadow: var(--surface-shadow, 0 8px 24px rgba(0, 0, 0, 0.35));
    backdrop-filter: blur(var(--surface-blur, 16px));
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    z-index: 30;
    border-radius: var(--radius, 8px);
  }

  .selection-left {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .count-badge {
    background: var(--accent, #6366f1);
    color: white;
    padding: 1px 7px;
    border-radius: var(--radius-lg, 12px);
    font-size: 11px;
    font-weight: 600;
    min-width: 18px;
    text-align: center;
  }

  .count-label {
    font-size: 12px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.7);
  }

  .selection-actions {
    display: flex;
    gap: 4px;
    align-items: center;
    flex-wrap: nowrap;
  }

  .selection-divider {
    width: 1px;
    height: 20px;
    background: rgba(255, 255, 255, 0.1);
    margin: 0 2px;
    flex-shrink: 0;
  }

  .selection-action {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    box-sizing: border-box;
    padding: 0 10px;
    border: none;
    background: rgba(255, 255, 255, 0.05);
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
    line-height: 1;
    font-size: 12px;
  }

  .selection-action.compact {
    padding: 0 8px;
    background: transparent;
    color: rgba(255, 255, 255, 0.5);
    font-size: 11px;
  }

  .selection-action.compact:hover {
    color: rgba(255, 255, 255, 0.8);
    background: rgba(255, 255, 255, 0.05);
  }

  .selection-action:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .selection-action.danger:hover {
    background: rgba(239, 68, 68, 0.15);
    color: #f87171;
  }

  .selection-action span {
    font-size: 12px;
    font-weight: 500;
  }

  @media (max-width: 700px) {
    .selection-action {
      padding: 0 8px;
      gap: 4px;
    }

    .selection-action span {
      display: none;
    }

    .selection-action.compact span {
      display: none;
    }

    .count-label {
      display: none;
    }
  }

  .menu-section-title {
    display: block;
    font-size: 11px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 6px 12px 4px;
  }

  .menu-option {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: var(--text-secondary, rgba(255, 255, 255, 0.8));
    font-size: 13px;
    cursor: pointer;
    transition: all 0.12s;
    text-align: left;
  }

  .menu-option span {
    flex: 1;
  }

  .menu-option:hover {
    background: var(--bg-hover, rgba(255, 255, 255, 0.08));
  }

  .menu-option.selected {
    color: var(--accent);
    background: var(--accent-alpha, rgba(99, 102, 241, 0.1));
  }

  .menu-option.selected:hover {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.15));
  }

  .menu-divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.08);
    margin: 6px 4px;
  }
</style>
