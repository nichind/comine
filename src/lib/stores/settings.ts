import { writable, get } from 'svelte/store';
import { load, type Store } from '@tauri-apps/plugin-store';
import { logs } from './logs';
import { isAndroid } from '$lib/utils/android';

export type VideoQuality = 'max' | '4k' | '1440p' | '1080p' | '720p' | '480p' | '360p' | '240p';
export type DownloadMode = 'auto' | 'audio' | 'mute';
export type AudioQuality = 'best' | '320' | '256' | '192' | '128' | '96';
export type DefaultProcessor = 'auto' | 'yt-dlp' | 'lux';

export interface CustomPreset {
  id: string;
  label: string;
  videoQuality: VideoQuality;
  downloadMode: DownloadMode;
  audioQuality: AudioQuality;
  remux: boolean;
  convertToMp4: boolean;
  clearMetadata: boolean;
  dontShowInHistory: boolean;
  useAria2: boolean;
  ignoreMixes: boolean;
  cookiesFromBrowser: string;
  sponsorBlock?: boolean;
  chapters?: boolean;
  embedSubtitles?: boolean;
  subtitleLanguages?: string;
  embedThumbnail?: boolean;
  outputTemplate?: string;
}

export type CloseBehavior = 'close' | 'minimize' | 'tray';
export type NotificationPosition =
  | 'bottom-right'
  | 'bottom-left'
  | 'top-right'
  | 'top-left'
  | 'bottom-center'
  | 'top-center';
export type ToastPosition =
  | 'bottom-right'
  | 'bottom-left'
  | 'top-right'
  | 'top-left'
  | 'bottom-center'
  | 'top-center';
export type NotificationMonitor = 'primary' | 'cursor';
/**
 * Window effect types by platform:
 * - Windows 10/11: acrylic, blur
 * - Windows 11: mica, mica-dark, mica-light, tabbed, tabbed-dark, tabbed-light
 * - macOS: vibrancy-* variants
 * - All: solid, oled, animated, image (CSS-based)
 */
export type BackgroundType =
  | 'solid'
  | 'oled'
  | 'animated'
  | 'image'
  | 'acrylic'
  | 'blur'
  | 'mica'
  | 'mica-dark'
  | 'mica-light'
  | 'tabbed'
  | 'tabbed-dark'
  | 'tabbed-light'
  | 'vibrancy-titlebar'
  | 'vibrancy-selection'
  | 'vibrancy-menu'
  | 'vibrancy-popover'
  | 'vibrancy-sidebar'
  | 'vibrancy-header'
  | 'vibrancy-sheet'
  | 'vibrancy-window'
  | 'vibrancy-hud'
  | 'vibrancy-fullscreen'
  | 'vibrancy-tooltip'
  | 'vibrancy-content'
  | 'vibrancy-under-window'
  | 'vibrancy-under-page';
export type AccentStyle = 'solid' | 'gradient' | 'glow';
export type ProxyMode = 'none' | 'system' | 'custom';

/**
 * yt-dlp Advanced Settings
 * Controls extraction, download, aria2, and post-processing behavior
 */
export interface YtDlpAdvancedSettings {
  // Master toggle for advanced settings visibility
  advancedMode: boolean;

  // Extraction settings
  extractionPlayerSkipWebpage: boolean;
  extractionPlayerSkipConfigs: boolean;
  extractionFlatPlaylist: boolean;
  extractionNoPlaylist: boolean;
  extractionCustomArgs: string;

  // Download settings
  downloadNoPlaylist: boolean;
  downloadConcurrentFragments: number;
  downloadRetries: number;
  downloadFragmentRetries: number;
  downloadCustomArgs: string;

  // aria2 override settings (when useAria2 is enabled)
  aria2OverrideGlobal: boolean; // false = use global aria2 settings, true = use yt-dlp specific
  aria2YtdlpConnections: number;
  aria2YtdlpSplits: number;
  aria2YtdlpMinSplitSize: string;
  aria2YtdlpDisableIPv6: boolean;
  aria2YtdlpCustomArgs: string;
  aria2FallbackToNative: boolean;

  // Post-processing settings
  postProcessCustomArgs: string;
  postProcessKeepOriginal: boolean;
  postProcessEmbedInfoJson: boolean;

  // Output settings
  outputTemplate: string;
  outputRestrictFilenames: boolean;
  outputWindowsFilenames: boolean;
}

