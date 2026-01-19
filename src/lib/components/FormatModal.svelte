<script lang="ts">
  import { t } from '$lib/i18n';
  import { getProxyConfig, getSettings } from '$lib/stores/settings';
  import { queue } from '$lib/stores/queue';
  import { logs } from '$lib/stores/logs';
  import { toast } from './Toast.svelte';
  import Modal from './Modal.svelte';
  import Icon from './Icon.svelte';
  import { formatSize, formatDuration } from '$lib/utils/format';
  import { getVideoFormatsBackend } from '$lib/backend/mediaBackend';

  interface VideoFormat {
    format_id: string;
    ext: string;
    resolution?: string | null;
    fps?: number | null;
    vcodec?: string | null;
    acodec?: string | null;
    filesize?: number | null;
    filesize_approx?: number | null;
    tbr?: number | null;
    vbr?: number | null;
    abr?: number | null;
    asr?: number | null;
    format_note?: string | null;
    has_video: boolean;
    has_audio: boolean;
    quality?: number | null;
  }

  interface VideoFormats {
    title: string;
    author?: string | null;
    thumbnail?: string | null;
    duration?: number | null;
    formats: VideoFormat[];
  }

  interface Props {
    open: boolean;
    url: string;
    cookiesFromBrowser?: string;
    customCookies?: string;
    onclose?: () => void;
  }

  let {
    open = $bindable(),
    url,
    cookiesFromBrowser = '',
    customCookies = '',
    onclose,
  }: Props = $props();

  let loading = $state(true);
  let error = $state<string | null>(null);
  let formats = $state<VideoFormats | null>(null);

  // For track builder: null means "No video" / "No audio"
  let selectedVideoFormat = $state<string | null>(null);
  let selectedAudioFormat = $state<string | null>(null);
  // For combined/audio tabs
  let selectedFormat = $state<string | null>(null);

  let viewMode = $state<'builder' | 'combined' | 'audio'>('builder');

  // Filter formats
  let videoOnlyFormats = $derived(
    formats?.formats
      .filter((f) => f.has_video && !f.has_audio)
      .sort((a, b) => {
        const aHeight = parseInt(a.resolution?.split('x')[1] || '0') || 0;
        const bHeight = parseInt(b.resolution?.split('x')[1] || '0') || 0;
        return bHeight - aHeight;
      }) ?? []
  );

  let audioOnlyFormats = $derived(
    formats?.formats
      .filter((f) => f.has_audio && !f.has_video)
      .sort((a, b) => {
        return (b.abr ?? 0) - (a.abr ?? 0);
      }) ?? []
  );

  let combinedFormats = $derived(
    formats?.formats
      .filter((f) => f.has_video && f.has_audio)
      .sort((a, b) => {
        const aHeight = parseInt(a.resolution?.split('x')[1] || '0') || 0;
        const bHeight = parseInt(b.resolution?.split('x')[1] || '0') || 0;
        return bHeight - aHeight;
      }) ?? []
  );

  // Load formats when modal opens
  $effect(() => {
    if (open && url) {
      loadFormats();
    }
  });

  async function loadFormats() {
    loading = true;
    error = null;
    formats = null;
    selectedVideoFormat = null;
    selectedAudioFormat = null;
    selectedFormat = null;

    try {
      logs.info('formats', `Fetching formats for: ${url}`);

      const currentSettings = getSettings();
      const backendFormats = await getVideoFormatsBackend({
        url,
        cookiesFromBrowser: cookiesFromBrowser || '',
        customCookies: customCookies || '',
        proxyConfig: getProxyConfig(),
        youtubePlayerClient: currentSettings.usePlayerClientForExtraction
          ? currentSettings.youtubePlayerClient
          : null,
      });

      formats = {
        title: backendFormats.title || url,
        author: backendFormats.author ?? null,
        thumbnail: backendFormats.thumbnail ?? null,
        duration: backendFormats.duration ?? null,
        formats: (backendFormats.formats ?? []) as VideoFormat[],
      };

      logs.info('formats', `Got ${formats.formats.length} formats`);

      // Auto-select best video and audio for builder
      if (videoOnlyFormats.length > 0) {
        selectedVideoFormat = videoOnlyFormats[0].format_id;
      }
      if (audioOnlyFormats.length > 0) {
        selectedAudioFormat = audioOnlyFormats[0].format_id;
      }
    } catch (e) {
      error = String(e);
      logs.error('formats', `Failed to get formats: ${e}`);
    } finally {
      loading = false;
    }
  }

  function getFormatSize(f: VideoFormat): string {
    const size = f.filesize ?? f.filesize_approx;
    if (size) {
      return formatSize(size);
    }
    if (f.tbr) {
      return `~${Math.round(f.tbr)} kbps`;
    }
    return '—';
  }

  function getCodecDisplay(f: VideoFormat): string {
    const parts: string[] = [];
    if (f.vcodec && f.vcodec !== 'none') {
      let codec = f.vcodec;
      if (codec.startsWith('avc1')) codec = 'H.264';
      else if (codec.startsWith('av01')) codec = 'AV1';
      else if (codec.startsWith('vp9') || codec.startsWith('vp09')) codec = 'VP9';
      else if (codec.startsWith('hev1') || codec.startsWith('hvc1')) codec = 'H.265';
      parts.push(codec);
    }
    if (f.acodec && f.acodec !== 'none') {
      let codec = f.acodec;
      if (codec.startsWith('mp4a')) codec = 'AAC';
      else if (codec.startsWith('opus')) codec = 'Opus';
      parts.push(codec);
    }
    return parts.join(' + ') || '—';
  }

  function downloadSelected() {
    if (!formats) return;

    let formatString: string;
    let downloadMode: 'auto' | 'audio' | 'mute' = 'auto';

    if (viewMode === 'builder') {
      // Track builder - combine selected video and audio
      if (selectedVideoFormat && selectedAudioFormat) {
        formatString = `${selectedVideoFormat}+${selectedAudioFormat}`;
        downloadMode = 'auto';
      } else if (selectedVideoFormat) {
        // Video only (no audio)
        formatString = selectedVideoFormat;
        downloadMode = 'mute';
      } else if (selectedAudioFormat) {
        // Audio only (no video)
        formatString = selectedAudioFormat;
        downloadMode = 'audio';
      } else {
        toast.error($t('download.formats.selectAtLeastOne'));
        return;
      }
    } else if (viewMode === 'combined') {
      if (!selectedFormat) {
        toast.error($t('download.formats.selectFormat'));
        return;
      }
      formatString = selectedFormat;
      downloadMode = 'auto';
    } else if (viewMode === 'audio') {
      if (!selectedFormat) {
        toast.error($t('download.formats.selectFormat'));
        return;
      }
      formatString = selectedFormat;
      downloadMode = 'audio';
    } else {
      return;
    }

    logs.info('formats', `Downloading with format: ${formatString}, mode: ${downloadMode}`);

    // Add to queue with specific format ID
    queue.add(url, {
      videoQuality: formatString,
      downloadMode: downloadMode,
      audioQuality: 'best',
      prefetchedInfo: {
        title: formats.title,
        author: formats.author ?? undefined,
        thumbnail: formats.thumbnail ?? undefined,
        duration: formats.duration ?? undefined,
      },
    });

    toast.success(`${$t('download.formats.addedToQueue')}: ${formats.title}`);
    open = false;
    onclose?.();
  }

  function handleClose() {
    open = false;
    onclose?.();
  }
