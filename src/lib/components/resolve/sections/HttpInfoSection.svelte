<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
  import MetaRow from '$lib/components/ui/MetaRow.svelte';
  import { formatSize } from '$lib/utils/format';
  import type { HttpInfo } from '$lib/bindings/HttpInfo';

  interface Props {
    http: HttpInfo;
    filesize?: number | null;
    mimeType?: string | null;
  }

  let { http, filesize = null, mimeType = null }: Props = $props();
</script>

<div class="http-info" style="--meta-label-width: 80px">
  {#if http.acceptRanges != null}
    <div class="resume-badge" class:resumable={http.acceptRanges}>
      <Icon name={http.acceptRanges ? 'check' : 'cross'} size={12} />
      <span>
        {http.acceptRanges ? $t('resolve.http.resumable') : $t('resolve.http.notResumable')}
      </span>
    </div>
  {/if}

  {#if filesize}
    <MetaRow icon="weight" label={$t('resolve.http.size')}>
      {formatSize(Number(filesize))}
    </MetaRow>
  {/if}

  {#if mimeType}
    <MetaRow icon="file_text" label={$t('resolve.http.contentType')}>
      {mimeType}
    </MetaRow>
  {/if}

  {#if http.server}
    <MetaRow icon="server" label={$t('resolve.http.server')}>
      {http.server}
    </MetaRow>
  {/if}

  {#if http.lastModified}
    <MetaRow icon="clock" label={$t('resolve.http.lastModified')}>
      {http.lastModified}
    </MetaRow>
  {/if}

  {#if http.etag}
    <MetaRow icon="code" label="ETag" mono>
      {http.etag}
    </MetaRow>
  {/if}
</div>

<style>
  .http-info {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: var(--radius, 8px);
    border: 1px solid rgba(255, 255, 255, 0.05);
  }

  .resume-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: 99px;
    font-size: 12px;
    font-weight: 500;
    align-self: flex-start;
  }

  .resume-badge.resumable {
    background: rgba(74, 222, 128, 0.12);
    color: #4ade80;
  }

  .resume-badge:not(.resumable) {
    background: rgba(248, 113, 113, 0.12);
    color: #f87171;
  }
</style>
