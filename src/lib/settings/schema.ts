import type { IconName } from '$lib/components/Icon.svelte';
import type { AppSettings } from '$lib/stores/settings';
import { invoke } from '@tauri-apps/api/core';
import { get } from 'svelte/store';
import { t, locales, setLocale, type Locale } from '$lib/i18n';

// Platform targeting
export type Platform = 'windows' | 'macos' | 'linux' | 'android';
export type PlatformGroup = 'desktop' | 'mobile';

const PLATFORM_GROUPS: Record<PlatformGroup, Platform[]> = {
  desktop: ['windows', 'macos', 'linux'],
  mobile: ['android'],
};

export function isVisibleOnPlatform(
  platforms: readonly (Platform | PlatformGroup)[] | undefined,
  currentPlatform: Platform
): boolean {
  if (!platforms) return true;
  return platforms.some((p) => {
    if (p in PLATFORM_GROUPS) {
      return PLATFORM_GROUPS[p as PlatformGroup].includes(currentPlatform);
    }
    return p === currentPlatform;
  });
}

export function getSettingValue(settings: AppSettings, key: string): unknown {
  if (key.includes('.')) {
    return key.split('.').reduce(
      (obj: Record<string, unknown> | undefined, k) => {
        if (obj && typeof obj === 'object')
          return (obj as Record<string, unknown>)[k] as Record<string, unknown> | undefined;
        return undefined;
      },
      settings as unknown as Record<string, unknown>
    );
  }
  return settings[key as keyof AppSettings];
}

interface BaseDef {
  key:
    | keyof AppSettings
    | `ytdlpAdvanced.${string}`
    | `ffmpeg.${string}`
    | `surfaceCustom.${string}`;
  section: string;
  subsection?: string;
  icon: IconName;
  titleKey: string;
  descriptionKey?: string;
  platforms?: (Platform | PlatformGroup)[]; // undefined = all platforms
  visible?: (settings: AppSettings) => boolean;
  disabled?: (settings: AppSettings) => boolean;
  keywords?: string[]; // Extra search terms (e.g. ['reset', 'clear'] for data items)
  onSet?: (value: unknown) => void | Promise<void>; // Side-effect handler
}

export type SettingDef =
  | (BaseDef & { type: 'toggle' })
  | (BaseDef & {
      type: 'select';
      options:
        | { value: string; label: string }[]
        | ((platform: Platform) => { value: string; label: string }[]);
      width?: number;
    })
  | (BaseDef & {
      type: 'slider';
      min: number;
      max: number;
      step?: number;
      suffix?: string;
      debounce?: number;
    })
  | (BaseDef & { type: 'input'; placeholder?: string; width?: number; debounce?: number })
  | (BaseDef & { type: 'color' })
  | (BaseDef & { type: 'path'; pickType: 'file' | 'folder' })
  | {
      type: 'action';
      key: string; // Unique ID, not an AppSettings key
      section: string;
      subsection?: string;
      platforms?: (Platform | PlatformGroup)[];
      icon: IconName;
      titleKey: string;
      descriptionKey?: string;
      buttonKey: string; // Translation key for button text
      action: () => void | Promise<void>;
      loading?: () => boolean; // Returns true while action is in progress
      keywords?: string[];
      visible?: (settings: AppSettings) => boolean;
      disabled?: (settings: AppSettings) => boolean;
    }
  | {
      type: 'custom';
      key: string;
      section: string;
      subsection?: string;
      platforms?: (Platform | PlatformGroup)[];
      keywords?: string[];
      titleKey: string;
      descriptionKey?: string;
      visible?: (settings: AppSettings) => boolean;
      disabled?: (settings: AppSettings) => boolean;
      icon?: IconName;
    };

// Initial empty arrays to be populated in migration
export const SECTIONS = [
  { id: 'general', titleKey: 'settings.general.title', icon: 'settings' },
  { id: 'downloads', titleKey: 'settings.downloads.title', icon: 'download' },
  { id: 'processing', titleKey: 'settings.processing.title', icon: 'server' },
  {
    id: 'notifications',
    titleKey: 'settings.notifications.title',
    icon: 'bell',
    platforms: ['desktop'],
  },
  { id: 'network', titleKey: 'settings.network.title', icon: 'globe' },
  {
    id: 'integration',
    titleKey: 'settings.integration.title',
    icon: 'extensions',
  },
  { id: 'app', titleKey: 'settings.app.title', icon: 'widgets' },
  { id: 'deps', titleKey: 'settings.deps.title', icon: 'package', platforms: ['desktop'] },
  { id: 'data', titleKey: 'settings.data.title', icon: 'folder' },
] as const;

