<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fade, slide } from 'svelte/transition';
  import { invoke, convertFileSrc } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { load } from '@tauri-apps/plugin-store';
  import type { BackgroundType } from '$lib/stores/settings';
  import { rgbToRgba, type RGB } from '$lib/utils/color';
  import Icon from '$lib/components/Icon.svelte';

  const WINDOW_ID =
    typeof window !== 'undefined'
      ? new URLSearchParams(window.location.search).get('window_id') || ''
      : '';

  const params =
    typeof window !== 'undefined'
      ? new URLSearchParams(window.location.search)
      : new URLSearchParams();

  const title = params.get('title') || 'Media Detected';
  const body = params.get('body') || '';
  const thumbnail = params.get('thumbnail') || '';
  const mediaUrl = params.get('url') || '';
  const isCompact = params.get('compact') === '1';
  const downloadLabel = params.get('dl') || 'Download';
  const dismissLabel = params.get('dm') || 'Dismiss';
  const isPlaylist = params.get('is_playlist') === '1';
  const isChannel = params.get('is_channel') === '1';
  const isFile = params.get('is_file') === '1';
  const fileInfoRaw = params.get('file_info');
  const fileInfo = fileInfoRaw
    ? (JSON.parse(fileInfoRaw) as { filename: string; size: number; mimeType: string })
    : null;
  const viewPlaylistLabel = 'View Playlist';
  const viewChannelLabel = 'View Channel';

  import { isHttpUrl } from '$lib/utils/format';

  const isYouTube = /youtube\.com|youtu\.be/i.test(mediaUrl);
  const isVideoUrl = isHttpUrl(mediaUrl);

  let isReady = $state(false);
  let isHovered = $state(false);
  let isDownloading = $state(false);
  let isDismissing = $state(false);
  let autoCloseTimer: ReturnType<typeof setTimeout> | null = null;
  let thumbnailError = $state(false);

  let fancyBackground = $state(false);
  let backgroundType = $state<BackgroundType>('solid');
  let backgroundColor = $state('#1a1a2e');
  let backgroundImage = $state('');
  let backgroundVideo = $state('');
  let backgroundBlur = $state(20);
  let accentColor = $state('#6366F1');
  let isRgbMode = $state(false);
  let rgbHue = $state(0);
  let rgbAnimationFrame: number | null = null;
  let lastRgbUpdate = 0;

  let thumbnailTheming = $state(false);
  let thumbnailColor = $state<RGB | null>(null);
  let thumbnailColorStyle = $derived(
    thumbnailColor
      ? `--thumb-color: ${rgbToRgba(thumbnailColor, 1)}; --thumb-color-alpha: ${rgbToRgba(thumbnailColor, 0.15)}; --thumb-color-glow: ${rgbToRgba(thumbnailColor, 0.3)};`
      : ''
  );

  function hslToHex(h: number, s: number, l: number): string {
    const hue2rgb = (p: number, q: number, t: number) => {
      if (t < 0) t += 1;
      if (t > 1) t -= 1;
      if (t < 1 / 6) return p + (q - p) * 6 * t;
      if (t < 1 / 2) return q;
      if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
      return p;
    };
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    const r = Math.round(hue2rgb(p, q, h + 1 / 3) * 255);
    const g = Math.round(hue2rgb(p, q, h) * 255);
    const b = Math.round(hue2rgb(p, q, h - 1 / 3) * 255);
    return `#${((1 << 24) | (r << 16) | (g << 8) | b).toString(16).slice(1)}`;
  }

  function rgbLoop(timestamp: number) {
    if (!isRgbMode) return;
    if (timestamp - lastRgbUpdate >= 100) {
      rgbHue = (rgbHue + 3) % 360;
      accentColor = hslToHex(rgbHue / 360, 0.75, 0.5);
      lastRgbUpdate = timestamp;
    }
    rgbAnimationFrame = requestAnimationFrame(rgbLoop);
  }

  let cornerDismiss = $state(false);
  let notificationDuration = $state(12000);
  let showProgress = $state(true);

  type DownloadState = 'idle' | 'downloading' | 'processing' | 'completed' | 'failed';
  let downloadState = $state<DownloadState>('idle');
  let downloadProgress = $state(0);
  let downloadSpeed = $state('');
  let downloadEta = $state('');
  let downloadFilePath = $state('');
  let downloadError = $state('');
  let unlistenProgress: UnlistenFn | null = null;
  let unlistenStatus: UnlistenFn | null = null;

  let lowDiskSpace = $state(false);
  let availableSpaceGb = $state(0);

  let accentAlpha = $derived(accentColor + '66'); // 40% opacity
  let accentHover = $derived(adjustBrightness(accentColor, -10));

  function adjustBrightness(hex: string, percent: number): string {
    const num = parseInt(hex.replace('#', ''), 16);
    const r = Math.min(255, Math.max(0, (num >> 16) + percent));
    const g = Math.min(255, Math.max(0, ((num >> 8) & 0x00ff) + percent));
    const b = Math.min(255, Math.max(0, (num & 0x0000ff) + percent));
    return '#' + (0x1000000 + r * 0x10000 + g * 0x100 + b).toString(16).slice(1);
  }

  let titleEl = $state<HTMLDivElement | null>(null);
  let subtitleEl = $state<HTMLDivElement | null>(null);
  let needsMarquee = $state(false);
  let needsSubtitleMarquee = $state(false);

  function checkMarquee() {
    if (titleEl) {
      needsMarquee = titleEl.scrollWidth > titleEl.clientWidth;
    }
    if (subtitleEl) {
      needsSubtitleMarquee = subtitleEl.scrollWidth > subtitleEl.clientWidth;
    }
  }

  async function closeNotification() {
    if (autoCloseTimer) clearTimeout(autoCloseTimer);
    isDismissing = true;
    await new Promise((r) => setTimeout(r, 200));
    try {
      await invoke('close_notification_window', { windowId: WINDOW_ID });
    } catch (e) {
      console.error('Close failed:', e);
    }
  }

  async function handleDownload(mode: 'auto' | 'audio' = 'auto') {
    isDownloading = true;
    const keepOpen = showProgress && !isPlaylist && !isChannel;

    if (keepOpen) {
      downloadState = 'downloading';
      if (autoCloseTimer) {
        clearTimeout(autoCloseTimer);
        autoCloseTimer = null;
      }
      await setupProgressListeners();
    }

    try {
      await invoke('notification_action', {
        windowId: WINDOW_ID,
        url: mediaUrl || null,
        metadata: {
          title: title !== 'Media Detected' ? title : null,
          thumbnail: thumbnail || null,
          uploader: body ? body.split(' • ')[0] : null,
          downloadMode: mode,
          isPlaylist: isPlaylist,
          isChannel: isChannel,
          isFile: isFile,
          fileInfo: fileInfo,
        },
        keepOpen: keepOpen,
      });
    } catch (e) {
      console.error('Action failed:', e);
      isDownloading = false;
      downloadState = 'idle';
    }
  }

  async function handleOpenTrackBuilder() {
    isDownloading = true;

    try {
      await invoke('notification_action', {
        windowId: WINDOW_ID,
        url: mediaUrl || null,
        metadata: {
          title: title !== 'Media Detected' ? title : null,
          thumbnail: thumbnail || null,
          uploader: body ? body.split(' • ')[0] : null,
          openTrackBuilder: true,
        },
      });
    } catch (e) {
      console.error('Action failed:', e);
      isDownloading = false;
    }
  }

  async function setupProgressListeners() {
    unlistenProgress = await listen<{
      url: string;
      progress: number;
      speed: string;
      eta: string;
      status: string;
      statusMessage: string;
    }>('download-progress-parsed', (event) => {
      const { url, progress, speed, eta, status } = event.payload;
      if (url !== mediaUrl) return;

      downloadProgress = progress;
      downloadSpeed = speed;
      downloadEta = eta;
      downloadState = status === 'processing' ? 'processing' : 'downloading';
    });

    unlistenStatus = await listen<{
      url: string;
      status: string;
      filePath?: string;
      error?: string;
    }>('download-status-changed', (event) => {
      const { url, status, filePath, error } = event.payload;
      if (url !== mediaUrl) return;

      if (status === 'completed') {
        downloadState = 'completed';
        downloadProgress = 100;
        downloadFilePath = filePath || '';
        downloadSpeed = '';
        downloadEta = '';
      } else if (status === 'failed') {
        downloadState = 'failed';
        downloadError = error || 'Download failed';
      } else if (status === 'cancelled') {
        closeNotification();
      }
    });
  }

  function cleanupListeners() {
    if (unlistenProgress) {
      unlistenProgress();
      unlistenProgress = null;
    }
    if (unlistenStatus) {
      unlistenStatus();
      unlistenStatus = null;
    }
  }

  async function handleOpenFile() {
    if (downloadFilePath) {
      try {
        const { openPath } = await import('@tauri-apps/plugin-opener');
        await openPath(downloadFilePath);
      } catch (e) {
        console.error('Failed to open file:', e);
      }
    }
    closeNotification();
  }

  async function handleShowInFolder() {
    if (downloadFilePath) {
      try {
        const { revealItemInDir } = await import('@tauri-apps/plugin-opener');
        await revealItemInDir(downloadFilePath);
      } catch (e) {
        console.error('Failed to reveal in folder:', e);
      }
    }
    closeNotification();
  }

  function scheduleAutoClose() {
    if (downloadState !== 'idle') return;

    if (autoCloseTimer) clearTimeout(autoCloseTimer);
    autoCloseTimer = setTimeout(() => {
      if (downloadState !== 'idle') return; // Double check
      if (!isHovered) closeNotification();
      else scheduleAutoClose();
    }, notificationDuration);
  }

  onDestroy(() => {
    cleanupListeners();
    if (rgbAnimationFrame) {
      cancelAnimationFrame(rgbAnimationFrame);
    }
  });

  onMount(() => {
    (async () => {
      let store: Awaited<ReturnType<typeof load>> | null = null;
      let downloadPath = '';

      try {
        store = await load('settings.json', { autoSave: false, defaults: {} });

        const storedAccent = (await store.get<string>('accentColor')) ?? '#6366F1';

        if (storedAccent === 'rgb') {
          isRgbMode = true;
          accentColor = hslToHex(rgbHue / 360, 0.75, 0.5);
          rgbAnimationFrame = requestAnimationFrame(rgbLoop);
        } else {
          accentColor = storedAccent;
        }

        fancyBackground = (await store.get<boolean>('notificationFancyBackground')) ?? false;
        cornerDismiss = (await store.get<boolean>('notificationCornerDismiss')) ?? false;
        thumbnailTheming = (await store.get<boolean>('notificationThumbnailTheming')) ?? true;
        notificationDuration = ((await store.get<number>('notificationDuration')) ?? 12) * 1000;
        showProgress = (await store.get<boolean>('notificationShowProgress')) ?? true;
        downloadPath = (await store.get<string>('downloadPath')) || '';

        if (fancyBackground) {
          backgroundType = (await store.get<BackgroundType>('backgroundType')) ?? 'solid';
          backgroundColor = (await store.get<string>('backgroundColor')) ?? '#1a1a2e';
          backgroundImage = (await store.get<string>('backgroundImage')) ?? '';
          backgroundVideo = (await store.get<string>('backgroundVideo')) ?? '';
          backgroundBlur = (await store.get<number>('backgroundBlur')) ?? 20;
        }
      } catch (e) {
        console.error('Failed to load settings:', e);
      }

      await new Promise((r) => setTimeout(r, 30));
      isReady = true;

      checkMarquee();

      try {
        await invoke('reveal_notification_window', { windowId: WINDOW_ID });
      } catch (e) {
        console.error('Reveal failed:', e);
      }

      scheduleAutoClose();

      if (thumbnailTheming && thumbnail) {
        invoke<[number, number, number]>('extract_thumbnail_color', { url: thumbnail })
          .then((colorArr) => {
            if (colorArr) {
              thumbnailColor = { r: colorArr[0], g: colorArr[1], b: colorArr[2] };
            }
          })
          .catch(() => {});
      }

      if (downloadPath) {
        try {
          const diskInfo = await invoke<{ availableGb: number }>('get_disk_space', {
            path: downloadPath,
          });
          if (diskInfo && diskInfo.availableGb < 2) {
            lowDiskSpace = true;
            availableSpaceGb = Math.round(diskInfo.availableGb * 10) / 10;
          }
        } catch (e) {
          console.warn('Could not check disk space:', e);
        }
      }
    })();

    return () => {
      if (autoCloseTimer) clearTimeout(autoCloseTimer);
      if (rgbAnimationFrame) cancelAnimationFrame(rgbAnimationFrame);
    };
  });
