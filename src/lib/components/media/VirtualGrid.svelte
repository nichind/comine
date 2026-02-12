<script lang="ts" generics="T">
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import type { Snippet } from 'svelte';
  import { preserveScroll } from '$lib/actions/preserveScroll';
  import { edgeMask } from '$lib/actions/edgeMask';

  interface Props {
    items: T[];
    totalCount?: number; // For infinite scroll - total including unfetched
    viewMode?: 'list' | 'grid';

    gap?: number;
    listRowHeight?: number;
    gridMinColumnWidth?: number;
    getGridRowHeight?: (colWidth: number) => number;

    overscan?: number;
    preserveScrollKey?: string;
    onscroll?: (scrollTop: number, scrollHeight: number, clientHeight: number) => void;
    getKey?: (item: T, index: number) => string | number;

    children: Snippet<[T | undefined, number, boolean]>;
    header?: Snippet;
    footer?: Snippet;
    empty?: Snippet; // When totalCount === 0
  }

  let {
    items,
    totalCount = undefined,
    viewMode = 'list',
    gap = 10,
    listRowHeight = 56,
    gridMinColumnWidth = 150,
    getGridRowHeight = (w) => Math.round((w * 9) / 16 + 60), // Default 16:9 + 60px info
    overscan = 5,
    preserveScrollKey,
    onscroll,
    getKey,
    children,
    header,
    footer,
    empty,
  }: Props = $props();

  let scrollElement: HTMLDivElement | null = $state(null);
  let containerWidth = $state(800);

  let count = $derived(totalCount ?? items.length);

  let itemsPerRow = $derived.by(() => {
    if (viewMode === 'list') return 1;
    if (containerWidth <= 0) return 1;
    const perRow = Math.floor((containerWidth + gap) / (gridMinColumnWidth + gap));
    return Math.max(1, perRow);
  });

  let rowCount = $derived(Math.ceil(count / itemsPerRow));

  let colWidth = $derived.by(() => {
    if (viewMode === 'list') return containerWidth;
    if (itemsPerRow <= 1) return containerWidth;
    return (containerWidth - gap * (itemsPerRow - 1)) / itemsPerRow;
  });

  let rowHeight = $derived.by(() => {
    if (viewMode === 'list') return listRowHeight;
    return getGridRowHeight(colWidth) + gap; // Include gap in row height for grid
  });

  const virtualizer = createVirtualizer({
    get count() {
      return rowCount;
    },
    getScrollElement: () => scrollElement,
    estimateSize: () => rowHeight,
    get overscan() {
      return overscan;
    },
    getItemKey: (index) => `row_${index}`,
  });

  let prevRowCount = $state(0);
  let prevRowHeight = $state(0);

  $effect(() => {
    const el = scrollElement;
    const rc = rowCount;
    const rh = rowHeight;

    if (el && (rc !== prevRowCount || rh !== prevRowHeight)) {
      $virtualizer.setOptions({
        getScrollElement: () => el,
        count: rc,
        estimateSize: () => rh,
      });
      $virtualizer.measure();
      prevRowCount = rc;
      prevRowHeight = rh;
    }
  });

  function handleScroll() {
    if (scrollElement && onscroll) {
      const { scrollTop, scrollHeight, clientHeight } = scrollElement;
      onscroll(scrollTop, scrollHeight, clientHeight);
    }
  }

  function getItemKey(index: number): string | number {
    if (index >= items.length) return `__placeholder_${index}`;
    const item = items[index];
    if (getKey && item !== undefined) return getKey(item, index);
    return index;
  }

  export function scrollToTop() {
    $virtualizer.scrollToIndex(0, { align: 'start' });
  }

  export function scrollToIndex(index: number, options?: { align?: 'start' | 'center' | 'end' }) {
    const rowIndex = Math.floor(index / itemsPerRow);
    $virtualizer.scrollToIndex(rowIndex, options);
  }

  export function getScrollElement() {
    return scrollElement;
  }

  export function getScrollTop(): number {
    return scrollElement?.scrollTop ?? 0;
  }

  export function refresh() {
    $virtualizer.measure();
  }
</script>

<div class="virtual-grid-root" style:--gap="{gap}px" style:--cols={itemsPerRow}>
  {#if header}
    <div class="header">
      {@render header()}
    </div>
  {/if}

  {#if count === 0 && empty}
    <div class="empty-container">
      {@render empty()}
    </div>
  {:else}
    <div
      class="scroll-container"
      bind:this={scrollElement}
      bind:clientWidth={containerWidth}
      onscroll={handleScroll}
      use:edgeMask
      use:preserveScroll={preserveScrollKey ? { key: preserveScrollKey } : undefined}
    >
      <div class="virtual-inner" style:height="{$virtualizer.getTotalSize()}px">
        {#each $virtualizer.getVirtualItems() as row (row.key)}
          <div
            class="virtual-row"
            class:grid-row={viewMode === 'grid'}
            class:list-row={viewMode === 'list'}
            style:top="{row.start}px"
            style:height="{viewMode === 'list' ? row.size : row.size - gap}px"
          >
            {#each Array(itemsPerRow) as _, cIndex}
              {@const itemIndex = row.index * itemsPerRow + cIndex}
              {#if itemIndex < count}
                {@const item = itemIndex < items.length ? items[itemIndex] : undefined}
                {@const isPlaceholder = item === undefined}
                <div
                  class="virtual-cell"
                  class:list-cell={viewMode === 'list'}
                  class:grid-cell={viewMode === 'grid'}
                >
                  {@render children(item, itemIndex, isPlaceholder)}
                </div>
              {/if}
            {/each}
          </div>
        {/each}
      </div>

      {#if footer}
        <div class="footer">
          {@render footer()}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .virtual-grid-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    position: relative;
  }

  .header {
    flex-shrink: 0;
  }

  .scroll-container {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    position: relative;
  }

  .virtual-inner {
    width: 100%;
    position: relative;
  }

  .virtual-row {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    box-sizing: border-box;
  }

  .virtual-row.list-row {
    display: block;
  }

  .virtual-row.grid-row {
    display: grid;
    grid-template-columns: repeat(var(--cols), 1fr);
    gap: var(--gap);
  }

  .virtual-cell {
    min-width: 0; /* Allow text truncation */
  }

  .virtual-cell.list-cell {
    width: 100%;
  }

  .empty-container {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .footer {
    flex-shrink: 0;
  }
</style>
