<script lang="ts">
  import type { UrlInfo } from '$lib/bindings/UrlInfo';
  import { formatDuration } from '$lib/utils/format';
  import { skeleton } from '$lib/actions/skeleton';
  import ThumbnailGlow from '../media/ThumbnailGlow.svelte';
  import Icon from '../ui/Icon.svelte';
  import { t } from '$lib/i18n';
  import { settings } from '$lib/stores/settings';

  interface Props {
    info: UrlInfo | null;
    loading?: boolean;
    onOpenChannel?: (
      channelUrl: string,
      previewData?: { name?: string; thumbnail?: string }
    ) => void;
    currentUrl?: string;
    children?: import('svelte').Snippet;
    prefetchedInfo?: { title?: string; thumbnail?: string; author?: string; duration?: number };
  }

  let {
    info,
    loading = false,
    onOpenChannel,
    currentUrl,
    children,
    prefetchedInfo,
  }: Props = $props();

  let isCollection = $derived(!!(info?.entries && info.entries.length > 0));
  let title = $derived(info?.title ?? prefetchedInfo?.title ?? null);
  let author = $derived.by(() => {
    if (info) return info.channel ?? info.uploader ?? null;
    return prefetchedInfo?.author ?? null;
  });
  let thumbnail = $derived(info?.thumbnail ?? prefetchedInfo?.thumbnail ?? null);
  let duration = $derived(
    info?.duration ? Number(info.duration) : (prefetchedInfo?.duration ?? null)
  );
  let description = $derived(info?.description?.trim().replace(/\n{3,}/g, '\n\n') ?? null);
  let viewCount = $derived(info?.viewCount ? Number(info.viewCount) : null);
  let likeCount = $derived(info?.likeCount ? Number(info.likeCount) : null);
  let uploadDate = $derived(info?.uploadDate ?? null);
  let channelUrl = $derived(info?.channelUrl ?? null);
  let entryCount = $derived.by(() => {
    if (!info) return null;
    const total = info.pagination?.totalCount ?? info.playlistCount;
    if (total) return total;
    if (info.entries && info.entries.length > 0) return info.entries.length;
    return null;
  });

  let canOpenChannel = $derived(!!(channelUrl && onOpenChannel && channelUrl !== currentUrl));

  let showDescriptionModal = $state(false);
  let thumbnailError = $state(false);

  function formatCount(num: number | null): string {
    if (!num) return '';
    if (num >= 1_000_000) return (num / 1_000_000).toFixed(1).replace(/\.0$/, '') + 'M';
    if (num >= 1_000) return (num / 1_000).toFixed(1).replace(/\.0$/, '') + 'K';
    return num.toString();
  }

  function formatDate(dateStr: string | null): string {
    if (!dateStr || dateStr.length !== 8) return '';
    const year = dateStr.slice(0, 4);
    const month = dateStr.slice(4, 6);
    const day = dateStr.slice(6, 8);
    const date = new Date(`${year}-${month}-${day}`);
    return date.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
  }

  function handleOpenChannel() {
    if (channelUrl && onOpenChannel) {
      onOpenChannel(channelUrl, { name: author ?? undefined });
    }
  }
</script>

