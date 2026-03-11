import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { formatPlayerTime } from '$lib/utils/subtitles';
import type { SubtitleCue } from '$lib/utils/subtitles';
import { openFile } from '$lib/utils/platform';
import { isMobile } from '$lib/utils/android';

export type { SubtitleCue };

export interface SubtitleTrack {
  path: string;
  label: string;
  format: string;
  cues?: SubtitleCue[]; // lazy-loaded
}

export interface PlayerOpenOptions {
  filePath: string;
  mediaType: 'video' | 'audio';
  title?: string;
  thumbnail?: string;
}

export class PlayerState {
  // Core state
  open = $state(false);
  filePath = $state<string | null>(null);
  mediaSrc = $state<string | null>(null); // convertFileSrc result
  mediaType = $state<'video' | 'audio'>('video');
  title = $state('');
  thumbnail = $state<string | null>(null);

  // Playback state
  playing = $state(false);
  currentTime = $state(0);
  duration = $state(0);
  buffered = $state(0); // buffered time in seconds
  volume = $state(1);
  muted = $state(false);
  playbackRate = $state(1);
  error = $state<string | null>(null);

  // Subtitles
  subtitleTracks = $state<SubtitleTrack[]>([]);
  activeSubtitleIndex = $state(-1); // -1 = off
  subtitleDelay = $state(0); // seconds

  // UI state
  controlsVisible = $state(true);

  // Internal
  private _hideTimer: ReturnType<typeof setTimeout> | null = null;
  private _controlsLocked = false; // when hovering over controls or menu open

  // Computed
  progress = $derived(this.duration > 0 ? this.currentTime / this.duration : 0);
  formattedTime = $derived.by(() => {
    return `${formatPlayerTime(this.currentTime)} / ${formatPlayerTime(this.duration)}`;
  });

  // Extensions natively supported by HTML5 <video>/<audio>.
  private static readonly HTML5_SUPPORTED_EXTENSIONS = new Set([
    'mp4',
    'webm',
    'ogg',
    'mp3',
    'wav',
    'm4a',
    'aac',
    'flac',
    'opus',
  ]);

  loading = $state(false);

  // Whether we're currently using an FFmpeg stream (need cleanup on close)
  private _streaming = false;

  // Methods
  async openMedia(opts: PlayerOpenOptions) {
    const ext = opts.filePath.split('.').pop()?.toLowerCase() ?? '';

    let mediaSrc: string;

    if (PlayerState.HTML5_SUPPORTED_EXTENSIONS.has(ext) || isMobile()) {
      // Natively supported, or on mobile where the WebView often handles
      // MKV/other containers via system codecs — play directly.
      mediaSrc = convertFileSrc(opts.filePath);
    } else {
      // Desktop non-native format — stream via FFmpeg (real-time remux)
      this.loading = true;
      this.title = opts.title ?? opts.filePath.split(/[\\/]/).pop() ?? 'Unknown';
      this.thumbnail = opts.thumbnail ?? null;
      this.open = true;
      try {
        const tempPath = await invoke<string>('start_media_stream', { filePath: opts.filePath });
        mediaSrc = convertFileSrc(tempPath);
        this._streaming = true;
      } catch (e) {
        // Streaming failed — fall back to system player
        this.close();
        await openFile(opts.filePath);
        return;
      } finally {
        this.loading = false;
      }
    }

    this.filePath = opts.filePath;
    this.mediaType = opts.mediaType;
    this.title = opts.title ?? opts.filePath.split(/[\\/]/).pop() ?? 'Unknown';
    this.thumbnail = opts.thumbnail ?? null;
    this.mediaSrc = mediaSrc;
    this.open = true;
    this.error = null;

    // Discover subtitles
    try {
      const tracks: { path: string; label: string; format: string }[] = await invoke(
        'find_subtitles',
        { filePath: opts.filePath }
      );
      this.subtitleTracks = tracks;
      // Auto-select first subtitle if available
      if (tracks.length > 0) {
        this.activeSubtitleIndex = 0;
      }
    } catch {
      // No subtitles found, that's fine
      this.subtitleTracks = [];
    }
  }

  close() {
    this.open = false;
    this.playing = false;
    this.filePath = null;
    this.mediaSrc = null;
    this.currentTime = 0;
    this.duration = 0;
    this.buffered = 0;
    this.error = null;
    this.subtitleTracks = [];
    this.activeSubtitleIndex = -1;
    this.subtitleDelay = 0;
    this.controlsVisible = true;
    this._clearHideTimer();

    if (this._streaming) {
      this._streaming = false;
      invoke('stop_media_stream').catch(() => {});
    }
  }

  togglePlay() {
    this.playing = !this.playing;
  }

  seek(time: number) {
    this.currentTime = Math.max(0, Math.min(time, this.duration));
  }

  seekRelative(delta: number) {
    this.seek(this.currentTime + delta);
  }

  setVolume(v: number) {
    this.volume = Math.max(0, Math.min(1, v));
    if (this.volume > 0) this.muted = false;
  }

  toggleMute() {
    this.muted = !this.muted;
  }

  cyclePlaybackRate() {
    const rates = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2];
    const idx = rates.indexOf(this.playbackRate);
    this.playbackRate = rates[(idx + 1) % rates.length];
  }

  setPlaybackRate(rate: number) {
    this.playbackRate = rate;
  }

  setSubtitleTrack(index: number) {
    this.activeSubtitleIndex = index;
  }

  adjustSubtitleDelay(delta: number) {
    this.subtitleDelay = Math.round((this.subtitleDelay + delta) * 10) / 10;
  }

  setSubtitleDelay(value: number) {
    this.subtitleDelay = Math.round(value * 10) / 10;
  }

  // Controls visibility
  showControls() {
    this.controlsVisible = true;
    this._resetHideTimer();
  }

  lockControls() {
    this._controlsLocked = true;
    this.controlsVisible = true;
    this._clearHideTimer();
  }

  unlockControls() {
    this._controlsLocked = false;
    this._resetHideTimer();
  }

  private _resetHideTimer() {
    this._clearHideTimer();
    if (this.playing && !this._controlsLocked) {
      this._hideTimer = setTimeout(() => {
        this.controlsVisible = false;
      }, 3000);
    }
  }

  private _clearHideTimer() {
    if (this._hideTimer) {
      clearTimeout(this._hideTimer);
      this._hideTimer = null;
    }
  }
}

export const player = new PlayerState();
