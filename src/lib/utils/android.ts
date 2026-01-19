/**
 * Android platform utilities
 * Handles the bridge to the native youtubedl-android library
 */

interface AndroidYtDlp {
  isReady(): boolean;
  getVersion(): string;
  getVideoInfo(url: string, callbackName: string): void;
  getVideoInfoWithClient(
    url: string,
    youtubePlayerClient: string | null,
    callbackName: string
  ): void;
  getPlaylistInfo(url: string, callbackName: string): void;
  getPlaylistInfoWithClient(
    url: string,
    youtubePlayerClient: string | null,
    callbackName: string
  ): void;
  // Unified job-based API (emits DOM 'job-event' CustomEvents)
  startDownloadJob?(jobId: string, url: string, settingsJson: string): void;
  // V2: Rust builds options; Android executes them
  startDownloadJobWithOptions?(jobId: string, optionsJson: string, outputConfigJson: string): void;
  cancelJob?(jobId: string): boolean;
  openFile(filePath: string): boolean;
  openFolder(filePath: string): boolean;
  pickFile(mimeTypes: string, callbackName: string): void;
  pickFolder(callbackName: string): void;
  processYtmThumbnail(thumbnailUrl: string, callbackName: string): void;
}

interface AndroidWindow extends Window {
  AndroidYtDlp?: AndroidYtDlp;
  __YTDLP_READY__?: boolean;
  __androidLog?: (level: string, source: string, message: string) => void;
}

declare let window: AndroidWindow;

type LogHandler = (
  level: 'trace' | 'debug' | 'info' | 'warn' | 'error',
  source: string,
  message: string
) => void;
let logHandler: LogHandler | null = null;

/**
 * Set up the Android log handler to forward logs to the logs store
 */
export function setupAndroidLogHandler(handler: LogHandler): void {
  logHandler = handler;
  window.__androidLog = (level: string, source: string, message: string) => {
    const validLevel = ['trace', 'debug', 'info', 'warn', 'error'].includes(level)
      ? (level as 'trace' | 'debug' | 'info' | 'warn' | 'error')
      : 'debug';
    handler(validLevel, source, message);
  };
}

/**
 * Check if running on Android
 */
export function isAndroid(): boolean {
  return typeof window !== 'undefined' && 'AndroidYtDlp' in window;
}

/**
 * Check if running on desktop (not Android)
 */
export function isDesktop(): boolean {
  return typeof window !== 'undefined' && !('AndroidYtDlp' in window);
}

/**
 * Check if the Android yt-dlp is ready
 */
export function isAndroidYtDlpReady(): boolean {
  if (!isAndroid()) return false;
  try {
    return window.AndroidYtDlp?.isReady() ?? false;
  } catch {
    return false;
  }
}

/**
 * Get the Android yt-dlp version
 */
export function getAndroidYtDlpVersion(): string | null {
  if (!isAndroid()) return null;
  try {
    return window.AndroidYtDlp?.getVersion() ?? null;
  } catch {
    return null;
  }
}

let callbackCounter = 0;

type AndroidCallback = (json: string) => void;

interface AndroidCallbacks {
  [key: string]: AndroidCallback | undefined;
}

const activeCallbacks = new Set<string>();

export function cleanupAndroidCallbacks(): void {
  activeCallbacks.forEach((name) => {
    const callbacks = window as unknown as AndroidCallbacks;
    delete callbacks[name];
    delete callbacks[`${name}_progress` as any];
  });
  activeCallbacks.clear();
}

/**
 * Download settings for Android
 */
export interface AndroidDownloadSettings {
  aria2Connections?: number;
  aria2Splits?: number;
  aria2MinSplitSize?: string;
  speedLimit?: number;
  downloadPath?: string | null;
  youtubePlayerClient?: string | null;
  // Extended settings (V3)
  outputTemplate?: string | null;
  embedThumbnail?: boolean;
  embedChapters?: boolean;
  embedSubtitles?: boolean;
  subtitleLanguages?: string | null;
  sponsorBlock?: boolean;
  sponsorBlockCategories?: string[]; // e.g. ['sponsor', 'intro', 'outro', 'selfpromo', 'interaction']
  clearMetadata?: boolean;
  remux?: boolean;
  convertToMp4?: boolean;
  clipRanges?: { start: number; end: number }[] | null;
}

