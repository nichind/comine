<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatSize } from '$lib/utils/format';
  import type { TorrentInfo } from '$lib/bindings/TorrentInfo';

  interface Props {
    torrent: TorrentInfo;
  }

  let { torrent }: Props = $props();

  let showTrackers = $state(false);

  function copyToClipboard(text: string) {
    navigator.clipboard.writeText(text);
  }
</script>

<div class="torrent-section">
  <div class="torrent-stats">
    {#if torrent.seeders != null || torrent.leechers != null}
      <div class="stat-row">
        {#if torrent.seeders != null}
          <span class="stat seeders">
            <Icon name="arrow_right" size={12} />
            {torrent.seeders}
            {$t('resolve.torrent.seeders')}
          </span>
        {/if}
        {#if torrent.leechers != null}
          <span class="stat leechers">
            <Icon name="arrow_down" size={12} />
            {torrent.leechers}
            {$t('resolve.torrent.leechers')}
          </span>
        {/if}
      </div>
    {/if}

    {#if torrent.totalSize != null}
      <div class="meta-row">
        <Icon name="weight" size={14} />
        <span class="meta-label">{$t('resolve.torrent.size')}</span>
        <span class="meta-value">{formatSize(Number(torrent.totalSize))}</span>
      </div>
    {/if}

    {#if torrent.infoHash}
      <div class="meta-row">
        <Icon name="code" size={14} />
        <span class="meta-label">{$t('resolve.torrent.hash')}</span>
        <button class="hash-value" onclick={() => copyToClipboard(torrent.infoHash!)}>
          <span class="hash-text">{torrent.infoHash}</span>
          <Icon name="copy" size={12} />
        </button>
      </div>
    {/if}

    {#if torrent.createdBy}
      <div class="meta-row">
        <Icon name="user" size={14} />
        <span class="meta-label">{$t('resolve.torrent.createdBy')}</span>
        <span class="meta-value">{torrent.createdBy}</span>
      </div>
    {/if}

    {#if torrent.creationDate}
      <div class="meta-row">
        <Icon name="date" size={14} />
        <span class="meta-label">{$t('resolve.torrent.date')}</span>
        <span class="meta-value">{torrent.creationDate}</span>
      </div>
    {/if}

    {#if torrent.comment}
      <div class="meta-row">
        <Icon name="chat" size={14} />
        <span class="meta-label">{$t('resolve.torrent.comment')}</span>
        <span class="meta-value">{torrent.comment}</span>
      </div>
    {/if}

    {#if torrent.pieceCount && torrent.pieceSize}
      <div class="meta-row">
        <Icon name="widgets" size={14} />
        <span class="meta-label">{$t('resolve.torrent.pieces')}</span>
        <span class="meta-value"
          >{torrent.pieceCount} × {formatSize(Number(torrent.pieceSize))}</span
        >
      </div>
    {/if}

    {#if torrent.isPrivate}
      <div class="meta-row">
        <Icon name="lock" size={14} />
        <span class="private-badge">{$t('resolve.torrent.private')}</span>
      </div>
    {/if}
  </div>

  {#if torrent.trackers && torrent.trackers.length > 0}
    <button class="tracker-toggle" onclick={() => (showTrackers = !showTrackers)}>
      <Icon name={showTrackers ? 'chevron_up' : 'chevron_down'} size={14} />
      <span>{$t('resolve.torrent.trackers')} ({torrent.trackers.length})</span>
    </button>

    {#if showTrackers}
      <div class="tracker-list">
        {#each torrent.trackers as tracker}
          <span class="tracker-url">{tracker}</span>
        {/each}
      </div>
    {/if}
  {/if}

  {#if torrent.magnetUri}
    <button class="magnet-btn" onclick={() => copyToClipboard(torrent.magnetUri!)}>
      <Icon name="link" size={14} />
      <span>{$t('resolve.torrent.copyMagnet')}</span>
    </button>
  {/if}
</div>

<style>
  .torrent-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: var(--radius, 8px);
    border: 1px solid rgba(255, 255, 255, 0.05);
  }

  .torrent-stats {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .stat-row {
    display: flex;
    gap: 16px;
  }

  .stat {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 13px;
    font-weight: 600;
  }

  .seeders {
    color: #4ade80;
  }

  .leechers {
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
    min-width: 70px;
    flex-shrink: 0;
  }

  .meta-value {
    color: var(--text-1, white);
    word-break: break-word;
  }

  .hash-value {
    display: flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: none;
    color: var(--text-1, white);
    cursor: pointer;
    padding: 0;
    font-family: monospace;
    font-size: 11px;
  }

  .hash-value:hover {
    color: var(--accent, #6366f1);
  }

  .hash-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 200px;
  }

  .private-badge {
    padding: 2px 8px;
    background: rgba(239, 68, 68, 0.15);
    border-radius: 99px;
    font-size: 11px;
    color: #f87171;
    font-weight: 500;
  }

  .tracker-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.5);
    font-size: 12px;
    cursor: pointer;
    padding: 4px 0;
  }

  .tracker-toggle:hover {
    color: white;
  }

  .tracker-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-left: 20px;
  }

  .tracker-url {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
    font-family: monospace;
    word-break: break-all;
  }

  .magnet-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    cursor: pointer;
    transition: all 0.15s;
    align-self: flex-start;
  }

  .magnet-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }
</style>
