<script lang="ts">
  import { skeleton } from '$lib/actions/skeleton';
  import SponsorBlockOptions from '$lib/components/resolve/SponsorBlockOptions.svelte';
  import EmbedOptions from '$lib/components/resolve/EmbedOptions.svelte';

  interface Props {
    skipSponsors?: boolean;
    skipIntros?: boolean;
    skipSelfPromo?: boolean;
    skipInteraction?: boolean;
    embedChapters?: boolean;
    embedThumbnail?: boolean;
    embedMetadata?: boolean;
    embedSubs?: boolean;
    subLangs?: string;
    showSponsorBlock?: boolean;
    showEmbedOptions?: boolean;
    loading?: boolean;
    variant?: 'chip' | 'checkbox';
  }

  let {
    skipSponsors = $bindable(false),
    skipIntros = $bindable(false),
    skipSelfPromo = $bindable(false),
    skipInteraction = $bindable(false),
    embedChapters = $bindable(true),
    embedThumbnail = $bindable(true),
    embedMetadata = $bindable(true),
    embedSubs = $bindable(false),
    subLangs = $bindable('en,ru'),
    showSponsorBlock = true,
    showEmbedOptions = true,
    loading = false,
    variant = 'chip',
  }: Props = $props();
</script>

{#if showSponsorBlock || showEmbedOptions}
  <div class="options-section">
    {#if showSponsorBlock}
      <div use:skeleton={{ loading, minHeight: '48px', radius: '8px' }}>
        {#if !loading}
          <SponsorBlockOptions
            bind:skipSponsors
            bind:skipIntros
            bind:skipSelfPromo
            bind:skipInteraction
            {variant}
          />
        {/if}
      </div>
    {/if}

    {#if showEmbedOptions}
      <div use:skeleton={{ loading, minHeight: '48px', radius: '8px' }}>
        {#if !loading}
          <EmbedOptions
            bind:embedChapters
            bind:embedThumbnail
            bind:embedMetadata
            bind:embedSubs
            bind:subLangs
            {variant}
          />
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .options-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
</style>
