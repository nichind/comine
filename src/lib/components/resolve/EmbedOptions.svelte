<script lang="ts">
  import { slide } from 'svelte/transition';
  import { t } from '$lib/i18n';
  import { skeleton } from '$lib/actions/skeleton';
  import Chip from '$lib/components/ui/Chip.svelte';
  import Checkbox from '$lib/components/ui/Checkbox.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';

  interface Props {
    embedChapters?: boolean;
    embedThumbnail?: boolean;
    embedMetadata?: boolean;
    embedSubs?: boolean;
    subLangs?: string;
    loading?: boolean;
    variant?: 'chip' | 'checkbox';
    showLabel?: boolean;
  }

  let {
    embedChapters = $bindable(true),
    embedThumbnail = $bindable(true),
    embedMetadata = $bindable(true),
    embedSubs = $bindable(false),
    subLangs = $bindable('en,ru'),
    loading = false,
    variant = 'chip',
    showLabel = true,
  }: Props = $props();
</script>

<div class="embed-options" use:skeleton={{ loading, minHeight: '48px', radius: '8px' }}>
  {#if !loading}
    {#if showLabel}
      <span class="section-label">{$t('download.tracks.embedOptions')}</span>
    {/if}

    {#if variant === 'chip'}
      <div class="chips-row">
        <Chip
          icon="checklist"
          selected={embedChapters}
          onclick={() => (embedChapters = !embedChapters)}
        >
          {$t('download.tracks.embedChapters')}
        </Chip>
        <Chip
          icon="image"
          selected={embedThumbnail}
          onclick={() => (embedThumbnail = !embedThumbnail)}
        >
          {$t('download.tracks.embedThumbnail')}
        </Chip>
        <Chip icon="info" selected={embedMetadata} onclick={() => (embedMetadata = !embedMetadata)}>
          {$t('download.tracks.embedMetadata')}
        </Chip>
        <Chip icon="text" selected={embedSubs} onclick={() => (embedSubs = !embedSubs)}>
          {$t('download.tracks.embedSubs')}
        </Chip>
      </div>
    {:else}
      <div class="checkbox-grid">
        <Checkbox bind:checked={embedChapters} label={$t('download.tracks.embedChapters')} />
        <Checkbox bind:checked={embedThumbnail} label={$t('download.tracks.embedThumbnail')} />
        <Checkbox bind:checked={embedMetadata} label={$t('download.tracks.embedMetadata')} />
        <Checkbox bind:checked={embedSubs} label={$t('download.tracks.embedSubs')} />
      </div>
    {/if}

    {#if embedSubs}
      <div class="sub-input-container" transition:slide={{ axis: 'y', duration: 200 }}>
        <div class="input-icon"><Icon name="text" size={14} /></div>
        <input
          type="text"
          class="clean-input"
          bind:value={subLangs}
          placeholder="en,ru (comma separated)"
        />
      </div>
    {/if}
  {/if}
</div>

<style>
  .embed-options {
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

  .sub-input-container {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
    height: 36px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius, 10px);
    margin-top: 2px;
  }

  .sub-input-container:focus-within {
    border-color: var(--accent);
    background: rgba(255, 255, 255, 0.08);
  }

  .input-icon {
    color: rgba(255, 255, 255, 0.5);
    display: flex;
    align-items: center;
  }

  .clean-input {
    background: transparent;
    border: none;
    color: white;
    width: 100%;
    font-size: 13px;
    font-weight: 500;
  }

  .clean-input:focus {
    outline: none;
  }

  .clean-input::placeholder {
    color: rgba(255, 255, 255, 0.3);
    font-weight: 400;
  }
</style>
