<script lang="ts">
  import { fade } from 'svelte/transition';
  import { invoke } from '@tauri-apps/api/core';
  import { portal } from '$lib/actions/portal';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { player } from '$lib/stores/player.svelte';
  import { t } from '$lib/i18n';
  import {
    parseSubtitleContent,
    applyDelay,
    getActiveCue,
    type SubtitleCue,
  } from '$lib/utils/subtitles';
  import LyricsView from './LyricsView.svelte';

  // ── Element refs ────────────────────────────────────────────────────────────
  let videoEl = $state<HTMLVideoElement | undefined>(undefined);
  let audioEl = $state<HTMLAudioElement | undefined>(undefined);

  // ── Local UI state ───────────────────────────────────────────────────────────
  let subtitleMenuOpen = $state(false);
  let speedMenuOpen = $state(false);
  let isFullscreen = $state(false);
  let loadedSubtitleCues = $state<SubtitleCue[]>([]);

  // Plain Map — not reactive, used only as a load cache
  const cueCache = new Map<string, SubtitleCue[]>();

  // ── Derived ──────────────────────────────────────────────────────────────────
  const mediaEl = $derived(videoEl ?? audioEl);

  const bufferedPercent = $derived.by(() => {
    return player.duration > 0 ? (player.buffered / player.duration) * 100 : 0;
  });

  const volumeIcon = $derived.by(() => {
    if (player.muted || player.volume === 0) return 'volume_off' as const;
    if (player.volume < 0.5) return 'volume_low' as const;
    return 'volume_high' as const;
  });

  const delayedCues = $derived(applyDelay(loadedSubtitleCues, player.subtitleDelay));
  const activeCue = $derived(getActiveCue(delayedCues, player.currentTime));
  const lyricsMode = $derived(
    player.mediaType === 'audio' && delayedCues.length > 0 && player.activeSubtitleIndex >= 0
  );

  // ── Load subtitle cues when active track changes ─────────────────────────────
  $effect(() => {
    const idx = player.activeSubtitleIndex;
    if (idx < 0 || idx >= player.subtitleTracks.length) {
      loadedSubtitleCues = [];
      return;
    }
    const track = player.subtitleTracks[idx];
    const cached = cueCache.get(track.path);
    if (cached) {
      loadedSubtitleCues = cached;
      return;
    }
    invoke<string>('read_subtitle_file', { filePath: track.path })
      .then((content) => {
        const cues = parseSubtitleContent(content, track.format);
        cueCache.set(track.path, cues);
        loadedSubtitleCues = cues;
      })
      .catch(() => {
        loadedSubtitleCues = [];
      });
  });

  // ── Sync media element src + events ─────────────────────────────────────────
  $effect(() => {
    const el = mediaEl;
    if (!el) return;

    el.src = player.mediaSrc ?? '';

    const onPlay = () => {
      player.playing = true;
    };
    const onPause = () => {
      player.playing = false;
    };
    const onTimeUpdate = () => {
      player.currentTime = el.currentTime;
    };
    const onDurationChange = () => {
      player.duration = el.duration || 0;
    };
    const onProgress = () => {
      if (el.buffered.length > 0) {
        player.buffered = el.buffered.end(el.buffered.length - 1);
      }
    };
    const onEnded = () => {
      player.playing = false;
    };
    const onError = () => {
      player.error = 'player.playbackError';
    };
    const onLoadedMetadata = () => {
      player.duration = el.duration;
      player.playing = true;
      el.play().catch(() => { player.playing = false; });
    };

    el.addEventListener('play', onPlay);
    el.addEventListener('pause', onPause);
    el.addEventListener('timeupdate', onTimeUpdate);
    el.addEventListener('durationchange', onDurationChange);
    el.addEventListener('progress', onProgress);
    el.addEventListener('ended', onEnded);
    el.addEventListener('error', onError);
    el.addEventListener('loadedmetadata', onLoadedMetadata);

    return () => {
      el.removeEventListener('play', onPlay);
      el.removeEventListener('pause', onPause);
      el.removeEventListener('timeupdate', onTimeUpdate);
      el.removeEventListener('durationchange', onDurationChange);
      el.removeEventListener('progress', onProgress);
      el.removeEventListener('ended', onEnded);
      el.removeEventListener('error', onError);
      el.removeEventListener('loadedmetadata', onLoadedMetadata);
      el.pause();
      el.src = '';
    };
  });

  // ── Sync play/pause ──────────────────────────────────────────────────────────
  $effect(() => {
    const el = mediaEl;
    if (!el || !el.src) return;
    if (player.playing) {
      el.play().catch(() => {});
    } else {
      el.pause();
    }
  });

  // ── Sync volume ──────────────────────────────────────────────────────────────
  $effect(() => {
    const el = mediaEl;
    if (!el) return;
    el.volume = player.volume;
    el.muted = player.muted;
  });

  // ── Sync playback rate ───────────────────────────────────────────────────────
  $effect(() => {
    const el = mediaEl;
    if (!el) return;
    el.playbackRate = player.playbackRate;
  });

  // ── Fullscreen change listener ───────────────────────────────────────────────
  $effect(() => {
    function onFsChange() {
      isFullscreen = !!document.fullscreenElement;
    }
    document.addEventListener('fullscreenchange', onFsChange);
    return () => document.removeEventListener('fullscreenchange', onFsChange);
  });

  // ── Handlers ─────────────────────────────────────────────────────────────────
  function handleSeek(e: Event) {
    const value = Number((e.target as HTMLInputElement).value);
    player.seek(value);
    const el = mediaEl;
    if (el) el.currentTime = player.currentTime;
  }

  function seekMediaElement() {
    const el = mediaEl;
    if (el) el.currentTime = player.currentTime;
  }

  function handleMouseMove() {
    player.showControls();
  }

  function toggleSubtitleMenu() {
    subtitleMenuOpen = !subtitleMenuOpen;
    if (subtitleMenuOpen) speedMenuOpen = false;
  }

  function toggleSpeedMenu() {
    speedMenuOpen = !speedMenuOpen;
    if (speedMenuOpen) subtitleMenuOpen = false;
    if (speedMenuOpen) player.lockControls();
    else player.unlockControls();
  }

  function toggleFullscreen() {
    const overlay = document.querySelector('.player-overlay');
    if (!overlay) return;
    if (!document.fullscreenElement) {
      overlay.requestFullscreen?.();
      isFullscreen = true;
    } else {
      document.exitFullscreen?.();
      isFullscreen = false;
    }
  }

  function handleOverlayClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (subtitleMenuOpen && !target.closest('.subtitle-control')) {
      subtitleMenuOpen = false;
    }
    if (speedMenuOpen && !target.closest('.speed-control')) {
      speedMenuOpen = false;
      player.unlockControls();
    }
  }

  function handleVideoClick(e: MouseEvent) {
    // Ignore if clicking a child interactive element
    if ((e.target as HTMLElement) !== e.currentTarget) return;
    player.togglePlay();
    player.showControls();
  }

  function handleVideoDblClick(e: MouseEvent) {
    if ((e.target as HTMLElement) !== e.currentTarget) return;
    toggleFullscreen();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!player.open) return;
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

    switch (e.key) {
      case ' ':
      case 'k':
      case 'K':
        e.preventDefault();
        player.togglePlay();
        break;
      case 'ArrowLeft':
      case 'j':
      case 'J':
        e.preventDefault();
        player.seekRelative(-10);
        seekMediaElement();
        break;
      case 'ArrowRight':
      case 'l':
      case 'L':
        e.preventDefault();
        player.seekRelative(10);
        seekMediaElement();
        break;
      case 'ArrowUp':
        e.preventDefault();
        player.setVolume(player.volume + 0.05);
        break;
      case 'ArrowDown':
        e.preventDefault();
        player.setVolume(player.volume - 0.05);
        break;
      case 'm':
      case 'M':
        player.toggleMute();
        break;
      case 'f':
      case 'F':
        if (player.mediaType === 'video') toggleFullscreen();
        break;
      case 'Escape':
        if (speedMenuOpen) {
          speedMenuOpen = false;
          player.unlockControls();
        } else if (subtitleMenuOpen) {
          subtitleMenuOpen = false;
        } else if (isFullscreen) {
          document.exitFullscreen?.();
        } else {
          player.close();
        }
        break;
      case '<':
      case ',':
      case '>':
      case '.':
        player.cyclePlaybackRate();
        break;
    }
    player.showControls();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if player.open}
  <div use:portal>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="player-overlay"
      class:show-cursor={player.controlsVisible}
      role="application"
      aria-label="Media player"
      tabindex="-1"
      onmousemove={handleMouseMove}
      onclick={handleOverlayClick}
      onkeydown={handleKeydown}
    >
      <!-- Video element -->
      {#if player.mediaType === 'video'}
        <!-- svelte-ignore a11y_media_has_caption -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <video
          bind:this={videoEl}
          class="media-video"
          preload="metadata"
          onclick={handleVideoClick}
          ondblclick={handleVideoDblClick}
        ></video>
      {:else}
        <!-- Audio element (invisible) -->
        <!-- svelte-ignore a11y_media_has_caption -->
        <audio bind:this={audioEl} preload="metadata"></audio>

        <!-- Audio visual (hidden when lyrics view is active) -->
        {#if !lyricsMode}
          <div class="audio-visual">
            {#if player.thumbnail}
              <img src={player.thumbnail} alt="" class="audio-thumbnail" />
            {:else}
              <Icon name="music" size={120} />
            {/if}
            <p class="audio-title">{player.title}</p>
          </div>
        {/if}
      {/if}

      <!-- Lyrics view (audio) or subtitle overlay (video) -->
      {#if lyricsMode}
        <LyricsView
          cues={delayedCues}
          currentTime={player.currentTime}
          onseek={(time) => {
            player.seek(time);
            const el = mediaEl;
            if (el) el.currentTime = time;
          }}
        />
      {:else if activeCue}
        <div class="subtitle-overlay">
          <p class="subtitle-text">{@html activeCue.text}</p>
        </div>
      {/if}

      <!-- Loading overlay (remux in progress) -->
      {#if player.loading}
        <div class="loading-display">
          <div class="loading-spinner"></div>
          <p>Preparing video...</p>
        </div>
      {/if}

      <!-- Error display -->
      {#if player.error}
        <div class="error-display">
          <p>{$t(player.error)}</p>
        </div>
      {/if}

      <!-- Top bar -->
      <div class="top-bar" class:visible={player.controlsVisible}>
        <span class="player-title">{player.title}</span>
        <button class="top-btn" onclick={() => player.close()} aria-label="Close player">
          <Icon name="close_large" size={20} />
        </button>
      </div>

      <!-- Bottom controls bar -->
      <div
        class="controls-bar"
        class:visible={player.controlsVisible}
        onmouseenter={() => player.lockControls()}
        onmouseleave={() => player.unlockControls()}
        role="toolbar"
        aria-label="Playback controls"
        tabindex="0"
      >
        <!-- Seekbar -->
        <div class="seekbar-container">
          <input
            type="range"
            class="seekbar"
            min={0}
            max={player.duration || 1}
            step={0.1}
            value={player.currentTime}
            oninput={handleSeek}
            aria-label="Seek"
            style:--seek-percent="{player.progress * 100}%"
            style:--buffered-percent="{bufferedPercent}%"
          />
        </div>

        <div class="controls-row">
          <!-- Left controls -->
          <div class="controls-left">
            <button
              class="ctrl-btn"
              onclick={() => player.togglePlay()}
              aria-label={player.playing ? 'Pause' : 'Play'}
            >
              <Icon name={player.playing ? 'pause' : 'play'} size={22} />
            </button>
            <button
              class="ctrl-btn"
              onclick={() => { player.seekRelative(-10); seekMediaElement(); }}
              aria-label="Seek back 10 seconds"
            >
              <Icon name="skip_backward" size={18} />
            </button>
            <button
              class="ctrl-btn"
              onclick={() => { player.seekRelative(10); seekMediaElement(); }}
              aria-label="Seek forward 10 seconds"
            >
              <Icon name="skip_forward" size={18} />
            </button>
            <span class="time-display" aria-live="off">{player.formattedTime}</span>
          </div>

          <!-- Right controls -->
          <div class="controls-right">
            <!-- Subtitle button -->
            {#if player.subtitleTracks.length > 0}
              <div class="subtitle-control">
                <button
                  class="ctrl-btn"
                  class:active={player.activeSubtitleIndex >= 0}
                  onclick={toggleSubtitleMenu}
                  aria-label="Subtitles"
                  aria-expanded={subtitleMenuOpen}
                >
                  <Icon name="subtitle" size={20} />
                </button>

                {#if subtitleMenuOpen}
                  <div class="subtitle-menu" transition:fade={{ duration: 100 }} role="menu">
                    <button
                      class="sub-option"
                      class:selected={player.activeSubtitleIndex === -1}
                      role="menuitemradio"
                      aria-checked={player.activeSubtitleIndex === -1}
                      onclick={() => {
                        player.setSubtitleTrack(-1);
                        subtitleMenuOpen = false;
                      }}
                    >
                      {$t('player.subtitlesOff')}
                    </button>
                    {#each player.subtitleTracks as track, i}
                      <button
                        class="sub-option"
                        class:selected={player.activeSubtitleIndex === i}
                        role="menuitemradio"
                        aria-checked={player.activeSubtitleIndex === i}
                        onclick={() => {
                          player.setSubtitleTrack(i);
                          subtitleMenuOpen = false;
                        }}
                      >
                        {track.label}
                      </button>
                    {/each}
                    <div class="sub-divider" role="separator"></div>
                    <div class="sub-delay">
                      <span>{$t('player.subtitleDelay')}</span>
                      <button
                        class="delay-btn"
                        onclick={() => player.adjustSubtitleDelay(-0.5)}
                        aria-label="Decrease subtitle delay"
                      >-</button>
                      <span class="delay-value">{player.subtitleDelay.toFixed(1)}s</span>
                      <button
                        class="delay-btn"
                        onclick={() => player.adjustSubtitleDelay(0.5)}
                        aria-label="Increase subtitle delay"
                      >+</button>
                    </div>
                  </div>
                {/if}
              </div>
            {/if}

            <!-- Volume control -->
            <div class="volume-control">
              <button
                class="ctrl-btn"
                onclick={() => player.toggleMute()}
                aria-label={player.muted ? 'Unmute' : 'Mute'}
              >
                <Icon name={volumeIcon} size={20} />
              </button>
              <input
                type="range"
                class="volume-slider"
                min={0}
                max={1}
                step={0.01}
                value={player.muted ? 0 : player.volume}
                oninput={(e) => player.setVolume(Number((e.target as HTMLInputElement).value))}
                aria-label="Volume"
                style:--fill-percent="{(player.muted ? 0 : player.volume) * 100}%"
              />
            </div>

            <!-- Playback speed -->
            <div class="speed-control">
              <button
                class="ctrl-btn speed-btn"
                onclick={toggleSpeedMenu}
                aria-label="Playback speed: {player.playbackRate}x"
                aria-expanded={speedMenuOpen}
              >
                {player.playbackRate}x
              </button>

              {#if speedMenuOpen}
                <div class="speed-menu" transition:fade={{ duration: 100 }} role="menu">
                  {#each [0.5, 0.75, 1, 1.25, 1.5, 1.75, 2, 3] as rate}
                    <button
                      class="sub-option"
                      class:selected={player.playbackRate === rate}
                      role="menuitemradio"
                      aria-checked={player.playbackRate === rate}
                      onclick={() => {
                        player.setPlaybackRate(rate);
                        speedMenuOpen = false;
                        player.unlockControls();
                      }}
                    >
                      {rate}x
                    </button>
                  {/each}
                </div>
              {/if}
            </div>

            <!-- Fullscreen (video only) -->
            {#if player.mediaType === 'video'}
              <button
                class="ctrl-btn"
                onclick={toggleFullscreen}
                aria-label={isFullscreen ? 'Exit fullscreen' : 'Enter fullscreen'}
              >
                <Icon name={isFullscreen ? 'fullscreen_exit' : 'fullscreen'} size={20} />
              </button>
            {/if}
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .player-overlay {
    position: fixed;
    inset: 0;
    background: #000;
    z-index: 2000;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    cursor: none;
    user-select: none;
  }

  .player-overlay.show-cursor {
    cursor: default;
  }

  /* ── Video ─────────────────────────────────────────────────────────────────── */
  .media-video {
    width: 100%;
    height: 100%;
    object-fit: contain;
    cursor: inherit;
  }

  /* ── Audio visual ──────────────────────────────────────────────────────────── */
  .audio-visual {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 24px;
    color: rgba(255, 255, 255, 0.4);
  }

  .audio-thumbnail {
    width: 280px;
    height: 280px;
    object-fit: cover;
    border-radius: var(--radius-lg, 12px);
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.5);
  }

  .audio-title {
    font-size: var(--text-xl, 18px);
    font-weight: 600;
    color: white;
    text-align: center;
    max-width: 500px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin: 0;
  }

  /* ── Subtitle overlay ──────────────────────────────────────────────────────── */
  .subtitle-overlay {
    position: absolute;
    bottom: 100px;
    left: 50%;
    transform: translateX(-50%);
    max-width: 80%;
    text-align: center;
    pointer-events: none;
    z-index: 1;
  }

  .subtitle-text {
    display: inline;
    font-size: 22px;
    line-height: 1.4;
    color: white;
    background: rgba(0, 0, 0, 0.6);
    padding: 4px 12px;
    border-radius: 4px;
    text-shadow:
      0 1px 4px rgba(0, 0, 0, 0.9),
      0 0 2px rgba(0, 0, 0, 0.9);
    box-decoration-break: clone;
    -webkit-box-decoration-break: clone;
  }

  /* ── Loading display (remux) ───────────────────────────────────────────────── */
  .loading-display {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    z-index: 2;
    background: rgba(0, 0, 0, 0.8);
  }

  .loading-display p {
    color: rgba(255, 255, 255, 0.7);
    font-size: var(--text-md, 14px);
    margin: 0;
  }

  .loading-spinner {
    width: 32px;
    height: 32px;
    border: 3px solid rgba(255, 255, 255, 0.2);
    border-top-color: var(--accent, #fff);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* ── Error display ─────────────────────────────────────────────────────────── */
  .error-display {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    z-index: 1;
  }

  .error-display p {
    color: rgba(255, 255, 255, 0.7);
    font-size: var(--text-md, 14px);
    text-align: center;
    padding: 20px;
    background: rgba(0, 0, 0, 0.5);
    border-radius: var(--radius, 8px);
    margin: 0;
  }

  /* ── Top bar ───────────────────────────────────────────────────────────────── */
  .top-bar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    background: linear-gradient(to bottom, rgba(0, 0, 0, 0.7), transparent);
    opacity: 0;
    transition: opacity 0.2s;
    z-index: 2;
  }

  .top-bar.visible {
    opacity: 1;
  }

  .player-title {
    font-size: var(--text-lg, 16px);
    font-weight: 600;
    color: white;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    margin-right: 12px;
  }

  .top-btn {
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    padding: 6px;
    border-radius: var(--radius-sm, 6px);
    transition: all 0.15s;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .top-btn:hover {
    color: white;
    background: rgba(255, 255, 255, 0.1);
  }

  /* ── Controls bar ──────────────────────────────────────────────────────────── */
  .controls-bar {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    padding: 0 16px 16px;
    background: linear-gradient(to top, rgba(0, 0, 0, 0.7), transparent);
    opacity: 0;
    transition: opacity 0.2s;
    z-index: 2;
  }

  .controls-bar.visible {
    opacity: 1;
  }

  /* ── Seekbar ───────────────────────────────────────────────────────────────── */
  .seekbar-container {
    width: 100%;
    padding: 8px 0;
  }

  .seekbar {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      var(--accent, #6366f1) 0%,
      var(--accent, #6366f1) var(--seek-percent, 0%),
      rgba(255, 255, 255, 0.2) var(--seek-percent, 0%),
      rgba(255, 255, 255, 0.2) var(--buffered-percent, 0%),
      rgba(255, 255, 255, 0.08) var(--buffered-percent, 0%),
      rgba(255, 255, 255, 0.08) 100%
    );
    cursor: pointer;
    transition: height 0.1s;
    display: block;
  }

  .seekbar:hover {
    height: 6px;
  }

  .seekbar::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 14px;
    height: 14px;
    background: white;
    border-radius: 50%;
    border: none;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.4);
    opacity: 0;
    transition: opacity 0.15s;
  }

  .seekbar-container:hover .seekbar::-webkit-slider-thumb {
    opacity: 1;
  }

  .seekbar::-moz-range-thumb {
    width: 14px;
    height: 14px;
    background: white;
    border-radius: 50%;
    border: none;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.4);
  }

  .seekbar::-moz-range-track {
    background: transparent;
    border: none;
  }

  /* ── Controls row ──────────────────────────────────────────────────────────── */
  .controls-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .controls-left,
  .controls-right {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .ctrl-btn {
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.9);
    cursor: pointer;
    padding: 8px;
    border-radius: var(--radius-sm, 6px);
    transition: all 0.15s;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .ctrl-btn:hover {
    color: white;
    background: rgba(255, 255, 255, 0.1);
  }

  .ctrl-btn.active {
    color: var(--accent, #6366f1);
  }

  .speed-btn {
    font-size: var(--text-sm, 12px);
    font-weight: 600;
    min-width: 36px;
    font-family: 'JetBrains Mono', monospace;
  }

  .time-display {
    font-size: var(--text-sm, 12px);
    font-family: 'JetBrains Mono', monospace;
    color: rgba(255, 255, 255, 0.8);
    margin-left: 8px;
    user-select: none;
  }

  /* ── Volume control ────────────────────────────────────────────────────────── */
  .volume-control {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .volume-slider {
    -webkit-appearance: none;
    appearance: none;
    width: 80px;
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      white 0%,
      white var(--fill-percent, 100%),
      rgba(255, 255, 255, 0.15) var(--fill-percent, 100%),
      rgba(255, 255, 255, 0.15) 100%
    );
    cursor: pointer;
  }

  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    background: white;
    border-radius: 50%;
    border: none;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
  }

  .volume-slider::-moz-range-thumb {
    width: 12px;
    height: 12px;
    background: white;
    border-radius: 50%;
    border: none;
  }

  /* ── Subtitle control & menu ───────────────────────────────────────────────── */
  .subtitle-control {
    position: relative;
  }

  .subtitle-menu {
    position: absolute;
    bottom: 100%;
    right: 0;
    margin-bottom: 8px;
    background: rgba(20, 20, 20, 0.95);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 8px);
    min-width: 180px;
    padding: 6px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
  }

  /* ── Speed control & menu ──────────────────────────────────────────────────── */
  .speed-control {
    position: relative;
  }

  .speed-menu {
    position: absolute;
    bottom: 100%;
    right: 0;
    margin-bottom: 8px;
    background: rgba(20, 20, 20, 0.95);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 8px);
    min-width: 80px;
    padding: 6px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
  }

  .sub-option {
    display: block;
    width: 100%;
    padding: 8px 12px;
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.8);
    font-size: var(--text-sm, 12px);
    text-align: left;
    cursor: pointer;
    border-radius: var(--radius-sm, 4px);
    transition: background 0.1s;
  }

  .sub-option:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .sub-option.selected {
    color: var(--accent, #6366f1);
    font-weight: 600;
  }

  .sub-divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.1);
    margin: 4px 0;
  }

  .sub-delay {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    font-size: var(--text-sm, 12px);
    color: rgba(255, 255, 255, 0.6);
  }

  .sub-delay span:first-child {
    flex: 1;
  }

  .delay-btn {
    background: rgba(255, 255, 255, 0.08);
    border: none;
    color: white;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm, 4px);
    cursor: pointer;
    font-size: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.1s;
  }

  .delay-btn:hover {
    background: rgba(255, 255, 255, 0.15);
  }

  .delay-value {
    font-family: 'JetBrains Mono', monospace;
    min-width: 44px;
    text-align: center;
    color: white;
  }

  /* ── Responsive ────────────────────────────────────────────────────────────── */
  @media (max-width: 640px) {
    .controls-bar {
      padding: 0 8px 8px;
    }

    .volume-slider {
      display: none;
    }

    .subtitle-text {
      font-size: 16px;
    }

    .subtitle-overlay {
      bottom: 80px;
      max-width: 90%;
    }
  }

  @media (pointer: coarse) {
    .ctrl-btn {
      padding: 12px;
    }

    .seekbar {
      height: 6px;
    }

    .seekbar::-webkit-slider-thumb {
      width: 20px;
      height: 20px;
      opacity: 1;
    }
  }
</style>
