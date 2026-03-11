<script lang="ts">
  import { t } from '$lib/i18n';
  import { queue } from '$lib/stores/queue';
  import { navigation } from '$lib/stores/navigation';
  import { toast } from '$lib/components/ui/Toast.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatSize } from '$lib/utils/format';
  import type { TorrentSearchResult, TorrentOption } from './TorrentSearch.svelte';

  interface Props {
    result: TorrentSearchResult;
    onBack?: () => void;
    onDownload?: (magnetUrl: string, title: string) => void;
  }

  let { result, onBack, onDownload }: Props = $props();

  let posterError = $state(false);
  let downloadingIndex = $state<number | null>(null);

  let sortedTorrents = $derived(
    [...result.torrents].sort((a, b) => (b.qualityScore ?? 0) - (a.qualityScore ?? 0))
  );

  function getQualityScoreColor(score: number | null): string {
    if (score === null) return 'rgba(255,255,255,0.3)';
    if (score >= 70) return '#22c55e';
    if (score >= 40) return '#f59e0b';
    return '#ef4444';
  }

  async function handleDownload(torrent: TorrentOption, index: number) {
    const magnetUrl = torrent.magnetUrl ?? torrent.torrentUrl;
    if (!magnetUrl) {
      toast.error('No download URL available for this torrent');
      return;
    }

    if (onDownload) {
      onDownload(magnetUrl, result.title);
      return;
    }

    downloadingIndex = index;
    try {
      queue.add(magnetUrl, {
        prefetchedInfo: {
          title: result.title,
          thumbnail: result.posterUrl ?? undefined,
        },
        useAria2: true,
      });
      toast.info(`${result.title} — ${$t('downloads.started').replace('{title}', result.title)}`);
      navigation.replace({ type: 'home' });
    } catch (e) {
      toast.error(String(e));
    } finally {
      downloadingIndex = null;
    }
  }
</script>