{#if $settings.builderThumbnailGlow && thumbnail}
  <ThumbnailGlow thumbnailUrl={thumbnail} enabled={true} />
{/if}

<div class="item-header">
  <div class="thumb-container" use:skeleton={{ loading: loading && !thumbnail, radius: '8px' }}>
    {#if thumbnail && !thumbnailError}
      <img
        src={thumbnail}
        alt=""
        class="thumb"
        decoding="async"
        onerror={() => (thumbnailError = true)}
      />
      {#if duration}
        <span class="thumb-duration">{formatDuration(duration)}</span>
      {/if}
    {:else if !loading}
      <div class="thumb placeholder">
        <Icon name="video" size={32} />
      </div>
    {/if}
  </div>

  <div class="info">
    <h1
      class="title"
      use:skeleton={{ loading: loading && !title, minWidth: '200px', randomWidth: [40, 90] }}
    >
      {title ?? '\u00A0'}
    </h1>

    <div class="meta">
      {#if author || loading}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span
          class="meta-item channel"
          class:clickable={canOpenChannel}
          onclick={canOpenChannel ? handleOpenChannel : undefined}
        >
          <span
            class="channel-avatar"
            use:skeleton={{
              loading: loading && !author,
              circle: true,
              minWidth: '20px',
              minHeight: '20px',
            }}
          >
            {#if !loading || author}
              <Icon name="user" size={12} />
            {/if}
          </span>
          <span
            class="channel-name"
            use:skeleton={{
              loading: loading && !author,
              minWidth: '60px',
              randomWidth: [60, 100],
              radius: '4px',
            }}
          >
            {author ?? '\u00A0'}
          </span>
          {#if canOpenChannel}
            <Icon name="alt_arrow_rigth" size={12} class="channel-arrow" />
          {/if}
        </span>
      {/if}

      {#if viewCount}
        <span class="meta-item">
          <Icon name="eye_line_duotone" size={14} />
          {formatCount(viewCount)}
        </span>
      {:else if loading}
        <span class="meta-item" use:skeleton={{ loading, minWidth: '50px' }}>&nbsp;</span>
      {/if}

      {#if likeCount}
        <span class="meta-item">
          <Icon name="heart" size={14} />
          {formatCount(likeCount)}
        </span>
      {:else if loading}
        <span class="meta-item" use:skeleton={{ loading, minWidth: '50px' }}>&nbsp;</span>
      {/if}

      {#if uploadDate}
        <span class="meta-item">
          <Icon name="date" size={14} />
          {formatDate(uploadDate)}
        </span>
      {/if}

      {#if entryCount}
        <span class="meta-item">
          <Icon name="playlist" size={14} />
          {entryCount}
          {entryCount === 1 ? 'item' : 'items'}
        </span>
      {/if}

      {#if duration && !viewCount && !loading}
        <span class="meta-item">
          <Icon name="clock" size={14} />
          {formatDuration(duration)}
        </span>
      {/if}
    </div>

    {@render children?.()}

    {#if description || loading}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="desc-preview"
        class:clickable={!!description}
        onclick={description ? () => (showDescriptionModal = true) : undefined}
        use:skeleton={{ loading: loading && !description, minHeight: '20px', radius: '4px' }}
      >
        {#if description}
          <p class="desc-text">{description}</p>
        {/if}
      </div>
    {/if}
  </div>
</div>

{#if showDescriptionModal && description}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="modal-overlay" onclick={() => (showDescriptionModal = false)}>
    <div class="modal-content" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <h2>{$t('download.tracks.description')}</h2>
        <button class="modal-close" onclick={() => (showDescriptionModal = false)}>
          <Icon name="close" size={18} />
        </button>
      </div>
      <div class="modal-body">
        <p class="full-description">{description}</p>
      </div>
    </div>
  </div>
{/if}

<style>
  .item-header {
    display: flex;
    gap: 16px;
    padding-bottom: 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .thumb-container {
    position: relative;
    flex-shrink: 0;
    width: 160px;
    aspect-ratio: 16 / 9;
    border-radius: var(--radius, 8px);
    overflow: hidden;
    background: rgba(255, 255, 255, 0.04);
  }

  @media (min-width: 480px) {
    .thumb-container {
      width: 200px;
    }
  }

  .thumb {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .thumb.placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.3);
  }

  .thumb-duration {
    position: absolute;
    bottom: 6px;
    right: 6px;
    padding: 2px 6px;
    background: rgba(0, 0, 0, 0.75);
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
    color: white;
  }

  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .title {
    font-size: 16px;
    font-weight: 600;
    color: white;
    margin: 0;
    line-height: 1.3;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.6);
  }

  .meta-item {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }

  .channel {
    font-weight: 500;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0;
  }

  .channel.clickable {
    cursor: pointer;
    transition: all 0.15s;
    padding: 4px 8px 4px 4px;
    margin: -4px;
    border-radius: var(--radius-sm, 6px);
  }

  .channel.clickable:hover {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .channel-avatar {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 50%;
    color: rgba(255, 255, 255, 0.5);
  }

  :global(.channel-arrow) {
    opacity: 0.5;
    transition: opacity 0.15s;
  }

  .channel.clickable:hover :global(.channel-arrow) {
    opacity: 1;
  }

  .desc-preview {
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin-top: 4px;
    cursor: pointer;
  }

  .desc-preview.clickable:hover .desc-text {
    color: rgba(255, 255, 255, 0.6);
  }

  .desc-text {
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
    color: rgba(255, 255, 255, 0.5);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 20px;
  }

  @media (max-width: 600px) {
    .item-header {
      flex-direction: column;
    }

    .thumb-container {
      width: 100%;
      max-width: 100%;
      aspect-ratio: 16/9;
    }

    .info {
      width: 100%;
    }
  }

  .modal-content {
    background: var(--surface, #1e1e2e);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-lg, 12px);
    width: 100%;
    max-width: 500px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .modal-header h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: white;
  }

  .modal-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    background: rgba(255, 255, 255, 0.06);
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    transition: all 0.15s;
  }

  .modal-close:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .modal-body {
    padding: 20px;
    overflow-y: auto;
  }

  .full-description {
    margin: 0;
    font-size: 14px;
    line-height: 1.6;
    color: rgba(255, 255, 255, 0.8);
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
