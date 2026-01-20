<script lang="ts" generics="T">
  import { onMount, onDestroy, tick } from 'svelte';
  import type { Snippet } from 'svelte';

  interface Props {
    items: T[];
    estimatedItemHeight: number;
    overscan?: number;
    containerClass?: string;
    onscroll?: () => void;
    getKey?: (item: T, index: number) => string | number;
    children: Snippet<[T, number]>;
    header?: Snippet;
    footer?: Snippet;
    useFadeMask?: boolean;
    useCustomScrollbar?: boolean;
    getItemSize?: (index: number, item: T) => number | undefined;
  }

  let {
    items,
    estimatedItemHeight,
    overscan = 5,
    containerClass = '',
    onscroll,
    getKey,
    children,
    header,
    footer,
    useFadeMask = false,
    useCustomScrollbar = false,
    getItemSize,
  }: Props = $props();

  let container: HTMLElement | null = $state(null);
  let innerContainer: HTMLElement | null = $state(null);
  let scrollTop = $state(0);
  let containerHeight = $state(0);
  let renderTrigger = $state(0);

  const heightCache = new Map<string | number, number>();
  const MAX_HEIGHT_CACHE_SIZE = 200;
  let lastItemCount = 0;

  let prefixSums: number[] = [];
  let prefixSumsValid = false;
  let dirtyRangeStart = 0;

  function getItemKey(item: T, index: number): string | number {
    if (getKey) return getKey(item, index);
    if (item && typeof item === 'object' && 'id' in item) {
      return (item as { id: string | number }).id;
    }
    return index;
  }

  function getItemHeight(index: number): number {
    if (index < 0 || index >= items.length) return estimatedItemHeight;
    
    if (getItemSize) {
        const explicit = getItemSize(index, items[index]);
        if (explicit !== undefined) return explicit;
    }

    const key = getItemKey(items[index], index);
    return heightCache.get(key) ?? estimatedItemHeight;
  }

  function rebuildPrefixSums(): void {
    const n = items.length;
    
    if (!prefixSums.length || prefixSums.length !== n + 1) {
      prefixSums = new Array(n + 1);
      prefixSums[0] = 0;
      for (let i = 0; i < n; i++) {
        prefixSums[i + 1] = prefixSums[i] + getItemHeight(i);
      }
    } else {
      for (let i = dirtyRangeStart; i < n; i++) {
        prefixSums[i + 1] = prefixSums[i] + getItemHeight(i);
      }
    }
    
    prefixSumsValid = true;
    dirtyRangeStart = n;
  }

  function ensurePrefixSums(): void {
    if (!prefixSumsValid || prefixSums.length !== items.length + 1) {
      rebuildPrefixSums();
    }
  }

  function getOffsetForIndex(index: number): number {
    ensurePrefixSums();
    return prefixSums[Math.min(index, items.length)] ?? 0;
  }

  function getTotalHeight(): number {
    ensurePrefixSums();
    return prefixSums[items.length] ?? 0;
  }

  function findIndexByOffset(targetOffset: number): number {
    ensurePrefixSums();
    let lo = 0, hi = items.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (prefixSums[mid + 1] <= targetOffset) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }
    return lo;
  }

  function calculateVisibleRange(): { start: number; end: number } {
    const totalItems = items.length;
    if (totalItems === 0 || containerHeight === 0) {
      return { start: 0, end: 0 };
    }

    ensurePrefixSums();

    let start = findIndexByOffset(scrollTop);
    start = Math.max(0, start - overscan);

    const endOffset = scrollTop + containerHeight + overscan * estimatedItemHeight;
    let end = findIndexByOffset(endOffset);
    end = Math.min(totalItems, end + overscan + 1);

    return { start, end };
  }

  function invalidatePrefixSums(fromIndex: number = 0): void {
    prefixSumsValid = false;
    dirtyRangeStart = Math.min(dirtyRangeStart, fromIndex);
  }

  let visibleRange = $derived.by(() => {
    void renderTrigger;
    void items.length;
    void scrollTop;
    void containerHeight;
    return calculateVisibleRange();
  });

  let visibleItems = $derived.by(() => {
    const { start, end } = visibleRange;
    return items.slice(start, end).map((item, i) => ({
      item,
      index: start + i,
      key: getItemKey(item, start + i),
    }));
  });

  let totalHeight = $derived.by(() => {
    void renderTrigger;
    void items.length;
    return getTotalHeight();
  });

  let topPadding = $derived.by(() => {
    void renderTrigger;
    return getOffsetForIndex(visibleRange.start);
  });

  const MASK_SIZE = 32;
  let topMaskHeight = $state(0);
  let bottomMaskHeight = $state(0);

  function updateMaskStyle() {
    if (!useFadeMask || !container) {
      topMaskHeight = 0;
      bottomMaskHeight = 0;
      return;
    }
    const { scrollTop: st, scrollHeight, clientHeight } = container;
    const maxScroll = scrollHeight - clientHeight;
    
    topMaskHeight = st > 0 ? Math.min(st, MASK_SIZE) : 0;
    bottomMaskHeight = (maxScroll > 0 && st < maxScroll) ? Math.min(maxScroll - st, MASK_SIZE) : 0;
  }

  function handleScroll() {
    if (container) {
      scrollTop = container.scrollTop;
      if (useFadeMask) updateMaskStyle();
    }
    onscroll?.();
  }

  let lastMeasuredWidth = 0;

  function measureItems() {
    if (!innerContainer || !container) return;
    
    if (getItemSize && visibleItems.length > 0) {
        let allExplicit = true;
        for (const { index, item } of visibleItems) {
            if (getItemSize(index, item) === undefined) {
                allExplicit = false;
                break;
            }
        }
        if (allExplicit) return;
    }

    const currentWidth = container.clientWidth;
    const isWidthSame = Math.abs(currentWidth - lastMeasuredWidth) < 1;
    
    if (!isWidthSame) {
        lastMeasuredWidth = currentWidth;
    }

    const itemElements = innerContainer.querySelectorAll('[data-virtual-key]');
    let hasChanges = false;

    itemElements.forEach((el) => {
      const key = el.getAttribute('data-virtual-key');
      if (key === null) return;

      if (isWidthSame && heightCache.has(key)) return;
      
      const rect = el.getBoundingClientRect();
      const height = rect.height;

      if (height > 0) {
        const existingHeight = heightCache.get(key);
        if (existingHeight === undefined || Math.abs(existingHeight - height) > 1) {
          heightCache.set(key, height);
          hasChanges = true;
        }
      }
    });

    if (hasChanges) {
      invalidatePrefixSums();
      renderTrigger++;
    }
  }

  let measureScheduled = false;
  function scheduleMeasurement() {
    if (measureScheduled) return;
    measureScheduled = true;
    requestAnimationFrame(() => {
      measureScheduled = false;
      measureItems();
    });
  }

  $effect(() => {
    if (visibleItems.length > 0) {
      tick().then(scheduleMeasurement);
    }
  });

  $effect(() => {
    const currentCount = items.length;
    invalidatePrefixSums();
    
    if (currentCount === 0) {
      heightCache.clear();
    } else if (
      heightCache.size > MAX_HEIGHT_CACHE_SIZE ||
      Math.abs(currentCount - lastItemCount) > 50
    ) {
      const currentKeys = new Set<string | number>();
      for (let i = 0; i < items.length; i++) {
        currentKeys.add(getItemKey(items[i], i));
      }
      for (const key of heightCache.keys()) {
        if (!currentKeys.has(key)) {
          heightCache.delete(key);
        }
      }
    }
    lastItemCount = currentCount;
  });

  let resizeObserver: ResizeObserver | null = null;
  let lastContainerWidth = 0;

  onMount(() => {
    if (container) {
      containerHeight = container.clientHeight;
      lastContainerWidth = container.clientWidth;

      resizeObserver = new ResizeObserver((entries) => {
        let widthChanged = false;
        
        for (const entry of entries) {
          if (entry.target === container) {
            const h = entry.contentRect.height;
            
            if (h > containerHeight || Math.abs(h - containerHeight) > 5) {
                containerHeight = h;
            }
            
            if (Math.abs(entry.contentRect.width - lastContainerWidth) > 1) {
              lastContainerWidth = entry.contentRect.width;
              widthChanged = true;
            }

            if (useFadeMask) requestAnimationFrame(updateMaskStyle);
          }
        }
        
        if (widthChanged) {
          scheduleMeasurement();
        }
      });
      resizeObserver.observe(container);
    }
    
    if (useFadeMask) {
      setTimeout(updateMaskStyle, 50);
    }
  });

  onDestroy(() => {
    resizeObserver?.disconnect();
  });

  export function scrollToTop() {
    if (container) {
      container.scrollTop = 0;
    }
  }

  export function scrollToBottom() {
    if (container) {
      container.scrollTop = container.scrollHeight;
    }
  }

  export function getScrollTop(): number {
    return container?.scrollTop ?? 0;
  }

  export function setScrollTop(value: number) {
    if (container) {
      container.scrollTop = value;
    }
  }

  export function refresh() {
    renderTrigger++;
  }
