<script lang="ts">
  import { portal } from '$lib/actions/portal';
  import Icon, { type IconName } from '$lib/components/ui/Icon.svelte';
  import { tick } from 'svelte';

  interface SelectOption {
    value: string;
    label: string;
    description?: string;
    icon?: IconName;
    image?: string;
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
  let isClosing = $state(false);
  let isAnimating = $state(false);
  let isTriggerVisible = $state(true);
  let focusedIndex = $state(-1);

  let triggerEl = $state<HTMLButtonElement>();
  let dropdownEl = $state<HTMLElement>();

  const isRich = $derived(options.some((o) => !!o.description));
  const rowHeight = $derived(isRich ? 60 : 36);

  let layout = $state({
    top: 0,
    left: 0,
    width: 0,
    height: 0,
    triggerTop: 0,
    selectedIndex: 0,
  });

  let themeStyle = $state('');

  const selectedOption = $derived(options.find((o) => o.value === value));
  const selectedIndex = $derived(
    Math.max(
      0,
      options.findIndex((o) => o.value === value)
    )
  );

  function measure() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const winHeight = window.innerHeight;
    const margin = 12;

    const styles = getComputedStyle(triggerEl);
    themeStyle = `
      --trigger-bg: ${styles.backgroundColor};
      --trigger-border-color: ${styles.borderColor};
      --trigger-radius: ${styles.borderRadius};
      --trigger-font-family: ${styles.fontFamily};
      --trigger-font-size: ${styles.fontSize};
    `;

    const currentHeight = rowHeight;
    const contentHeight = options.length * currentHeight + 2;
    let idealTop = rect.top - selectedIndex * currentHeight;
    let finalTop = idealTop;
    let finalHeight = contentHeight;

    if (finalTop < margin) finalTop = margin;

    if (finalTop + finalHeight > winHeight - margin) {
      const overflow = finalTop + finalHeight - (winHeight - margin);
      if (finalTop - overflow >= margin) {
        finalTop -= overflow;
      } else {
        finalTop = margin;
        finalHeight = winHeight - 2 * margin;
      }
    }

    layout = {
      top: finalTop,
      left: rect.left,
      width: rect.width,
      height: finalHeight,
      triggerTop: rect.top,
      selectedIndex,
    };
  }

  function toggle() {
    if (disabled) return;
    if (isOpen) {
      close();
    } else {
      open();
    }
  }

  function open() {
    measure();
    isOpen = true;
    isClosing = false;
    isAnimating = true;
    focusedIndex = selectedIndex;
    isTriggerVisible = false;
  }

  function close() {
    if (!isOpen) return;
    isClosing = true;
    isAnimating = true;
  }

  function select(option: SelectOption) {
    if (disabled) return;
    if (option.value !== value) {
      value = option.value;
      onchange?.(option.value);
    }
    close();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!isOpen && !isClosing) {
      if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(e.key)) {
        e.preventDefault();
        open();
      }
      return;
    }

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        focusedIndex = Math.min(focusedIndex + 1, options.length - 1);
        scrollFocusedIntoView();
        break;
      case 'ArrowUp':
        e.preventDefault();
        focusedIndex = Math.max(focusedIndex - 1, 0);
        scrollFocusedIntoView();
        break;
      case 'Home':
        e.preventDefault();
        focusedIndex = 0;
        scrollFocusedIntoView();
        break;
      case 'End':
        e.preventDefault();
        focusedIndex = options.length - 1;
        scrollFocusedIntoView();
        break;
      case 'Enter':
      case ' ':
        e.preventDefault();
        if (focusedIndex >= 0 && focusedIndex < options.length) {
          select(options[focusedIndex]);
        }
        break;
      case 'Escape':
        e.preventDefault();
        close();
        triggerEl?.focus();
        break;
      case 'Tab':
        close();
        break;
    }
  }

  function scrollFocusedIntoView() {
    if (!dropdownEl || focusedIndex < 0) return;
    const optionEls = dropdownEl.querySelectorAll('.select-option');
    const el = optionEls[focusedIndex] as HTMLElement | undefined;
    el?.scrollIntoView({ block: 'nearest' });
  }

  async function onAnimationEnd(e: AnimationEvent) {
    if (e.target !== dropdownEl) return;

    if (isClosing) {
      isTriggerVisible = true;
      await tick();
      isOpen = false;
      isClosing = false;
      isAnimating = false;
    } else {
      isAnimating = false;
    }
  }

  const dropdownStyle = $derived(`
    --t-left: ${layout.left}px;
    --t-width: ${layout.width}px;
    --t-height: ${layout.height}px;
    --t-top: ${layout.top}px;
    --t-trigger-top: ${layout.triggerTop}px;
    --row-height: ${rowHeight}px;
    --selected-idx: ${selectedIndex};
  `);
