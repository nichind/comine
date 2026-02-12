<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { CloudInfo } from '$lib/bindings/CloudInfo';

  interface Props {
    cloud: CloudInfo;
  }

  let { cloud }: Props = $props();

  let hasContent = $derived(
    !!cloud.sharedBy ||
      !!cloud.shareDate ||
      !!cloud.expiryDate ||
      cloud.passwordProtected != null ||
      cloud.downloadLimit != null ||
      !!cloud.folderPath
  );
</script>

{#if hasContent}
  <div class="cloud-info">
    {#if cloud.passwordProtected}
      <div class="warning-badge">
        <Icon name="lock" size={12} />
        <span>{$t('resolve.cloud.passwordProtected')}</span>
      </div>
    {/if}

    {#if cloud.sharedBy}
      <div class="meta-row">
        <Icon name="user" size={14} />
        <span class="meta-label">{$t('resolve.cloud.sharedBy')}</span>
        <span class="meta-value">{cloud.sharedBy}</span>
      </div>
    {/if}

    {#if cloud.shareDate}
      <div class="meta-row">
        <Icon name="date" size={14} />
        <span class="meta-label">{$t('resolve.cloud.shareDate')}</span>
        <span class="meta-value">{cloud.shareDate}</span>
      </div>
    {/if}

    {#if cloud.expiryDate}
      <div class="meta-row">
        <Icon name="clock" size={14} />
        <span class="meta-label">{$t('resolve.cloud.expires')}</span>
        <span class="meta-value">{cloud.expiryDate}</span>
      </div>
    {/if}

    {#if cloud.downloadLimit != null}
      <div class="meta-row">
        <Icon name="download" size={14} />
        <span class="meta-label">{$t('resolve.cloud.downloadLimit')}</span>
        <span class="meta-value">
          {cloud.downloadCount ?? 0} / {cloud.downloadLimit}
        </span>
      </div>
    {/if}

    {#if cloud.folderPath}
      <div class="meta-row">
        <Icon name="folder" size={14} />
        <span class="meta-label">{$t('resolve.cloud.path')}</span>
        <span class="meta-value">{cloud.folderPath}</span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .cloud-info {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: var(--radius, 8px);
    border: 1px solid rgba(255, 255, 255, 0.05);
  }

  .warning-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    background: rgba(251, 191, 36, 0.12);
    border-radius: 99px;
    font-size: 12px;
    font-weight: 500;
    color: #fbbf24;
    align-self: flex-start;
  }

  .meta-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.6);
  }

  .meta-row :global(svg) {
    flex-shrink: 0;
  }

  .meta-label {
    color: rgba(255, 255, 255, 0.4);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    min-width: 80px;
    flex-shrink: 0;
  }

  .meta-value {
    color: var(--text-1, white);
    word-break: break-word;
  }
</style>