<div class="torrent-detail">
  <div class="detail-header">
    {#if onBack}
      <button class="back-btn" onclick={onBack}>
        <Icon name="alt_arrow_rigth" size={16} class="rotate-180" />
        <span>{$t('common.back')}</span>
      </button>
    {/if}
  </div>

  <div class="detail-content">
    <div class="hero">
      <div class="poster-wrap">
        {#if result.posterUrl && !posterError}
          <img
            src={result.posterUrl}
            alt={result.title}
            class="poster"
            decoding="async"
            onerror={() => (posterError = true)}
          />
        {:else}
          <div class="poster-fallback">
            <Icon name="video" size={48} />
          </div>
        {/if}
      </div>

      <div class="hero-info">
        <h1 class="hero-title">{result.title}</h1>

        <div class="hero-meta">
          {#if result.year}
            <span class="meta-pill year">
              <Icon name="date" size={12} />
              {result.year}
            </span>
          {/if}
          {#if result.contentType}
            <span class="meta-pill type">{result.contentType}</span>
          {/if}
          {#if result.ratingImdb}
            <span class="meta-pill rating">
              <Icon name="star" size={12} />
              {result.ratingImdb}
            </span>
          {/if}
        </div>

        <p class="torrent-count">
          {sortedTorrents.length}
          {$t('torrentSearch.availableTorrents')}
        </p>
      </div>
    </div>

    <div class="torrents-section">
      <h2 class="section-title">{$t('torrentSearch.availableTorrents')}</h2>

      {#if sortedTorrents.length === 0}
        <div class="empty-torrents">
          <Icon name="warning" size={24} />
          <span>{$t('torrentSearch.noResults')}</span>
        </div>
      {:else}
        <div class="torrents-table">
          <div class="table-head">
            <span>{$t('torrentSearch.filters.quality')}</span>
            <span>Codec</span>
            <span>{$t('torrentSearch.seeders')}</span>
            <span>Size</span>
            <span>{$t('torrentSearch.qualityScore')}</span>
            <span></span>
          </div>

          {#each sortedTorrents as torrent, i}
            {@const sizeNum = torrent.sizeBytes ?? 0}
            {@const scoreColor = getQualityScoreColor(torrent.qualityScore)}
            <div class="table-row">
              <span class="col-quality">
                {#if torrent.quality}
                  <span class="quality-tag">{torrent.quality}</span>
                {:else}
                  <span class="col-empty">—</span>
                {/if}
              </span>

              <span class="col-codec">
                {torrent.codec ?? '—'}
              </span>

              <span class="col-seeders">
                {#if torrent.seeders !== null && torrent.seeders > 0}
                  <span class="seeders-dot"></span>
                  {torrent.seeders}
                {:else}
                  <span class="col-empty">—</span>
                {/if}
              </span>

              <span class="col-size">
                {sizeNum > 0 ? formatSize(sizeNum) : '—'}
              </span>

              <span class="col-score">
                {#if torrent.qualityScore !== null}
                  <span class="score-badge" style="--score-color: {scoreColor}">
                    {torrent.qualityScore}
                  </span>
                {:else}
                  <span class="col-empty">—</span>
                {/if}
              </span>

              <span class="col-action">
                <button
                  class="download-row-btn"
                  onclick={() => handleDownload(torrent, i)}
                  disabled={downloadingIndex === i}
                  aria-label={$t('torrentSearch.download')}
                >
                  {#if downloadingIndex === i}
                    <Icon name="spinner" size={14} class="spin" />
                  {:else}
                    <Icon name="download" size={14} />
                  {/if}
                  <span>{$t('torrentSearch.download')}</span>
                </button>
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  :global(.rotate-180) {
    transform: rotate(180deg);
  }

  :global(.spin) {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .torrent-detail {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  /* Header */
  .detail-header {
    display: flex;
    align-items: center;
    padding: 10px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    flex-shrink: 0;
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

  /* Content */
  .detail-content {
    flex: 1;
    overflow-y: auto;
    padding: 16px 0;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  /* Hero */
  .hero {
    display: flex;
    gap: 20px;
    align-items: flex-start;
  }

  .poster-wrap {
    flex-shrink: 0;
    width: 120px;
    aspect-ratio: 2 / 3;
    border-radius: var(--radius, 8px);
    overflow: hidden;
    background: rgba(255, 255, 255, 0.04);
  }

  @media (min-width: 480px) {
    .poster-wrap {
      width: 160px;
    }
  }

  .poster {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .poster-fallback {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.2);
  }

  .hero-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .hero-title {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
    color: white;
    line-height: 1.2;
  }

  .hero-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .meta-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 500;
  }

  .meta-pill.year {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .meta-pill.type {
    background: rgba(99, 102, 241, 0.15);
    color: #a5b4fc;
    text-transform: capitalize;
  }

  .meta-pill.rating {
    background: rgba(251, 191, 36, 0.1);
    color: #fbbf24;
  }

  .torrent-count {
    margin: 0;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.45);
  }

  /* Torrents section */
  .torrents-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .section-title {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.8);
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .empty-torrents {
    display: flex;
    align-items: center;
    gap: 8px;
    color: rgba(255, 255, 255, 0.4);
    font-size: 14px;
    padding: 20px 0;
  }

  /* Table */
  .torrents-table {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .table-head,
  .table-row {
    display: grid;
    grid-template-columns: 90px 80px 80px 90px 110px 1fr;
    align-items: center;
    gap: 8px;
  }

  @media (max-width: 600px) {
    .table-head,
    .table-row {
      grid-template-columns: 70px 60px 60px 70px 80px 1fr;
      gap: 6px;
    }
  }

  .table-head {
    padding: 6px 10px;
    font-size: 11px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.35);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .table-row {
    padding: 8px 10px;
    border-radius: var(--radius-sm, 6px);
    transition: background 0.1s;
  }

  .table-row:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .col-quality {
    font-size: 13px;
    font-weight: 500;
  }

  .quality-tag {
    display: inline-block;
    padding: 2px 7px;
    background: rgba(99, 102, 241, 0.15);
    border-radius: 4px;
    color: #a5b4fc;
    font-size: 12px;
    font-weight: 600;
  }

  .col-codec {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.55);
    font-family: monospace;
  }

  .col-seeders {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.7);
  }

  .seeders-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #22c55e;
    flex-shrink: 0;
  }

  .col-size {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
  }

  .col-score {
    font-size: 12px;
  }

  .score-badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 4px;
    font-weight: 700;
    font-size: 12px;
    color: white;
    background: color-mix(in srgb, var(--score-color) 25%, transparent);
    border: 1px solid color-mix(in srgb, var(--score-color) 50%, transparent);
  }

  .col-empty {
    color: rgba(255, 255, 255, 0.2);
  }

  .col-action {
    display: flex;
    justify-content: flex-end;
  }

  .download-row-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 12px;
    background: rgba(255, 255, 255, 0.07);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: var(--radius-sm, 6px);
    color: white;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
  }

  .download-row-btn:not(:disabled):hover {
    background: var(--accent, #6366f1);
    border-color: var(--accent, #6366f1);
  }

  .download-row-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