</script>

<Modal bind:open title={$t('download.formats.title')} onclose={handleClose}>
  <div class="format-modal-content">
    {#if loading}
      <div class="loading">
        <Icon name="spinner" size={32} />
        <span>{$t('download.formats.loading')}</span>
      </div>
    {:else if error}
      <div class="error">
        <Icon name="warning" size={24} />
        <span>{error}</span>
        <button class="retry-btn" onclick={loadFormats}>
          <Icon name="restart" size={16} />
          {$t('common.retry')}
        </button>
      </div>
    {:else if formats}
      <!-- Header with video info -->
      <div class="video-info">
        {#if formats.thumbnail}
          <img src={formats.thumbnail} alt="" class="thumbnail" />
        {/if}
        <div class="video-meta">
          <h3 class="video-title">{formats.title}</h3>
          {#if formats.duration}
            <span class="video-duration">{formatDuration(formats.duration)}</span>
          {/if}
        </div>
      </div>

      <!-- View mode tabs -->
      <div class="tabs">
        <button
          class="tab"
          class:active={viewMode === 'builder'}
          onclick={() => (viewMode = 'builder')}
        >
          <Icon name="tuning" size={16} />
          {$t('download.formats.trackBuilder')}
        </button>
        <button
          class="tab"
          class:active={viewMode === 'combined'}
          onclick={() => (viewMode = 'combined')}
        >
          <Icon name="video" size={16} />
          {$t('download.formats.combined')} ({combinedFormats.length})
        </button>
        <button
          class="tab"
          class:active={viewMode === 'audio'}
          onclick={() => (viewMode = 'audio')}
        >
          <Icon name="music" size={16} />
          {$t('download.formats.audioOnly')} ({audioOnlyFormats.length})
        </button>
      </div>

      <!-- Format lists -->
      <div class="formats-container">
        {#if viewMode === 'builder'}
          <!-- Track Builder - select video + audio separately -->
          <div class="split-view">
            <div class="split-column">
              <h4>{$t('download.formats.videoTrack')}</h4>
              <div class="format-list">
                <!-- No Video option -->
                <button
                  class="format-item no-track"
                  class:selected={selectedVideoFormat === null}
                  onclick={() => (selectedVideoFormat = null)}
                >
                  <div class="format-main">
                    <Icon name="cross" size={14} />
                    <span class="format-resolution">{$t('download.formats.noVideo')}</span>
                  </div>
                </button>
                {#each videoOnlyFormats as fmt}
                  <button
                    class="format-item"
                    class:selected={selectedVideoFormat === fmt.format_id}
                    onclick={() => (selectedVideoFormat = fmt.format_id)}
                  >
                    <div class="format-main">
                      <span class="format-resolution">{fmt.resolution || '—'}</span>
                      {#if fmt.fps}
                        <span class="format-fps">{Math.round(fmt.fps)}fps</span>
                      {/if}
                      <span class="format-ext">{fmt.ext}</span>
                    </div>
                    <div class="format-details">
                      <span class="format-codec">{getCodecDisplay(fmt)}</span>
                      <span class="format-size">{getFormatSize(fmt)}</span>
                    </div>
                  </button>
                {/each}
              </div>
            </div>
            <div class="split-column">
              <h4>{$t('download.formats.audioTrack')}</h4>
              <div class="format-list">
                <!-- No Audio option -->
                <button
                  class="format-item no-track"
                  class:selected={selectedAudioFormat === null}
                  onclick={() => (selectedAudioFormat = null)}
                >
                  <div class="format-main">
                    <Icon name="cross" size={14} />
                    <span class="format-resolution">{$t('download.formats.noAudio')}</span>
                  </div>
                </button>
                {#each audioOnlyFormats as fmt}
                  <button
                    class="format-item"
                    class:selected={selectedAudioFormat === fmt.format_id}
                    onclick={() => (selectedAudioFormat = fmt.format_id)}
                  >
                    <div class="format-main">
                      <span class="format-resolution"
                        >{fmt.abr ? `${Math.round(fmt.abr)} kbps` : 'audio'}</span
                      >
                      {#if fmt.asr}
                        <span class="format-fps">{Math.round(fmt.asr / 1000)}kHz</span>
                      {/if}
                      <span class="format-ext">{fmt.ext}</span>
                    </div>
                    <div class="format-details">
                      <span class="format-codec">{getCodecDisplay(fmt)}</span>
                      <span class="format-size">{getFormatSize(fmt)}</span>
                    </div>
                  </button>
                {/each}
              </div>
            </div>
          </div>
        {:else if viewMode === 'combined'}
          <!-- Combined video+audio formats -->
          <div class="format-list scrollable-list">
            {#each combinedFormats as fmt}
              <button
                class="format-item"
                class:selected={selectedFormat === fmt.format_id}
                onclick={() => (selectedFormat = fmt.format_id)}
              >
                <div class="format-main">
                  <span class="format-resolution">{fmt.resolution || '—'}</span>
                  {#if fmt.fps}
                    <span class="format-fps">{Math.round(fmt.fps)}fps</span>
                  {/if}
                  <span class="format-ext">{fmt.ext}</span>
                </div>
                <div class="format-details">
                  <span class="format-codec">{getCodecDisplay(fmt)}</span>
                  <span class="format-size">{getFormatSize(fmt)}</span>
                </div>
                {#if fmt.format_note}
                  <span class="format-note">{fmt.format_note}</span>
                {/if}
              </button>
            {/each}
            {#if combinedFormats.length === 0}
              <div class="no-formats">{$t('download.formats.noCombined')}</div>
            {/if}
          </div>
        {:else if viewMode === 'audio'}
          <!-- Audio-only formats -->
          <div class="format-list scrollable-list">
            {#each audioOnlyFormats as fmt}
              <button
                class="format-item"
                class:selected={selectedFormat === fmt.format_id}
                onclick={() => (selectedFormat = fmt.format_id)}
              >
                <div class="format-main">
                  <span class="format-resolution"
                    >{fmt.abr ? `${Math.round(fmt.abr)} kbps` : 'audio'}</span
                  >
                  {#if fmt.asr}
                    <span class="format-fps">{Math.round(fmt.asr / 1000)}kHz</span>
                  {/if}
                  <span class="format-ext">{fmt.ext}</span>
                </div>
                <div class="format-details">
                  <span class="format-codec">{getCodecDisplay(fmt)}</span>
                  <span class="format-size">{getFormatSize(fmt)}</span>
                </div>
              </button>
            {/each}
            {#if audioOnlyFormats.length === 0}
              <div class="no-formats">{$t('download.formats.noAudio')}</div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#snippet actions()}
    <button class="modal-btn" onclick={handleClose}>
      {$t('common.cancel')}
    </button>
    <button
      class="modal-btn primary"
      onclick={downloadSelected}
      disabled={loading ||
        !!error ||
        (viewMode === 'builder' ? !selectedVideoFormat && !selectedAudioFormat : !selectedFormat)}
    >
      <Icon name="download" size={16} />
      {$t('download.formats.download')}
    </button>
  {/snippet}
</Modal>

<style>
  .format-modal-content {
    width: 520px;
    max-width: 80vw;
    overflow-x: hidden;
  }

  .loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 48px 24px;
    color: rgba(255, 255, 255, 0.6);
  }

  .loading :global(svg) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .error {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 32px;
    color: #ef4444;
    text-align: center;
  }

  .retry-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    margin-top: 8px;
  }

  .retry-btn:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .video-info {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: rgba(0, 0, 0, 0.2);
    border-radius: 10px;
    margin-bottom: 12px;
  }

  .thumbnail {
    width: 64px;
    height: 36px;
    object-fit: cover;
    border-radius: 6px;
    flex-shrink: 0;
  }

  .video-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .video-title {
    font-size: 13px;
    font-weight: 600;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    line-clamp: 1;
    -webkit-line-clamp: 1;
    -webkit-box-orient: vertical;
  }

  .video-duration {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
  }

  .tabs {
    display: flex;
    gap: 4px;
    padding: 4px;
    background: rgba(255, 255, 255, 0.05);
    border-radius: 10px;
    margin-bottom: 16px;
  }

  .tab {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 10px 12px;
    background: transparent;
    border: none;
    border-radius: 8px;
    color: rgba(255, 255, 255, 0.6);
    font-size: 13px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .tab:hover {
    color: rgba(255, 255, 255, 0.8);
    background: rgba(255, 255, 255, 0.05);
  }

  .tab.active {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .formats-container {
    display: flex;
    flex-direction: column;
    min-height: 350px;
  }

  .split-view {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }

  .split-column {
    display: flex;
    flex-direction: column;
  }

  /* Mobile: stack columns vertically */
  @media (max-width: 600px) {
    .format-modal-content {
      width: 100%;
    }

    .split-view {
      grid-template-columns: 1fr;
      gap: 16px;
    }

    .tabs {
      flex-wrap: wrap;
    }

    .tab {
      flex: 1 1 45%;
      padding: 8px 10px;
      font-size: 12px;
    }

    .video-info {
      gap: 10px;
    }

    .thumbnail {
      width: 48px;
      height: 27px;
    }

    .video-title {
      font-size: 12px;
    }
  }

  .split-column h4 {
    font-size: 12px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.5);
    text-transform: uppercase;
    margin: 0 0 8px 0;
    letter-spacing: 0.5px;
  }

  .split-column .format-list {
    flex: 1;
  }

  .format-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .format-list.scrollable-list {
    flex: 1;
  }

  .format-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid transparent;
    border-radius: 8px;
    cursor: pointer;
    transition: all 0.15s;
    text-align: left;
  }

  .format-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .format-item.selected {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.15));
    border-color: var(--accent, rgba(99, 102, 241, 0.5));
  }

  .format-item.no-track {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .format-item.no-track .format-main {
    gap: 6px;
  }

  .format-item.no-track.selected {
    background: rgba(239, 68, 68, 0.15);
    border-color: rgba(239, 68, 68, 0.5);
    color: rgba(239, 68, 68, 0.9);
  }

  .format-main {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .format-resolution {
    font-weight: 600;
    font-size: 14px;
    color: white;
  }

  .format-fps {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    padding: 2px 6px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 4px;
  }

  .format-ext {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
    padding: 2px 6px;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 4px;
  }

  .format-details {
    display: flex;
    gap: 12px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .format-note {
    font-size: 11px;
    color: var(--accent, rgba(129, 140, 248, 0.8));
  }

  .no-formats {
    padding: 24px;
    text-align: center;
    color: rgba(255, 255, 255, 0.4);
    font-size: 14px;
  }

  .modal-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 18px;
    font-size: 14px;
    font-weight: 500;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    transition: all 0.15s;
  }

  .modal-btn:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .modal-btn.primary {
    background: var(--accent, rgba(99, 102, 241, 0.85));
    border-color: transparent;
    color: white;
  }

  .modal-btn.primary:hover:not(:disabled) {
    background: var(--accent-hover, rgba(99, 102, 241, 1));
  }

  .modal-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