export type AndroidYtDlpJobSettings = AndroidDownloadSettings & {
  format?: string;
  playlistFolder?: string | null;
  isAudioOnly?: boolean;
};

export function startAndroidDownloadJob(url: string, settings: AndroidYtDlpJobSettings): string {
  if (!isAndroid() || !window.AndroidYtDlp?.startDownloadJob) {
    throw new Error('Android job bridge not available');
  }
  if (!isAndroidYtDlpReady()) {
    throw new Error('Android yt-dlp is initializing, please wait...');
  }

  const jobId = `android-${Date.now()}-${++callbackCounter}`;
  const settingsPayload = {
    format: settings.format ?? 'best',
    playlistFolder: settings.playlistFolder ?? null,
    isAudioOnly: settings.isAudioOnly ?? false,
    aria2Connections: settings.aria2Connections ?? 8,
    aria2Splits: settings.aria2Splits ?? 8,
    aria2MinSplitSize: settings.aria2MinSplitSize ?? '1M',
    speedLimit: settings.speedLimit ?? 0,
    youtubePlayerClient: settings.youtubePlayerClient ?? null,
    outputTemplate: settings.outputTemplate ?? null,
    embedThumbnail: settings.embedThumbnail ?? true,
    embedChapters: settings.embedChapters ?? true,
    embedSubtitles: settings.embedSubtitles ?? false,
    subtitleLanguages: settings.subtitleLanguages ?? null,
    sponsorBlock: settings.sponsorBlock ?? false,
    sponsorBlockCategories: settings.sponsorBlockCategories ?? [],
    remux: settings.remux ?? true,
    convertToMp4: settings.convertToMp4 ?? false,
    clipRanges: settings.clipRanges ?? null,
  };

  const outputConfigJson = JSON.stringify({
    downloadPath: settings.downloadPath ?? null,
    playlistFolder: settings.playlistFolder ?? null,
  });

  // Prefer V2 path: Rust builds yt-dlp options; Android only executes.
  if (window.AndroidYtDlp.startDownloadJobWithOptions) {
    // Note: invoke is available on Android; keep logic centralized in Rust.
    // Dynamic import avoids pulling tauri api into non-android builds unnecessarily.
    void (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const result = await invoke<{ options: Array<{ key: string; value?: string | null }> }>(
          'build_android_ytdlp_options',
          {
            url,
            settingsJson: JSON.stringify(settingsPayload),
          }
        );
        window.AndroidYtDlp!.startDownloadJobWithOptions!(
          jobId,
          JSON.stringify(result.options),
          outputConfigJson
        );
      } catch (e) {
        logHandler?.('warn', 'Android', `build_android_ytdlp_options failed; using legacy: ${e}`);
        const legacySettingsJson = JSON.stringify({
          ...settingsPayload,
          downloadPath: settings.downloadPath ?? null,
          clearMetadata: settings.clearMetadata ?? false,
        });
        window.AndroidYtDlp!.startDownloadJob!(jobId, url, legacySettingsJson);
      }
    })();
    return jobId;
  }

  // Fallback: legacy settings-based method
  const settingsJson = JSON.stringify({
    ...settingsPayload,
    downloadPath: settings.downloadPath ?? null,
    clearMetadata: settings.clearMetadata ?? false,
  });
  window.AndroidYtDlp.startDownloadJob(jobId, url, settingsJson);
  return jobId;
}

export function cancelAndroidJob(jobId: string): boolean {
  if (!isAndroid() || !window.AndroidYtDlp?.cancelJob) return false;
  try {
    return window.AndroidYtDlp.cancelJob(jobId);
  } catch (e) {
    logHandler?.('warn', 'Android', `cancelJob failed: ${e}`);
    return false;
  }
}