export const SETTINGS: SettingDef[] = [
  {
    type: 'select',
    key: 'language',
    section: 'general',
    subsection: 'language',
    icon: 'globe',
    titleKey: 'settings.general.language',
    options: locales.map((l) => ({ value: l.code, label: l.nativeName })),
    onSet: (v) => {
      setLocale(v as Locale);
    },
  },
  {
    type: 'toggle',
    key: 'startOnBoot',
    section: 'general',
    subsection: 'startup',
    icon: 'run',
    titleKey: 'settings.general.startOnBoot',
    descriptionKey: 'settings.general.startOnBootDescription',
    platforms: ['desktop'],
    onSet: async (v) => {
      await invoke('set_auto_start', { enable: v });
    },
  },
  {
    type: 'toggle',
    key: 'startMinimized',
    section: 'general',
    subsection: 'startup',
    icon: 'minimize',
    titleKey: 'settings.general.startMinimized',
    descriptionKey: 'settings.general.startMinimizedDescription',
    platforms: ['desktop'],
    disabled: (s) => !s.startOnBoot,
  },
  {
    type: 'toggle',
    key: 'watchClipboard',
    section: 'general',
    subsection: 'clipboard',
    icon: 'clipboard',
    titleKey: 'settings.general.watchClipboard',
    descriptionKey: 'settings.general.watchClipboardTooltip',
    platforms: ['desktop'],
  },
  {
    type: 'toggle',
    key: 'statusPopup',
    section: 'general',
    subsection: 'clipboard',
    icon: 'monitor',
    titleKey: 'settings.general.statusPopup',
    platforms: ['desktop'],
  },
  {
    type: 'select',
    key: 'closeBehavior',
    section: 'general',
    subsection: 'closeBehavior',
    icon: 'close',
    titleKey: 'settings.general.closeBehavior',
    descriptionKey: 'settings.general.closeBehaviorDescription',
    platforms: ['desktop'],
    options: () => {
      const $t = get(t);
      return [
        { value: 'close', label: $t('settings.general.closeBehaviorClose') },
        { value: 'minimize', label: $t('settings.general.closeBehaviorMinimize') },
        { value: 'tray', label: $t('settings.general.closeBehaviorTray') },
      ];
    },
  },
  {
    type: 'path',
    key: 'downloadPath',
    section: 'downloads',
    subsection: 'paths',
    icon: 'folder',
    titleKey: 'settings.downloads.downloadPath',
    descriptionKey: 'settings.downloads.downloadPathDescription',
    pickType: 'folder',
  },
  {
    type: 'toggle',
    key: 'useAudioPath',
    section: 'downloads',
    subsection: 'paths',
    icon: 'music',
    titleKey: 'settings.downloads.useAudioPath',
    descriptionKey: 'settings.downloads.useAudioPathTooltip',
  },
  {
    type: 'path',
    key: 'audioPath',
    section: 'downloads',
    subsection: 'paths',
    icon: 'folder',
    titleKey: 'settings.downloads.audioPath',
    pickType: 'folder',
    visible: (s) => s.useAudioPath,
  },
  {
    type: 'toggle',
    key: 'usePlaylistFolders',
    section: 'downloads',
    subsection: 'folders',
    icon: 'playlist',
    titleKey: 'settings.downloads.usePlaylistFolders',
    descriptionKey: 'settings.downloads.usePlaylistFoldersTooltip',
  },
  {
    type: 'toggle',
    key: 'youtubeMusicAudioOnly',
    section: 'downloads',
    subsection: 'folders',
    icon: 'headphones',
    titleKey: 'settings.downloads.youtubeMusicAudioOnly',
    descriptionKey: 'settings.downloads.youtubeMusicAudioOnlyTooltip',
  },
  {
    type: 'toggle',
    key: 'embedThumbnail',
    section: 'downloads',
    subsection: 'folders',
    icon: 'image',
    titleKey: 'settings.downloads.embedThumbnail',
    descriptionKey: 'settings.downloads.embedThumbnailTooltip',
  },
  // Preferred codecs subsection
  {
    type: 'select',
    key: 'preferredVideoCodec',
    section: 'downloads',
    subsection: 'codecs',
    icon: 'video',
    titleKey: 'settings.downloads.preferredVideoCodec',
    descriptionKey: 'settings.downloads.preferredVideoCodecDescription',
    options: () => {
      const $t = get(t);
      return [
        { value: 'any', label: $t('settings.downloads.codecAny') },
        { value: 'h264', label: 'H.264 (AVC)' },
        { value: 'h265', label: 'H.265 (HEVC)' },
        { value: 'vp9', label: 'VP9' },
        { value: 'av1', label: 'AV1' },
      ];
    },
  },
  {
    type: 'select',
    key: 'preferredAudioCodec',
    section: 'downloads',
    subsection: 'codecs',
    icon: 'music',
    titleKey: 'settings.downloads.preferredAudioCodec',
    descriptionKey: 'settings.downloads.preferredAudioCodecDescription',
    options: () => {
      const $t = get(t);
      return [
        { value: 'any', label: $t('settings.downloads.codecAny') },
        { value: 'opus', label: 'Opus' },
        { value: 'aac', label: 'AAC' },
        { value: 'mp3', label: 'MP3' },
        { value: 'vorbis', label: 'Vorbis' },
      ];
    },
  },
  // Concurrency subsection
  {
    type: 'slider',
    key: 'concurrentDownloads',
    section: 'downloads',
    subsection: 'concurrency',
    icon: 'queue',
    titleKey: 'settings.downloads.concurrentDownloads',
    descriptionKey: 'settings.downloads.concurrentDownloadsDescription',
    min: 1,
    max: 5,
    step: 1,
    debounce: 300,
  },
  {
    type: 'slider',
    key: 'downloadSpeedLimit',
    section: 'downloads',
    subsection: 'concurrency',
    icon: 'tuning',
    titleKey: 'settings.downloads.downloadSpeedLimit',
    descriptionKey: 'settings.downloads.downloadSpeedLimitDescription',
    min: 0,
    max: 50,
    step: 1,
    suffix: ' MB/s',
    debounce: 300,
  },
  // Notifications subsection (in downloads)
  {
    type: 'toggle',
    key: 'watchClipboardForFiles',
    section: 'downloads',
    subsection: 'notifications',
    icon: 'clipboard',
    titleKey: 'settings.downloads.watchClipboardForFiles',
    descriptionKey: 'settings.downloads.watchClipboardForFilesTooltip',
    platforms: ['desktop'],
  },
  {
    type: 'toggle',
    key: 'fileDownloadNotifications',
    section: 'downloads',
    subsection: 'notifications',
    icon: 'bell',
    titleKey: 'settings.downloads.fileDownloadNotifications',
    descriptionKey: 'settings.downloads.fileDownloadNotificationsTooltip',
    platforms: ['desktop'],
  },
  // aria2 subsection
  {
    type: 'slider',
    key: 'aria2Connections',
    section: 'downloads',
    subsection: 'aria2',
    icon: 'link',
    titleKey: 'settings.downloads.aria2Connections',
    descriptionKey: 'settings.downloads.aria2ConnectionsDescription',
    min: 1,
    max: 16,
    step: 1,
    debounce: 300,
  },
  {
    type: 'slider',
    key: 'aria2Splits',
    section: 'downloads',
    subsection: 'aria2',
    icon: 'queue',
    titleKey: 'settings.downloads.aria2Splits',
    descriptionKey: 'settings.downloads.aria2SplitsDescription',
    min: 1,
    max: 16,
    step: 1,
    debounce: 300,
  },
  {
    type: 'toggle',
    key: 'aria2DisableIPv6',
    section: 'downloads',
    subsection: 'aria2',
    icon: 'globe',
    titleKey: 'settings.downloads.aria2DisableIPv6',
    descriptionKey: 'settings.downloads.aria2DisableIPv6Description',
  },
  {
    type: 'input',
    key: 'aria2CustomArgs',
    section: 'downloads',
    subsection: 'aria2',
    icon: 'code',
    titleKey: 'settings.downloads.aria2CustomArgs',
    descriptionKey: 'settings.downloads.aria2CustomArgsDescription',
    placeholder: '--max-tries=5',
    width: 250,
  },
  // Network subsection (in downloads)
  {
    type: 'toggle',
    key: 'bypassProxyForDownloads',
    section: 'downloads',
    subsection: 'network',
    icon: 'download',
    titleKey: 'settings.downloads.bypassProxyForDownloads',
    descriptionKey: 'settings.downloads.bypassProxyForDownloadsDescription',
    visible: (s) => s.proxyMode !== 'none',
  },

  {
    type: 'select',
    key: 'defaultProcessor',
    section: 'processing',
    subsection: 'backend',
    icon: 'server',
    titleKey: 'settings.processing.defaultProcessor',
    descriptionKey: 'settings.processing.defaultProcessorDescription',
    options: (p) => {
      const $t = get(t);
      const opts = [
        { value: 'auto', label: $t('settings.processing.auto') },
        { value: 'yt-dlp', label: 'yt-dlp' },
      ];
      if (p !== 'android') opts.push({ value: 'lux', label: 'Lux' });
      return opts;
    },
  },
  {
    type: 'input',
    key: 'youtubePlayerClient',
    section: 'processing',
    subsection: 'youtube',
    icon: 'video',
    titleKey: 'settings.processing.youtubePlayerClient',
    descriptionKey: 'settings.processing.youtubePlayerClientDescription',
    placeholder: 'default,-android_sdkless',
    width: 200,
  },
  {
    type: 'toggle',
    key: 'usePlayerClientForExtraction',
    section: 'processing',
    subsection: 'youtube',
    icon: 'link',
    titleKey: 'settings.processing.usePlayerClientForExtraction',
    descriptionKey: 'settings.processing.usePlayerClientForExtractionDescription',
  },
  {
    type: 'input',
    key: 'extractionPlayerClient',
    section: 'processing',
    subsection: 'youtube',
    icon: 'link',
    titleKey: 'settings.processing.extractionPlayerClient',
    descriptionKey: 'settings.processing.extractionPlayerClientDescription',
    placeholder: 'default,-android_sdkless',
    visible: (s) => !s.usePlayerClientForExtraction,
  },
  {
    type: 'toggle',
    key: 'ytdlpAdvanced.extractionFlatPlaylist',
    section: 'processing',
    subsection: 'extraction',
    icon: 'queue',
    titleKey: 'ytdlp.advanced.extraction.flatPlaylist',
    descriptionKey: 'ytdlp.advanced.extraction.flatPlaylistHint',
  },
  {
    type: 'toggle',
    key: 'ytdlpAdvanced.extractionNoPlaylist',
    section: 'processing',
    subsection: 'extraction',
    icon: 'video',
    titleKey: 'ytdlp.advanced.extraction.noPlaylist',
    descriptionKey: 'ytdlp.advanced.extraction.noPlaylistHint',
  },
  {
    type: 'toggle',
    key: 'ytdlpAdvanced.extractionPlayerSkipWebpage',
    section: 'processing',
    subsection: 'extraction',
    icon: 'graph',
    titleKey: 'ytdlp.advanced.extraction.skipWebpage',
    descriptionKey: 'ytdlp.advanced.extraction.skipWebpageHint',
  },
  {
    type: 'toggle',
    key: 'ytdlpAdvanced.extractionPlayerSkipConfigs',
    section: 'processing',
    subsection: 'extraction',
    icon: 'cog',
    titleKey: 'ytdlp.advanced.extraction.skipConfigs',
    descriptionKey: 'ytdlp.advanced.extraction.skipConfigsHint',
  },
  {
    type: 'input',
    key: 'ytdlpAdvanced.extractionCustomArgs',
    section: 'processing',
    subsection: 'extraction',
    icon: 'code',
    titleKey: 'ytdlp.advanced.extraction.customArgs',
    descriptionKey: 'ytdlp.advanced.extraction.customArgsHint',
    placeholder: '--geo-bypass --ignore-errors',
    width: 250,
  },

  {
    type: 'toggle',
    key: 'ytdlpAdvanced.downloadNoPlaylist',
    section: 'processing',
    subsection: 'download',
    icon: 'video',
    titleKey: 'ytdlp.advanced.download.noPlaylist',
    descriptionKey: 'ytdlp.advanced.download.noPlaylistHint',
  },
  {
    type: 'slider',
    key: 'ytdlpAdvanced.downloadConcurrentFragments',
    section: 'processing',
    subsection: 'download',
    icon: 'documents',
    titleKey: 'ytdlp.advanced.download.concurrentFragments',
    descriptionKey: 'ytdlp.advanced.download.concurrentFragmentsHint',
    min: 1,
    max: 8,
    step: 1,
    debounce: 150,
  },
  {
    type: 'slider',
    key: 'ytdlpAdvanced.downloadRetries',
    section: 'processing',
    subsection: 'download',
    icon: 'refresh',
    titleKey: 'ytdlp.advanced.download.retries',
    descriptionKey: 'ytdlp.advanced.download.retriesHint',
    min: 1,
    max: 50,
    step: 1,
    debounce: 150,
  },
  {
    type: 'slider',
    key: 'ytdlpAdvanced.downloadFragmentRetries',
    section: 'processing',
    subsection: 'download',
    icon: 'refresh',
    titleKey: 'ytdlp.advanced.download.fragmentRetries',
    descriptionKey: 'ytdlp.advanced.download.fragmentRetriesHint',
    min: 1,
    max: 50,
    step: 1,
    debounce: 150,
  },
  {
    type: 'input',
    key: 'ytdlpAdvanced.downloadCustomArgs',
    section: 'processing',
    subsection: 'download',
    icon: 'code',
    titleKey: 'ytdlp.advanced.download.customArgs',
    descriptionKey: 'ytdlp.advanced.download.customArgsHint',
    placeholder: '--buffer-size 32K',
    width: 250,
  },

  {
    type: 'input',
    key: 'ytdlpAdvanced.outputTemplate',
    section: 'processing',
    subsection: 'output',
    icon: 'file_text',
    titleKey: 'ytdlp.advanced.output.template',
    descriptionKey: 'ytdlp.advanced.output.templateHint',
    placeholder: '%(title)s.%(ext)s',
    width: 250,
  },
  {
    type: 'toggle',
    key: 'ytdlpAdvanced.outputRestrictFilenames',
    section: 'processing',
    subsection: 'output',
    icon: 'file_text',
    titleKey: 'ytdlp.advanced.output.restrictFilenames',
    descriptionKey: 'ytdlp.advanced.output.restrictFilenamesHint',
  },
  {
    type: 'toggle',
    key: 'ytdlpAdvanced.outputWindowsFilenames',
    section: 'processing',
    subsection: 'output',
    icon: 'file_text',
    titleKey: 'ytdlp.advanced.output.windowsFilenames',
    descriptionKey: 'ytdlp.advanced.output.windowsFilenamesHint',
  },

  {
    type: 'toggle',
    key: 'ytdlpAdvanced.postProcessKeepOriginal',
    section: 'processing',
    subsection: 'postProcess',
    icon: 'copy',
    titleKey: 'ytdlp.advanced.postProcess.keepOriginal',
    descriptionKey: 'ytdlp.advanced.postProcess.keepOriginalHint',
  },
  {
    type: 'toggle',
    key: 'ytdlpAdvanced.postProcessEmbedInfoJson',
    section: 'processing',
    subsection: 'postProcess',
    icon: 'code',
    titleKey: 'ytdlp.advanced.postProcess.embedInfoJson',
    descriptionKey: 'ytdlp.advanced.postProcess.embedInfoJsonHint',
  },
  {
    type: 'input',
    key: 'ytdlpAdvanced.postProcessCustomArgs',
    section: 'processing',
    subsection: 'postProcess',
    icon: 'code',
    titleKey: 'ytdlp.advanced.postProcess.customArgs',
    descriptionKey: 'ytdlp.advanced.postProcess.customArgsHint',
    placeholder: '--exec "echo {}"',
    width: 250,
  },

  // FFmpeg Settings
  {
    type: 'select',
    key: 'ffmpeg.hwAccel',
    section: 'processing',
    subsection: 'ffmpeg',
    icon: 'server',
    titleKey: 'settings.ffmpeg.hwAccel',
    descriptionKey: 'settings.ffmpeg.hwAccelDescription',
    platforms: ['desktop'],
    options: () => {
      const $t = get(t);
      return [
        { value: 'auto', label: $t('settings.ffmpeg.hwAccelAuto') },
        { value: 'none', label: $t('settings.ffmpeg.hwAccelNone') },
        { value: 'nvenc', label: 'NVIDIA NVENC' },
        { value: 'qsv', label: 'Intel Quick Sync' },
        { value: 'amf', label: 'AMD AMF' },
        { value: 'videotoolbox', label: 'VideoToolbox (macOS)' },
      ];
    },
    width: 200,
  },

  {
    type: 'toggle',
    key: 'notificationsEnabled',
    section: 'notifications',
    subsection: 'general',
    icon: 'bell',
    titleKey: 'settings.notifications.enabled',
    descriptionKey: 'settings.notifications.enabledTooltip',
    platforms: ['desktop'],
  },
  {
    type: 'toggle',
    key: 'notificationShowProgress',
    section: 'notifications',
    subsection: 'general',
    icon: 'download',
    titleKey: 'settings.notifications.showProgress',
    descriptionKey: 'settings.notifications.showProgressTooltip',
    platforms: ['desktop'],
  },
  {
    type: 'select',
    key: 'notificationPosition',
    section: 'notifications',
    subsection: 'layout',
    icon: 'widgets',
    titleKey: 'settings.notifications.position',
    descriptionKey: 'settings.notifications.positionDescription',
    platforms: ['desktop'],
    options: () => {
      const $t = get(t);
      return [
        { value: 'bottom-right', label: $t('settings.notifications.positionBottomRight') },
        { value: 'bottom-left', label: $t('settings.notifications.positionBottomLeft') },
        { value: 'bottom-center', label: $t('settings.notifications.positionBottomCenter') },
        { value: 'top-right', label: $t('settings.notifications.positionTopRight') },
        { value: 'top-left', label: $t('settings.notifications.positionTopLeft') },
        { value: 'top-center', label: $t('settings.notifications.positionTopCenter') },
      ];
    },
    width: 180,
  },
  {
    type: 'select',
    key: 'notificationMonitor',
    section: 'notifications',
    subsection: 'layout',
    icon: 'cursor',
    titleKey: 'settings.notifications.monitor',
    descriptionKey: 'settings.notifications.monitorDescription',
    platforms: ['desktop'],
    options: () => {
      const $t = get(t);
      return [
        { value: 'primary', label: $t('settings.notifications.monitorPrimary') },
        { value: 'cursor', label: $t('settings.notifications.monitorCursor') },
      ];
    },
    width: 180,
  },
  {
    type: 'slider',
    key: 'notificationOffset',
    section: 'notifications',
    subsection: 'layout',
    icon: 'sort_vertical',
    titleKey: 'settings.notifications.offset',
    descriptionKey: 'settings.notifications.offsetDescription',
    platforms: ['desktop'],
    min: 0,
    max: 200,
    step: 4,
    suffix: 'px',
    debounce: 150,
  },
  {
    type: 'toggle',
    key: 'compactNotifications',
    section: 'notifications',
    subsection: 'style',
    icon: 'minimize_square',
    titleKey: 'settings.notifications.compact',
    descriptionKey: 'settings.notifications.compactTooltip',
    platforms: ['desktop'],
  },
  {
    type: 'toggle',
    key: 'notificationFancyBackground',
    section: 'notifications',
    subsection: 'style',
    icon: 'image',
    titleKey: 'settings.notifications.fancyBackground',
    descriptionKey: 'settings.notifications.fancyBackgroundTooltip',
    platforms: ['desktop'],
  },
  {
    type: 'toggle',
    key: 'notificationThumbnailTheming',
    section: 'notifications',
    subsection: 'style',
    icon: 'image',
    titleKey: 'settings.notifications.thumbnailTheming',
    descriptionKey: 'settings.notifications.thumbnailThemingTooltip',
    platforms: ['desktop'],
  },
  {
    type: 'toggle',
    key: 'notificationCornerDismiss',
    section: 'notifications',
    subsection: 'timing',
    icon: 'cross_circle',
    titleKey: 'settings.notifications.cornerDismiss',
    descriptionKey: 'settings.notifications.cornerDismissTooltip',
    platforms: ['desktop'],
  },
  {
    type: 'slider',
    key: 'notificationDuration',
    section: 'notifications',
    subsection: 'timing',
    icon: 'hourglass',
    titleKey: 'settings.notifications.duration',
    descriptionKey: 'settings.notifications.durationDescription',
    platforms: ['desktop'],
    min: 2,
    max: 60,
    step: 1,
    suffix: 's',
    debounce: 150,
  },

  {
    type: 'select',
    key: 'proxyMode',
    section: 'network',
    icon: 'link',
    titleKey: 'settings.network.proxyMode',
    descriptionKey: 'settings.network.proxyModeDescription',
    options: () => {
      const $t = get(t);
      return [
        { value: 'none', label: $t('settings.network.proxyModeNone') },
        { value: 'system', label: $t('settings.network.proxyModeSystem') },
        { value: 'custom', label: $t('settings.network.proxyModeCustom') },
      ];
    },
  },
  {
    type: 'custom',
    key: 'proxy-config',
    section: 'network',
    titleKey: 'settings.network.customProxyUrl',
    visible: (s) => s.proxyMode !== 'none',
  },
  {
    type: 'toggle',
    key: 'retryWithoutProxy',
    section: 'network',
    icon: 'restart',
    titleKey: 'settings.network.retryWithoutProxy',
    descriptionKey: 'settings.network.retryWithoutProxyTooltip',
    visible: (s) => s.proxyMode !== 'none',
  },
  {
    type: 'custom',
    key: 'network-check',
    section: 'network',
    titleKey: 'settings.network.checkIp',
    platforms: ['desktop'],
    visible: (s) => s.proxyMode !== 'none',
  },

  {
    type: 'custom',
    key: 'integration-settings',
    section: 'integration',
    titleKey: 'settings.integration.title',
    platforms: ['desktop', 'mobile'],
  },

  {
    type: 'custom',
    key: 'app-updates',
    section: 'app',
    subsection: 'updates',
    titleKey: 'settings.app.updates',
  },
  {
    type: 'toggle',
    key: 'allowPreReleases',
    section: 'app',
    subsection: 'updates',
    icon: 'star',
    titleKey: 'settings.app.allowPreReleases',
    descriptionKey: 'settings.app.allowPreReleasesTooltip',
  },
  {
    type: 'toggle',
    key: 'sendStats',
    section: 'app',
    subsection: 'privacy',
    icon: 'stats',
    titleKey: 'settings.app.sendStats',
    descriptionKey: 'settings.app.sendStatsTooltip',
  },
  {
    type: 'select',
    key: 'backgroundType',
    section: 'app',
    subsection: 'appearance',
    icon: 'image',
    titleKey: 'settings.app.background',
    descriptionKey: 'settings.app.backgroundDescription',
    options: (p) => {
      const $t = get(t);
      const baseOpts = [
        { value: 'animated', label: $t('settings.app.backgroundAnimated') },
        { value: 'solid', label: $t('settings.app.backgroundSolid') },
        { value: 'oled', label: $t('settings.app.backgroundOled') },
        { value: 'image', label: $t('settings.app.backgroundImage') },
      ];
      if (p === 'windows') {
        return [
          { value: 'mica', label: $t('settings.app.backgroundMica') },
          { value: 'mica-dark', label: $t('settings.app.backgroundMicaDark') },
          { value: 'mica-light', label: $t('settings.app.backgroundMicaLight') },
          { value: 'acrylic', label: $t('settings.app.backgroundAcrylic') },
          { value: 'blur', label: $t('settings.app.backgroundBlur') },
          { value: 'tabbed', label: $t('settings.app.backgroundTabbed') },
          { value: 'tabbed-dark', label: $t('settings.app.backgroundTabbedDark') },
          { value: 'tabbed-light', label: $t('settings.app.backgroundTabbedLight') },
          ...baseOpts,
        ];
      }
      if (p === 'macos') {
        return [
          { value: 'vibrancy-sidebar', label: $t('settings.app.backgroundVibrancySidebar') },
          { value: 'vibrancy-hud', label: $t('settings.app.backgroundVibrancyHud') },
          { value: 'vibrancy-window', label: $t('settings.app.backgroundVibrancyWindow') },
          { value: 'vibrancy-popover', label: $t('settings.app.backgroundVibrancyPopover') },
          { value: 'vibrancy-menu', label: $t('settings.app.backgroundVibrancyMenu') },
          { value: 'vibrancy-content', label: $t('settings.app.backgroundVibrancyContent') },
          {
            value: 'vibrancy-under-window',
            label: $t('settings.app.backgroundVibrancyUnderWindow'),
          },
          ...baseOpts,
        ];
      }
      return baseOpts;
    },
  },
  {
    type: 'color',
    key: 'backgroundColor',
    section: 'app',
    subsection: 'appearance',
    icon: 'starry',
    titleKey: 'settings.app.backgroundColor',
    visible: (s) => s.backgroundType === 'solid',
  },
  {
    type: 'path',
    key: 'backgroundVideo',
    section: 'app',
    subsection: 'appearance',
    icon: 'video',
    titleKey: 'settings.app.backgroundVideoUrl',
    descriptionKey: 'settings.app.backgroundVideoUrlDescription',
    pickType: 'file',
    visible: (s) => s.backgroundType === 'animated',
  },
  {
    type: 'path',
    key: 'backgroundImage',
    section: 'app',
    subsection: 'appearance',
    icon: 'image',
    titleKey: 'settings.app.backgroundImageUrl',
    descriptionKey: 'settings.app.backgroundImageUrlDescription',
    pickType: 'file',
    visible: (s) => s.backgroundType === 'image',
  },
  {
    type: 'slider',
    key: 'backgroundBlur',
    section: 'app',
    subsection: 'appearance',
    icon: 'blur',
    titleKey: 'settings.app.backgroundBlurAmount',
    descriptionKey: 'settings.app.backgroundBlurAmountDescription',
    min: 0,
    max: 50,
    step: 1,
    suffix: 'px',
    debounce: 150,
    visible: (s) => s.backgroundType === 'animated' || s.backgroundType === 'image',
  },
  {
    type: 'slider',
    key: 'backgroundOpacity',
    section: 'app',
    subsection: 'appearance',
    icon: 'image',
    titleKey: 'settings.app.backgroundOpacity',
    descriptionKey: 'settings.app.backgroundOpacityDescription',
    min: 0,
    max: 100,
    step: 1,
    suffix: '%',
    debounce: 150,
    visible: (s) => {
      const type = s.backgroundType;
      if (type === 'oled') return false;
      const isWindowEffect = [
        'acrylic',
        'mica',
        'mica-dark',
        'mica-light',
        'tabbed',
        'tabbed-dark',
        'tabbed-light',
        'blur',
        'vibrancy',
      ].some((k) => type.startsWith(k));
      return !isWindowEffect;
    },
    platforms: ['desktop'],
  },
  {
    type: 'slider',
    key: 'windowTint',
    section: 'app',
    subsection: 'appearance',
    icon: 'image',
    titleKey: 'settings.app.windowTint',
    descriptionKey: 'settings.app.windowTintDescription',
    min: 0,
    max: 100,
    step: 1,
    suffix: '%',
    debounce: 150,
    visible: (s) => s.backgroundType !== 'solid' && s.backgroundType !== 'oled',
    platforms: ['desktop'],
  },

  {
    type: 'custom',
    key: 'accent-picker',
    section: 'app',
    subsection: 'appearance',
    titleKey: 'settings.app.accentColor',
    keywords: ['accent', 'color', 'theme', 'system', 'material', 'rgb'],
  },
  {
    type: 'custom',
    key: 'accent-style',
    section: 'app',
    subsection: 'appearance',
    titleKey: 'settings.app.accentStyle',
  },
  {
    type: 'select',
    key: 'surfaceStyle',
    section: 'app',
    subsection: 'appearance',
    icon: 'widgets',
    titleKey: 'settings.app.surfaceStyle',
    descriptionKey: 'settings.app.surfaceStyleDescription',
    options: () => {
      const $t = get(t);
      return [
        { value: 'glass', label: $t('settings.app.surfaceGlass') },
        { value: 'frosted', label: $t('settings.app.surfaceFrosted') },
        { value: 'elevated', label: $t('settings.app.surfaceElevated') },
        { value: 'accent', label: $t('settings.app.surfaceAccent') },
        { value: 'contrast', label: $t('settings.app.surfaceContrast') },
        { value: 'custom', label: $t('settings.app.surfaceCustom') },
      ];
    },
    keywords: ['surface', 'glass', 'frosted', 'blur', 'transparency'],
  },
  {
    type: 'slider',
    key: 'surfaceCustom.opacity',
    section: 'app',
    subsection: 'appearance',
    icon: 'blur',
    titleKey: 'settings.app.surfaceOpacity',
    min: 30,
    max: 100,
    step: 5,
    suffix: '%',
    debounce: 100,
    visible: (s) => s.surfaceStyle === 'custom',
  },
  {
    type: 'slider',
    key: 'surfaceCustom.borderOpacity',
    section: 'app',
    subsection: 'appearance',
    icon: 'tuning',
    titleKey: 'settings.app.surfaceBorderOpacity',
    min: 0,
    max: 40,
    step: 2,
    suffix: '%',
    debounce: 100,
    visible: (s) => s.surfaceStyle === 'custom',
  },
  {
    type: 'select',
    key: 'surfaceCustom.shadowIntensity',
    section: 'app',
    subsection: 'appearance',
    icon: 'starry',
    titleKey: 'settings.app.surfaceShadow',
    options: () => {
      const $t = get(t);
      return [
        { value: 'none', label: $t('settings.app.shadowNone') },
        { value: 'subtle', label: $t('settings.app.shadowSubtle') },
        { value: 'medium', label: $t('settings.app.shadowMedium') },
        { value: 'strong', label: $t('settings.app.shadowStrong') },
      ];
    },
    visible: (s) => s.surfaceStyle === 'custom',
  },
  {
    type: 'slider',
    key: 'surfaceCustom.accentTint',
    section: 'app',
    subsection: 'appearance',
    icon: 'pen_new',
    titleKey: 'settings.app.surfaceAccentTint',
    min: 0,
    max: 30,
    step: 5,
    suffix: '%',
    debounce: 100,
    visible: (s) => s.surfaceStyle === 'custom',
  },
  {
    type: 'select',
    key: 'borderRadius',
    section: 'app',
    subsection: 'appearance',
    icon: 'widget',
    titleKey: 'settings.app.borderRadius',
    descriptionKey: 'settings.app.borderRadiusDescription',
    options: () => {
      const $t = get(t);
      return [
        { value: 'none', label: $t('settings.app.radiusNone') },
        { value: 'subtle', label: $t('settings.app.radiusSubtle') },
        { value: 'rounded', label: $t('settings.app.radiusRounded') },
        { value: 'pill', label: $t('settings.app.radiusPill') },
        { value: 'custom', label: $t('settings.app.radiusCustom') },
      ];
    },
    keywords: ['corners', 'rounded', 'square', 'radius'],
  },
  {
    type: 'slider',
    key: 'borderRadiusCustom',
    section: 'app',
    subsection: 'appearance',
    icon: 'widget',
    titleKey: 'settings.app.borderRadiusCustom',
    min: 0,
    max: 24,
    step: 2,
    suffix: 'px',
    debounce: 100,
    visible: (s) => s.borderRadius === 'custom',
  },
  {
    type: 'select',
    key: 'textScale',
    section: 'app',
    subsection: 'appearance',
    icon: 'text',
    titleKey: 'settings.app.textScale',
    descriptionKey: 'settings.app.textScaleDescription',
    options: () => {
      const $t = get(t);
      return [
        { value: 'compact', label: $t('settings.app.textCompact') },
        { value: 'default', label: $t('settings.app.textDefault') },
        { value: 'large', label: $t('settings.app.textLarge') },
        { value: 'custom', label: $t('settings.app.textCustom') },
      ];
    },
    keywords: ['font', 'size', 'text', 'accessibility'],
  },
  {
    type: 'slider',
    key: 'textScaleCustom',
    section: 'app',
    subsection: 'appearance',
    icon: 'text',
    titleKey: 'settings.app.textScaleCustom',
    min: 0.8,
    max: 1.4,
    step: 0.05,
    suffix: 'x',
    debounce: 100,
    visible: (s) => s.textScale === 'custom',
  },

  {
    type: 'toggle',
    key: 'disableAnimations',
    section: 'app',
    subsection: 'general',
    icon: 'stop',
    titleKey: 'settings.app.disableAnimations',
  },
  {
    type: 'select',
    key: 'toastPosition',
    section: 'app',
    subsection: 'general',
    icon: 'chat',
    titleKey: 'settings.notifications.toastPosition',
    descriptionKey: 'settings.notifications.toastPositionDescription',
    options: () => {
      const $t = get(t);
      return [
        { value: 'bottom-right', label: $t('settings.notifications.positionBottomRight') },
        { value: 'bottom-left', label: $t('settings.notifications.positionBottomLeft') },
        { value: 'bottom-center', label: $t('settings.notifications.positionBottomCenter') },
        { value: 'top-right', label: $t('settings.notifications.positionTopRight') },
        { value: 'top-left', label: $t('settings.notifications.positionTopLeft') },
        { value: 'top-center', label: $t('settings.notifications.positionTopCenter') },
      ];
    },
  },
  {
    type: 'select',
    key: 'sizeUnit',
    section: 'app',
    subsection: 'general',
    icon: 'weight',
    titleKey: 'settings.app.sizeUnit',
    descriptionKey: 'settings.app.sizeUnitDescription',
    options: () => {
      const $t = get(t);
      return [
        { value: 'binary', label: $t('settings.app.sizeUnitBinary') },
        { value: 'decimal', label: $t('settings.app.sizeUnitDecimal') },
      ];
    },
  },
  {
    type: 'toggle',
    key: 'showHistoryStats',
    section: 'app',
    subsection: 'general',
    icon: 'history',
    titleKey: 'settings.app.showHistoryStats',
    descriptionKey: 'settings.app.showHistoryStatsDescription',
  },
  {
    type: 'slider',
    key: 'gridItemSize',
    section: 'app',
    subsection: 'general',
    icon: 'gallery',
    titleKey: 'settings.app.gridItemSize',
    descriptionKey: 'settings.app.gridItemSizeDescription',
    min: 120,
    max: 400,
    step: 40,
    suffix: 'px',
    debounce: 100,
  },
  {
    type: 'toggle',
    key: 'thumbnailTheming',
    section: 'app',
    subsection: 'appearance',
    icon: 'pen_new',
    titleKey: 'settings.app.thumbnailTheming',
    descriptionKey: 'settings.app.thumbnailThemingDescription',
  },
  {
    type: 'toggle',
    key: 'builderThumbnailGlow',
    section: 'app',
    subsection: 'appearance',
    icon: 'blur',
    titleKey: 'settings.app.builderThumbnailGlow',
    descriptionKey: 'settings.app.builderThumbnailGlowDescription',
  },

  {
    type: 'custom',
    key: 'deps-manager',
    section: 'deps',
    titleKey: 'settings.deps.title',
    platforms: ['desktop'],
  },

  {
    type: 'custom',
    key: 'data-actions',
    section: 'data',
    titleKey: 'settings.data.title',
    keywords: ['reset', 'clear', 'export', 'import', 'history', 'cache'],
  },
];
