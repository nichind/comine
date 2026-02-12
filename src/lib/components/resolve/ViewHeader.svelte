<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { IconName } from '$lib/components/ui/Icon.svelte';

  interface Props {
    platformName?: string;
    platformIcon?: IconName;
    onBack?: () => void;
    backLabel?: string;
    estimatedSize?: string;
    selectionCount?: number;
    totalCount?: number;
    loading?: boolean;
    downloadDisabled?: boolean;
    onDownload?: () => void;
    onCancel?: () => void;
    children?: import('svelte').Snippet;
  }

  let {
    platformName = 'Media',
    platformIcon = 'play',
    onBack,
    backLabel,
    estimatedSize,
    selectionCount,
    totalCount,
    loading = false,
    downloadDisabled = false,
    onDownload,
    onCancel,
    children,
  }: Props = $props();

  let isCollection = $derived(totalCount !== undefined && totalCount > 1);
</script>

<div class="view-header">
  {#if onBack}
    <button class="back-btn" onclick={onBack}>
      <Icon name="alt_arrow_rigth" size={16} class="rotate-180" />
      <span>{backLabel ?? $t('common.back')}</span>
    </button>
  {/if}

  <div class="header-badge">
    <Icon name={platformIcon} size={12} />
    <span>{platformName}</span>
  </div>

  <div class="header-spacer"></div>

  {@render children?.()}

  {#if isCollection && selectionCount !== undefined}
    <span class="selection-count">{selectionCount} / {totalCount}</span>
  {:else if estimatedSize}
    <span class="size-estimate">~{estimatedSize}</span>
  {/if}

  {#if onCancel}
    <button class="header-cancel-btn" onclick={onCancel}>
      <Icon name="close" size={14} />
      <span>{$t('common.cancel')}</span>
    </button>
  {/if}

  <button class="header-download-btn" disabled={loading || downloadDisabled} onclick={onDownload}>
    <Icon name="download" size={16} />
    <span>{$t('common.download')}</span>
  </button>
</div>

<style>
  .view-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  .back-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.6);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }

  .back-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .header-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    background: rgba(255, 0, 0, 0.12);
    border-radius: var(--radius-sm, 6px);
    color: #ff6b6b;
    font-size: 11px;
    font-weight: 600;
  }

  .header-spacer {
    flex: 1;
  }

  .size-estimate {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    font-weight: 500;
  }

  .selection-count {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    font-weight: 500;
  }

  .header-download-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius, 8px);
    color: white;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    flex-shrink: 0;
  }

  .header-download-btn:hover:not(:disabled) {
    background: var(--accent, #6366f1);
    border-color: var(--accent, #6366f1);
  }

  .header-download-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .header-cancel-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 10px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: var(--radius-sm, 6px);
    color: rgba(239, 68, 68, 0.8);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    flex-shrink: 0;
  }

  .header-cancel-btn:hover {
    background: rgba(239, 68, 68, 0.2);
    color: rgb(239, 68, 68);
  }

  :global(.rotate-180) {
    transform: rotate(180deg);
  }
</style>