export function getVideoInfoOnAndroid(
  url: string,
  youtubePlayerClient?: string | null
): Promise<Record<string, unknown> | null> {
  return new Promise((resolve, reject) => {
    if (!isAndroid() || !window.AndroidYtDlp) {
      resolve(null);
      return;
    }

    if (!isAndroidYtDlpReady()) {
      reject(new Error('Android yt-dlp is initializing, please wait...'));
      return;
    }

    const callbackName = `ytdlp_info_cb_${++callbackCounter}`;
    activeCallbacks.add(callbackName);
    let hasCompleted = false;

    const timeout = setTimeout(() => {
      if (!hasCompleted) {
        hasCompleted = true;
        activeCallbacks.delete(callbackName);
        const callbacks = window as unknown as AndroidCallbacks;
        delete callbacks[callbackName];
        logHandler?.('warn', 'Android', 'Video info fetch timeout');
        reject(new Error('Video info fetch timeout'));
      }
    }, 60000);

    const callbacks = window as unknown as AndroidCallbacks;
    callbacks[callbackName] = (json: string) => {
      if (hasCompleted) return;
      hasCompleted = true;
      clearTimeout(timeout);

      activeCallbacks.delete(callbackName);
      delete callbacks[callbackName];

      try {
        const data = JSON.parse(json);
        if (data.error) {
          logHandler?.('warn', 'Android', `Failed to get video info: ${data.error}`);
          reject(new Error(data.error));
        } else {
          logHandler?.('info', 'Android', `Got video info: ${data.title}`);
          resolve(data);
        }
      } catch (e) {
        logHandler?.(
          'error',
          'Android',
          `Failed to parse video info: ${e}, raw: ${json.substring(0, 100)}`
        );
        reject(new Error(`Failed to parse video info: ${e}`));
      }
    };

    try {
      logHandler?.(
        'debug',
        'Android',
        `Fetching video info for: ${url}${youtubePlayerClient ? ` (playerClient: ${youtubePlayerClient})` : ''}`
      );
      if (youtubePlayerClient && window.AndroidYtDlp.getVideoInfoWithClient) {
        window.AndroidYtDlp.getVideoInfoWithClient(url, youtubePlayerClient, callbackName);
      } else {
        window.AndroidYtDlp.getVideoInfo(url, callbackName);
      }
    } catch (error) {
      hasCompleted = true;
      clearTimeout(timeout);
      activeCallbacks.delete(callbackName);
      delete callbacks[callbackName];
      logHandler?.('error', 'Android', `Failed to call getVideoInfo: ${error}`);
      reject(error);
    }
  });
}

/**
 * Playlist entry type matching the Android/Rust PlaylistEntry struct
 */
export interface PlaylistEntry {
  id: string;
  url: string;
  title: string;
  duration: number | null;
  thumbnail: string | null;
  uploader: string | null;
  is_music: boolean;
}

/**
 * Playlist info result type matching the Android/Rust PlaylistInfo struct
 */
export interface PlaylistInfo {
  is_playlist: boolean;
  id: string | null;
  title: string;
  uploader: string | null;
  thumbnail: string | null;
  total_count: number;
  entries: PlaylistEntry[];
  has_more: boolean;
}

export function getPlaylistInfoOnAndroid(
  url: string,
  youtubePlayerClient?: string | null
): Promise<PlaylistInfo> {
  return new Promise((resolve, reject) => {
    if (!isAndroid() || !window.AndroidYtDlp) {
      reject(new Error('Android yt-dlp bridge not available'));
      return;
    }

    if (!isAndroidYtDlpReady()) {
      reject(new Error('Android yt-dlp is initializing, please wait...'));
      return;
    }

    const callbackName = `ytdlp_playlist_cb_${++callbackCounter}`;
    activeCallbacks.add(callbackName);
    let hasCompleted = false;

    const timeout = setTimeout(() => {
      if (!hasCompleted) {
        hasCompleted = true;
        activeCallbacks.delete(callbackName);
        const callbacks = window as unknown as AndroidCallbacks;
        delete callbacks[callbackName];
        logHandler?.('warn', 'Android', 'Playlist info fetch timeout');
        reject(new Error('Playlist info fetch timeout'));
      }
    }, 120000);

    const callbacks = window as unknown as AndroidCallbacks;
    callbacks[callbackName] = (json: string) => {
      if (hasCompleted) return;
      hasCompleted = true;
      clearTimeout(timeout);

      activeCallbacks.delete(callbackName);
      delete callbacks[callbackName];

      try {
        const data = JSON.parse(json);
        if (data.error) {
          logHandler?.('warn', 'Android', `Failed to get playlist info: ${data.error}`);
          reject(new Error(data.error));
        } else {
          logHandler?.(
            'info',
            'Android',
            `Got playlist info: ${data.title} with ${data.total_count} entries`
          );
          resolve(data as PlaylistInfo);
        }
      } catch (e) {
        logHandler?.(
          'error',
          'Android',
          `Failed to parse playlist info: ${e}, raw: ${json.substring(0, 100)}`
        );
        reject(new Error(`Failed to parse playlist info: ${e}`));
      }
    };

    try {
      logHandler?.(
        'debug',
        'Android',
        `Fetching playlist info for: ${url}${youtubePlayerClient ? ` (playerClient: ${youtubePlayerClient})` : ''}`
      );
      if (youtubePlayerClient && window.AndroidYtDlp.getPlaylistInfoWithClient) {
        window.AndroidYtDlp.getPlaylistInfoWithClient(url, youtubePlayerClient, callbackName);
      } else {
        window.AndroidYtDlp.getPlaylistInfo(url, callbackName);
      }
    } catch (error) {
      hasCompleted = true;
      clearTimeout(timeout);
      activeCallbacks.delete(callbackName);
      delete callbacks[callbackName];
      logHandler?.('error', 'Android', `Failed to call getPlaylistInfo: ${error}`);
      reject(error);
    }
  });
}

