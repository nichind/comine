<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte';
  import { slide } from 'svelte/transition';

  interface Props {
    title: string;
    icon?: IconName;
    description?: string;
    badge?: string;
    badgeType?: 'success' | 'warning' | 'error' | 'info';
    disabled?: boolean;
    disabledReason?: string;
    expanded?: boolean;
    children?: import('svelte').Snippet;
  }

  let {
    title,
    icon,
    description,
    badge,
    badgeType = 'info',
    disabled = false,
    disabledReason,
    expanded = $bindable(false),
    children,
  }: Props = $props();

  function toggle() {
    if (!disabled) {
      expanded = !expanded;
    }
  }
</script>

<div class="collapsible-block" class:disabled class:expanded>
  <button class="block-header" onclick={toggle} type="button">
    <div class="header-left">
      {#if icon}
        <div class="icon-wrapper">
          <Icon name={icon} size={20} />
        </div>
      {/if}
      <div class="header-text">
        <div class="title-row">
          <h3 class="block-title">{title}</h3>
          {#if badge}
            <span class="badge {badgeType}">{badge}</span>
          {/if}
        </div>
        {#if description}
          <p class="block-description">{description}</p>
        {/if}
      </div>
    </div>
    <div class="header-right">
      {#if disabled && disabledReason}
        <span class="disabled-reason">{disabledReason}</span>
      {/if}
      <div class="chevron" class:rotated={expanded}>
        <Icon name="chevron" size={18} />
      </div>
    </div>
  </button>

  {#if expanded && !disabled}
    <div class="block-content" transition:slide={{ duration: 200 }}>
      {@render children?.()}
    </div>
  {/if}
</div>

<style>
  .collapsible-block {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-lg, 12px);
    overflow: hidden;
    transition: all 0.2s;
  }

  .collapsible-block:hover:not(.disabled) {
    border-color: rgba(255, 255, 255, 0.12);
  }

  .collapsible-block.expanded {
    background: rgba(255, 255, 255, 0.04);
    border-color: rgba(255, 255, 255, 0.12);
  }

  .collapsible-block.disabled {
    opacity: 0.6;
  }

  .block-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 14px 16px;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
    color: inherit;
    font: inherit;
    transition: background 0.15s;
  }

  .collapsible-block.disabled .block-header {
    cursor: not-allowed;
  }

  .block-header:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.02);
  }

  .header-left {
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
    width: 36px;
    height: 36px;
    background: var(--accent-alpha, rgba(99, 102, 241, 0.15));
    border-radius: var(--radius, 10px);
    color: var(--accent, #6366f1);
    flex-shrink: 0;
  }

  .header-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .block-title {
    font-size: var(--text-md, 15px);
    font-weight: 600;
    color: rgba(255, 255, 255, 0.95);
    margin: 0;
  }

  .badge {
    padding: 3px 8px;
    font-size: var(--text-xs, 11px);
    font-weight: 600;
    border-radius: var(--radius-sm, 6px);
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .badge.success {
    background: rgba(34, 197, 94, 0.15);
    color: rgba(34, 197, 94, 0.9);
  }

  .badge.warning {
    background: rgba(251, 191, 36, 0.15);
    color: rgba(251, 191, 36, 0.9);
  }

  .badge.error {
    background: rgba(239, 68, 68, 0.15);
    color: rgba(239, 68, 68, 0.9);
  }

  .badge.info {
    background: rgba(99, 102, 241, 0.15);
    color: rgba(99, 102, 241, 0.9);
  }

  .block-description {
    font-size: var(--text-base, 13px);
    color: rgba(255, 255, 255, 0.5);
    margin: 0;
    line-height: 1.4;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-shrink: 0;
  }

  .disabled-reason {
    font-size: 12px;
    color: rgba(251, 191, 36, 0.8);
    white-space: nowrap;
  }

  .chevron {
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.4);
    transition: transform 0.2s ease;
  }

  .chevron.rotated {
    transform: rotate(180deg);
  }

  .block-content {
    padding: 0 16px 16px 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  @media (max-width: 640px) {
    .block-header {
      padding: 12px 14px;
    }

    .icon-wrapper {
      width: 32px;
      height: 32px;
    }

    .block-title {
      font-size: 14px;
    }

    .block-description {
      font-size: 12px;
    }

    .disabled-reason {
      display: none;
    }
  }
</style>
