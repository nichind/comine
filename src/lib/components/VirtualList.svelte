<script lang="ts" generics="T">
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import type { Snippet } from 'svelte';
  import { preserveScroll } from '$lib/actions/preserveScroll';

  interface Props {
    items: T[];
    estimatedItemHeight: number;
    overscan?: number;
    containerClass?: string;
    onscroll?: () => void;
    getKey?: (item: T, index: number) => string | number;
    getItemSize?: (index: number, item: T) => number | undefined;
    measureItems?: boolean;
    children: Snippet<[T, number]>;
    header?: Snippet;
    footer?: Snippet;
    useFadeMask?: boolean;
    useCustomScrollbar?: boolean;
    preserveScrollKey?: string;
    preserveScrollThrottleMs?: number;
  }

  let {
    items,
    estimatedItemHeight,
    overscan = 5,
    containerClass = '',
    onscroll,
    getKey,
    getItemSize,
    measureItems = false,
    children,
    header,
    footer,
    useFadeMask = false,
    useCustomScrollbar = false,
    preserveScrollKey,
    preserveScrollThrottleMs,
  }: Props = $props();

  function measure(node: Element, enabled: boolean) {
    if (enabled) {
      $virtualizer.measureElement(node as any);
    }

    return {
      update(nextEnabled: boolean) {
        if (nextEnabled) {
          $virtualizer.measureElement(node as any);
        }
      },
    };
  }

  let scrollElement: HTMLDivElement | null = $state(null);

  const virtualizer = createVirtualizer({
    get count() {
      return items.length;
    },
    getScrollElement: () => scrollElement,
    estimateSize: (index: number) => {
      if (getItemSize && items[index]) {
        const size = getItemSize(index, items[index]);
        if (size !== undefined) return size;
      }
      return estimatedItemHeight;
    },
    get overscan() {
      return overscan;
    },
    getItemKey: (index: number) => {
      if (getKey && items[index]) {
        return getKey(items[index], index);
      }
      return index;
    },
  });

  let prevHeight = $state(0);
  let prevCount = $state(0);

  $effect(() => {
    const count = items.length;
    const height = estimatedItemHeight;
    const el = scrollElement;

    if (el && count > 0) {
      const heightChanged = height !== prevHeight;
      const countChanged = count !== prevCount;

      $virtualizer.setOptions({
        getScrollElement: () => el,
        count,
        estimateSize: (index: number) => {
          if (getItemSize && items[index]) {
            const size = getItemSize(index, items[index]);
            if (size !== undefined) return size;
          }
          return height;
        },
        getItemKey: (index: number) => {
          if (getKey && items[index]) {
            return getKey(items[index], index);
          }
          return index;
        },
      });

      if (heightChanged) {
        prevHeight = height;
        $virtualizer.measure();
      }
      prevCount = count;
    }
  });

  function handleScroll() {
    onscroll?.();
  }

  const MASK_SIZE = 32;
  let topMaskHeight = $state(0);
  let bottomMaskHeight = $state(0);

  function updateMaskStyle() {
    if (!useFadeMask || !scrollElement) {
      topMaskHeight = 0;
      bottomMaskHeight = 0;
      return;
    }
    const { scrollTop: st, scrollHeight, clientHeight } = scrollElement;
    const maxScroll = scrollHeight - clientHeight;

    topMaskHeight = st > 0 ? Math.min(st, MASK_SIZE) : 0;
    bottomMaskHeight = maxScroll > 0 && st < maxScroll ? Math.min(maxScroll - st, MASK_SIZE) : 0;
  }

  $effect(() => {
    if (scrollElement && useFadeMask) {
      updateMaskStyle();
    }
  });

  function onScrollHandler() {
    if (useFadeMask) updateMaskStyle();
    handleScroll();
  }

  export function scrollToTop() {
    $virtualizer.scrollToIndex(0, { align: 'start' });
  }

  export function scrollToBottom() {
    $virtualizer.scrollToIndex(items.length - 1, { align: 'end' });
  }

  export function scrollToIndex(index: number, options?: { align?: 'start' | 'center' | 'end' }) {
    $virtualizer.scrollToIndex(index, options);
  }

  export function getScrollTop(): number {
    return scrollElement?.scrollTop ?? 0;
  }

  export function setScrollTop(value: number) {
    if (scrollElement) {
      scrollElement.scrollTop = value;
    }
  }

  export function refresh() {
    $virtualizer.measure();
  }

  export function invalidateHeights(_keys?: (string | number)[]) {
    $virtualizer.measure();
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
    bind:this={scrollElement}
    onscroll={onScrollHandler}
    class:custom-scrollbar={useCustomScrollbar}
    style="--mask-t: {topMaskHeight}px; --mask-b: {bottomMaskHeight}px;"
    use:preserveScroll={preserveScrollKey
      ? { key: preserveScrollKey, throttleMs: preserveScrollThrottleMs }
      : undefined}
  >
    <div
      class="virtual-list-inner"
      style="height: {$virtualizer.getTotalSize()}px; width: 100%; position: relative;"
    >
      {#each $virtualizer.getVirtualItems() as row (row.key)}
        {@const item = items[row.index]}
        {#if item}
          <div
            class="virtual-list-item"
            data-index={row.index}
            style="position: absolute; top: 0; left: 0; width: 100%; transform: translateY({row.start}px);"
            use:measure={measureItems}
          >
            {@render children(item, row.index)}
          </div>
        {/if}
      {/each}
    </div>

    {#if footer}
      <div class="virtual-list-footer" style="position: relative;">
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
    padding-right: 2px;
    margin-right: 0;
  }

  .virtual-list-header {
    flex: 0 0 auto;
    z-index: 10;
    position: relative;
  }

  .virtual-list-header.custom-scrollbar {
    padding-right: 4px;
    margin-right: 0;
  }

  .virtual-list-inner {
    position: relative;
    box-sizing: border-box;
  }

  .virtual-list-item {
    box-sizing: border-box;
  }

  .virtual-list-footer {
    flex-shrink: 0;
  }

  @media (max-width: 700px) {
    .virtual-list-scroll.custom-scrollbar,
    .virtual-list-header.custom-scrollbar {
      padding-right: 0;
    }
  }
</style>
