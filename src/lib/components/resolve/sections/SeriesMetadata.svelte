<script lang="ts">
  import { t } from '$lib/i18n';
  import MetaRow from '$lib/components/ui/MetaRow.svelte';
  import type { SeriesInfo } from '$lib/bindings/SeriesInfo';

  interface Props {
    series: SeriesInfo;
  }

  let { series }: Props = $props();

  let hasContent = $derived(
    !!series.series ||
      series.seasonNumber != null ||
      series.episodeNumber != null ||
      !!series.episode
  );
</script>

{#if hasContent}
  <div class="series-metadata" style="--meta-label-width: 60px">
    {#if series.series}
      <MetaRow icon="video2" label={$t('resolve.series.series')}>
        {series.series}
      </MetaRow>
    {/if}

    {#if series.season || series.seasonNumber != null}
      <MetaRow icon="playlist" label={$t('resolve.series.season')}>
        {series.season ?? `Season ${series.seasonNumber}`}
      </MetaRow>
    {/if}

    {#if series.episode || series.episodeNumber != null}
      <MetaRow icon="play" label={$t('resolve.series.episode')}>
        {#if series.episodeNumber != null}
          <span class="episode-num">E{series.episodeNumber}</span>
        {/if}
        {#if series.episode}
          {series.episode}
        {/if}
      </MetaRow>
    {/if}
  </div>
{/if}

<style>
  .series-metadata {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: var(--radius, 8px);
    border: 1px solid rgba(255, 255, 255, 0.05);
  }

  .episode-num {
    font-weight: 600;
    color: var(--accent, #6366f1);
    margin-right: 4px;
  }
</style>
