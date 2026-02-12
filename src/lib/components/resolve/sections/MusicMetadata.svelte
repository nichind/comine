<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { MusicInfo } from '$lib/bindings/MusicInfo';

  interface Props {
    music: MusicInfo;
  }

  let { music }: Props = $props();

  let hasContent = $derived(
    !!music.track || !!music.album || !!music.artist || !!music.genre || music.releaseYear != null
  );
</script>

{#if hasContent}
  <div class="music-metadata">
    {#if music.track}
      <div class="meta-row">
        <Icon name="music" size={14} />
        <span class="meta-label">{$t('resolve.music.track')}</span>
        <span class="meta-value">
          {music.track}
          {#if music.trackNumber}
            <span class="meta-dim">#{music.trackNumber}</span>
          {/if}
        </span>
      </div>
    {/if}

    {#if music.artist}
      <div class="meta-row">
        <Icon name="user" size={14} />
        <span class="meta-label">{$t('resolve.music.artist')}</span>
        <span class="meta-value">{music.artist}</span>
      </div>
    {/if}

    {#if music.album}
      <div class="meta-row">
        <Icon name="album" size={14} />
        <span class="meta-label">{$t('resolve.music.album')}</span>
        <span class="meta-value">
          {music.album}
          {#if music.albumArtist && music.albumArtist !== music.artist}
            <span class="meta-dim">by {music.albumArtist}</span>
          {/if}
        </span>
      </div>
    {/if}

    {#if music.genre}
      <div class="meta-row">
        <Icon name="star" size={14} />
        <span class="meta-label">{$t('resolve.music.genre')}</span>
        <span class="meta-value">{music.genre}</span>
      </div>
    {/if}

    {#if music.releaseYear}
      <div class="meta-row">
        <Icon name="clock" size={14} />
        <span class="meta-label">{$t('resolve.music.year')}</span>
        <span class="meta-value">{music.releaseYear}</span>
      </div>
    {/if}

    {#if music.discNumber}
      <div class="meta-row">
        <Icon name="pie" size={14} />
        <span class="meta-label">{$t('resolve.music.disc')}</span>
        <span class="meta-value">
          {music.discNumber}
          {#if music.discCount}
            <span class="meta-dim">/ {music.discCount}</span>
          {/if}
        </span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .music-metadata {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: var(--radius, 8px);
    border: 1px solid rgba(255, 255, 255, 0.05);
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
    min-width: 55px;
    flex-shrink: 0;
  }

  .meta-value {
    color: var(--text-1, white);
  }

  .meta-dim {
    color: rgba(255, 255, 255, 0.4);
    font-size: 12px;
  }
</style>
