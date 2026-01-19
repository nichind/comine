<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { Snippet } from 'svelte';

  interface Props {
    children?: Snippet;
    class?: string;
    maskSize?: number;
    /** Initial scroll position to restore */
    initialScrollTop?: number;
    /** Callback when scroll position changes (throttled) */
    onscroll?: (position: number) => void;
  }

  let {
    children,
    class: className = '',
    maskSize = 25,
    initialScrollTop,
    onscroll,
  }: Props = $props();

  let scrollContainer: HTMLDivElement;
  let maskStyle = $state('');

  let scrollRAF: number | null = null;
  let lastReportedPosition = 0;

  function updateScrollState() {
    if (!scrollContainer) return;

    const { scrollTop, scrollHeight, clientHeight } = scrollContainer;
    const maxScroll = scrollHeight - clientHeight;

    if (maxScroll <= 0) {
      maskStyle = '';
      return;
    }

    const topProgress = Math.min(scrollTop / maskSize, 1);
    const bottomProgress = Math.min((maxScroll - scrollTop) / maskSize, 1);

    const topFade =
      topProgress > 0 ? `transparent, black ${maskSize * topProgress}px` : 'black, black 0px';
    const bottomFade =
      bottomProgress > 0
        ? `black calc(100% - ${maskSize * bottomProgress}px), transparent`
        : 'black 100%, black 100%';

    maskStyle = `mask-image: linear-gradient(to bottom, ${topFade}, ${bottomFade}); -webkit-mask-image: linear-gradient(to bottom, ${topFade}, ${bottomFade});`;
  }

  function handleScroll() {
    updateScrollState();

    if (scrollRAF !== null) return;

    scrollRAF = requestAnimationFrame(() => {
      scrollRAF = null;
      if (!scrollContainer) return;

      const position = scrollContainer.scrollTop;

      if (Math.abs(position - lastReportedPosition) > 5) {
        lastReportedPosition = position;
        onscroll?.(position);
      }
    });
  }

  export function restoreScroll(position: number): void {
    if (position <= 0) return;

    const doRestore = () => {
      if (scrollContainer) {
        scrollContainer.scrollTop = position;
        lastReportedPosition = position;
        updateScrollState();
      }
    };

    if (scrollContainer) {
      doRestore();
    }

    requestAnimationFrame(() => {
      doRestore();
      requestAnimationFrame(doRestore);
    });
  }

  export function getScroll(): number {
    return scrollContainer?.scrollTop ?? 0;
  }

  export function scrollToTop(smooth = false): void {
    if (!scrollContainer) return;
    scrollContainer.scrollTo({
      top: 0,
      behavior: smooth ? 'smooth' : 'instant',
    });
  }

  onMount(() => {
    let resizeObserver: ResizeObserver | null = null;

    tick().then(() => {
      setTimeout(updateScrollState, 50);

      resizeObserver = new ResizeObserver(() => {
        updateScrollState();
      });
      if (scrollContainer) {
        resizeObserver.observe(scrollContainer);
      }

      if (typeof initialScrollTop === 'number' && initialScrollTop > 0) {
        restoreScroll(initialScrollTop);
      }
    });

    return () => {
      resizeObserver?.disconnect();
      if (scrollRAF !== null) {
        cancelAnimationFrame(scrollRAF);
      }
    };
  });
</script>

<div class="scroll-area-wrapper {className}">
  <div class="scroll-area" bind:this={scrollContainer} onscroll={handleScroll} style={maskStyle}>
    {@render children?.()}
  </div>
</div>

<style>
  .scroll-area-wrapper {
    position: relative;
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .scroll-area {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    /* Reserve scrollbar space to prevent layout shift during animations */
    scrollbar-gutter: stable;
    /* 4px from right/bottom edge of window */
    margin-right: 4px;
    margin-bottom: 4px;
    /* 6px gap between content and scrollbar */
    padding-right: 6px;
    /* Improve scroll performance */
    will-change: scroll-position;
    -webkit-overflow-scrolling: touch;
    contain: strict;
  }

  @media (max-width: 480px) {
    .scroll-area {
      padding-right: 4px;
      padding-bottom: 100px;
    }
  }
</style>
