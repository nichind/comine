<script lang="ts">
  import { t } from '$lib/i18n';
  import Select from '$lib/components/ui/Select.svelte';
  import Chip from '$lib/components/ui/Chip.svelte';
  import type { IconName } from '$lib/components/ui/Icon.svelte';
  import type { VideoFormat } from '$lib/bindings/VideoFormat';
  import { makeVideoLabel, makeAudioLabel, getOriginalFormatIds } from './utils';

  interface Props {
    formats: VideoFormat[];
    selectedVideo: string;
    selectedAudio: string;
    loading?: boolean;
  }

  let {
    formats = [],
    selectedVideo = $bindable('best'),
    selectedAudio = $bindable('best'),
    loading = false,
  }: Props = $props();

  type PresetId = 'best' | 'music' | 'custom' | string;
  let selectedPreset = $state<PresetId>('best');

  let originalIds = $derived(getOriginalFormatIds(formats));

  function isOriginal(f: VideoFormat): boolean {
    return !originalIds || originalIds.has(f.formatId);
  }

  function originalFirst(a: VideoFormat, b: VideoFormat): number {
    const aOrig = isOriginal(a) ? 0 : 1;
    const bOrig = isOriginal(b) ? 0 : 1;
    return aOrig - bOrig;
  }

  let videoFormats = $derived(
    formats
      .filter((f) => {
        const hasVideo = !!f.vcodec && f.vcodec !== 'none';
        const hasAudio = !!f.acodec && f.acodec !== 'none';
        return hasVideo && !hasAudio;
      })
      .sort((a, b) => {
        const aH = parseInt(a.resolution?.split('x')[1] || '0') || 0;
        const bH = parseInt(b.resolution?.split('x')[1] || '0') || 0;
        if (bH !== aH) return bH - aH;
        return originalFirst(a, b);
      })
  );

  let audioFormats = $derived(
    formats
      .filter((f) => {
        const hasAudio = !!f.acodec && f.acodec !== 'none';
        const hasVideo = !!f.vcodec && f.vcodec !== 'none';
        return hasAudio && !hasVideo;
      })
      .sort((a, b) => {
        const diff = (b.abr ?? 0) - (a.abr ?? 0);
        if (diff !== 0) return diff;
        return originalFirst(a, b);
      })
  );

  let hasSeparateStreams = $derived(
    formats.some((f) => {
      const hasVideo = !!f.vcodec && f.vcodec !== 'none';
      const hasAudio = !!f.acodec && f.acodec !== 'none';
      return (hasVideo && !hasAudio) || (hasAudio && !hasVideo);
    })
  );

  let muxedFormats = $derived(
    formats
      .filter((f) => {
        const hasVideo = !!f.vcodec && f.vcodec !== 'none';
        const hasAudio = !!f.acodec && f.acodec !== 'none';
        return hasVideo && hasAudio;
      })
      .sort((a, b) => {
        const aH = parseInt(a.resolution?.split('x')[1] || '0') || 0;
        const bH = parseInt(b.resolution?.split('x')[1] || '0') || 0;
        if (bH !== aH) return bH - aH;
        return originalFirst(a, b);
      })
  );

  let useDualSelectors = $derived(hasSeparateStreams);

  let selectedMuxed = $state('best');

  const presets = $derived.by(() => {
    const available: { id: PresetId; label: string; icon: IconName }[] = [];
    available.push({ id: 'best', label: $t('download.tracks.presetBest'), icon: 'video' });

    const resolutionCounts = new Map<number, number>();
    videoFormats.forEach((f) => {
      let height = 0;
      if (f.resolution) {
        const parts = f.resolution.split('x');
        if (parts.length >= 2) {
          height = parseInt(parts[1]);
        } else if (parts.length === 1) {
          height = parseInt(f.resolution.replace('p', ''));
        }
      }
      if (height === 0 && f.formatNote) {
        const match = f.formatNote.match(/(\d+)p/);
        if (match) height = parseInt(match[1]);
      }
      if (height > 0) {
        resolutionCounts.set(height, (resolutionCounts.get(height) || 0) + 1);
      }
    });

    const sortedResolutions = Array.from(resolutionCounts.keys())
      .sort((a, b) => b - a)
      .slice(0, 3);

    sortedResolutions.forEach((height) => {
      available.push({ id: `${height}p`, label: `${height}p`, icon: 'video' });
    });

    if (
      audioFormats.length > 0 ||
      muxedFormats.some((f) => {
        const hasAudio = !!f.acodec && f.acodec !== 'none';
        return hasAudio;
      })
    ) {
      available.push({ id: 'music', label: $t('download.tracks.presetMusic'), icon: 'music' });
    }

    return available;
  });

  function findVideoByHeight(targetHeight: number): string | null {
    if (!videoFormats.length) return null;
    const candidates = videoFormats.filter((f) => {
      const h = parseInt(f.resolution?.split('x')[1] || '0') || 0;
      return h === targetHeight;
    });
    if (!candidates.length) return null;
    const original = candidates.find(isOriginal);
    return (original ?? candidates[0]).formatId;
  }

  function findBestOriginalAudio(): string {
    if (!audioFormats.length) return 'best';
    const original = audioFormats.find(isOriginal);
    if (original) return original.formatId;
    return 'best';
  }

  function findBestOriginalVideo(): string {
    if (!videoFormats.length) return 'best';
    const original = videoFormats.find(isOriginal);
    if (original) return original.formatId;
    return 'best';
  }

  function findBestOriginalMuxed(): string {
    if (!muxedFormats.length) return 'best';
    const original = muxedFormats.find(isOriginal);
    if (original) return original.formatId;
    return 'best';
  }

  function applyPreset(preset: PresetId) {
    selectedPreset = preset;

    const hasNonOriginalAudio = audioFormats.some((f) => !isOriginal(f));
    const hasNonOriginalVideo = videoFormats.some((f) => !isOriginal(f));
    const hasNonOriginalMuxed = muxedFormats.some((f) => !isOriginal(f));

    if (preset === 'best') {
      selectedVideo = hasNonOriginalVideo ? findBestOriginalVideo() : 'best';
      selectedAudio = hasNonOriginalAudio ? findBestOriginalAudio() : 'best';
      selectedMuxed = hasNonOriginalMuxed ? findBestOriginalMuxed() : 'best';
    } else if (preset === 'music') {
      selectedVideo = 'none';
      selectedAudio = hasNonOriginalAudio ? findBestOriginalAudio() : 'best';
    } else if (preset.endsWith('p')) {
      const height = parseInt(preset.slice(0, -1));
      selectedVideo = findVideoByHeight(height) || 'best';
      selectedAudio = hasNonOriginalAudio ? findBestOriginalAudio() : 'best';
    }
  }

  $effect(() => {
    if (selectedVideo === 'none' && selectedAudio === 'none') {
      selectedAudio = 'best';
    }
  });

  let bestVideoLabel = $derived.by(() => {
    if (videoFormats.length === 0) return '';
    const best = videoFormats.find(isOriginal) ?? videoFormats[0];
    return makeVideoLabel(best);
  });

  let bestAudioLabel = $derived.by(() => {
    if (audioFormats.length === 0) return '';
    const best = audioFormats.find(isOriginal) ?? audioFormats[0];
    return makeAudioLabel(best);
  });

  let prevFormatCount = $state(0);
  $effect(() => {
    const count = formats.length;
    if (count > 0 && prevFormatCount === 0 && selectedPreset === 'best') {
      applyPreset('best');
    }
    prevFormatCount = count;
  });

  let videoOptions = $derived(
    loading
      ? [{ value: 'best', label: $t('download.tracks.loading') }]
      : [
          {
            value: 'best',
            label: bestVideoLabel
              ? `${$t('download.tracks.bestQuality')} (${bestVideoLabel})`
              : $t('download.tracks.bestQuality'),
          },
          { value: 'none', label: $t('download.tracks.noVideo') },
          ...videoFormats.map((f) => ({ value: f.formatId, label: makeVideoLabel(f) })),
        ]
  );

  let audioOptions = $derived(
    loading
      ? [{ value: 'best', label: $t('download.tracks.loading') }]
      : [
          {
            value: 'best',
            label: bestAudioLabel
              ? `${$t('download.tracks.bestQuality')} (${bestAudioLabel})`
              : $t('download.tracks.bestQuality'),
          },
          ...(selectedVideo !== 'none'
            ? [{ value: 'none', label: $t('download.tracks.noAudio') }]
            : []),
          ...audioFormats.map((f) => ({ value: f.formatId, label: makeAudioLabel(f) })),
        ]
  );

  let muxedOptions = $derived(
    loading
      ? [{ value: 'best', label: $t('download.tracks.loading') }]
      : [
          { value: 'best', label: $t('download.tracks.best') },
          ...muxedFormats.map((f) => ({ value: f.formatId, label: makeVideoLabel(f) })),
        ]
  );

  function handleVideoChange(val: string) {
    selectedVideo = val;
    selectedPreset = 'custom';
  }

  function handleAudioChange(val: string) {
    selectedAudio = val;
    selectedPreset = 'custom';
  }

  function handleMuxedChange(val: string) {
    selectedMuxed = val;
    selectedVideo = val;
    selectedAudio = val === 'best' ? 'best' : 'none';
    selectedPreset = 'custom';
  }
