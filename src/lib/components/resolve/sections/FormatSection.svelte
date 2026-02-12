<script lang="ts">
  import { slide } from 'svelte/transition';
  import { skeleton } from '$lib/actions/skeleton';
  import FormatSelector from '$lib/components/resolve/FormatSelector.svelte';
  import ClipRangeSelector from '$lib/components/builders/ClipRangeSelector.svelte';
  import type { VideoFormat } from '$lib/bindings/VideoFormat';
  import type { Storyboard } from '$lib/bindings/Storyboard';
  import type { Chapter } from '$lib/bindings/Chapter';
  import type { ClipRange } from '$lib/bindings/ClipRange';

  interface Props {
    formats: VideoFormat[];
    selectedVideo?: string;
    selectedAudio?: string;
    loading?: boolean;
    duration?: number | null;
    storyboard?: Storyboard | null;
    chapters?: Chapter[] | null;
    clipRanges?: ClipRange[];
  }

  let {
    formats,
    selectedVideo = $bindable('best'),
    selectedAudio = $bindable('best'),
    loading = false,
    duration = null,
    storyboard = null,
    chapters = null,
    clipRanges = $bindable([]),
  }: Props = $props();

  let showClip = $derived(!!(duration && duration > 0));
</script>

{#if formats.length > 0 || loading}
  <div class="format-section">
    <div
      use:skeleton={{ loading: loading && formats.length === 0, minHeight: '100px', radius: '8px' }}
    >
      {#if formats.length > 0 || !loading}
        <FormatSelector {formats} bind:selectedVideo bind:selectedAudio {loading} />
      {/if}
    </div>

    {#if loading || showClip}
      <div
        class="clip-area"
        use:skeleton={{ loading: loading && !duration, minHeight: '80px', radius: '8px' }}
        transition:slide={{ axis: 'y' }}
      >
        {#if showClip && duration}
          <ClipRangeSelector
            {duration}
            bind:ranges={clipRanges}
            disabled={loading}
            {storyboard}
            {chapters}
          />
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .format-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .clip-area {
    min-height: 0;
  }
</style>
