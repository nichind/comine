<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
  import MetaRow from '$lib/components/ui/MetaRow.svelte';
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
  <div class="cloud-info" style="--meta-label-width: 80px">
    {#if cloud.passwordProtected}
      <div class="warning-badge">
        <Icon name="lock" size={12} />
        <span>{$t('resolve.cloud.passwordProtected')}</span>
      </div>
    {/if}

    {#if cloud.sharedBy}
      <MetaRow icon="user" label={$t('resolve.cloud.sharedBy')}>
        {cloud.sharedBy}
      </MetaRow>
    {/if}

    {#if cloud.shareDate}
      <MetaRow icon="date" label={$t('resolve.cloud.shareDate')}>
        {cloud.shareDate}
      </MetaRow>
    {/if}

    {#if cloud.expiryDate}
      <MetaRow icon="clock" label={$t('resolve.cloud.expires')}>
        {cloud.expiryDate}
      </MetaRow>
    {/if}

    {#if cloud.downloadLimit != null}
      <MetaRow icon="download" label={$t('resolve.cloud.downloadLimit')}>
        {cloud.downloadCount ?? 0} / {cloud.downloadLimit}
      </MetaRow>
    {/if}

    {#if cloud.folderPath}
      <MetaRow icon="folder" label={$t('resolve.cloud.path')}>
        {cloud.folderPath}
      </MetaRow>
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
</style>
