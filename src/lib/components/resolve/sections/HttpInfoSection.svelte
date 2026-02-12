<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatSize } from '$lib/utils/format';
  import type { HttpInfo } from '$lib/bindings/HttpInfo';

  interface Props {
    http: HttpInfo;
    filesize?: number | null;
    mimeType?: string | null;
  }

  let { http, filesize = null, mimeType = null }: Props = $props();
</script>

<div class="http-info">
  {#if http.acceptRanges != null}
    <div class="resume-badge" class:resumable={http.acceptRanges}>
      <Icon name={http.acceptRanges ? 'check' : 'cross'} size={12} />
      <span>
        {http.acceptRanges ? $t('resolve.http.resumable') : $t('resolve.http.notResumable')}
      </span>
    </div>
  {/if}

  {#if filesize}
    <div class="meta-row">
      <Icon name="weight" size={14} />
      <span class="meta-label">{$t('resolve.http.size')}</span>
      <span class="meta-value">{formatSize(Number(filesize))}</span>
    </div>
  {/if}

  {#if mimeType}
    <div class="meta-row">
      <Icon name="file_text" size={14} />
      <span class="meta-label">{$t('resolve.http.contentType')}</span>
      <span class="meta-value">{mimeType}</span>
    </div>
  {/if}

  {#if http.server}
    <div class="meta-row">
      <Icon name="server" size={14} />
      <span class="meta-label">{$t('resolve.http.server')}</span>
      <span class="meta-value">{http.server}</span>
    </div>
  {/if}

  {#if http.lastModified}
    <div class="meta-row">
      <Icon name="clock" size={14} />
      <span class="meta-label">{$t('resolve.http.lastModified')}</span>
      <span class="meta-value">{http.lastModified}</span>
    </div>
  {/if}

  {#if http.etag}
    <div class="meta-row">
      <Icon name="code" size={14} />
      <span class="meta-label">ETag</span>
      <span class="meta-value mono">{http.etag}</span>
    </div>
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

  .mono {
    font-family: monospace;
    font-size: 11px;
  }
</style>