/**
 * Wait for Android yt-dlp to be ready
 */
export function waitForAndroidYtDlp(): Promise<void> {
  return new Promise((resolve) => {
    if (isAndroidYtDlpReady()) {
      resolve();
      return;
    }

    let resolved = false;
    let interval: ReturnType<typeof setInterval> | null = null;
    let timeout: ReturnType<typeof setTimeout> | null = null;

    const cleanup = () => {
      if (resolved) return;
      resolved = true;
      if (interval) clearInterval(interval);
      if (timeout) clearTimeout(timeout);
      window.removeEventListener('ytdlp-ready', handler);
      resolve();
    };

    const handler = () => cleanup();

    window.addEventListener('ytdlp-ready', handler);

    interval = setInterval(() => {
      if (isAndroidYtDlpReady()) {
        cleanup();
      }
    }, 500);

    timeout = setTimeout(cleanup, 30000);
  });
}

/**
 * Share intent event detail type
 */
export interface ShareIntentEvent {
  url: string;
}

/**
 * Listen for share intent events from Android
 * @param callback Function to call when a URL is shared to the app
 * @returns Cleanup function to remove the listener
 */
export function onShareIntent(callback: (url: string) => void): () => void {
  if (!isAndroid()) {
    return () => {}; // No-op on desktop
  }

  const handler = (event: Event) => {
    const customEvent = event as CustomEvent<ShareIntentEvent>;
    const url = customEvent.detail?.url;
    if (url) {
      callback(url);
    }
  };

  window.addEventListener('share-intent', handler);
  return () => window.removeEventListener('share-intent', handler);
}

/**
 * Open a file on Android using the default app
 */
export function openFileOnAndroid(filePath: string): boolean {
  if (!isAndroid() || !window.AndroidYtDlp) {
    return false;
  }
  try {
    return window.AndroidYtDlp.openFile(filePath);
  } catch (e) {
    logHandler?.('error', 'Android', `Failed to open file: ${e}`);
    return false;
  }
}

/**
 * Open the folder containing a file on Android
 */
export function openFolderOnAndroid(filePath: string): boolean {
  if (!isAndroid() || !window.AndroidYtDlp) {
    return false;
  }
  try {
    return window.AndroidYtDlp.openFolder(filePath);
  } catch (e) {
    logHandler?.('error', 'Android', `Failed to open folder: ${e}`);
    return false;
  }
}

/**
 * Pick a file on Android using the system file picker
 * @param mimeTypes Comma-separated MIME types (e.g., "video/*" or "image/*")
 * @returns Promise that resolves with the content:// URI or null if cancelled
 */
export function pickFileOnAndroid(mimeTypes: string): Promise<string | null> {
  return new Promise((resolve) => {
    if (!isAndroid() || !window.AndroidYtDlp) {
      resolve(null);
      return;
    }

    const callbackName = `file_pick_cb_${++callbackCounter}`;
    activeCallbacks.add(callbackName);

    const timeout = setTimeout(() => {
      activeCallbacks.delete(callbackName);
      const callbacks = window as unknown as AndroidCallbacks;
      delete callbacks[callbackName];
      resolve(null);
    }, 300000);

    const callbacks = window as unknown as AndroidCallbacks;
    callbacks[callbackName] = (uri: string | null) => {
      clearTimeout(timeout);
      activeCallbacks.delete(callbackName);
      delete callbacks[callbackName];
      resolve(uri || null);
    };

    try {
      window.AndroidYtDlp.pickFile(mimeTypes, callbackName);
    } catch (error) {
      clearTimeout(timeout);
      activeCallbacks.delete(callbackName);
      const callbacks = window as unknown as AndroidCallbacks;
      delete callbacks[callbackName];
      logHandler?.('error', 'Android', `Failed to open file picker: ${error}`);
      resolve(null);
    }
  });
}

