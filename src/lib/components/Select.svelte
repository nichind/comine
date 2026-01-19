<script lang="ts">
  import Icon from './Icon.svelte';
  import { portal } from '$lib/actions/portal';

  interface SelectOption {
    value: string;
    label: string;
  }

  interface Props {
    value?: string;
    options?: SelectOption[];
    placeholder?: string;
    disabled?: boolean;
    onchange?: (value: string) => void;
  }

  let {
    value = $bindable(''),
    options = [],
    placeholder = 'Select...',
    disabled = false,
    onchange,
  }: Props = $props();

  let isOpen = $state(false);
  let triggerEl: HTMLButtonElement;
  let dropdownStyle = $state('');
  let scrollParent: HTMLElement | null = null;

  const selectedLabel = $derived(options.find((o) => o.value === value)?.label ?? placeholder);

  function toggle() {
    if (disabled) return;
    isOpen = !isOpen;
    if (isOpen) {
      positionDropdown();
      addScrollListener();
    } else {
      removeScrollListener();
    }
  }

  function positionDropdown() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();

    const spaceBelow = window.innerHeight - rect.bottom - 8;
    const spaceAbove = rect.top - 8;
    const dropdownHeight = Math.min(240, options.length * 42 + 8);

    let top: number;
    let maxHeight: number;

    if (spaceBelow >= dropdownHeight || spaceBelow >= spaceAbove) {
      top = rect.bottom + 4;
      maxHeight = Math.min(240, spaceBelow - 4);
    } else {
      top = rect.top - dropdownHeight - 4;
      maxHeight = Math.min(240, spaceAbove - 4);
    }

    let left = rect.left;
    const dropdownWidth = rect.width;
    if (left + dropdownWidth > window.innerWidth - 8) {
      left = window.innerWidth - dropdownWidth - 8;
    }
    if (left < 8) left = 8;

    dropdownStyle = `
      top: ${top}px;
      left: ${left}px;
      width: ${rect.width}px;
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
    scrollParent = findScrollParent(triggerEl);
    if (scrollParent) {
      scrollParent.addEventListener('scroll', handleScroll, { passive: true });
    }
    window.addEventListener('scroll', handleScroll, { passive: true });
    window.addEventListener('resize', handleScroll, { passive: true });
  }

  function removeScrollListener() {
    if (scrollParent) {
      scrollParent.removeEventListener('scroll', handleScroll);
      scrollParent = null;
    }
    window.removeEventListener('scroll', handleScroll);
    window.removeEventListener('resize', handleScroll);
  }

  function handleScroll() {
    if (isOpen) {
      isOpen = false;
      removeScrollListener();
    }
  }

  function select(option: SelectOption) {
    value = option.value;
    isOpen = false;
    removeScrollListener();
    onchange?.(option.value);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      isOpen = false;
      removeScrollListener();
    }
  }

  function handleClickOutside(e: MouseEvent) {
    if (!triggerEl?.contains(e.target as Node)) {
      isOpen = false;
      removeScrollListener();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} onclick={handleClickOutside} />

<button
  bind:this={triggerEl}
  class="select-trigger"
  class:open={isOpen}
  class:placeholder={!value}
  {disabled}
  onclick={toggle}
  type="button"
>
  <span class="select-value">{selectedLabel}</span>
  <Icon name="chevron" size={16} class="select-icon" />
</button>

{#if isOpen}
  <div use:portal>
    <div class="select-dropdown" style={dropdownStyle}>
      {#each options as option}
        <button
          class="select-option"
          class:selected={option.value === value}
          onclick={() => select(option)}
          type="button"
        >
          {option.label}
          {#if option.value === value}
            <Icon name="check" size={14} />
          {/if}
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .select-trigger {
    width: 100%;
    height: 36px;
    padding: 0 14px;
    font-family: inherit;
    font-size: 13px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px; /* Matching buttons radius */
    color: white;
    cursor: pointer;
    outline: none;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    text-align: left;
    box-sizing: border-box;
  }

  .select-trigger:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.2);
  }

  .select-trigger:focus {
    border-color: rgba(255, 255, 255, 0.3);
  }

  .select-trigger:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .select-trigger.placeholder .select-value {
    color: rgba(255, 255, 255, 0.5);
  }

  .select-value {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .select-trigger :global(.select-icon) {
    flex-shrink: 0;
    transition: transform 0.2s;
    color: rgba(255, 255, 255, 0.5);
  }

  .select-trigger.open :global(.select-icon) {
    transform: rotate(180deg);
  }

  .select-dropdown {
    position: fixed;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    padding: 3px;
    z-index: 1100;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    max-height: 240px;
    overflow-y: auto;
  }

  .select-option {
    width: 100%;
    padding: 11px 14px;
    font-family: inherit;
    font-size: 13px;
    background: transparent;
    border: none;
    border-radius: 5px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    text-align: left;
    transition: all 0.15s;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .select-option:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .select-option.selected {
    color: white;
    background: rgba(255, 255, 255, 0.08);
  }

  .select-option.selected :global(svg) {
    color: var(--accent, #6366f1);
  }
</style>