export const defaultYtDlpAdvanced: YtDlpAdvancedSettings = {
  advancedMode: false,

  extractionPlayerSkipWebpage: true,
  extractionPlayerSkipConfigs: true,
  extractionFlatPlaylist: true,
  extractionNoPlaylist: true,
  extractionCustomArgs: '',

  downloadNoPlaylist: true,
  downloadConcurrentFragments: 1,
  downloadRetries: 10,
  downloadFragmentRetries: 10,
  downloadCustomArgs: '',

  aria2OverrideGlobal: false,
  aria2YtdlpConnections: 8,
  aria2YtdlpSplits: 8,
  aria2YtdlpMinSplitSize: '1M',
  aria2YtdlpDisableIPv6: true,
  aria2YtdlpCustomArgs: '',
  aria2FallbackToNative: true,

  postProcessCustomArgs: '',
  postProcessKeepOriginal: false,
  postProcessEmbedInfoJson: false,

  outputTemplate: '%(uploader)s - %(title)s.%(ext)s',
  outputRestrictFilenames: false,
  outputWindowsFilenames: true,
};

export interface AppSettings {
  onboardingCompleted: boolean;
  onboardingVersion: number;

  language: string;
  startOnBoot: boolean;
  startMinimized: boolean;
  watchClipboard: boolean;
  clipboardPatterns: string[];
  statusPopup: boolean;

  extensionServerEnabled: boolean;
  extensionLocalPort: number;

  notificationsEnabled: boolean;
  notificationPosition: NotificationPosition;
  notificationMonitor: NotificationMonitor;
  compactNotifications: boolean;
  notificationFancyBackground: boolean;
  notificationThumbnailTheming: boolean;
  notificationOffset: number;
  notificationCornerDismiss: boolean;
  notificationDuration: number;
  notificationShowProgress: boolean;

  toastPosition: ToastPosition;

  closeBehavior: CloseBehavior;


  autoUpdate: boolean;
  allowPreReleases: boolean;
  sendStats: boolean;
  acrylicBackground: boolean;
  disableAnimations: boolean;

  backgroundType: BackgroundType;
  backgroundColor: string;
  backgroundImage: string;
  backgroundVideo: string;
  backgroundBlur: number;
  backgroundOpacity: number;
  windowTint: number;

  accentColor: string;
  accentStyle: AccentStyle;
  useSystemAccent: boolean;

  sizeUnit: 'binary' | 'decimal';
  showHistoryStats: boolean;

  downloadPath: string;
  useAudioPath: boolean;
  audioPath: string;
  usePlaylistFolders: boolean;
  youtubeMusicAudioOnly: boolean;
  embedThumbnail: boolean;
  concurrentDownloads: number;

  convertToMp4: boolean;
  remux: boolean;

  defaultVideoQuality: VideoQuality;
  defaultDownloadMode: DownloadMode;
  defaultAudioQuality: AudioQuality;
  selectedPreset: string;
  clearMetadata: boolean;
  dontShowInHistory: boolean;
  useAria2: boolean;
  ignoreMixes: boolean;
  cookiesFromBrowser: string;
  customCookies: string;

  // Tracks cookie updates pushed from the browser extension (for visibility/debugging).
  extensionCookiesReceived: Array<{
    domain: string;
    sourceUrl?: string | null;
    count: number;
    receivedAt: number;
  }>;
  sponsorBlock: boolean;
  sponsorBlockSkipSponsors: boolean;
  sponsorBlockSkipIntros: boolean;
  sponsorBlockSkipSelfPromo: boolean;
  sponsorBlockSkipInteraction: boolean;
  chapters: boolean;
  embedSubtitles: boolean;
  subtitleLanguages: string;

  defaultProcessor: DefaultProcessor;
  youtubePlayerClient: string;
  usePlayerClientForExtraction: boolean;
  extractionPlayerClient: string;

  thumbnailTheming: boolean;
  builderThumbnailGlow: boolean;

  proxyMode: ProxyMode;
  customProxyUrl: string;
  retryWithoutProxy: boolean;
  bypassProxyForDownloads: boolean;

  dismissedUpdateVersion: string;

  aria2Connections: number;
  aria2Splits: number;
  aria2MinSplitSize: string;
  aria2DisableIPv6: boolean;
  aria2CustomArgs: string;

