import { writable, get } from 'svelte/store';
import { load, type Store } from '@tauri-apps/plugin-store';
import { invoke } from '@tauri-apps/api/core';
import { logs } from './logs';
import { isAndroid } from '$lib/utils/android';

export type VideoQuality = 'max' | '4k' | '1440p' | '1080p' | '720p' | '480p' | '360p' | '240p';
export type DownloadMode = 'auto' | 'audio' | 'mute';
export type AudioQuality = 'best' | '320' | '256' | '192' | '128' | '96';
type DefaultProcessor = 'auto' | 'yt-dlp' | 'lux';
type PreferredVideoCodec = 'any' | 'h264' | 'h265' | 'vp9' | 'av1';
type PreferredAudioCodec = 'any' | 'opus' | 'aac' | 'mp3' | 'vorbis';
type AudioFormat = 'any' | 'mp3' | 'm4a' | 'opus' | 'wav' | 'flac';

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
type AccentStyle = 'solid' | 'gradient' | 'glow';
export type SurfaceStyle = 'glass' | 'frosted' | 'elevated' | 'accent' | 'contrast' | 'custom';
export type ShadowIntensity = 'none' | 'subtle' | 'medium' | 'strong';

export interface SurfaceSettings {
  opacity: number;
  borderOpacity: number;
  shadowIntensity: ShadowIntensity;
  accentTint: number;
}

export type WindowControlsStyle = 'auto' | 'windows' | 'macos';
export type BorderRadiusPreset = 'none' | 'subtle' | 'rounded' | 'pill' | 'custom';
export type TextScalePreset = 'compact' | 'default' | 'large' | 'custom';

type ProxyMode = 'none' | 'system' | 'custom';

export interface YtDlpAdvancedSettings {
  advancedMode: boolean;

  extractionPlayerSkipWebpage: boolean;
  extractionPlayerSkipConfigs: boolean;
  extractionFlatPlaylist: boolean;
  extractionNoPlaylist: boolean;
  extractionCustomArgs: string;

  downloadNoPlaylist: boolean;
  downloadConcurrentFragments: number;
  downloadRetries: number;
  downloadFragmentRetries: number;
  downloadCustomArgs: string;

  aria2OverrideGlobal: boolean;
  aria2YtdlpConnections: number;
  aria2YtdlpSplits: number;
  aria2YtdlpMinSplitSize: string;
  aria2YtdlpDisableIPv6: boolean;
  aria2YtdlpCustomArgs: string;
  aria2FallbackToNative: boolean;