</script>

<div class="format-selector">
  <div class="presets-row">
    {#each presets as p (p.id)}
      <Chip selected={selectedPreset === p.id} icon={p.icon} onclick={() => applyPreset(p.id)}>
        {p.label}
      </Chip>
    {/each}
  </div>

  {#if useDualSelectors}
    <div class="select-grid">
      <div class="select-group">
        <span class="select-label">{$t('download.tracks.video')}</span>
        <Select
          value={selectedVideo}
          options={videoOptions}
          onchange={handleVideoChange}
          disabled={loading}
        />
      </div>
      <div class="select-group">
        <span class="select-label">{$t('download.tracks.audio')}</span>
        <Select
          value={selectedAudio}
          options={audioOptions}
          onchange={handleAudioChange}
          disabled={loading}
        />
      </div>
    </div>
  {:else}
    <div class="select-single">
      <span class="select-label">{$t('download.tracks.quality')}</span>
      <Select
        value={selectedMuxed}
        options={muxedOptions}
        onchange={handleMuxedChange}
        disabled={loading}
      />
    </div>
  {/if}
</div>

<style>
  .format-selector {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .presets-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .select-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .select-single {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .select-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .select-label {
    font-size: 11px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.5);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding-left: 2px;
  }

  @media (max-width: 480px) {
    .select-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