</script>

<svelte:window onresize={() => isOpen && close()} onscroll={() => isOpen && close()} />

{#snippet rowContent(opt: SelectOption | undefined, mode: 'trigger' | 'option', active: boolean)}
  <div class="row-content">
    {#if opt?.image}
      <img src={opt.image} alt="" class="row-image" />
    {:else if opt?.icon}
      <div class="row-item-icon">
        <Icon name={opt.icon} size={16} />
      </div>
    {/if}

    <div class="row-text">
      <span class="row-label"
        >{mode === 'trigger' ? (active ? opt?.label : placeholder) : opt?.label}</span
      >
      {#if opt?.description}
        <span class="row-desc">{opt.description}</span>
      {/if}
    </div>

    <div class="row-icon">
      {#if mode === 'trigger'}
        <Icon name="chevron" size={16} class="icon-chevron" />
      {:else if active}
        <div class="morph-icon-wrapper">
          <Icon name="check" size={16} class="icon-morph icon-check" />
          <Icon name="chevron" size={16} class="icon-morph icon-chevron-morph" />
        </div>
      {/if}
    </div>
  </div>
{/snippet}

<button
  bind:this={triggerEl}
  class="select-trigger"
  class:rich={isRich}
  {disabled}
  onclick={toggle}
  onkeydown={handleKeydown}
  type="button"
  role="combobox"
  aria-haspopup="listbox"
  aria-controls="select-listbox"
  aria-expanded={isOpen}
  aria-activedescendant={isOpen && focusedIndex >= 0 ? `select-option-${focusedIndex}` : undefined}
  style:opacity={isTriggerVisible ? 1 : 0}
  style:height="{rowHeight}px"
>
  {@render rowContent(selectedOption, 'trigger', !!selectedOption)}
</button>

{#if isOpen}
  <div use:portal>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="select-backdrop" onclick={close}></div>

    <div
      bind:this={dropdownEl}
      class="select-dropdown"
      class:closing={isClosing}
      class:animating={isAnimating}
      style="{dropdownStyle} {themeStyle}"
      onanimationend={onAnimationEnd}
      onkeydown={handleKeydown}
      role="listbox"
      id="select-listbox"
      tabindex="-1"
      aria-label={placeholder}
    >
      <div class="select-list">
        {#each options as option, i}
          {@const isActive = option.value === value}
          {@const isFocused = i === focusedIndex}
          <button
            id="select-option-{i}"
            class="select-option"
            class:selected={isActive}
            class:focused={isFocused}
            class:rich={isRich}
            onclick={() => select(option)}
            onmouseenter={() => {
              focusedIndex = i;
            }}
            type="button"
            role="option"
            aria-selected={isActive}
            style:height="{rowHeight}px"
          >
            {@render rowContent(option, 'option', isActive)}
          </button>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .row-content {
    display: flex;
    align-items: center;
    width: 100%;
    height: 100%;
    gap: 10px;
    box-sizing: border-box;
    position: relative;
  }

  .row-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    overflow: hidden;
    text-align: left;
    min-width: 0;
  }

  .row-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: normal;
  }

  .row-desc {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    line-height: 1.3;
    margin-top: 2px;
  }

  .row-image {
    width: 20px;
    height: 20px;
    border-radius: 4px;
    object-fit: cover;
    flex-shrink: 0;
  }

  .row-item-icon {
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.6);
    flex-shrink: 0;
  }

  .row-icon {
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    right: 0;
    top: 50%;
    transform: translateY(-50%);
  }

  .select-trigger :global(.icon-chevron) {
    color: rgba(255, 255, 255, 0.5);
    transition: transform 0.2s;
  }

  .select-trigger:hover :global(.icon-chevron) {
    color: rgba(255, 255, 255, 0.8);
  }

  .morph-icon-wrapper {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
  }

  .morph-icon-wrapper :global(.icon-morph) {
    grid-area: 1 / 1;
    position: relative;
    transition:
      opacity 0.15s ease-out,
      transform 0.15s ease-out;
  }

  .morph-icon-wrapper :global(.icon-check) {
    color: var(--accent, #6366f1);
    opacity: 1;
    transform: scale(1);
  }

  .morph-icon-wrapper :global(.icon-chevron-morph) {
    color: rgba(255, 255, 255, 0.5);
    opacity: 0;
    transform: scale(0.8);
  }

  .select-dropdown.closing :global(.icon-check) {
    opacity: 0;
    transform: scale(0.8);
  }

  .select-dropdown.closing :global(.icon-chevron-morph) {
    opacity: 1;
    transform: scale(1);
  }

  .select-trigger {
    width: 100%;
    padding: 0 14px;
    font-family: inherit;
    font-size: var(--text-base, 13px);
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius, 8px);
    box-sizing: border-box;
    color: white;
    cursor: pointer;
    outline: none;
    display: block;
    user-select: none;
    transition:
      background 0.15s,
      border-color 0.15s;

    will-change: opacity;
  }

  .select-trigger:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.2);
  }

  .select-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    z-index: 9998;
    cursor: default;
  }

  .select-dropdown {
    position: fixed;
    z-index: 9999;
    left: var(--t-left);
    width: var(--t-width);

    background: var(--trigger-bg, #1a1a1a);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);

    border: 1px solid var(--trigger-border-color, rgba(255, 255, 255, 0.15));
    border-radius: var(--trigger-radius, 8px);
    font-family: var(--trigger-font-family, inherit);

    box-sizing: border-box;

    animation: expand 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards;
    overflow: hidden;

    backdrop-filter: blur(24px);
    -webkit-backdrop-filter: blur(24px);

    will-change: top, height;
  }

  .select-list {
    width: 100%;
    position: relative;
    padding: 0;
    margin: 0;
    will-change: transform;
  }

  .select-dropdown {
    overflow-y: overlay;

    scrollbar-width: thin;
    scrollbar-color: rgba(255, 255, 255, 0.2) transparent;
  }
  .select-dropdown::-webkit-scrollbar {
    width: 4px;
  }
  .select-dropdown::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.2);
    border-radius: 4px;
  }

  .select-dropdown.closing,
  .select-dropdown.animating {
    overflow: hidden;
  }

  .select-option {
    width: 100%;
    padding: 0 14px;
    font-family: inherit;
    font-size: var(--trigger-font-size, 13px);
    color: var(--surface-text, rgba(255, 255, 255, 0.7));
    background: transparent;
    border: none;
    border-radius: 0;
    cursor: pointer;
    box-sizing: border-box;
    display: block;
    transition: background 0.15s;
    flex-shrink: 0;
    text-align: left;
  }

  .select-option:hover,
  .select-option.focused {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .select-option.selected {
    color: white;
    background: rgba(255, 255, 255, 0.04);
  }

  @keyframes expand {
    0% {
      top: var(--t-trigger-top);
      height: var(--row-height);
    }
    100% {
      top: var(--t-top);
      height: var(--t-height);
    }
  }

  .select-dropdown.closing {
    animation: collapse 0.25s cubic-bezier(0.16, 1, 0.3, 1) forwards !important;
    pointer-events: none;
  }

  @keyframes collapse {
    0% {
      top: var(--t-top);
      height: var(--t-height);
    }
    100% {
      top: var(--t-trigger-top);
      height: var(--row-height);
    }
  }

  .select-dropdown:not(.closing) .select-list {
    animation: slideListOpen 0.3s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }

  .select-dropdown.closing .select-list {
    animation: slideListClosed 0.25s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }

  @keyframes slideListOpen {
    0% {
      transform: translateY(calc(-1 * (var(--selected-idx) * var(--row-height))));
    }
    100% {
      transform: translateY(0);
    }
  }

  @keyframes slideListClosed {
    0% {
      transform: translateY(0);
    }
    100% {
      transform: translateY(calc(-1 * (var(--selected-idx) * var(--row-height))));
    }
  }
</style>