</script>

<div class="virtual-list-wrapper {containerClass}">
  {#if header}
    <div class="virtual-list-header" class:custom-scrollbar={useCustomScrollbar}>
      {@render header()}
    </div>
  {/if}

  <div 
    class="virtual-list-scroll"
    bind:this={container} 
    onscroll={handleScroll}
    class:custom-scrollbar={useCustomScrollbar}
    style="--mask-t: {topMaskHeight}px; --mask-b: {bottomMaskHeight}px;"
  >
    <div
      class="virtual-list-inner"
      style="height: {totalHeight}px; padding-top: {topPadding}px;"
      bind:this={innerContainer}
    >
      {#each visibleItems as { item, index, key } (key)}
        <div class="virtual-list-item" data-virtual-key={key}>
          {@render children(item, index)}
        </div>
      {/each}
    </div>

    {#if footer}
      <div class="virtual-list-footer">
        {@render footer()}
      </div>
    {/if}
  </div>
</div>

<style>
  .virtual-list-wrapper {
    display: flex;
    flex-direction: column;
    height: 100%;
    position: relative;
    overflow: hidden;
  }

  .virtual-list-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    position: relative;
    will-change: scroll-position;
    
    mask-image: linear-gradient(
      to bottom,
      transparent 0px,
      black var(--mask-t),
      black calc(100% - var(--mask-b)),
      transparent 100%
    );
    -webkit-mask-image: linear-gradient(
      to bottom,
      transparent 0px,
      black var(--mask-t),
      black calc(100% - var(--mask-b)),
      transparent 100%
    );
  }

  .virtual-list-scroll.custom-scrollbar {
    padding-right: 6px;
    margin-right: 4px;
  }

  .virtual-list-header {
    flex: 0 0 auto;
    z-index: 10;
    position: relative; 
  }

  .virtual-list-header.custom-scrollbar {
    padding-right: 6px;
    margin-right: 4px;
  }

  /* No explicit sticky needed for header in flex column layout */

  .virtual-list-inner {
    position: relative;
    box-sizing: border-box;
    contain: strict;
  }

  .virtual-list-item {
    box-sizing: border-box;
    contain: layout style;
  }
</style>