  downloadSpeedLimit: number;

  watchClipboardForFiles: boolean;
  fileDownloadNotifications: boolean;

  downloadsViewMode: 'list' | 'grid';
  downloadsSortType: 'date' | 'name' | 'size' | 'duration' | 'format';
  downloadsSortDirection: 'asc' | 'desc';
  historyViewMode: 'list' | 'grid';
  gridItemSize: number;

  showMobileNavLabels: boolean;

  customPresets: CustomPreset[];

  // yt-dlp Advanced Settings
  ytdlpAdvanced: YtDlpAdvancedSettings;
}

export const defaultSettings: AppSettings = {
  onboardingCompleted: false,
  onboardingVersion: 1,

  language: 'en',
  startOnBoot: false,
  startMinimized: true,
  watchClipboard: true,
  clipboardPatterns: [
    'youtube.com',
    'youtu.be',
    'vimeo.com',
    'dailymotion.com',
    'twitter.com',
    'x.com',
    'instagram.com',
    'tiktok.com',
    'reddit.com',
    'twitch.tv',
    'soundcloud.com',
    'spotify.com',
    'bilibili.com',
    'b23.tv',
    'douyin.com',
    'iqiyi.com',
    'youku.com',
    'qq.com',
    'mgtv.com',
    'le.com',
    'weibo.com',
    'kuaishou.com',
    'xiaohongshu.com',
    'xhslink.com',
    'huya.com',
    'douyu.com',
    'acfun.cn',
  ],
  statusPopup: false,

  extensionServerEnabled: true,
  extensionLocalPort: 9549,

  notificationsEnabled: true,
  notificationPosition: 'bottom-right',
  notificationMonitor: 'primary',
  compactNotifications: false,
  notificationFancyBackground: false,
  notificationThumbnailTheming: true,
  notificationOffset: 48,
  notificationCornerDismiss: false,
  notificationDuration: 12,
  notificationShowProgress: true,

  toastPosition: 'bottom-right',

  closeBehavior: 'tray',


  autoUpdate: true,
  allowPreReleases: false,
  sendStats: true,
  acrylicBackground: true,
  disableAnimations: false,

  backgroundType: 'acrylic',
  backgroundColor: '#1a1a2e',
  backgroundImage: '',
  backgroundVideo: 'https://nichind.dev/assets/video/atri.mp4',
  backgroundBlur: 20,
  backgroundOpacity: 100,
  windowTint: 32,

  accentColor: '#6366F1',
  accentStyle: 'solid',
  useSystemAccent: false,

  sizeUnit: 'binary',
  showHistoryStats: false,

  downloadPath: '',
  useAudioPath: false,
  audioPath: '',
  usePlaylistFolders: true,
  youtubeMusicAudioOnly: true,
  embedThumbnail: true,
  concurrentDownloads: 2,
  convertToMp4: false,
  remux: true,

  defaultVideoQuality: 'max',
  defaultDownloadMode: 'auto',
  defaultAudioQuality: 'best',
  selectedPreset: 'custom',
  clearMetadata: false,
  dontShowInHistory: false,
  useAria2: true,
  ignoreMixes: true,
  cookiesFromBrowser: '',
  customCookies: '',
  extensionCookiesReceived: [],
  sponsorBlock: false,
  sponsorBlockSkipSponsors: true,
  sponsorBlockSkipIntros: false,
  sponsorBlockSkipSelfPromo: false,
  sponsorBlockSkipInteraction: false,
  chapters: true,
  embedSubtitles: false,
  subtitleLanguages: 'en,ru',

  defaultProcessor: 'auto',
  youtubePlayerClient: 'android_sdkless',
  usePlayerClientForExtraction: false,
  extractionPlayerClient: 'android_sdkless',

  thumbnailTheming: true,
  builderThumbnailGlow: true,

  proxyMode: 'system',
  customProxyUrl: '',
  retryWithoutProxy: true,
  bypassProxyForDownloads: true,

  dismissedUpdateVersion: '',

  aria2Connections: 8,
  aria2Splits: 8,
  aria2MinSplitSize: '1M',
  aria2DisableIPv6: true,
  aria2CustomArgs: '',

  downloadSpeedLimit: 0,

  watchClipboardForFiles: true,
  fileDownloadNotifications: true,

  downloadsViewMode: 'list',
  downloadsSortType: 'date',
  downloadsSortDirection: 'desc',
  historyViewMode: 'list',
  gridItemSize: 200,

  showMobileNavLabels: true,

  customPresets: [],

  ytdlpAdvanced: defaultYtDlpAdvanced,
};

