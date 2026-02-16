<script lang="ts">
  import { t } from '$lib/i18n';
  import MetaRow from '$lib/components/ui/MetaRow.svelte';
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
  <div class="music-metadata" style="--meta-label-width: 55px">
    {#if music.track}
      <MetaRow icon="music" label={$t('resolve.music.track')}>
        {music.track}
        {#if music.trackNumber}
          <span class="meta-dim">#{music.trackNumber}</span>
        {/if}
      </MetaRow>
    {/if}

    {#if music.artist}
      <MetaRow icon="user" label={$t('resolve.music.artist')}>
        {music.artist}
      </MetaRow>
    {/if}

    {#if music.album}
      <MetaRow icon="album" label={$t('resolve.music.album')}>
        {music.album}
        {#if music.albumArtist && music.albumArtist !== music.artist}
          <span class="meta-dim">by {music.albumArtist}</span>
        {/if}
      </MetaRow>
    {/if}

    {#if music.genre}
      <MetaRow icon="star" label={$t('resolve.music.genre')}>
        {music.genre}
      </MetaRow>
    {/if}

    {#if music.releaseYear}
      <MetaRow icon="clock" label={$t('resolve.music.year')}>
        {music.releaseYear}
      </MetaRow>
    {/if}

    {#if music.discNumber}
      <MetaRow icon="pie" label={$t('resolve.music.disc')}>
        {music.discNumber}
        {#if music.discCount}
          <span class="meta-dim">/ {music.discCount}</span>
        {/if}
      </MetaRow>
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

  .meta-dim {
    color: rgba(255, 255, 255, 0.4);
    font-size: 12px;
  }
</style>