  postProcessCustomArgs: string;
  postProcessKeepOriginal: boolean;
  postProcessEmbedInfoJson: boolean;

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

export type FFmpegHwAccel = 'auto' | 'none' | 'nvenc' | 'qsv' | 'amf' | 'videotoolbox';

export interface FFmpegSettings {
  hwAccel: FFmpegHwAccel;
}

const defaultFFmpegSettings: FFmpegSettings = {
  hwAccel: 'auto',
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
  extensionServerToken: string;

  notificationsEnabled: boolean;
  notificationPosition: NotificationPosition;
  notificationMonitor: NotificationMonitor;
  compactNotifications: boolean;
  notificationFancyBackground: boolean;
  notificationThumbnailTheming: boolean;
  notificationOffset: number;
  notificationCornerDismiss: boolean;
  notificationDuration: number;
  notificationCompletionTimeout: number;
  notificationShowProgress: boolean;

  toastPosition: ToastPosition;

  closeBehavior: CloseBehavior;

  autoUpdate: boolean;
  allowPreReleases: boolean;
  checkDepUpdates: boolean;
  autoUpdateDeps: boolean;
  sendStats: boolean;
  acrylicBackground: boolean;
  disableAnimations: boolean;

  windowControlsStyle: WindowControlsStyle;

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

  surfaceStyle: SurfaceStyle;
  surfaceCustom: SurfaceSettings;

  borderRadius: BorderRadiusPreset;
  borderRadiusCustom: number;

  textScale: TextScalePreset;
  textScaleCustom: number;
  useAlternativeSelector: boolean;

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
  preferredVideoCodec: PreferredVideoCodec;
  preferredAudioCodec: PreferredAudioCodec;
  audioFormat: AudioFormat;
  selectedPreset: string;
  clearMetadata: boolean;
  dontShowInHistory: boolean;
  useAria2: boolean;
  ignoreMixes: boolean;
  cookiesFromBrowser: string;
  customCookies: string;

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
  compactSidebar: boolean;

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
  listItemSize: number;
  downloadsVisibleColumns: ('format' | 'size' | 'duration')[];
  hideMissingFiles: boolean;
  showSourceTags: boolean;
  downloadsUngroupPlaylistsOnSort: boolean;

  showMobileNavLabels: boolean;

  navigationStyle: 'auto' | 'navbar' | 'sidebar';

  discordRpc: boolean;

  customPresets: CustomPreset[];

  ffmpeg: FFmpegSettings;

  ytdlpAdvanced: YtDlpAdvancedSettings;
}

function generateExtensionServerToken(bytesLength = 32): string {
  const bytes = new Uint8Array(bytesLength);
  const cryptoObj = (globalThis as unknown as { crypto?: Crypto }).crypto;
  if (cryptoObj?.getRandomValues) {
    cryptoObj.getRandomValues(bytes);
  } else {
    for (let i = 0; i < bytes.length; i++) {
      bytes[i] = Math.floor(Math.random() * 256);
    }
  }
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
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
  extensionServerToken: '',

  notificationsEnabled: true,
  notificationPosition: 'top-center',
  notificationMonitor: 'primary',
  compactNotifications: false,
  notificationFancyBackground: false,
  notificationThumbnailTheming: true,
  notificationOffset: 24,
  notificationCornerDismiss: false,
  notificationDuration: 12,
  notificationCompletionTimeout: 0,
  notificationShowProgress: true,

  toastPosition: 'bottom-right',

  closeBehavior: 'tray',

  autoUpdate: true,
  allowPreReleases: false,
  checkDepUpdates: true,
  autoUpdateDeps: false,
  sendStats: true,
  acrylicBackground: true,
  disableAnimations: false,

  windowControlsStyle: 'auto',

  backgroundType: 'acrylic',
  backgroundColor: '#1a1a2e',
  backgroundImage: '',
  backgroundVideo: 'https://nichind.dev/assets/video/atri.mp4',
  backgroundBlur: 20,
  backgroundOpacity: 100,
  windowTint: 48,

  accentColor: '#0EA5E9',
  accentStyle: 'gradient',
  useSystemAccent: false,

  surfaceStyle: 'glass',
  surfaceCustom: {
    opacity: 80,
    borderOpacity: 15,
    shadowIntensity: 'medium',
    accentTint: 5,
  },

  borderRadius: 'rounded',
  borderRadiusCustom: 10,
  textScale: 'default',
  textScaleCustom: 1.0,
  useAlternativeSelector: false,

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
  preferredVideoCodec: 'any',
  preferredAudioCodec: 'aac',
  audioFormat: 'any',
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
  youtubePlayerClient: 'default,-android_sdkless',
  usePlayerClientForExtraction: false,
  extractionPlayerClient: 'default,-android_sdkless',

  thumbnailTheming: true,
  builderThumbnailGlow: true,
  compactSidebar: true,

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
  listItemSize: 56,
  downloadsVisibleColumns: ['format', 'size', 'duration'],
  hideMissingFiles: false,
  showSourceTags: true,
  downloadsUngroupPlaylistsOnSort: false,

  showMobileNavLabels: true,

  navigationStyle: 'auto',

  discordRpc: false,

  customPresets: [],

  ffmpeg: defaultFFmpegSettings,

  ytdlpAdvanced: defaultYtDlpAdvanced,
};

let store: Store | null = null;

export const settings = writable<AppSettings>(defaultSettings);

export const settingsReady = writable(false);

const USER_MODIFIED_STORE_KEY = '_userModifiedKeys';

let userModifiedKeys = new Set<string>();

export const userModifiedKeysStore = writable<ReadonlySet<string>>(new Set());

async function persistUserModifiedKeys(): Promise<void> {
  if (!store) return;
  try {
    await store.set(USER_MODIFIED_STORE_KEY, [...userModifiedKeys]);
    await store.save();
  } catch (e) {
    logs.error('settings', `Failed to persist user-modified keys: ${e}`);
  }
}

function markKeyModified(key: string): void {
  userModifiedKeys.add(key);
  userModifiedKeysStore.set(userModifiedKeys);
}

function unmarkKeyModified(key: string): void {
  userModifiedKeys.delete(key);
  userModifiedKeysStore.set(userModifiedKeys);
}

function deepEquals(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a == null || b == null) return a === b;
  if (typeof a !== typeof b) return false;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    return a.every((v, i) => deepEquals(v, b[i]));
  }
  if (typeof a === 'object' && typeof b === 'object') {
    const keysA = Object.keys(a as Record<string, unknown>);
    const keysB = Object.keys(b as Record<string, unknown>);
    if (keysA.length !== keysB.length) return false;
    return keysA.every((k) =>
      deepEquals((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k])
    );
  }
  return false;
}

