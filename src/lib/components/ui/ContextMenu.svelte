<script lang="ts">
  import { portal } from '$lib/actions/portal';
  import Icon, { type IconName } from '$lib/components/ui/Icon.svelte';

  export interface MenuItem {
    id: string;
    label: string;
    icon?: IconName;
    danger?: boolean;
    disabled?: boolean;
    divider?: boolean;
    shortcut?: string;
  }

  interface Props {
    open: boolean;
    x: number;
    y: number;
    items: MenuItem[];
    onclose: () => void;
    onselect: (id: string) => void;
  }

  let { open = $bindable(false), x, y, items, onclose, onselect }: Props = $props();

  let menuStyle = $state('');
  let menuEl: HTMLDivElement | null = $state(null);
  let focusedIndex = $state(-1);

  $effect(() => {
    if (open) {
      focusedIndex = -1;
      requestAnimationFrame(() => positionMenu());
      requestAnimationFrame(() => {
        document.addEventListener('click', handleClickOutside, true);
        document.addEventListener('contextmenu', handleClickOutside, true);
      });
    } else {
      document.removeEventListener('click', handleClickOutside, true);
      document.removeEventListener('contextmenu', handleClickOutside, true);
    }

    return () => {
      document.removeEventListener('click', handleClickOutside, true);
      document.removeEventListener('contextmenu', handleClickOutside, true);
    };
  });

  function positionMenu() {
    const MENU_PADDING = 12;
    const ITEM_HEIGHT = 32;
    const DIVIDER_HEIGHT = 9;
    const MOBILE_ITEM_HEIGHT = 44;

    const isMobile = window.matchMedia('(max-width: 480px), (hover: none)').matches;

    const MENU_WIDTH = isMobile ? Math.min(240, window.innerWidth - 24) : 200;
    const itemHeight = isMobile ? MOBILE_ITEM_HEIGHT : ITEM_HEIGHT;

    const itemCount = items.filter((i) => !i.divider).length;
    const dividerCount = items.filter((i) => i.divider).length;
    const estimatedHeight = itemCount * itemHeight + dividerCount * DIVIDER_HEIGHT + 12;

    const viewWidth = window.innerWidth;
    const viewHeight = window.innerHeight;

    let left = x;
    let top = y;

    if (left + MENU_WIDTH > viewWidth - MENU_PADDING) {
      left = x - MENU_WIDTH;
    }
    if (left < MENU_PADDING) {
      left = MENU_PADDING;
    }

    if (top + estimatedHeight > viewHeight - MENU_PADDING) {
      top = Math.max(MENU_PADDING, y - estimatedHeight);
    }

    if (top + estimatedHeight > viewHeight - MENU_PADDING) {
      top = viewHeight - estimatedHeight - MENU_PADDING;
    }

    top = Math.max(MENU_PADDING, top);

    menuStyle = `left: ${left}px; top: ${top}px; width: ${MENU_WIDTH}px;`;
  }

  function handleClickOutside(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (menuEl && !menuEl.contains(target)) {
      onclose();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!open) return;

    const actionableItems = items.filter((i) => !i.divider && !i.disabled);

    switch (e.key) {
      case 'Escape':
        e.preventDefault();
        e.stopPropagation();
        onclose();
        break;
      case 'ArrowDown':
        e.preventDefault();
        focusedIndex = (focusedIndex + 1) % actionableItems.length;
        break;
      case 'ArrowUp':
        e.preventDefault();
        focusedIndex = focusedIndex <= 0 ? actionableItems.length - 1 : focusedIndex - 1;
        break;
      case 'Enter':
        e.preventDefault();
        if (focusedIndex >= 0 && focusedIndex < actionableItems.length) {
          handleItemClick(actionableItems[focusedIndex]);
        }
        break;
    }
  }

  function handleItemClick(item: MenuItem) {
    if (item.disabled || item.divider) return;
    onselect(item.id);
    onclose();
  }

  function getItemIndex(item: MenuItem): number {
    return items.filter((i) => !i.divider && !i.disabled).indexOf(item);
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div use:portal>
    <div
      class="context-menu-backdrop"
      onclick={onclose}
      onkeydown={(e) => e.key === 'Escape' && onclose()}
      role="presentation"
    ></div>
    <div bind:this={menuEl} class="context-menu" style={menuStyle} role="menu" tabindex="-1">
      {#each items as item}
        {#if item.divider}
          <div class="divider"></div>
        {:else}
          <button
            class="item"
            class:danger={item.danger}
            class:disabled={item.disabled}
            class:focused={getItemIndex(item) === focusedIndex}
            role="menuitem"
            disabled={item.disabled}
            onclick={() => handleItemClick(item)}
            onmouseenter={() => (focusedIndex = getItemIndex(item))}
          >
            {#if item.icon}
              <span class="item-icon">
                <Icon name={item.icon} size={15} />
              </span>
            {/if}
            <span class="item-label">{item.label}</span>
            {#if item.shortcut}
              <span class="item-shortcut">{item.shortcut}</span>
            {/if}
          </button>
        {/if}
      {/each}
    </div>
  </div>
{/if}

<style>
  .context-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1099;
  }

  .context-menu {
    position: fixed;
    z-index: 1100;
    background: var(--surface-bg, rgba(18, 18, 18, 0.75));
    backdrop-filter: blur(var(--surface-blur, 16px));
    -webkit-backdrop-filter: blur(var(--surface-blur, 16px));
    border: 1px solid var(--surface-border, rgba(255, 255, 255, 0.12));
    border-radius: var(--radius, 10px);
    padding: 6px;
    box-shadow: var(--surface-shadow, 0 8px 24px rgba(0, 0, 0, 0.25));
    animation: menuAppear 0.12s ease-out;
  }

  @keyframes menuAppear {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(-4px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: var(--surface-text, rgba(255, 255, 255, 0.85));
    font-size: var(--text-base, 13px);
    font-weight: 400;
    cursor: pointer;
    transition: all 0.15s ease;
    text-align: left;
  }

  .item:hover:not(.disabled),
  .item.focused:not(.disabled) {
    background: var(--surface-bg-hover, rgba(255, 255, 255, 0.1));
    color: white;
  }

  .item:hover:not(.disabled) .item-icon,
  .item.focused:not(.disabled) .item-icon {
    color: rgba(255, 255, 255, 0.9);
  }

  .item:hover:not(.disabled) .item-shortcut,
  .item.focused:not(.disabled) .item-shortcut {
    color: rgba(255, 255, 255, 0.5);
  }

  .item.danger:hover:not(.disabled),
  .item.danger.focused:not(.disabled) {
    background: rgba(239, 68, 68, 0.15);
    color: #f87171;
  }

  .item.disabled {
    color: var(--surface-text-muted, rgba(255, 255, 255, 0.3));
    cursor: not-allowed;
  }

  .item-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    color: rgba(255, 255, 255, 0.5);
    transition: color 0.15s;
  }

  .item-label {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-shortcut {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.35);
    font-weight: 500;
    transition: color 0.08s;
  }

  .divider {
    height: 1px;
    background: var(--surface-border, rgba(255, 255, 255, 0.08));
    margin: 4px 8px;
  }

  @media (max-width: 480px), (hover: none) {
    .context-menu-backdrop {
      background: rgba(0, 0, 0, 0.3);
    }

    .context-menu {
      min-width: 180px;
      max-width: min(240px, calc(100vw - 24px));
      border-radius: var(--radius-lg, 12px);
      padding: 6px;
      background: var(--surface-bg, rgba(18, 18, 18, 0.92));
      border: 1px solid var(--surface-border, rgba(255, 255, 255, 0.12));
      box-shadow: var(--surface-shadow, 0 8px 32px rgba(0, 0, 0, 0.5));
    }

    .item {
      height: 44px;
      padding: 0 14px;
      font-size: 15px;
      border-radius: var(--radius, 8px);
    }

    .item-icon {
      width: 22px;
    }

    .divider {
      margin: 4px 10px;
    }
  }
</style>
