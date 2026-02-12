<script lang="ts">
  import { t } from '$lib/i18n';
  import { skeleton } from '$lib/actions/skeleton';
  import Chip from '$lib/components/ui/Chip.svelte';
  import Checkbox from '$lib/components/ui/Checkbox.svelte';

  interface Props {
    skipSponsors?: boolean;
    skipIntros?: boolean;
    skipSelfPromo?: boolean;
    skipInteraction?: boolean;
    loading?: boolean;
    variant?: 'chip' | 'checkbox';
    showLabel?: boolean;
  }

  let {
    skipSponsors = $bindable(false),
    skipIntros = $bindable(false),
    skipSelfPromo = $bindable(false),
    skipInteraction = $bindable(false),
    loading = false,
    variant = 'chip',
    showLabel = true,
  }: Props = $props();
</script>

<div class="sponsorblock-options" use:skeleton={{ loading, minHeight: '48px', radius: '8px' }}>
  {#if !loading}
    {#if showLabel}
      <span class="section-label">SponsorBlock</span>
    {/if}

    {#if variant === 'chip'}
      <div class="chips-row">
        <Chip icon="star" selected={skipSponsors} onclick={() => (skipSponsors = !skipSponsors)}>
          {$t('download.tracks.skipSponsors')}
        </Chip>
        <Chip
          icon="double_arrow_right"
          selected={skipIntros}
          onclick={() => (skipIntros = !skipIntros)}
        >
          {$t('download.tracks.skipIntros')}
        </Chip>
        <Chip icon="user" selected={skipSelfPromo} onclick={() => (skipSelfPromo = !skipSelfPromo)}>
          {$t('download.tracks.skipSelfPromo')}
        </Chip>
        <Chip
          icon="cursor"
          selected={skipInteraction}
          onclick={() => (skipInteraction = !skipInteraction)}
        >
          {$t('download.tracks.skipInteraction')}
        </Chip>
      </div>
    {:else}
      <div class="checkbox-grid">
        <Checkbox bind:checked={skipSponsors} label={$t('download.tracks.skipSponsors')} />
        <Checkbox bind:checked={skipIntros} label={$t('download.tracks.skipIntros')} />
        <Checkbox bind:checked={skipSelfPromo} label={$t('download.tracks.skipSelfPromo')} />
        <Checkbox bind:checked={skipInteraction} label={$t('download.tracks.skipInteraction')} />
      </div>
    {/if}
  {/if}
</div>

<style>
  .sponsorblock-options {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .section-label {
    font-size: 11px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .chips-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .checkbox-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 8px;
  }
</style>