/**
 * Bootstrap user-modified-keys on first run after update.
 * Compares each stored value against the hardcoded default;
 * any value that differs is marked as user-modified.
 */
function migrateUserModifiedKeys(loaded: AppSettings): Set<string> {
  const modified = new Set<string>();
  for (const key of Object.keys(defaultSettings) as (keyof AppSettings)[]) {
    if (key === 'extensionServerToken') continue;
    if (key === 'onboardingCompleted' || key === 'onboardingVersion') continue;
    if (key === 'dismissedUpdateVersion') continue;
    if (key === 'extensionCookiesReceived') continue;
    if (key === 'customPresets') continue;

    const loadedVal = loaded[key];
    const defaultVal = defaultSettings[key];

    if (!deepEquals(loadedVal, defaultVal)) {
      // For nested objects, check individual sub-keys
      if (
        typeof loadedVal === 'object' &&
        loadedVal !== null &&
        !Array.isArray(loadedVal) &&
        typeof defaultVal === 'object' &&
        defaultVal !== null &&
        !Array.isArray(defaultVal)
      ) {
        for (const subKey of Object.keys(defaultVal as unknown as Record<string, unknown>)) {
          const subLoaded = (loadedVal as unknown as Record<string, unknown>)[subKey];
          const subDefault = (defaultVal as unknown as Record<string, unknown>)[subKey];
          if (!deepEquals(subLoaded, subDefault)) {
            modified.add(`${key}.${subKey}`);
          }
        }
      } else {
        modified.add(key);
      }
    }
  }
  return modified;
}

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

    // Ensure the local extension server has a stable shared-secret token.
    const tokenIdx = keys.indexOf('extensionServerToken');
    const rawToken = tokenIdx >= 0 ? values[tokenIdx] : null;
    if (!rawToken || typeof rawToken !== 'string' || !rawToken.trim()) {
      loaded.extensionServerToken = generateExtensionServerToken();
      await store!.set('extensionServerToken', loaded.extensionServerToken);
      await store!.save();
    }

    if (isAndroid()) {
      const extensionEnabledIdx = keys.indexOf('extensionServerEnabled');
      if (
        extensionEnabledIdx >= 0 &&
        (values[extensionEnabledIdx] === null || values[extensionEnabledIdx] === undefined)
      ) {
        loaded.extensionServerEnabled = false;
      }

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
          loaded.backgroundType = 'solid';
        }
      }
    }

    loaded.ytdlpAdvanced = {
      ...defaultYtDlpAdvanced,
      ...(loaded.ytdlpAdvanced ?? {}),
    };

    if (loaded.youtubePlayerClient === 'android_sdkless') {
      loaded.youtubePlayerClient = 'default,-android_sdkless';
      await store!.set('youtubePlayerClient', loaded.youtubePlayerClient);
    }
    if (loaded.extractionPlayerClient === 'android_sdkless') {
      loaded.extractionPlayerClient = 'default,-android_sdkless';
      await store!.set('extractionPlayerClient', loaded.extractionPlayerClient);
    }

    const storedModifiedKeys = await store!.get<string[]>(USER_MODIFIED_STORE_KEY);
    if (storedModifiedKeys && Array.isArray(storedModifiedKeys)) {
      userModifiedKeys = new Set(storedModifiedKeys);
    } else {
      userModifiedKeys = migrateUserModifiedKeys(loaded);
      await persistUserModifiedKeys();
      logs.info(
        'settings',
        `Migrated user-modified keys: ${userModifiedKeys.size} keys marked as user-modified`
      );
    }
    userModifiedKeysStore.set(userModifiedKeys);

    const { applyRemoteDefaultsToLoaded } = await import('$lib/composables/remoteSync');
    const applied = applyRemoteDefaultsToLoaded(
      loaded as unknown as Record<string, unknown>,
      userModifiedKeys
    );
    if (applied.size > 0) {
      logs.debug(
        'settings',
        `Applied ${applied.size} remote defaults on init: ${[...applied].join(', ')}`
      );
    }

    settings.set(loaded);
    settingsReady.set(true);

    syncDownloadSettings();
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

    markKeyModified(String(key));
    persistUserModifiedKeys();

    if (isDownloadSetting(String(key))) {
      syncDownloadSettings();
    }
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

    for (const key of Object.keys(updates)) {
      markKeyModified(key);
    }
    persistUserModifiedKeys();

    if (Object.keys(updates).some(isDownloadSetting)) {
      syncDownloadSettings();
    }
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

    userModifiedKeys = new Set();
    userModifiedKeysStore.set(userModifiedKeys);
    await persistUserModifiedKeys();

    settings.set(defaultSettings);
    syncDownloadSettings();
  } catch (error) {
    logs.error('settings', `Failed to reset settings: ${error}`);
  }
}