let store: Store | null = null;

export const settings = writable<AppSettings>(defaultSettings);

export const settingsReady = writable(false);

export async function initSettings(): Promise<void> {
  try {
    store = await load('settings.json', {
      autoSave: true,
      defaults: defaultSettings as unknown as Record<string, unknown>,
    });

    const keys = Object.keys(defaultSettings) as (keyof AppSettings)[];
    const values = await Promise.all(keys.map((key) => store!.get(key)));

    const loaded: AppSettings = { ...defaultSettings };
    keys.forEach((key, index) => {
      const value = values[index];
      if (value !== null && value !== undefined) {
        (loaded as unknown as Record<string, unknown>)[key] = value;
      }
    });

    if (isAndroid()) {
      if (
        values[keys.indexOf('toastPosition')] === null ||
        values[keys.indexOf('toastPosition')] === undefined
      ) {
        loaded.toastPosition = 'top-right';
      }
      if (
        values[keys.indexOf('backgroundType')] === null ||
        values[keys.indexOf('backgroundType')] === undefined
      ) {
        loaded.backgroundType = 'animated';
      }
      if (
        values[keys.indexOf('backgroundBlur')] === null ||
        values[keys.indexOf('backgroundBlur')] === undefined
      ) {
        loaded.backgroundBlur = 14;
      }
      if (
        values[keys.indexOf('useSystemAccent')] === null ||
        values[keys.indexOf('useSystemAccent')] === undefined
      ) {
        loaded.useSystemAccent = true;
      }
    } else if (typeof navigator !== 'undefined') {
      const backgroundTypeIdx = keys.indexOf('backgroundType');
      if (values[backgroundTypeIdx] === null || values[backgroundTypeIdx] === undefined) {
        const userAgent = navigator.userAgent.toLowerCase();
        if (userAgent.includes('mac')) {
          loaded.backgroundType = 'vibrancy-sidebar';
        } else if (userAgent.includes('linux')) {
          loaded.backgroundType = loaded.backgroundVideo ? 'animated' : 'solid';
        }
      }
    }

    loaded.ytdlpAdvanced = {
      ...defaultYtDlpAdvanced,
      ...(loaded.ytdlpAdvanced ?? {}),
    };

    settings.set(loaded);
    settingsReady.set(true);
  } catch (error) {
    logs.error('settings', `Failed to load settings: ${error}`);
    settingsReady.set(true);
  }
}

export async function updateSetting<K extends keyof AppSettings>(
  key: K,
  value: AppSettings[K]
): Promise<void> {
  if (!store) {
    logs.warn('settings', 'Store not initialized');
    return;
  }

  try {
    await store.set(key, value);
    await store.save();

    settings.update((s) => ({ ...s, [key]: value }));
  } catch (error) {
    logs.error('settings', `Failed to update ${String(key)}: ${error}`);
  }
}

export async function updateSettings(updates: Partial<AppSettings>): Promise<void> {
  if (!store) {
    logs.warn('settings', 'Store not initialized');
    return;
  }

  try {
    await Promise.all(Object.entries(updates).map(([key, value]) => store!.set(key, value)));
    await store.save();

    settings.update((s) => ({ ...s, ...updates }));
  } catch (error) {
    logs.error('settings', `Failed to update settings: ${error}`);
  }
}

export async function resetSettings(): Promise<void> {
  if (!store) {
    logs.warn('settings', 'Store not initialized');
    return;
  }

  try {
    await store.clear();
    await Promise.all(
      Object.entries(defaultSettings).map(([key, value]) => store!.set(key, value))
    );
    await store.save();

    settings.set(defaultSettings);
  } catch (error) {
    logs.error('settings', `Failed to reset settings: ${error}`);
  }
}

export function getSettings(): AppSettings {
  return get(settings);
}

export interface ProxyConfig {
  mode: ProxyMode;
  customUrl: string;
  retryWithoutProxy: boolean;
}

export function getProxyConfig(): ProxyConfig {
  const s = getSettings();
  return {
    mode: s.proxyMode,
    customUrl: s.customProxyUrl,
    retryWithoutProxy: s.retryWithoutProxy,
  };
}
