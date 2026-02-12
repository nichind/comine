<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconName } from '$lib/components/ui/Icon.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';

  interface Props {
    icon: IconName;
    title: Snippet;
    description: Snippet;
    headerAction?: Snippet;
    children?: Snippet;
    class?: string;
  }

  let {
    icon,
    title,
    description,
    headerAction,
    children,
    class: extraClass = '',
  }: Props = $props();
</script>

<div class="settings-card {extraClass}">
  <div class="header">
    <div class="header-content">
      <div class="icon-wrapper">
        <Icon name={icon} size={18} />
      </div>
      <div class="text-content">
        <div class="title">
          {@render title()}
        </div>
        <div class="description">
          {@render description()}
        </div>
      </div>
    </div>
    {#if headerAction}
      {@render headerAction()}
    {/if}
  </div>
  {#if children}
    {@render children()}
  {/if}
</div>

<style>
  .settings-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: var(--radius-lg, 12px);
  }

  .header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .header-content {
    display: flex;
    align-items: flex-start;
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
    padding-top: 2px;
  }

  .text-content {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .title {
    font-size: var(--text-md, 14px);
    font-weight: 450;
    color: rgba(255, 255, 255, 0.9);
    line-height: 1.3;
  }

  .description {
    font-size: var(--text-sm, 12px);
    font-weight: 350;
    color: rgba(255, 255, 255, 0.5);
    line-height: 1.4;
  }

  @media (max-width: 480px) {
    .settings-card {
      padding: 14px 16px;
      gap: 14px;
    }
  }
</style>