export async function updateSettingDotNotation(
  parentKey: string,
  parentValue: unknown,
  dotKey: string
): Promise<void> {
  if (!store) return;
  try {
    await store.set(parentKey, parentValue);
    await store.save();
    markKeyModified(dotKey);
    persistUserModifiedKeys();
    if (isDownloadSetting(parentKey) || isDownloadSetting(dotKey)) {
      syncDownloadSettings();
    }
  } catch (error) {
    logs.error('settings', `Failed to update ${dotKey}: ${error}`);
  }
}

export async function resetSetting(key: string): Promise<void> {
  if (!store) return;

  try {
    const { getEffectiveDefault } = await import('$lib/composables/remoteSync');
    const effectiveDefault = getEffectiveDefault(key);

    if (key.includes('.')) {
      const [parent, child] = key.split('.');
      settings.update((s) => {
        const parentObj = s[parent as keyof AppSettings];
        if (typeof parentObj === 'object' && parentObj !== null) {
          return { ...s, [parent]: { ...parentObj, [child]: effectiveDefault } };
        }
        return s;
      });
      const current = getSettings();
      await store.set(parent, current[parent as keyof AppSettings]);
    } else {
      settings.update((s) => ({ ...s, [key]: effectiveDefault }));
      await store.set(key, effectiveDefault);
    }
    await store.save();
    unmarkKeyModified(key);
    await persistUserModifiedKeys();

    if (isDownloadSetting(key)) {
      syncDownloadSettings();
    }
  } catch (error) {
    logs.error('settings', `Failed to reset setting ${key}: ${error}`);
  }
}

