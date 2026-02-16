<script lang="ts">
  import type { Snippet } from 'svelte';
  import Divider from '$lib/components/ui/Divider.svelte';
  import PageHeader from '$lib/components/layout/PageHeader.svelte';
  import ScrollArea from '$lib/components/ui/ScrollArea.svelte';

  /**
   * Unified page shell that standardises layout, scrolling, headers, and padding
   * for every route in the app.
   *
   * scrollMode:
   *   - 'scroll-area' — wraps children in a <ScrollArea> with optional fade masks
   *   - 'virtual-list' — children manage their own scroll (VirtualList owns the scroll root)
   *   - 'custom'       — no scroll wrapper; the page handles everything inside
   *
   * Header: Use the `header` snippet for full control, OR pass `title`/`subtitle`
   * for the default PageHeader rendering (backward compat).
   *
   * noPadding: When true the shell has no inline padding — useful for pages whose
   * scroll containers need to run edge-to-edge (e.g. VirtualList, ViewStack).
   */
  interface Props {
    /** @deprecated Use header snippet with <PageHeader> instead */
    title?: string;
    /** @deprecated Use header snippet with <PageHeader> instead */
    subtitle?: string;
    scrollMode?: 'scroll-area' | 'virtual-list' | 'custom';
    preserveScrollKey?: string;
    class?: string;
    noPadding?: boolean;
    header?: Snippet;
    toolbar?: Snippet;
    children?: Snippet;
    overlay?: Snippet;
  }

  let {
    title,
    subtitle,
    scrollMode = 'scroll-area',
    preserveScrollKey,
    class: className = '',
    noPadding = false,
    header,
    toolbar,
    children,
    overlay,
  }: Props = $props();

  let scrollAreaRef: ScrollArea | undefined = $state(undefined);

  import { beforeNavigate } from '$app/navigation';
  import { onMount } from 'svelte';

  let initialScrollTop = $state(0);

  function storageKey(key: string) {
    return `page-scroll:${key}`;
  }

  function loadScroll(): number {
    if (!preserveScrollKey) return 0;
    try {
      const raw = sessionStorage.getItem(storageKey(preserveScrollKey));
      return raw ? Number(raw) || 0 : 0;
    } catch {
      return 0;
    }
  }

  function saveScroll(pos: number): void {
    if (!preserveScrollKey) return;
    try {
      sessionStorage.setItem(storageKey(preserveScrollKey), String(pos));
    } catch {}
  }

  beforeNavigate(() => {
    if (scrollMode === 'scroll-area' && preserveScrollKey) {
      const pos = scrollAreaRef?.getScroll() ?? 0;
      saveScroll(pos);
    }
  });

  onMount(() => {
    if (scrollMode === 'scroll-area' && preserveScrollKey) {
      initialScrollTop = loadScroll();
    }
  });

  export function getScrollArea(): ScrollArea | undefined {
    return scrollAreaRef;
  }
</script>

<div class="page-shell {className}" class:no-padding={noPadding}>
  {#if header}
    {@render header()}
  {:else if title}
    <PageHeader {title} {subtitle} />
  {/if}

  {#if toolbar}
    <div class="page-toolbar-slot">
      {@render toolbar()}
    </div>
  {/if}

  {#if scrollMode === 'scroll-area'}
    <ScrollArea bind:this={scrollAreaRef} {initialScrollTop} onscroll={(pos) => saveScroll(pos)}>
      {@render children?.()}
    </ScrollArea>
  {:else}
    <div class="page-content">
      {@render children?.()}
    </div>
  {/if}

  {#if overlay}
    {@render overlay()}
  {/if}
</div>

<style>
  .page-shell {
    padding: 0 0 0 var(--page-padding-inline);
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    position: relative;
    animation: pageIn 0.2s ease-out;
  }

  .page-shell.no-padding {
    padding-left: 0;
  }

  @keyframes pageIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .page-toolbar-slot {
    flex-shrink: 0;
  }

  .page-content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
</style>
