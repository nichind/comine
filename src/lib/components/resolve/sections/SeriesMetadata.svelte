<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
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
  <div class="series-metadata">
    {#if series.series}
      <div class="meta-row">
        <Icon name="video2" size={14} />
        <span class="meta-label">{$t('resolve.series.series')}</span>
        <span class="meta-value">{series.series}</span>
      </div>
    {/if}

    {#if series.season || series.seasonNumber != null}
      <div class="meta-row">
        <Icon name="playlist" size={14} />
        <span class="meta-label">{$t('resolve.series.season')}</span>
        <span class="meta-value">
          {series.season ?? `Season ${series.seasonNumber}`}
        </span>
      </div>
    {/if}

    {#if series.episode || series.episodeNumber != null}
      <div class="meta-row">
        <Icon name="play" size={14} />
        <span class="meta-label">{$t('resolve.series.episode')}</span>
        <span class="meta-value">
          {#if series.episodeNumber != null}
            <span class="episode-num">E{series.episodeNumber}</span>
          {/if}
          {#if series.episode}
            {series.episode}
          {/if}
        </span>
      </div>
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
    min-width: 60px;
    flex-shrink: 0;
  }

  .meta-value {
    color: var(--text-1, white);
  }

  .episode-num {
    font-weight: 600;
    color: var(--accent, #6366f1);
    margin-right: 4px;
  }
</style>