export async function applyRemoteDefaultsMidSession(): Promise<void> {
  if (!store) return;

  const current = getSettings();
  const loaded = JSON.parse(JSON.stringify(current)) as Record<string, unknown>;
  const { applyRemoteDefaultsToLoaded: applyRemote } = await import('$lib/composables/remoteSync');
  const applied = applyRemote(loaded, userModifiedKeys);

  if (applied.size === 0) return;

  const updates: Partial<AppSettings> = {};
  for (const key of applied) {
    if (key.includes('.')) {
      const [parent] = key.split('.');
      (updates as Record<string, unknown>)[parent] = (loaded as Record<string, unknown>)[parent];
    } else {
      (updates as Record<string, unknown>)[key] = (loaded as Record<string, unknown>)[key];
    }
  }

  await Promise.all(Object.entries(updates).map(([k, v]) => store!.set(k, v)));
  await store.save();

  settings.update((s) => ({ ...s, ...updates }));

  if ([...applied].some(isDownloadSetting)) {
    syncDownloadSettings();
  }

  logs.debug(
    'settings',
    `Applied ${applied.size} remote defaults mid-session: ${[...applied].join(', ')}`
  );
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

const DOWNLOAD_SETTING_KEYS: ReadonlySet<string> = new Set([
  'downloadPath',
  'audioPath',
  'useAudioPath',
  'defaultDownloadMode',
  'defaultVideoQuality',
  'defaultAudioQuality',
  'preferredVideoCodec',
  'preferredAudioCodec',
  'audioFormat',
  'embedThumbnail',
  'clearMetadata',
  'embedSubtitles',
  'subtitleLanguages',
  'sponsorBlock',
  'sponsorBlockSkipSponsors',
  'sponsorBlockSkipIntros',
  'sponsorBlockSkipSelfPromo',
  'sponsorBlockSkipInteraction',
  'cookiesFromBrowser',
  'customCookies',
  'proxyMode',
  'customProxyUrl',
  'downloadSpeedLimit',
  'useAria2',
  'aria2Connections',
  'aria2Splits',
  'aria2MinSplitSize',
  'aria2DisableIPv6',
  'aria2CustomArgs',
  'youtubePlayerClient',
  'concurrentDownloads',
]);

function buildDownloadSettings(s: AppSettings) {
  return {
    downloadPath: s.downloadPath || '',
    audioPath: s.audioPath || '',
    useAudioPath: s.useAudioPath ?? false,
    defaultDownloadMode: s.defaultDownloadMode || 'auto',
    defaultVideoQuality: s.defaultVideoQuality || 'max',
    defaultAudioQuality: s.defaultAudioQuality || 'best',
    preferredVideoCodec: s.preferredVideoCodec || 'any',
    preferredAudioCodec: s.preferredAudioCodec || 'any',
    audioFormat: s.audioFormat || 'any',
    embedThumbnail: s.embedThumbnail ?? true,
    embedMetadata: !(s.clearMetadata ?? false),
    embedSubtitles: s.embedSubtitles ?? false,
    subtitleLanguages: s.subtitleLanguages || '',
    sponsorBlock: s.sponsorBlock ?? false,
    sponsorBlockSkipSponsors: s.sponsorBlockSkipSponsors ?? true,
    sponsorBlockSkipIntros: s.sponsorBlockSkipIntros ?? false,
    sponsorBlockSkipSelfPromo: s.sponsorBlockSkipSelfPromo ?? false,
    sponsorBlockSkipInteraction: s.sponsorBlockSkipInteraction ?? false,
    cookiesFromBrowser: s.cookiesFromBrowser || '',
    customCookies: s.customCookies || '',
    proxyMode: s.proxyMode || 'none',
    customProxyUrl: s.customProxyUrl || '',
    // Convert from MB/s (UI) to bytes/sec (backend)
    downloadSpeedLimit: s.downloadSpeedLimit ? s.downloadSpeedLimit * 1024 * 1024 : 0,
    useAria2: s.useAria2 ?? false,
    aria2Connections: s.aria2Connections ?? 8,
    aria2Splits: s.aria2Splits ?? 8,
    aria2MinSplitSize: s.aria2MinSplitSize || '1M',
    aria2DisableIpv6: s.aria2DisableIPv6 ?? false,
    aria2CustomArgs: s.aria2CustomArgs || '',
    youtubePlayerClient: s.youtubePlayerClient || '',
    concurrentDownloads: s.concurrentDownloads ?? 3,
  };
}

export function syncDownloadSettings(): void {
  const s = getSettings();
  invoke('sync_download_settings', { settings: buildDownloadSettings(s) }).catch(() => {});
}

export function isDownloadSetting(key: string): boolean {
  return DOWNLOAD_SETTING_KEYS.has(key);
}
