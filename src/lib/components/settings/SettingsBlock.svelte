<script lang="ts">
  import Icon, { type IconName } from '$lib/components/ui/Icon.svelte';
  import Divider from '$lib/components/ui/Divider.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { t } from '$lib/i18n';

  interface Props {
    title: string;
    icon?: IconName;
    onResetSection?: () => void;
    showHeader?: boolean;
    children?: import('svelte').Snippet;
  }

  let { title, icon, onResetSection, showHeader = true, children }: Props = $props();
</script>

<section class="settings-block" class:headerless={!showHeader}>
  {#if showHeader}
    <div class="block-header">
      <div class="header-content">
        {#if icon}
          <Icon name={icon} size={18} class="header-icon" />
        {/if}
        <h2 class="block-title">{title}</h2>
      </div>

      {#if onResetSection}
        <button
          class="section-reset-btn"
          onclick={onResetSection}
          use:tooltip={$t('settings.resetSectionTooltip')}
        >
          <Icon name="undo" size={14} />
          <span class="reset-text">{$t('settings.resetSection')}</span>
        </button>
      {/if}
    </div>
  {:else if onResetSection}
    <div class="block-actions">
      <button
        class="section-reset-btn"
        onclick={onResetSection}
        use:tooltip={$t('settings.resetSectionTooltip')}
      >
        <Icon name="undo" size={14} />
        <span class="reset-text">{$t('settings.resetSection')}</span>
      </button>
    </div>
  {/if}

  <div class="block-content">
    {@render children?.()}
  </div>
</section>

<style>
  .settings-block {
    margin-bottom: 24px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .settings-block.headerless {
    gap: 0px;
  }

  .block-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 32px;
    margin-bottom: 8px;
  }

  .header-content {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  :global(.header-icon) {
    color: var(--accent-color, #6366f1);
    opacity: 0.9;
    width: 24px;
    display: flex;
    justify-content: center;
  }

  .block-title {
    font-size: 20px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.85);
    letter-spacing: 0.01em;
    margin: 0;
  }

  .section-reset-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-radius: var(--radius-sm, 4px);
    background: transparent;
    border: 1px solid transparent;
    color: rgba(255, 255, 255, 0.4);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
  }

  .section-reset-btn:hover {
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.8);
  }

  .block-content {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .block-actions {
    display: flex;
    justify-content: flex-end;
    margin-bottom: 8px;
  }

  @media (max-width: 640px) {
    .reset-text {
      display: none;
    }
  }
</style>
