<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { fade } from 'svelte/transition';
  import HighlightText from './HighlightText.svelte';

  interface Props {
    title: string;
    description?: string;
    icon?: IconName;
    value?: any;
    defaultValue?: any;
    onReset?: () => void;
    class?: string;
    children?: import('svelte').Snippet;
    descriptionSnippet?: import('svelte').Snippet;
    highlight?: string;
  }

  let {
    title,
    description,
    icon,
    value,
    defaultValue,
    onReset,
    class: className = '',
    children,
    descriptionSnippet,
    highlight = '',
  }: Props = $props();

  let showReset = $derived(
    value !== undefined && defaultValue !== undefined && value !== defaultValue
  );

  let rootEl = $state<HTMLDivElement | null>(null);

  function getActivator(): HTMLElement | null {
    return rootEl?.querySelector<HTMLElement>('[data-setting-activator="true"]') ?? null;
  }

  function handleContainerClick(event: MouseEvent) {
    const target = event.target as Element | null;
    if (!target) return;

    // If the user clicked an interactive element, don't intercept.
    if (
      target.closest(
        'a,button,input,select,textarea,[role="button"],[role="switch"],[contenteditable="true"],[data-no-settingitem-activate]'
      )
    ) {
      return;
    }

    const activator = getActivator();
    if (!activator) return;

    activator.click();
  }

  function handleContainerKeydown(event: KeyboardEvent) {
    if (event.key !== 'Enter' && event.key !== ' ') return;

    const activator = getActivator();
    if (!activator) return;

    event.preventDefault();
    activator.click();
  }
</script>

<div
  class="setting-item {className}"
  bind:this={rootEl}
  onclick={handleContainerClick}
  onkeydown={handleContainerKeydown}
  role="group"
>
  <div class="content-side">
    {#if icon}
      <div class="icon-wrapper">
        <Icon name={icon} size={18} />
      </div>
    {/if}
    <div class="text-content">
      <div class="title">
        <HighlightText text={title} {highlight} />
      </div>
      {#if description}
        <div class="description">
          <HighlightText text={description} {highlight} />
        </div>
      {/if}
      {@render descriptionSnippet?.()}
    </div>
  </div>

  <div class="control-side">
    {#if showReset && onReset}
      <button
        class="reset-btn"
        onclick={onReset}
        transition:fade={{ duration: 150 }}
        use:tooltip={'Reset to default'}
        aria-label="Reset setting"
      >
        <Icon name="undo" size={14} />
      </button>
    {/if}
    <div class="control-wrapper">
      {@render children?.()}
    </div>
  </div>
</div>

<style>
  .setting-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    gap: 12px;
    background: rgba(255, 255, 255, 0.04);
    border: none;
    border-radius: 12px;
    min-height: 44px;
    cursor: pointer;
  }

  .content-side {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: 1;
    min-width: 0;
  }

  .icon-wrapper {
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.5);
    flex-shrink: 0;
    width: 24px;
  }

  .text-content {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .title {
    font-size: 14px;
    font-weight: 450;
    color: rgba(255, 255, 255, 0.9);
    line-height: 1.3;
  }

  .description {
    font-size: 12px;
    font-weight: 350;
    color: rgba(255, 255, 255, 0.5);
    line-height: 1.3;
  }

  .control-side {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    min-height: 24px;
  }

  .control-wrapper {
    display: flex;
    align-items: center;
  }

  .reset-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    background: transparent;
    border: 1px solid transparent;
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    transition: all 0.2s;
  }

  .reset-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.9);
  }

  @media (max-width: 640px) {
    .setting-item {
      padding: 14px 16px;
      gap: 16px;
      min-height: 52px;
    }

    .title {
      font-size: 15px;
    }

    .description {
      font-size: 13px;
    }

    .reset-btn {
      width: 28px;
      height: 28px;
    }
  }
</style>
