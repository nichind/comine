<script lang="ts">
  import { portal } from '$lib/actions/portal';
  import type { Snippet } from 'svelte';

  interface Props {
    open: boolean;
    anchorEl: HTMLElement | null;
    onclose: () => void;
    children: Snippet;
    minWidth?: number;
    align?: 'left' | 'right' | 'center';
  }

  let {
    open = $bindable(false),
    anchorEl,
    onclose,
    children,
    minWidth = 140,
    align = 'left',
  }: Props = $props();

  let dropdownStyle = $state('');
  let scrollParent: HTMLElement | null = null;

  $effect(() => {
    if (open && anchorEl) {
      positionDropdown();
      addScrollListener();
      // Delay adding click listener to avoid immediate close
      requestAnimationFrame(() => {
        document.addEventListener('click', handleClickOutside, true);
      });
    } else {
      removeScrollListener();
      document.removeEventListener('click', handleClickOutside, true);
    }
    
    return () => {
      removeScrollListener();
      document.removeEventListener('click', handleClickOutside, true);
    };
  });

  function positionDropdown() {
    if (!anchorEl) return;
    const rect = anchorEl.getBoundingClientRect();

    const dropdownWidth = Math.max(minWidth, rect.width);
    const estimatedHeight = 120; // Rough estimate for 2-3 options

    const spaceBelow = window.innerHeight - rect.bottom - 8;
    const spaceAbove = rect.top - 8;

    let top: number;
    let maxHeight: number;

    // Prefer below, fallback to above
    if (spaceBelow >= estimatedHeight || spaceBelow >= spaceAbove) {
      top = rect.bottom + 4;
      maxHeight = Math.min(300, spaceBelow - 4);
    } else {
      top = rect.top - estimatedHeight - 4;
      maxHeight = Math.min(300, spaceAbove - 4);
    }

    // Horizontal positioning
    let left: number;
    if (align === 'right') {
      left = rect.right - dropdownWidth;
    } else if (align === 'center') {
      left = rect.left + (rect.width - dropdownWidth) / 2;
    } else {
      left = rect.left;
    }

    // Clamp to viewport
    if (left + dropdownWidth > window.innerWidth - 8) {
      left = window.innerWidth - dropdownWidth - 8;
    }
    if (left < 8) left = 8;

    dropdownStyle = `
      top: ${top}px;
      left: ${left}px;
      min-width: ${dropdownWidth}px;
      max-height: ${maxHeight}px;
    `;
  }

  function findScrollParent(element: HTMLElement | null): HTMLElement | null {
    if (!element) return null;
    const style = getComputedStyle(element);
    if (
      style.overflow === 'auto' ||
      style.overflow === 'scroll' ||
      style.overflowY === 'auto' ||
      style.overflowY === 'scroll'
    ) {
      return element;
    }
    return findScrollParent(element.parentElement);
  }

  function addScrollListener() {
    if (!anchorEl) return;
    scrollParent = findScrollParent(anchorEl);
    if (scrollParent) {
      scrollParent.addEventListener('scroll', handleScroll, { passive: true });
    }
    window.addEventListener('scroll', handleScroll, { passive: true });
    window.addEventListener('resize', handleResize, { passive: true });
  }

  function removeScrollListener() {
    if (scrollParent) {
      scrollParent.removeEventListener('scroll', handleScroll);
      scrollParent = null;
    }
    window.removeEventListener('scroll', handleScroll);
    window.removeEventListener('resize', handleResize);
  }

  function handleScroll() {
    if (open) {
      onclose();
    }
  }

  function handleResize() {
    if (open) {
      positionDropdown();
    }
  }

  function handleClickOutside(e: MouseEvent) {
    const target = e.target as HTMLElement;
    // Check if click is outside both the dropdown and the anchor
    if (anchorEl && !anchorEl.contains(target)) {
      const dropdownEl = document.querySelector('.dropdown-menu');
      if (!dropdownEl?.contains(target)) {
        onclose();
      }
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && open) {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div use:portal>
    <div class="dropdown-menu" style={dropdownStyle} role="menu">
      {@render children()}
    </div>
  </div>
{/if}

<style>
  .dropdown-menu {
    position: fixed;
    z-index: 1100;
    background: var(--bg-elevated, rgba(30, 30, 30, 0.98));
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
    border-radius: 8px;
    padding: 4px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
    overflow-y: auto;
    animation: dropdownIn 0.12s ease-out;
  }

  @keyframes dropdownIn {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
</style>