/**
 * Pick a folder on Android using the Storage Access Framework.
 * @returns Promise that resolves with the folder path or null if cancelled
 */
export function pickFolderOnAndroid(): Promise<string | null> {
  return new Promise((resolve) => {
    if (!isAndroid() || !window.AndroidYtDlp) {
      resolve(null);
      return;
    }

    const callbackName = `folder_pick_cb_${++callbackCounter}`;
    activeCallbacks.add(callbackName);

    const timeout = setTimeout(() => {
      activeCallbacks.delete(callbackName);
      const callbacks = window as unknown as AndroidCallbacks;
      delete callbacks[callbackName];
      resolve(null);
    }, 300000);

    const callbacks = window as unknown as AndroidCallbacks;
    callbacks[callbackName] = (resultJson: string) => {
      clearTimeout(timeout);
      activeCallbacks.delete(callbackName);
      delete callbacks[callbackName];

      try {
        const result = JSON.parse(resultJson);
        if (result.success && result.path) {
          resolve(result.path);
        } else if (result.cancelled) {
          resolve(null);
        } else {
          logHandler?.(
            'error',
            'Android',
            `Folder picker error: ${result.error || 'Unknown error'}`
          );
          resolve(null);
        }
      } catch (error) {
        logHandler?.('error', 'Android', `Failed to parse folder picker result: ${error}`);
        resolve(null);
      }
    };

    try {
      window.AndroidYtDlp.pickFolder(callbackName);
    } catch (error) {
      clearTimeout(timeout);
      activeCallbacks.delete(callbackName);
      const callbacks = window as unknown as AndroidCallbacks;
      delete callbacks[callbackName];
      logHandler?.('error', 'Android', `Failed to open folder picker: ${error}`);
      resolve(null);
    }
  });
}

/**
 * Process a YouTube Music thumbnail on Android - detect letterboxing and crop to 1:1
 * @param thumbnailUrl URL of the thumbnail to process
 * @returns Promise that resolves with the processed thumbnail URL (base64 data URI if cropped, original URL if not)
 */
export function processYtmThumbnailOnAndroid(thumbnailUrl: string): Promise<string> {
  return new Promise((resolve) => {
    if (!isAndroid() || !window.AndroidYtDlp) {
      resolve(thumbnailUrl);
      return;
    }

    const callbackName = `thumb_process_cb_${++callbackCounter}`;
    activeCallbacks.add(callbackName);

    const timeout = setTimeout(() => {
      activeCallbacks.delete(callbackName);
      const callbacks = window as unknown as AndroidCallbacks;
      delete callbacks[callbackName];
      logHandler?.('warn', 'Android', 'Thumbnail processing timeout');
      resolve(thumbnailUrl);
    }, 30000);

    const callbacks = window as unknown as AndroidCallbacks;
    callbacks[callbackName] = (json: string) => {
      clearTimeout(timeout);
      activeCallbacks.delete(callbackName);
      delete callbacks[callbackName];

      try {
        const data = JSON.parse(json);
        const resultUrl = data.url || thumbnailUrl;
        if (resultUrl !== thumbnailUrl) {
          logHandler?.('info', 'Android', 'Thumbnail cropped successfully');
        }
        resolve(resultUrl);
      } catch (e) {
        logHandler?.('error', 'Android', `Failed to parse thumbnail result: ${e}`);
        resolve(thumbnailUrl);
      }
    };

    try {
      logHandler?.('debug', 'Android', `Processing YTM thumbnail: ${thumbnailUrl}`);
      window.AndroidYtDlp.processYtmThumbnail(thumbnailUrl, callbackName);
    } catch (error) {
      clearTimeout(timeout);
      activeCallbacks.delete(callbackName);
      const callbacks = window as unknown as AndroidCallbacks;
      delete callbacks[callbackName];
      logHandler?.('error', 'Android', `Failed to call processYtmThumbnail: ${error}`);
      resolve(thumbnailUrl);
    }
  });
}