</script>

{#if fancyBackground}
  <div class="notification-background" style="--accent: {accentColor};">
    {#if backgroundType === 'animated' && backgroundVideo}
      <video
        class="bg-video"
        style="filter: blur({backgroundBlur}px) brightness(0.4) saturate(1.2);"
        src={backgroundVideo}
        autoplay
        loop
        muted
        playsinline
      ></video>
      <div class="bg-overlay"></div>
    {:else if backgroundType === 'image' && backgroundImage}
      <div
        class="bg-image"
        style="background-image: url('{backgroundImage}'); filter: blur({backgroundBlur}px) brightness(0.4) saturate(1.2);"
      ></div>
      <div class="bg-overlay"></div>
    {:else if backgroundType === 'solid'}
      <div class="bg-solid" style="background-color: {backgroundColor}"></div>
    {/if}
  </div>
{/if}

{#if isCompact}
  <div
    class="notification compact"
    class:fancy={fancyBackground}
    class:themed={thumbnailColor}
    style="--accent: {accentColor}; --accent-hover: {accentHover}; --accent-alpha: {accentAlpha}; {thumbnailColorStyle}"
    role="alert"
    onmouseenter={() => (isHovered = true)}
    onmouseleave={() => {
      isHovered = false;
      scheduleAutoClose();
    }}
  >
    <button class="close-corner" onclick={closeNotification} aria-label="Close">
      <svg viewBox="0 0 24 24" fill="none" width="12" height="12">
        <path
          d="M6 18L18 6M6 6l12 12"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </button>

    {#if thumbnail && !thumbnailError}
      <img src={thumbnail} alt="" class="thumb-compact" onerror={() => (thumbnailError = true)} />
    {:else}
      <div class="thumb-compact placeholder">
        <svg viewBox="0 0 24 24" fill="none" width="16" height="16">
          <path
            d="M12 3V16M12 16L16 11.625M12 16L8 11.625"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </div>
    {/if}

    <div class="text-compact">
      <div class="title-wrapper" class:masked={needsMarquee}>
        <div class="title" class:marquee={needsMarquee} bind:this={titleEl}>
          <span>{title}</span>
          {#if needsMarquee}<span aria-hidden="true">{title}</span>{/if}
        </div>
      </div>
    </div>

    <div class="actions-compact">
      {#if downloadState === 'downloading' || downloadState === 'processing'}
        <div class="progress-compact" in:fade={{ duration: 200 }}>
          <div class="progress-ring" style="--progress: {downloadProgress}">
            <svg viewBox="0 0 36 36" width="28" height="28">
              <circle
                cx="18"
                cy="18"
                r="15"
                fill="none"
                stroke="rgba(255,255,255,0.1)"
                stroke-width="3"
              />
              <circle
                cx="18"
                cy="18"
                r="15"
                fill="none"
                stroke="var(--accent, #6366f1)"
                stroke-width="3"
                stroke-dasharray="{downloadProgress * 0.942} 100"
                stroke-linecap="round"
                transform="rotate(-90 18 18)"
                class="progress-circle"
              />
            </svg>
            <span class="progress-text">{downloadProgress.toFixed(0)}</span>
          </div>
        </div>
      {:else if downloadState === 'completed'}
        <div class="compact-completion" in:fade={{ duration: 200, delay: 100 }}>
          <button class="icon-btn open" onclick={handleOpenFile} title="Open">
            <Icon name="arrow_outward" size={16} />
          </button>
          <button class="icon-btn folder" onclick={handleShowInFolder} title="Show in folder">
            <Icon name="folder" size={16} />
          </button>
        </div>
      {:else if downloadState === 'failed'}
        <div class="icon-btn error" title="Failed" in:fade={{ duration: 200 }}>
          <svg viewBox="0 0 24 24" fill="none" width="16" height="16">
            <path
              d="M18 6L6 18M6 6L18 18"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
            />
          </svg>
        </div>
      {:else}
        <div class="compact-default" out:fade={{ duration: 150 }}>
          {#if isChannel}
            <button
              class="icon-btn download channel"
              class:downloading={isDownloading}
              onclick={() => handleDownload('auto')}
              title={viewChannelLabel}
              disabled={isDownloading}
            >
              {#if isDownloading}
                <svg class="spinner" viewBox="0 0 24 24" width="18" height="18">
                  <circle
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="2"
                    fill="none"
                    stroke-dasharray="31.4 31.4"
                    stroke-linecap="round"
                  />
                </svg>
              {:else}
                <svg viewBox="0 0 24 24" fill="none" width="18" height="18">
                  <circle cx="12" cy="8" r="4" stroke="currentColor" stroke-width="2" />
                  <path
                    d="M4 20C4 16.6863 7.13401 14 11 14H13C16.866 14 20 16.6863 20 20"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                  />
                </svg>
              {/if}
            </button>
          {:else if isPlaylist}
            <button
              class="icon-btn download playlist"
              class:downloading={isDownloading}
              onclick={() => handleDownload('auto')}
              title={viewPlaylistLabel}
              disabled={isDownloading}
            >
              {#if isDownloading}
                <svg class="spinner" viewBox="0 0 24 24" width="18" height="18">
                  <circle
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="2"
                    fill="none"
                    stroke-dasharray="31.4 31.4"
                    stroke-linecap="round"
                  />
                </svg>
              {:else}
                <svg viewBox="0 0 24 24" fill="none" width="18" height="18">
                  <path
                    d="M4 6H20M4 10H20M4 14H14M4 18H14"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                  />
                  <circle cx="18" cy="16" r="3" stroke="currentColor" stroke-width="2" />
                </svg>
              {/if}
            </button>
          {:else if isFile}
            <button
              class="icon-btn download"
              class:downloading={isDownloading}
              onclick={() => handleDownload('auto')}
              title={downloadLabel}
              disabled={isDownloading}
            >
              {#if isDownloading}
                <svg class="spinner" viewBox="0 0 24 24" width="18" height="18">
                  <circle
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="2"
                    fill="none"
                    stroke-dasharray="31.4 31.4"
                    stroke-linecap="round"
                  />
                </svg>
              {:else}
                <svg class="download-icon" viewBox="0 0 24 24" fill="none" width="18" height="18">
                  <path
                    d="M12 3V16M12 16L16 11.625M12 16L8 11.625"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  />
                  <path
                    d="M3 15C3 17.828 3 19.243 3.879 20.121C4.757 21 6.172 21 9 21H15C17.828 21 19.243 21 20.121 20.121C21 19.243 21 17.828 21 15"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  />
                </svg>
              {/if}
            </button>
          {:else if isVideoUrl}
            <button
              class="icon-btn download youtube"
              class:downloading={isDownloading}
              onclick={() => handleDownload('auto')}
              title="Download"
              disabled={isDownloading}
            >
              {#if isDownloading}
                <svg class="spinner" viewBox="0 0 24 24" width="16" height="16">
                  <circle
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="2"
                    fill="none"
                    stroke-dasharray="31.4 31.4"
                    stroke-linecap="round"
                  />
                </svg>
              {:else}
                <svg viewBox="0 0 24 24" fill="none" width="16" height="16">
                  <path
                    d="M12 3V16M12 16L16 11.625M12 16L8 11.625"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  />
                  <path
                    d="M3 15C3 17.828 3 19.243 3.879 20.121C4.757 21 6.172 21 9 21H15C17.828 21 19.243 21 20.121 20.121C21 19.243 21 17.828 21 15"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  />
                </svg>
              {/if}
            </button>
            <button
              class="icon-btn track-builder"
              onclick={handleOpenTrackBuilder}
              title="Quality"
              disabled={isDownloading}
            >
              <Icon name="extensions" size={16} />
            </button>
          {:else}
            <button
              class="icon-btn download"
              class:downloading={isDownloading}
              onclick={() => handleDownload('auto')}
              title={downloadLabel}
              disabled={isDownloading}
            >
              {#if isDownloading}
                <svg class="spinner" viewBox="0 0 24 24" width="18" height="18">
                  <circle
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="2"
                    fill="none"
                    stroke-dasharray="31.4 31.4"
                    stroke-linecap="round"
                  />
                </svg>
              {:else}
                <svg class="download-icon" viewBox="0 0 24 24" fill="none" width="18" height="18">
                  <path
                    d="M12 3V16M12 16L16 11.625M12 16L8 11.625"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  />
                  <path
                    d="M3 15C3 17.828 3 19.243 3.879 20.121C4.757 21 6.172 21 9 21H15C17.828 21 19.243 21 20.121 20.121C21 19.243 21 17.828 21 15"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  />
                </svg>
              {/if}
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{:else}
  <div
    class="notification"
    class:fancy={fancyBackground}
    class:themed={thumbnailColor}
    style="--accent: {accentColor}; --accent-hover: {accentHover}; --accent-alpha: {accentAlpha}; {thumbnailColorStyle}"
    role="alert"
    onmouseenter={() => (isHovered = true)}
    onmouseleave={() => {
      isHovered = false;
      scheduleAutoClose();
    }}
  >
    <button class="close-x" onclick={closeNotification} aria-label="Close">✕</button>

    <div class="content">
      {#if thumbnail && !thumbnailError}
        <img src={thumbnail} alt="" class="thumb" onerror={() => (thumbnailError = true)} />
      {:else}
        <div class="thumb placeholder">
          <svg viewBox="0 0 24 24" fill="none" width="20" height="20">
            <path
              d="M12 3V16M12 16L16 11.625M12 16L8 11.625"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
            <path
              opacity="0.5"
              d="M3 15C3 17.828 3 19.243 3.879 20.121C4.757 21 6.172 21 9 21H15C17.828 21 19.243 21 20.121 20.121C21 19.243 21 17.828 21 15"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </div>
      {/if}

      <div class="text">
        <div class="title-wrapper" class:masked={needsMarquee}>
          <div class="title" class:marquee={needsMarquee} bind:this={titleEl}>
            <span>{title}</span>
            {#if needsMarquee}<span aria-hidden="true">{title}</span>{/if}
          </div>
        </div>
        {#if body}
          <div class="subtitle-wrapper" class:masked={needsSubtitleMarquee}>
            <div class="subtitle" class:marquee={needsSubtitleMarquee} bind:this={subtitleEl}>
              <span>{body}</span>
              {#if needsSubtitleMarquee}<span aria-hidden="true">{body}</span>{/if}
            </div>
          </div>
        {/if}
      </div>
    </div>

    <div class="actions">
      {#if downloadState === 'downloading' || downloadState === 'processing'}
        <div class="progress-view" in:fade={{ duration: 200 }} out:fade={{ duration: 150 }}>
          <div class="progress-bar-container">
            <div class="progress-bar" style="width: {downloadProgress}%"></div>
          </div>
          <div class="progress-info">
            <span class="progress-percent">{downloadProgress.toFixed(0)}%</span>
            {#if downloadState === 'processing'}
              <span class="progress-status">Processing...</span>
            {:else if downloadProgress === 0 && !downloadSpeed}
              <span class="progress-status">Starting...</span>
            {:else}
              <span class="progress-speed">
                {#if downloadSpeed}
                  <Icon name="arrow_down" size={10} />
                  {downloadSpeed}
                {/if}
                {#if downloadSpeed && downloadEta}<span class="speed-separator">·</span>{/if}
                {#if downloadEta}
                  <Icon name="clock" size={10} />
                  {downloadEta}
                {/if}
              </span>
            {/if}
          </div>
        </div>
      {:else if downloadState === 'completed'}
        <div class="completion-actions" in:fade={{ duration: 200, delay: 100 }}>
          <button class="btn open" onclick={handleOpenFile}>
            <Icon name="arrow_outward" size={14} />
            Open
          </button>
          <button class="btn folder" onclick={handleShowInFolder}>
            <Icon name="folder" size={14} />
            Folder
          </button>
        </div>
      {:else if downloadState === 'failed'}
        <div class="error-actions" in:fade={{ duration: 200 }}>
          <div class="error-view">
            <span class="error-icon">✕</span>
            <span class="error-text">Failed</span>
          </div>
          <button class="btn dismiss" onclick={closeNotification}>Close</button>
        </div>
      {:else}
        {#if lowDiskSpace}
          <div class="disk-warning" in:fade={{ duration: 200 }}>
            <Icon name="warning" size={12} />
            <span>Low disk space: {availableSpaceGb} GB left</span>
          </div>
        {/if}
        <div class="default-actions" out:fade={{ duration: 150 }}>
          {#if isChannel}
            <button
              class="btn download channel"
              class:downloading={isDownloading}
              class:full-width={true}
              onclick={() => handleDownload('auto')}
              disabled={isDownloading}
            >
              {#if isDownloading}
                <svg class="spinner" viewBox="0 0 24 24" width="14" height="14">
                  <circle
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="2"
                    fill="none"
                    stroke-dasharray="31.4 31.4"
                    stroke-linecap="round"
                  />
                </svg>
                Opening...
              {:else}
                <svg viewBox="0 0 24 24" fill="none" width="14" height="14">
                  <circle cx="12" cy="8" r="4" stroke="currentColor" stroke-width="2" />
                  <path
                    d="M4 20C4 16.6863 7.13401 14 11 14H13C16.866 14 20 16.6863 20 20"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                  />
                </svg>
                {viewChannelLabel}
              {/if}
            </button>
          {:else if isPlaylist}
            <button
              class="btn download playlist"
              class:downloading={isDownloading}
              class:full-width={true}
              onclick={() => handleDownload('auto')}
              disabled={isDownloading}
            >
              {#if isDownloading}
                <svg class="spinner" viewBox="0 0 24 24" width="14" height="14">
                  <circle
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="2"
                    fill="none"
                    stroke-dasharray="31.4 31.4"
                    stroke-linecap="round"
                  />
                </svg>
                Opening...
              {:else}
                <svg viewBox="0 0 24 24" fill="none" width="14" height="14">
                  <path
                    d="M4 6H20M4 10H20M4 14H14M4 18H14"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                  />
                  <circle cx="18" cy="16" r="3" stroke="currentColor" stroke-width="2" />
                </svg>
                {viewPlaylistLabel}
              {/if}
            </button>
          {:else if isFile}
            <button
              class="btn download"
              class:downloading={isDownloading}
              class:full-width={cornerDismiss}
              onclick={() => handleDownload('auto')}
              disabled={isDownloading}
            >
              {#if isDownloading}
                <svg class="spinner" viewBox="0 0 24 24" width="14" height="14">
                  <circle
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="2"
                    fill="none"
                    stroke-dasharray="31.4 31.4"
                    stroke-linecap="round"
                  />
                </svg>
                Starting...
              {:else}
                {downloadLabel}
              {/if}
            </button>
            {#if !cornerDismiss}
              <button class="btn dismiss" onclick={closeNotification}>{dismissLabel}</button>
            {/if}
          {:else if isVideoUrl}
            <button
              class="btn download"
              class:downloading={isDownloading}
              onclick={() => handleDownload('auto')}
              disabled={isDownloading}
            >
              {#if isDownloading}
                <svg class="spinner" viewBox="0 0 24 24" width="14" height="14">
                  <circle
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="2"
                    fill="none"
                    stroke-dasharray="31.4 31.4"
                    stroke-linecap="round"
                  />
                </svg>
                Starting...
              {:else}
                Download
              {/if}
            </button>
            <button
              class="btn track-builder"
              onclick={handleOpenTrackBuilder}
              disabled={isDownloading}
            >
              <Icon name="extensions" size={14} />
              Quality
            </button>
          {:else}
            <button
              class="btn download"
              class:downloading={isDownloading}
              class:full-width={cornerDismiss}
              onclick={() => handleDownload('auto')}
              disabled={isDownloading}
            >
              {#if isDownloading}
                <svg class="spinner" viewBox="0 0 24 24" width="14" height="14">
                  <circle
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="2"
                    fill="none"
                    stroke-dasharray="31.4 31.4"
                    stroke-linecap="round"
                  />
                </svg>
                Starting...
              {:else}
                {downloadLabel}
              {/if}
            </button>
            {#if !cornerDismiss}
              <button class="btn dismiss" onclick={closeNotification}>{dismissLabel}</button>
            {/if}
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  :global(*) {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }
  :global(html) {
    background: transparent !important;
  }
  :global(body) {
    background: transparent !important;
    font-family: 'Jost', system-ui, sans-serif;
    color: white;
    overflow: hidden;
    padding: 0;
  }

  .notification-background {
    position: fixed;
    inset: 0;
    z-index: 0;
    overflow: hidden;
    pointer-events: none;
    border-radius: var(--radius-lg, 12px);
  }

  .bg-video {
    position: absolute;
    top: 50%;
    left: 50%;
    min-width: 100%;
    min-height: 100%;
    width: auto;
    height: auto;
    transform: translate(-50%, -50%) scale(1.2);
    object-fit: cover;
  }

  .bg-image {
    position: absolute;
    inset: -20px;
    background-size: cover;
    background-position: center;
  }

  .bg-solid {
    position: absolute;
    inset: 0;
  }

  .bg-overlay {
    position: absolute;
    inset: 0;
    background: linear-gradient(180deg, rgba(0, 0, 0, 0.2) 0%, rgba(0, 0, 0, 0.4) 100%);
  }

  .notification {
    background: #1a1a1e;
    padding: 7px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    position: relative;
    z-index: 1;
    border-radius: var(--radius-lg, 12px);
    height: 100vh;
    box-sizing: border-box;
    border: 1px solid rgba(255, 255, 255, 0.08);
    transition: border-color 0.4s ease;
    overflow: hidden;
    isolation: isolate;
  }

  .notification::before {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(135deg, var(--thumb-color-alpha, transparent) 0%, transparent 60%);
    pointer-events: none;
    border-radius: inherit;
    opacity: 0;
    transition: opacity 0.4s ease;
    z-index: 0;
  }

  .notification.fancy {
    background: rgba(26, 26, 30, 0.92);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
  }

  .notification.themed {
    border-color: var(--thumb-color-alpha, rgba(255, 255, 255, 0.08));
  }

  .notification.themed::before {
    opacity: 1;
    animation: gradient-breathe 3s ease-in-out infinite;
  }

  @keyframes gradient-breathe {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.3;
    }
  }

  .notification.themed.fancy {
    background: rgba(26, 26, 30, 0.92);
  }

  .close-x {
    position: absolute;
    top: 8px;
    right: 10px;
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.4);
    font-size: var(--text-md, 14px);
    cursor: pointer;
    padding: 2px 6px;
    border-radius: var(--radius-sm, 4px);
    z-index: 2;
  }
  .close-x:hover {
    color: white;
    background: rgba(255, 255, 255, 0.1);
  }

  .close-corner {
    position: absolute;
    top: 2px;
    right: 2px;
    background: rgba(255, 255, 255, 0.1);
    border: none;
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    padding: 2px;
    border-radius: var(--radius-sm, 4px);
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s ease;
    width: 16px;
    height: 16px;
  }
  .close-corner:hover {
    color: white;
    background: rgba(255, 255, 255, 0.2);
  }

  .content {
    display: flex;
    gap: 10px;
    align-items: center;
    padding-right: 24px;
    position: relative;
    z-index: 1;
  }

  .thumb {
    width: 40px;
    height: 40px;
    border-radius: var(--radius-sm, 6px);
    object-fit: cover;
    flex-shrink: 0;
    position: relative;
    z-index: 1;
  }

  .thumb.placeholder {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.2));
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent, #6366f1);
  }

  .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .title-wrapper,
  .subtitle-wrapper {
    overflow: hidden;
    position: relative;
  }
  .title-wrapper.masked,
  .subtitle-wrapper.masked {
    mask-image: linear-gradient(to right, transparent, black 8%, black 92%, transparent);
    -webkit-mask-image: linear-gradient(to right, transparent, black 8%, black 92%, transparent);
  }

  .title {
    font-size: var(--text-base, 13px);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .title.marquee {
    display: flex;
    gap: 3em;
    animation: marquee 8s linear infinite;
    animation-delay: 1s;
    text-overflow: clip;
    overflow: visible;
    width: max-content;
  }
  .title.marquee span {
    flex-shrink: 0;
  }

  @keyframes marquee {
    0%,
    5% {
      transform: translateX(0);
    }
    95%,
    100% {
      transform: translateX(calc(-50% - 1.5em));
    }
  }

  .subtitle {
    font-size: var(--text-xs, 11px);
    color: rgba(255, 255, 255, 0.5);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .subtitle.marquee {
    display: flex;
    gap: 3em;
    animation: marquee 10s linear infinite;
    animation-delay: 1s;
    text-overflow: clip;
    overflow: visible;
    width: max-content;
  }
  .subtitle.marquee span {
    flex-shrink: 0;
  }

  .actions {
    display: flex;
    gap: 8px;
    position: relative;
    min-height: 32px;
    z-index: 1;
  }

  .default-actions,
  .completion-actions,
  .error-actions,
  .progress-view {
    display: flex;
    gap: 8px;
    flex: 1;
    position: absolute;
    inset: 0;
  }

  .default-actions,
  .completion-actions,
  .error-actions {
    align-items: center;
  }

  .btn {
    flex: 1;
    padding: 6px 12px;
    border: none;
    border-radius: var(--radius-sm, 6px);
    font-size: var(--text-sm, 12px);
    font-weight: 500;
    cursor: pointer;
    font-family: inherit;
    transition: all 0.15s ease;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  .btn.download {
    background: var(--accent, #6366f1);
    color: white;
    transition:
      all 0.15s ease,
      background-color 0.4s ease;
  }
  .themed .btn.download {
    background: var(--thumb-color, var(--accent, #6366f1));
  }
  .btn.download:hover:not(:disabled) {
    background: var(--accent-hover, #5558e3);
    transform: scale(1.02);
  }
  .themed .btn.download:hover:not(:disabled) {
    filter: brightness(0.9);
    background: var(--thumb-color, var(--accent-hover, #5558e3));
  }
  .btn.download:active:not(:disabled) {
    transform: scale(0.98);
  }
  .btn.download.downloading {
    background: var(--accent-hover, #4f46e5);
    cursor: default;
  }
  .btn.download:disabled {
    opacity: 0.9;
  }
  .btn.download.full-width {
    flex: 1;
  }
  .btn.dismiss {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }
  .btn.dismiss:hover {
    background: rgba(255, 255, 255, 0.12);
    color: white;
  }
  .btn.track-builder {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.9);
  }
  .btn.track-builder:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.15);
    color: white;
  }

  .btn .spinner {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .progress-view {
    flex-direction: column;
    justify-content: center;
  }

  .progress-bar-container {
    width: 100%;
    height: 6px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 3px;
    overflow: hidden;
  }

  .progress-bar {
    height: 100%;
    background: var(--accent, #6366f1);
    border-radius: 3px;
    transition: width 0.3s cubic-bezier(0.4, 0, 0.2, 1);
    position: relative;
    overflow: hidden;
  }
  .progress-bar::after {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    width: 200%;
    height: 100%;
    background: linear-gradient(
      90deg,
      transparent 0%,
      transparent 25%,
      rgba(255, 255, 255, 0.25) 50%,
      transparent 75%,
      transparent 100%
    );
    animation: shimmer 2s ease-in-out infinite;
  }
  @keyframes shimmer {
    0% {
      transform: translateX(-50%);
    }
    100% {
      transform: translateX(50%);
    }
  }
  .themed .progress-bar {
    background: var(--thumb-color, var(--accent, #6366f1));
  }

  .progress-info {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: var(--text-xs, 11px);
  }

  .progress-percent {
    font-weight: 600;
    color: rgba(255, 255, 255, 0.9);
  }

  .progress-speed,
  .progress-status {
    color: rgba(255, 255, 255, 0.5);
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .progress-speed :global(svg) {
    opacity: 0.7;
    flex-shrink: 0;
  }

  .speed-separator {
    margin: 0 2px;
    opacity: 0.4;
  }

  .btn.open {
    flex: 1;
    background: var(--accent, #6366f1);
    color: white;
  }
  .themed .btn.open {
    background: var(--thumb-color, var(--accent, #6366f1));
  }
  .btn.open:hover {
    filter: brightness(0.9);
  }

  .btn.folder {
    flex: 1;
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.9);
  }
  .btn.folder:hover {
    background: rgba(255, 255, 255, 0.15);
    color: white;
  }

  .error-view {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: rgba(239, 68, 68, 0.15);
    border-radius: var(--radius-sm, 6px);
    color: #ef4444;
    font-size: var(--text-sm, 12px);
    font-weight: 500;
  }

  .disk-warning {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    margin-bottom: 8px;
    background: rgba(245, 158, 11, 0.15);
    border-radius: var(--radius-sm, 6px);
    color: #fbbf24;
    font-size: var(--text-xs, 11px);
    font-weight: 500;
  }

  .error-icon {
    font-weight: bold;
  }

  .error-text {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .notification.compact {
    flex-direction: row;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    padding-top: 6px;
    position: relative;
  }

  .thumb-compact {
    width: 32px;
    height: 32px;
    border-radius: var(--radius-sm, 6px);
    object-fit: cover;
    flex-shrink: 0;
    position: relative;
    z-index: 1;
  }

  .thumb-compact.placeholder {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.2));
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent, #6366f1);
  }

  .text-compact {
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }
  .text-compact .title-wrapper {
    overflow: hidden;
    position: relative;
  }
  .text-compact .title-wrapper.masked {
    mask-image: linear-gradient(to right, transparent, black 8%, black 92%, transparent);
    -webkit-mask-image: linear-gradient(to right, transparent, black 8%, black 92%, transparent);
  }
  .text-compact .title {
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .text-compact .title.marquee {
    display: flex;
    gap: 3em;
    animation: marquee 8s linear infinite;
    animation-delay: 1s;
    text-overflow: clip;
    overflow: visible;
    width: max-content;
  }
  .text-compact .title.marquee span {
    flex-shrink: 0;
  }

  .actions-compact {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
    position: relative;
    min-width: 64px;
    min-height: 32px;
  }

  .compact-default,
  .compact-completion,
  .progress-compact {
    display: flex;
    gap: 4px;
    position: absolute;
    right: 0;
    top: 50%;
    transform: translateY(-50%);
  }

  .icon-btn {
    width: 32px;
    height: 32px;
    border: none;
    border-radius: var(--radius-sm, 6px);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition:
      all 0.15s ease,
      background-color 0.4s ease;
  }
  .icon-btn.download {
    background: var(--accent, #6366f1);
    color: white;
  }
  .themed .icon-btn.download {
    background: var(--thumb-color, var(--accent, #6366f1));
  }
  .icon-btn.download:hover:not(:disabled) {
    background: var(--accent-hover, #5558e3);
    transform: scale(1.08);
  }
  .themed .icon-btn.download:hover:not(:disabled) {
    filter: brightness(0.9);
    background: var(--thumb-color, var(--accent-hover, #5558e3));
  }
  .icon-btn.download:active:not(:disabled) {
    transform: scale(0.95);
  }
  .icon-btn.download.downloading {
    background: var(--accent-hover, #4f46e5);
    cursor: default;
  }
  .icon-btn.download:disabled {
    opacity: 0.9;
  }
  .icon-btn.download .spinner {
    animation: spin 0.8s linear infinite;
  }
  .icon-btn.download .download-icon {
    transition: transform 0.15s ease;
  }

  .icon-btn.track-builder,
  .icon-btn.download.youtube {
    width: 28px;
    height: 28px;
  }
  .icon-btn.track-builder {
    background: rgba(255, 255, 255, 0.12);
    color: white;
  }
  .icon-btn.track-builder:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.2);
    transform: scale(1.08);
  }

  .icon-btn.playlist {
    width: 32px;
    height: 32px;
  }

  .icon-btn.channel {
    width: 32px;
    height: 32px;
    background: #ef4444;
  }
  .icon-btn.channel:hover:not(:disabled) {
    background: #dc2626;
  }

  .btn.channel {
    background: #ef4444;
  }
  .btn.channel:hover:not(:disabled) {
    background: #dc2626;
  }

  .progress-compact {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .progress-ring {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .progress-ring .progress-circle {
    transition: stroke-dasharray 0.2s ease;
  }
  .themed .progress-ring .progress-circle {
    stroke: var(--thumb-color, var(--accent, #6366f1));
  }

  .progress-ring .progress-text {
    position: absolute;
    font-size: 8px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.9);
  }

  .icon-btn.open {
    background: var(--accent, #6366f1);
    color: white;
  }
  .themed .icon-btn.open {
    background: var(--thumb-color, var(--accent, #6366f1));
  }
  .icon-btn.open:hover {
    filter: brightness(0.9);
  }

  .icon-btn.folder {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.9);
  }
  .icon-btn.folder:hover {
    background: rgba(255, 255, 255, 0.15);
    color: white;
  }

  .icon-btn.error {
    background: rgba(239, 68, 68, 0.2);
    color: #ef4444;
    cursor: default;
  }
</style>
