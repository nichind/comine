<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { Snippet } from 'svelte';
  import { edgeMask } from '$lib/actions/edgeMask';

  interface Props {
    children?: Snippet;
    class?: string;
    maskSize?: number;
    initialScrollTop?: number;
    onscroll?: (position: number) => void;
  }

  let {
    children,
    class: className = '',
    maskSize = 28,
    initialScrollTop,
    onscroll,
  }: Props = $props();

  let scrollContainer: HTMLDivElement;

  let scrollRAF: number | null = null;
  let lastReportedPosition = 0;

  function handleScroll() {
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
    tick().then(() => {
      if (typeof initialScrollTop === 'number' && initialScrollTop > 0) {
        restoreScroll(initialScrollTop);
      }
    });

    return () => {
      if (scrollRAF !== null) {
        cancelAnimationFrame(scrollRAF);
      }
    };
  });
</script>

<div class="scroll-area-wrapper {className}">
  <div
    class="scroll-area"
    bind:this={scrollContainer}
    onscroll={handleScroll}
    use:edgeMask={{ size: maskSize }}
  >
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
    scrollbar-gutter: stable;
    margin-bottom: 4px;
    padding-right: 6px;
    will-change: scroll-position;
    -webkit-overflow-scrolling: touch;
  }

  @media (max-width: 480px) {
    .scroll-area {
      padding-right: 4px;
      padding-bottom: 100px;
    }
  }
</style>
