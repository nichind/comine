<script lang="ts">
  import { onMount, onDestroy, type Snippet } from 'svelte';
  import { get } from 'svelte/store';
  import { browser } from '$app/environment';
  import { getCurrentWindow, type Window as TauriWindow } from '@tauri-apps/api/window';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { readText } from '@tauri-apps/plugin-clipboard-manager';
  import { attachLogger } from '@tauri-apps/plugin-log';
  import { invoke } from '@tauri-apps/api/core';
  import { isPermissionGranted, sendNotification } from '@tauri-apps/plugin-notification';
  import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import Icon, { type IconName } from '$lib/components/Icon.svelte';
  import NavItem from '$lib/components/NavItem.svelte';
  import Toast from '$lib/components/Toast.svelte';
  import BackgroundProvider from '$lib/components/BackgroundProvider.svelte';
  import AccentProvider from '$lib/components/AccentProvider.svelte';
  import SurfaceProvider from '$lib/components/SurfaceProvider.svelte';
  import { toast, updateToast, dismissToast } from '$lib/components/Toast.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { t } from '$lib/i18n';
  import {
    initSettings,
    settings,
    settingsReady,
    type CloseBehavior,
    getSettings,
    getProxyConfig,
    updateSettings,
  } from '$lib/stores/settings';
  import { history } from '$lib/stores/history';
  import { queue, activeDownloadsCount } from '$lib/stores/queue';
  import { deps } from '$lib/stores/deps';
  import { logs, type LogLevel } from '$lib/stores/logs';
  import { mediaCache } from '$lib/stores/mediaCache';
  import { clearAllScrollPositions } from '$lib/stores/scroll';
  import { clearColorCache } from '$lib/utils/color';
  import { isTypingTarget } from '$lib/utils/keyboard';
  import {
    cleanUrl,
    isLikelyPlaylist,
    isLikelyChannel,
    isValidMediaUrl,
    isHttpUrl,
    getQuickThumbnail,
    isDirectFileUrl,
    formatSpeed,
    formatSize,
  } from '$lib/utils/format';
  import { resolveUrl, convertProxyConfig } from '$lib/backend/mediaBackend';
  import {
    isAndroid,
    openFileOnAndroid,
    onShareIntent,
    setupAndroidLogHandler,
    cleanupAndroidCallbacks,
  } from '$lib/utils/android';
  import {
    startUpdateChecker,
    stopUpdateChecker,
    clearDismissedVersionIfUpdated,
  } from '$lib/stores/updates';
  import { appStats } from '$lib/stores/stats';
  import { navigation } from '$lib/stores/navigation';
  import { setupServerSync } from '$lib/stores/serverSync';
  import NotificationPopup from '$lib/components/NotificationPopup.svelte';

  let { children }: { children: Snippet } = $props();

  let totalDownloadSpeed = $derived.by(() => {
    const items = $queue.items.filter((i) => i.status === 'downloading' && i.speed);
    if (items.length === 0) return '';

    let totalBytesPerSec = 0;
    for (const item of items) {
      const speed = item.speed.toLowerCase();
      const match = speed.match(/([\d.]+)\s*(k|m|g)?i?b?\/s?/i);
      if (match) {
        let value = parseFloat(match[1]);
        const unit = (match[2] || '').toLowerCase();
        if (unit === 'k') value *= 1024;
        else if (unit === 'm') value *= 1024 * 1024;
        else if (unit === 'g') value *= 1024 * 1024 * 1024;
        totalBytesPerSec += value;
      }
    }

    if (totalBytesPerSec === 0) return '';

    return formatSpeed(totalBytesPerSec);
  });

  let isDownloading = $derived($activeDownloadsCount > 0 && totalDownloadSpeed !== '');

  let isNotificationWindow = $derived(
    browser && window.location.pathname.startsWith('/notification')
  );

  let appWindow: TauriWindow | null = $state(null);

  let isMobile = $state(false);
  let windowWidth = $state(0);
  let lastClipboardText = $state('');
  let clipboardCheckInterval: ReturnType<typeof setInterval> | null = null;

  let lastClipboardSystemNotificationKey: string | null = null;
  let lastClipboardSystemNotificationAtMs = 0;

  async function maybeSendClipboardSystemNotification(
    key: string,
    title: string,
    body: string
  ): Promise<void> {
    if (isAndroid()) return;
    if (!$settings.notificationsEnabled) return;

    const now = Date.now();
    if (
      lastClipboardSystemNotificationKey === key &&
      now - lastClipboardSystemNotificationAtMs < 5000
    ) {
      return;
    }

    try {
      const hasPermission = await isPermissionGranted();
      if (!hasPermission) return;

      lastClipboardSystemNotificationKey = key;
      lastClipboardSystemNotificationAtMs = now;

      sendNotification({
        title,
        body,
      });
    } catch (e) {
      logs.debug('layout', `System clipboard notification skipped: ${e}`);
    }
  }

  let hasShownTrayNotification = false;
  let isWindowHidden = $state(false);
  let depsToastId: number | null = null;

  let unlistenClose: UnlistenFn | null = null;
  let unlistenTrayDownload: UnlistenFn | null = null;
  let unlistenNotificationDownload: UnlistenFn | null = null;
  let unlistenNotificationStartDownload: UnlistenFn | null = null;
  let unlistenWindowShown: UnlistenFn | null = null;
  let unlistenDeepLink: UnlistenFn | null = null;
  let unlistenExtensionDownload: UnlistenFn | null = null;
  let unlistenExtensionCancel: UnlistenFn | null = null;
  let unlistenServerOpen: UnlistenFn | null = null;
  let unlistenServerReveal: UnlistenFn | null = null;
  let unlistenExtensionCookies: UnlistenFn | null = null;
  let extensionProgressUnsub: (() => void) | null = null;
  let detachLogger: (() => void) | null = null;
  let cleanupShareIntent: (() => void) | null = null;

  let broadcastPollTimer: ReturnType<typeof setInterval> | null = null;
  let broadcastsFetchInFlight = false;
  const BROADCAST_POLL_INTERVAL_MS = 30 * 60 * 1000;
  let cleanupBroadcastVisibilityListener: (() => void) | null = null;
  const activeBroadcastNotifIds = new Map<number, string>();

  const extensionDownloads = new Map<
    string,
    { id: string; lastState: string; lastProgress: number }
  >();

  const MOBILE_BREAKPOINT = 480;
  const CLIPBOARD_CHECK_INTERVAL = 250;

  let cleanupResize: (() => void) | null = null;
  let cleanupKeyboard: (() => void) | null = null;

  function setupKeyboardShortcuts() {
    const handleKeyDown = async (e: KeyboardEvent) => {
      const isEditable = isTypingTarget(e.target as Element | null);

      if ((e.ctrlKey || e.metaKey) && e.key === 'v') {
        if (!isEditable) {
          e.preventDefault();
          try {
            const text = await readText();
            if (text && isHttpUrl(text)) {
              const url = cleanUrl(text);
              goto(`/?url=${encodeURIComponent(url)}`);
              toast.info(`📋 ${$t('clipboard.detected')}`);
            }
          } catch (err) {
            console.error('Clipboard read failed:', err);
          }
        }
        return;
      }

      if (isEditable) return;

      const pages = allNavItems.map((i) => i.path);

      if (e.ctrlKey && e.key === 'Tab') {
        e.preventDefault();
        const currentPath = $page.url.pathname;
        const currentIndex = pages.indexOf(currentPath);
        const idx = currentIndex === -1 ? 0 : currentIndex;

        if (e.shiftKey) {
          const prevIndex = idx === 0 ? pages.length - 1 : idx - 1;
          goto(pages[prevIndex]);
        } else {
          const nextIndex = idx === pages.length - 1 ? 0 : idx + 1;
          goto(pages[nextIndex]);
        }
        return;
      }

      if (e.altKey && !e.ctrlKey && !e.metaKey) {
        const num = Number.parseInt(e.key, 10);
        if (num >= 1 && num <= pages.length) {
          e.preventDefault();
          goto(pages[num - 1]);
          return;
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    cleanupKeyboard = () => window.removeEventListener('keydown', handleKeyDown);
  }

  async function autoInstallDependencies() {
    await new Promise((r) => setTimeout(r, 1000));

    const state = $deps;
    const missing: Array<{ name: string; key: 'ytdlp' | 'ffmpeg' | 'aria2' | 'quickjs' }> = [];

    if (!state.ytdlp?.installed) missing.push({ name: 'yt-dlp', key: 'ytdlp' });
    if (!state.ffmpeg?.installed) missing.push({ name: 'FFmpeg', key: 'ffmpeg' });
    if (!state.aria2?.installed) missing.push({ name: 'aria2', key: 'aria2' });
    if (!state.quickjs?.installed) missing.push({ name: 'QuickJS', key: 'quickjs' });

    if (missing.length === 0) return;

    logs.info('deps', `Auto-installing ${missing.length} missing dependencies...`);

    depsToastId = toast.progress(
      $t('deps.installing') || 'Installing components...',
      0,
      `0/${missing.length} ${$t('deps.components') || 'components'}`
    );

    let installed = 0;

    const aria2Idx = missing.findIndex((d) => d.key === 'aria2');
    if (aria2Idx !== -1) {
      const aria2 = missing.splice(aria2Idx, 1)[0];
      updateToast(depsToastId, {
        message: `${$t('deps.installing') || 'Installing'} ${aria2.name}...`,
        subMessage: `${installed}/${missing.length + 1} ${$t('deps.components') || 'components'}`,
      });
      const success = await deps.installAria2();
      if (success) installed++;
      updateToast(depsToastId, {
        progress: (installed / (missing.length + 1)) * 100,
        subMessage: `${installed}/${missing.length + 1} ${$t('deps.components') || 'components'}`,
      });
    }

    const results = await Promise.all(
      missing.map(async (dep, i) => {
        updateToast(depsToastId!, {
          message: `${$t('deps.installing') || 'Installing'} ${dep.name}...`,
        });

        let success = false;
        switch (dep.key) {
          case 'ytdlp':
            success = await deps.installYtdlp();
            break;
          case 'ffmpeg':
            success = await deps.installFfmpeg();
            break;
          case 'quickjs':
            success = await deps.installQuickjs();
            break;
        }

        if (success) {
          installed++;
          updateToast(depsToastId!, {
            progress: (installed / (missing.length + (aria2Idx !== -1 ? 1 : 0))) * 100,
            subMessage: `${installed}/${missing.length + (aria2Idx !== -1 ? 1 : 0)} ${$t('deps.components') || 'components'}`,
          });
        }

        return success;
      })
    );

    if (depsToastId) {
      const allSuccess = results.every(Boolean) && (aria2Idx === -1 || installed > 0);
      if (allSuccess) {
        updateToast(depsToastId, {
          type: 'success',
          message: $t('deps.ready') || 'Components ready!',
          progress: 100,
        });
        setTimeout(() => {
          if (depsToastId) dismissToast(depsToastId);
          depsToastId = null;
        }, 3000);
      } else {
        updateToast(depsToastId, {
          type: 'warning',
          message: $t('deps.someError') || 'Some components failed to install',
          subMessage: $t('deps.checkSettings') || 'Check Settings → Dependencies',
        });
        setTimeout(() => {
          if (depsToastId) dismissToast(depsToastId);
          depsToastId = null;
        }, 5000);
      }
    }
  }

  let diskSpaceWarningShown = false;

  async function autoStartExtensionServer() {
    if (!$settingsReady) {
      await new Promise<void>((resolve) => {
        const unsub = settingsReady.subscribe((ready) => {
          if (ready) {
            unsub();
            resolve();
          }
        });
      });
    }

    if ($settings.extensionServerEnabled) {
      const port = $settings.extensionLocalPort || 9549;
      try {
        const isRunning = await invoke<boolean>('server_is_running');
        if (!isRunning) {
          await invoke('server_start', { port, token: $settings.extensionServerToken });
          logs.info('server', `Extension server auto-started on port ${port}`);
        }
      } catch (e) {
        logs.warn('server', `Failed to auto-start extension server: ${e}`);
      }
    }
  }

  async function checkDiskSpace() {
    logs.info('system', 'checkDiskSpace() called');
    if (diskSpaceWarningShown) {
      return;
    }

    if (!$settingsReady) {
      await new Promise<void>((resolve) => {
        const unsub = settingsReady.subscribe((ready) => {
          if (ready) {
            unsub();
            resolve();
          }
        });
      });
    }

    try {
      const downloadPath = $settings.downloadPath || '';
      const diskInfo = await invoke<{ availableGb: number; availableBytes: number }>(
        'get_disk_space',
        { path: downloadPath }
      );

      if (diskInfo && diskInfo.availableGb < 2) {
        diskSpaceWarningShown = true;
        const available = Math.round(diskInfo.availableGb * 10) / 10;
        const warningMsg = (
          $t('disk.lowSpaceWarning') || 'Only {available} GB free on your download drive'
        ).replace('{available}', String(available));
        toast.warning(warningMsg, 0);
        logs.warn('system', `Low disk space: ${available} GB available`);
      }
    } catch (e) {
      logs.error('system', `Could not check disk space: ${e}`);
    }
  }

  onMount(() => {
    if (window.location.pathname.startsWith('/notification')) {
      return;
    }

    appWindow = getCurrentWindow();

    initSettings();
    queue.init();

    if (!isAndroid()) {
      setupServerSync();
    }
    autoStartExtensionServer();

    setTimeout(async () => {
      await deps.checkAll();
      if (!isAndroid()) {
        autoInstallDependencies();
        checkDiskSpace();
      }
    }, 1500);

    windowWidth = window.innerWidth;
    isMobile = windowWidth < MOBILE_BREAKPOINT;

    const handleResize = () => {
      windowWidth = window.innerWidth;
      isMobile = windowWidth < MOBILE_BREAKPOINT;

      if (!isMobile) {
        queueSidebarNavIndicatorUpdate();
      }
    };

    window.addEventListener('resize', handleResize);
    cleanupResize = () => window.removeEventListener('resize', handleResize);

    setupListeners();

    if (!isAndroid()) {
      setupKeyboardShortcuts();
    }

    setupLogForwarding();

    if (!isAndroid()) {
      startClipboardWatcher();
    }

    startUpdateChecker();
    clearDismissedVersionIfUpdated();
    initStats();

    if (isAndroid()) {
      cleanupShareIntent = onShareIntent(handleAndroidShareIntent);

      setupAndroidLogHandler((level, source, message) => {
        logs.log(level, source, message);
      });
      logs.info('system', 'Android log forwarding initialized');
    }
  });

  function levelNumberToLogLevel(level: number): LogLevel {
    switch (level) {
      case 1:
        return 'error';
      case 2:
        return 'warn';
      case 3:
        return 'info';
      case 4:
        return 'debug';
      case 5:
        return 'trace';
      default:
        return 'info';
    }
  }

  async function setupLogForwarding() {
    try {
      detachLogger = await attachLogger(({ level, message }) => {
        const levelStr = levelNumberToLogLevel(level);

        let source = 'rust';
        let msg = message;

        const timestampMatch = message.match(/^\[\d{4}-\d{2}-\d{2}\]\[\d{2}:\d{2}:\d{2}\]/);
        if (timestampMatch) {
          msg = message.substring(timestampMatch[0].length);

          const targetLevelMatch = msg.match(/^\[([^\]]+)\]\[([A-Z]+)\]\s*/);
          if (targetLevelMatch) {
            source = targetLevelMatch[1].split('::').pop()?.split(' ').pop() || 'rust';
            msg = msg.substring(targetLevelMatch[0].length);
          }
        } else {
          const colonIdx = message.indexOf('::');
          if (colonIdx > 0 && colonIdx < 40) {
            source = message.substring(0, colonIdx).split('_').pop() || 'rust';
            msg = message.substring(colonIdx + 2).trim();

            const levelMatch = msg.match(/^\[([A-Z]+)\]\s*/);
            if (levelMatch) {
              msg = msg.substring(levelMatch[0].length);
            }
          }
        }

        logs.log(levelStr, source, msg.trim());
      });
      logs.info('system', 'Backend log forwarding initialized');
    } catch (e) {
      console.error('Failed to attach logger:', e);
    }
  }

  async function setupListeners() {
    unlistenClose = await listen('close-requested', async () => {
      await handleCloseRequest();
    });

    unlistenTrayDownload = await listen('tray-download-clipboard', async () => {
      await downloadFromClipboard();
    });

    unlistenNotificationDownload = await listen<string>('notification-download', async (event) => {
      const url = cleanUrl(event.payload);
      if (url) {
        goto(`/?url=${encodeURIComponent(url)}`);
      }
    });

    interface NotificationPayload {
      url: string;
      metadata?: {
        title?: string | null;
        thumbnail?: string | null;
        uploader?: string | null;
        downloadMode?: 'auto' | 'audio' | 'mute';
        isPlaylist?: boolean | null;
        isChannel?: boolean | null;
        isFile?: boolean | null;
        openTrackBuilder?: boolean | null;
        fileInfo?: {
          filename: string;
          size: number;
          mimeType: string;
        } | null;
      } | null;
    }

    unlistenNotificationStartDownload = await listen<NotificationPayload>(
      'notification-start-download',
      async (event) => {
        const { url: rawUrl, metadata } = event.payload;
        const url = cleanUrl(rawUrl);
        const notificationDownloadMode = metadata?.downloadMode;
        const isPlaylistNotification = metadata?.isPlaylist === true;
        const isChannelNotification = metadata?.isChannel === true;
        const isFileNotification = metadata?.isFile === true;
        logs.info(
          'layout',
          `notification-start-download received: ${url}, isPlaylist: ${isPlaylistNotification}, isChannel: ${isChannelNotification}, isFile: ${isFileNotification}`
        );
        logs.debug(
          'layout',
          `Prefetched metadata: title=${metadata?.title}, uploader=${metadata?.uploader}, mode=${notificationDownloadMode}`
        );

        if (!url) return;

        if (isFileNotification && metadata?.fileInfo) {
          logs.info('layout', `Starting file download: ${metadata.fileInfo.filename}`);

          const queueId = queue.addFile({
            url: rawUrl,
            filename: metadata.fileInfo.filename,
            size: metadata.fileInfo.size,
            mimeType: metadata.fileInfo.mimeType,
          });

          if (queueId) {
            toast.success($t('notification.downloadStarted'));
          } else {
            toast.info($t('queue.alreadyInQueue') || 'Already in queue');
          }
          return;
        }

        if (isChannelNotification) {
          logs.info('layout', `Channel detected - showing window and opening channel view: ${url}`);

          if (appWindow) {
            try {
              await appWindow.show();
              await appWindow.setFocus();
            } catch (e) {
              logs.warn('layout', `Failed to show/focus window: ${e}`);
            }
          }

          if (metadata?.title || metadata?.thumbnail || metadata?.uploader) {
            mediaCache.setPreview(url, {
              title: metadata.title || undefined,
              thumbnail: metadata.thumbnail || undefined,
              author: metadata.uploader || undefined,
            });
          }

          navigation.openChannel(url, {
            title: metadata?.title || undefined,
            thumbnail: metadata?.thumbnail || undefined,
            author: metadata?.uploader || undefined,
          });
          await goto('/');
          toast.info($t('channel.notification.opening') || 'Opening channel...');
          return;
        }

        if (isPlaylistNotification) {
          logs.info(
            'layout',
            `Playlist detected - showing window and opening playlist view: ${url}`
          );

          if (appWindow) {
            try {
              await appWindow.show();
              await appWindow.setFocus();
            } catch (e) {
              logs.warn('layout', `Failed to show/focus window: ${e}`);
            }
          }

          if (metadata?.title || metadata?.thumbnail || metadata?.uploader) {
            mediaCache.setPreview(url, {
              title: metadata.title || undefined,
              thumbnail: metadata.thumbnail || undefined,
              author: metadata.uploader || undefined,
            });
          }

          navigation.openPlaylist(url, {
            title: metadata?.title || undefined,
            thumbnail: metadata?.thumbnail || undefined,
            author: metadata?.uploader || undefined,
          });
          await goto('/');
          toast.info($t('playlist.notification.openingModal'));
          return;
        }

        if (metadata?.openTrackBuilder) {
          logs.info('layout', `Opening Track Builder for: ${url}`);

          if (appWindow) {
            try {
              await appWindow.show();
              await appWindow.setFocus();
            } catch (e) {
              logs.warn('layout', `Failed to show/focus window: ${e}`);
            }
          }

          if (metadata?.title || metadata?.thumbnail || metadata?.uploader) {
            mediaCache.setPreview(url, {
              title: metadata.title || undefined,
              thumbnail: metadata.thumbnail || undefined,
              author: metadata.uploader || undefined,
            });
          }

          navigation.openVideo(url, {
            title: metadata?.title || undefined,
            thumbnail: metadata?.thumbnail || undefined,
            author: metadata?.uploader || undefined,
          });
          await goto('/');
          return;
        }

        if (!isAndroid()) {
          await deps.checkAll();
        }

        const currentSettings = getSettings();

        const isYtmUrl = /music\.youtube\.com/i.test(url);
        const shouldForceAudio =
          isYtmUrl &&
          currentSettings.youtubeMusicAudioOnly &&
          (!notificationDownloadMode || notificationDownloadMode === 'auto');

        logs.debug(
          'layout',
          `YTM check: isYtmUrl=${isYtmUrl}, setting=${currentSettings.youtubeMusicAudioOnly}, notifMode=${notificationDownloadMode}, forceAudio=${shouldForceAudio}`
        );

        const queueId = queue.add(url, {
          ignoreMixes: currentSettings.ignoreMixes ?? true,
          videoQuality: currentSettings.defaultVideoQuality ?? 'max',
          downloadMode: shouldForceAudio
            ? 'audio'
            : notificationDownloadMode === 'auto'
              ? undefined
              : (notificationDownloadMode ?? undefined),
          audioQuality: currentSettings.defaultAudioQuality ?? 'best',
          convertToMp4: currentSettings.convertToMp4 ?? false,
          remux: currentSettings.remux ?? true,
          clearMetadata: currentSettings.clearMetadata ?? false,
          dontShowInHistory: currentSettings.dontShowInHistory ?? false,
          useAria2: currentSettings.useAria2 ?? true,
          cookiesFromBrowser: currentSettings.cookiesFromBrowser ?? '',
          customCookies: currentSettings.customCookies ?? '',
          prefetchedInfo: metadata
            ? {
                title: metadata.title || undefined,
                thumbnail: metadata.thumbnail || undefined,
                author: metadata.uploader || undefined,
              }
            : undefined,
        });
        logs.info(
          'layout',
          `Added to queue: ${queueId ? queueId : 'failed (already in queue or deps missing)'}`
        );
        if (queueId) {
          toast.success($t('notification.downloadStarted'));
        }
      }
    );

    unlistenWindowShown = await listen('window-shown', () => {
      onWindowShown();
    });

    unlistenDeepLink = await listen<string>('deep-link-url', async (event) => {
      logs.info('layout', `Deep link received: ${event.payload}`);

      let videoUrl = event.payload;

      if (videoUrl.startsWith('download?url=') || videoUrl.startsWith('download/?url=')) {
        videoUrl = videoUrl.replace(/^download\/??\?url=/, '');
      } else if (videoUrl.startsWith('url=')) {
        videoUrl = videoUrl.replace(/^url=/, '');
      }

      try {
        videoUrl = decodeURIComponent(videoUrl);
      } catch (e) {}

      const url = cleanUrl(videoUrl);
      logs.info('layout', `Extracted video URL: ${url}`);

      if (url && isHttpUrl(url)) {
        goto(`/?url=${encodeURIComponent(url)}`);
        toast.info(`🔗 ${$t('deeplink.received') || 'URL received from browser'}`);

        if (appWindow) {
          try {
            await appWindow.show();
            await appWindow.unminimize();
            await appWindow.setFocus();
          } catch (e) {
            logs.warn('layout', `Failed to focus window: ${e}`);
          }
        }
      }
    });

    unlistenExtensionDownload = await listen<{
      url: string;
      title?: string | null;
      thumbnail?: string | null;
      id: string;
      openApp?: boolean;
      deviceId?: string;
      fromRelay?: boolean;
      options?: {
        videoQuality?: string;
        downloadMode?: string;
        audioQuality?: string;
        remux?: boolean;
        convertToMp4?: boolean;
        embedThumbnail?: boolean;
        clearMetadata?: boolean;
      };
    }>('extension-download', async (event) => {
      const {
        url: rawUrl,
        title,
        thumbnail,
        id,
        openApp = true,
        deviceId,
        fromRelay,
        options: extOptions,
      } = event.payload;
      logs.info(
        'layout',
        `Extension download received: ${rawUrl} (id: ${id}, openApp: ${openApp}, fromRelay: ${fromRelay})`
      );

      const url = cleanUrl(rawUrl);
      if (!url) {
        logs.warn('layout', 'Invalid URL from extension');
        return;
      }

      if (title || thumbnail) {
        mediaCache.setPreview(url, {
          title: title || undefined,
          thumbnail: thumbnail || undefined,
        });
      }

      if (openApp) {
        if (appWindow) {
          try {
            await appWindow.show();
            await appWindow.unminimize();
            await appWindow.requestUserAttention(1);
            await appWindow.setFocus();
            await onWindowShown();
          } catch (e) {
            logs.warn('layout', `Failed to focus window: ${e}`);
          }
        }

        goto(`/?url=${encodeURIComponent(url)}`);
        toast.info(`🔗 ${$t('extension.received') || 'URL received from browser extension'}`);
      } else {
        const currentSettings = getSettings();

        extensionDownloads.set(url, { id, lastState: 'queued', lastProgress: 0 });
        const videoQuality =
          extOptions?.videoQuality ?? currentSettings.defaultVideoQuality ?? 'max';
        const isYtmUrl = /music\.youtube\.com/i.test(url);
        const shouldForceAudio =
          isYtmUrl &&
          currentSettings.youtubeMusicAudioOnly &&
          (!extOptions?.downloadMode || extOptions?.downloadMode === 'auto');
        const downloadMode = shouldForceAudio
          ? 'audio'
          : ((extOptions?.downloadMode as 'auto' | 'audio' | 'mute' | undefined) ?? undefined);
        const audioQuality =
          extOptions?.audioQuality ?? currentSettings.defaultAudioQuality ?? 'best';
        const convertToMp4 = extOptions?.convertToMp4 ?? currentSettings.convertToMp4 ?? false;
        const remux = extOptions?.remux ?? currentSettings.remux ?? true;
        const clearMetadata = extOptions?.clearMetadata ?? currentSettings.clearMetadata ?? false;
        const embedThumbnail = extOptions?.embedThumbnail ?? currentSettings.embedThumbnail ?? true;

        const queueId = queue.add(url, {
          ignoreMixes: currentSettings.ignoreMixes ?? true,
          videoQuality,
          downloadMode,
          audioQuality,
          convertToMp4,
          remux,
          clearMetadata,
          embedThumbnail,
          dontShowInHistory: currentSettings.dontShowInHistory ?? false,
          useAria2: currentSettings.useAria2 ?? true,
          cookiesFromBrowser: currentSettings.cookiesFromBrowser ?? '',
          customCookies: currentSettings.customCookies ?? '',
          prefetchedInfo:
            title || thumbnail
              ? {
                  title: title || undefined,
                  thumbnail: thumbnail || undefined,
                }
              : undefined,
        });

        if (queueId) {
          toast.info(`${$t('extension.quickDownload') || 'Quick download started'}`);
          logs.info('layout', `Quick download queued: ${queueId}`);
        } else {
          toast.info($t('queue.alreadyInQueue') || 'Already in queue');
          extensionDownloads.delete(url);
        }
      }
    });

    unlistenExtensionCancel = await listen<{
      url: string;
      id: string;
      deviceId?: string;
      fromRelay?: boolean;
    }>('extension-cancel', async (event) => {
      const { url, id, deviceId, fromRelay } = event.payload;
      logs.info('layout', `Extension cancel received: ${url} (id: ${id}, fromRelay: ${fromRelay})`);

      const state = get(queue);
      const item = state.items.find((i) => i.url === url);
      if (item) {
        queue.cancel(item.id);
        extensionDownloads.delete(url);
      }
    });

    unlistenServerOpen = await listen<string>('server-open', async (event) => {
      const filePath = event.payload;
      logs.info('layout', `Server open request: ${filePath}`);
      try {
        if (isAndroid()) {
          await openFileOnAndroid(filePath);
        } else {
          await openPath(filePath);
        }
      } catch (e) {
        logs.error('layout', `Failed to open file: ${e}`);
        toast.error($t('downloads.openError') || 'Failed to open file');
      }
    });

    unlistenServerReveal = await listen<string>('server-reveal', async (event) => {
      const filePath = event.payload;
      logs.info('layout', `Server reveal request: ${filePath}`);
      try {
        await revealItemInDir(filePath);
      } catch (e) {
        logs.error('layout', `Failed to reveal file: ${e}`);
        toast.error($t('downloads.revealError') || 'Failed to show in folder');
      }
    });

    unlistenExtensionCookies = await listen<{
      domain: string;
      sourceUrl?: string | null;
      count: number;
      cookies: string;
    }>('extension-cookies', async (event) => {
      const { domain, sourceUrl, count, cookies } = event.payload;
      logs.info('layout', `Extension cookies received: ${count} cookies from ${domain}`);

      const currentCookies = getSettings().customCookies || '';
      let newCookies = cookies;

      if (currentCookies && currentCookies.includes('# Netscape HTTP Cookie File')) {
        const existingMap = new Map<string, string>();
        for (const line of currentCookies.split('\n')) {
          if (line.startsWith('#') || !line.trim()) continue;
          const parts = line.split('\t');
          if (parts.length >= 7) {
            const key = `${parts[0]}|${parts[5]}`;
            existingMap.set(key, line);
          }
        }

        for (const line of cookies.split('\n')) {
          if (line.startsWith('#') || !line.trim()) continue;
          const parts = line.split('\t');
          if (parts.length >= 7) {
            const key = `${parts[0]}|${parts[5]}`;
            existingMap.set(key, line);
          }
        }

        newCookies = '# Netscape HTTP Cookie File\n' + Array.from(existingMap.values()).join('\n');
      }

      const currentReceipt = getSettings().extensionCookiesReceived || [];
      const nextEntry = { domain, sourceUrl: sourceUrl ?? null, count, receivedAt: Date.now() };
      const merged = [nextEntry, ...currentReceipt.filter((e) => e.domain !== domain)].slice(0, 12);

      await updateSettings({
        customCookies: newCookies,
        cookiesFromBrowser: 'custom',
        extensionCookiesReceived: merged,
      });

      toast.success($t('extension.cookiesReceived') || `${count} cookies received from extension`);
    });

    extensionProgressUnsub = queue.subscribe((state) => {
      for (const [url, tracking] of extensionDownloads) {
        const item = state.items.find((i) => i.url === url);
        if (!item) continue;

        let extState: string;
        switch (item.status) {
          case 'pending':
          case 'paused':
          case 'fetching-info':
            extState = 'queued';
            break;
          case 'downloading':
          case 'processing':
            extState = 'downloading';
            break;
          case 'completed':
            extState = 'completed';
            break;
          case 'failed':
            extState = 'error';
            break;
          default:
            extState = 'queued';
        }

        const progressChanged = Math.abs(item.progress - tracking.lastProgress) >= 1;
        const stateChanged = extState !== tracking.lastState;

        if (stateChanged || progressChanged) {
          tracking.lastState = extState;
          tracking.lastProgress = item.progress;

          invoke('extension_update_status', {
            id: tracking.id,
            state: extState,
            progress: Math.round(item.progress) || null,
            speed: item.speed || null,
            eta: item.eta || null,
            error: item.error || null,
            filePath: extState === 'completed' ? item.filePath || null : null,
            duration: item.duration || null,
          }).catch((err) => {
            logs.debug('layout', `Failed to update extension status: ${err}`);
          });

          if (extState === 'completed' || extState === 'error') {
            extensionDownloads.delete(url);
          }
        }
      }
    });

    try {
      const { getCurrent } = await import('@tauri-apps/plugin-deep-link');
      const startupUrls = await getCurrent();
      if (startupUrls && startupUrls.length > 0) {
        logs.info('layout', `App launched with deep link URLs: ${startupUrls.join(', ')}`);
        for (const rawUrl of startupUrls) {
          let videoUrl = rawUrl;
          if (rawUrl.startsWith('comine://download?url=')) {
            videoUrl = decodeURIComponent(rawUrl.replace('comine://download?url=', ''));
          } else if (rawUrl.startsWith('comine://download?')) {
            videoUrl = decodeURIComponent(rawUrl.replace('comine://download?', ''));
          } else if (rawUrl.startsWith('comine://')) {
            videoUrl = decodeURIComponent(rawUrl.replace('comine://', ''));
          }

          const cleanedUrl = cleanUrl(videoUrl);
          if (cleanedUrl && isHttpUrl(cleanedUrl)) {
            goto(`/?url=${encodeURIComponent(cleanedUrl)}`);
            toast.info(`🔗 ${$t('deeplink.received') || 'URL received from browser'}`);
            break;
          }
        }
      }
    } catch (e) {
      logs.debug('layout', `Could not check startup deep links: ${e}`);
    }
  }

  onDestroy(() => {
    if (cleanupResize) {
      cleanupResize();
    }
    if (sidebarNavIndicatorRaf !== null) {
      cancelAnimationFrame(sidebarNavIndicatorRaf);
      sidebarNavIndicatorRaf = null;
    }
    if (cleanupKeyboard) {
      cleanupKeyboard();
    }
    if (clipboardCheckInterval) {
      clearInterval(clipboardCheckInterval);
    }
    stopUpdateChecker();
    if (unlistenClose) {
      unlistenClose();
    }
    if (unlistenTrayDownload) {
      unlistenTrayDownload();
    }
    if (unlistenNotificationDownload) {
      unlistenNotificationDownload();
    }
    if (unlistenNotificationStartDownload) {
      unlistenNotificationStartDownload();
    }
    if (unlistenWindowShown) {
      unlistenWindowShown();
    }
    if (unlistenDeepLink) {
      unlistenDeepLink();
    }
    if (unlistenExtensionDownload) {
      unlistenExtensionDownload();
    }
    if (unlistenExtensionCancel) {
      unlistenExtensionCancel();
    }
    if (unlistenServerOpen) {
      unlistenServerOpen();
    }
    if (unlistenServerReveal) {
      unlistenServerReveal();
    }
    if (unlistenExtensionCookies) {
      unlistenExtensionCookies();
    }
    if (extensionProgressUnsub) {
      extensionProgressUnsub();
    }
    if (detachLogger) {
      detachLogger();
    }
    if (cleanupShareIntent) {
      cleanupShareIntent();
    }
    if (isAndroid()) {
      cleanupAndroidCallbacks();
    }

    if (broadcastPollTimer) {
      clearInterval(broadcastPollTimer);
      broadcastPollTimer = null;
    }
    if (cleanupBroadcastVisibilityListener) {
      cleanupBroadcastVisibilityListener();
      cleanupBroadcastVisibilityListener = null;
    }
    queue.cleanup();
  });

  async function initStats() {
    logs.info('stats', 'initStats() called');

    if (!$settingsReady) {
      logs.debug('stats', 'Waiting for settings to load...');
      await new Promise<void>((resolve) => {
        const unsub = settingsReady.subscribe((ready) => {
          if (ready) {
            unsub();
            resolve();
          }
        });
      });
    }

    logs.debug('stats', `Settings loaded. sendStats=${$settings.sendStats}`);

    await maybeBackfillStatsFromHistory();
    setupBroadcastPolling();
    fetchBroadcasts();

    if (!$settings.sendStats) {
      logs.info('stats', 'Stats disabled in settings, skipping');
      return;
    }

    const lastSyncKey = 'comine_last_stats_sync';
    const lastSyncTime = localStorage.getItem(lastSyncKey);
    const now = Date.now();
    if (lastSyncTime && now - parseInt(lastSyncTime) < 3600000) {
      logs.debug(
        'stats',
        `Rate limited - last sync was ${Math.round((now - parseInt(lastSyncTime)) / 60000)}min ago`
      );
      return;
    }

    const payload = appStats.getPayload();
    logs.info('stats', `Sending stats: ${JSON.stringify(payload)}`);
    fetch('https://stats.comine.app/', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    })
      .then((res) => {
        logs.info('stats', `Stats sent! Response: ${res.status}`);
      })
      .catch((e) => {
        logs.warn('stats', `Failed to send stats: ${e}`);
      });

    localStorage.setItem(lastSyncKey, now.toString());
  }

  async function maybeBackfillStatsFromHistory() {
    if (!browser) return;

    const version = getAppVersionForBroadcast();
    const migrationKey = 'comine_stats_history_backfill_v1';
    const already = localStorage.getItem(migrationKey);
    if (already === version) return;

    try {
      const items = await history.getItems();
      const totalSuccessfulDownloads = items.length;
      const totalSizeBytes = items.reduce((sum, item) => sum + (item.size || 0), 0);

      appStats.mergeFromHistory({ totalSuccessfulDownloads, totalSizeBytes });
      localStorage.setItem(migrationKey, version);

      logs.info(
        'stats',
        `Backfilled stats from history (v${version}): ${totalSuccessfulDownloads} downloads`
      );
    } catch (e) {
      logs.debug('stats', `History backfill skipped/failed: ${e}`);
    }
  }

  function setupBroadcastPolling() {
    if (!browser) return;
    if (broadcastPollTimer) return;

    broadcastPollTimer = setInterval(() => {
      if (document.visibilityState === 'visible') {
        fetchBroadcasts();
      }
    }, BROADCAST_POLL_INTERVAL_MS);

    const onVisibility = () => {
      if (document.visibilityState === 'visible') {
        fetchBroadcasts();
      }
    };
    document.addEventListener('visibilitychange', onVisibility);
    cleanupBroadcastVisibilityListener = () => {
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }

  interface Broadcast {
    id: number;
    message: string;
    type: 'info' | 'warning' | 'error' | 'success';
    title?: string;
    icon?: string;
    url?: string;
    button_text?: string;
    platforms?: string;
    min_version?: string;
    max_version?: string;
  }

  async function fetchBroadcasts() {
    if (!browser) return;
    if (document.visibilityState !== 'visible') return;
    if (broadcastsFetchInFlight) return;
    broadcastsFetchInFlight = true;

    const dismissedKey = 'comine_dismissed_broadcasts';
    const dismissedRaw = localStorage.getItem(dismissedKey);
    const dismissed: number[] = dismissedRaw ? JSON.parse(dismissedRaw) : [];

    try {
      const res = await fetch('https://stats.comine.app/broadcast');
      if (!res.ok) {
        logs.debug('stats', `No active broadcasts (${res.status})`);
        return;
      }

      const broadcasts: Broadcast[] = await res.json();
      if (!Array.isArray(broadcasts) || broadcasts.length === 0) {
        logs.debug('stats', 'No broadcasts returned');
        return;
      }

      const platform = getPlatformForBroadcast();
      const version = getAppVersionForBroadcast();

      for (const bc of broadcasts) {
        if (dismissed.includes(bc.id)) {
          logs.debug('stats', `Broadcast ${bc.id} already dismissed`);
          continue;
        }

        if (activeBroadcastNotifIds.has(bc.id)) {
          continue;
        }

        if (bc.platforms) {
          const platforms = bc.platforms.split(',').map((p) => p.trim().toLowerCase());
          if (!platforms.includes('all') && !platforms.includes(platform)) {
            logs.debug('stats', `Broadcast ${bc.id} not for platform ${platform}`);
            continue;
          }
        }

        if (bc.min_version && compareVersions(version, bc.min_version) < 0) {
          logs.debug('stats', `Broadcast ${bc.id} requires min version ${bc.min_version}`);
          continue;
        }
        if (bc.max_version && compareVersions(version, bc.max_version) > 0) {
          logs.debug('stats', `Broadcast ${bc.id} requires max version ${bc.max_version}`);
          continue;
        }

        logs.info('stats', `Showing broadcast ${bc.id}: ${bc.message}`);

        const notifPopup = await import('$lib/components/NotificationPopup.svelte');
        const notifId = notifPopup.show({
          title: bc.title || getBroadcastTitle(bc.type),
          body: bc.message,
          thumbnail: bc.icon,
          duration: 0,
          url: bc.url,
          actionLabel: bc.button_text || (bc.url ? $t('broadcast.learnMore') : undefined),
          onAction: bc.url
            ? () => {
                window.open(bc.url, '_blank');
              }
            : undefined,
          onDismiss: () => {
            const curRaw = localStorage.getItem(dismissedKey);
            const cur: number[] = curRaw ? JSON.parse(curRaw) : [];
            if (!cur.includes(bc.id)) {
              cur.push(bc.id);
              localStorage.setItem(dismissedKey, JSON.stringify(cur));
            }
            activeBroadcastNotifIds.delete(bc.id);
          },
        });

        activeBroadcastNotifIds.set(bc.id, notifId);
      }
    } catch (e) {
      logs.debug('stats', `Failed to fetch broadcasts: ${e}`);
    } finally {
      broadcastsFetchInFlight = false;
    }
  }

  function getBroadcastTitle(type: string): string {
    switch (type) {
      case 'warning':
        return $t('broadcast.warning') || 'Warning';
      case 'error':
        return $t('broadcast.important') || 'Important';
      case 'success':
        return $t('broadcast.success') || 'Good news';
      default:
        return $t('broadcast.announcement') || 'Announcement';
    }
  }

  function getPlatformForBroadcast(): string {
    if (!browser) return 'unknown';
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes('android')) return 'android';
    if (ua.includes('win')) return 'windows';
    if (ua.includes('linux')) return 'linux';
    if (ua.includes('mac')) return 'macos';
    return 'unknown';
  }

  function getAppVersionForBroadcast(): string {
    if (!browser) return '0.0.0';
    // @ts-ignore
    return typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '0.0.0';
  }

  function compareVersions(a: string, b: string): number {
    const partsA = a.split('.').map((n) => parseInt(n) || 0);
    const partsB = b.split('.').map((n) => parseInt(n) || 0);
    for (let i = 0; i < Math.max(partsA.length, partsB.length); i++) {
      const numA = partsA[i] || 0;
      const numB = partsB[i] || 0;
      if (numA > numB) return 1;
      if (numA < numB) return -1;
    }
    return 0;
  }

  function handleAndroidShareIntent(rawUrl: string) {
    const url = cleanUrl(rawUrl);
    logs.info('layout', `Android share intent received: ${url}`);
    if (url) {
      goto(`/?url=${encodeURIComponent(url)}`);
      toast.info($t('clipboard.detected'));
    }
  }

  async function releaseMemoryOnHide() {
    logs.info('layout', 'Window hidden - flushing caches and releasing memory');

    await mediaCache.unload();
    clearAllScrollPositions();
    clearColorCache();
    logs.clearMemory();

    try {
      await invoke('clear_memory_caches');
    } catch (e) {
      logs.warn('layout', `Failed to clear Rust caches: ${e}`);
    }

    navigation.reset();

    logs.info('layout', 'Memory release complete');
  }

  async function onWindowShown() {
    logs.info('layout', 'Window restored - loading caches from disk');
    isWindowHidden = false;

    await mediaCache.load();

    deps.checkAll();
  }

  async function handleCloseRequest() {
    if (!appWindow) return;
    const behavior: CloseBehavior = $settings.closeBehavior || 'tray';

    switch (behavior) {
      case 'close':
        await appWindow.destroy();
        break;
      case 'minimize':
        await appWindow.minimize();
        break;
      case 'tray':
      default:
        if (!hasShownTrayNotification) {
          hasShownTrayNotification = true;
          try {
            const hasPermission = await isPermissionGranted();
            if (hasPermission) {
              sendNotification({
                title: 'Comine',
                body: $t('tray.hiddenToTray'),
              });
            }
          } catch (e) {
            console.warn('Failed to send tray notification:', e);
          }
        }

        isWindowHidden = true;
        await releaseMemoryOnHide();

        await appWindow.hide();
        break;
    }
  }

  function startClipboardWatcher() {
    clipboardCheckInterval = setInterval(async () => {
      if (!$settings.watchClipboard) return;

      try {
        const text = await readText();
        if (!text || text === lastClipboardText) return;

        lastClipboardText = text;
        logs.debug('layout', `Clipboard changed: ${text.substring(0, 100)}...`);

        if (isValidMediaUrl(text, $settings.clipboardPatterns || [])) {
          logs.debug('layout', `Media URL detected: ${text}`);
          await handleDetectedUrl(text);
          return;
        }

        const fileCheck = isDirectFileUrl(text);
        logs.debug(
          'layout',
          `Checking file URL: watchClipboardForFiles=${$settings.watchClipboardForFiles}, isDirectFileUrl=${fileCheck.isFile}`
        );
        if ($settings.watchClipboardForFiles && fileCheck.isFile) {
          logs.info('layout', `Direct file URL detected: ${fileCheck.filename}`);
          await handleDetectedFileUrl(text, fileCheck.filename);
        }
      } catch (err) {
        const errorStr = String(err);
        if (
          errorStr.includes('not available in the requested format') ||
          errorStr.includes('clipboard is empty')
        ) {
          return;
        }
        logs.error('layout', `Clipboard watcher error: ${err}`);
      }
    }, CLIPBOARD_CHECK_INTERVAL);
  }

  async function handleDetectedFileUrl(rawUrl: string, detectedFilename: string | null) {
    if (!appWindow) return;

    if (!$settings.fileDownloadNotifications) return;

    const isVisible = await appWindow.isVisible();
    const isFocused = await appWindow.isFocused();

    if (isVisible && isFocused) {
      toast.info(
        `📋 ${$t('clipboard.fileDetected') || 'File URL detected'}: ${detectedFilename || 'file'}`
      );
      return;
    }

    if (!$settings.notificationsEnabled) return;

    try {
      interface FileUrlInfo {
        isFile: boolean;
        filename: string;
        size: number;
        mimeType: string;
        supportsResume: boolean;
      }

      const fileInfo = await invoke<FileUrlInfo>('check_file_url', {
        url: rawUrl,
        proxyConfig: getProxyConfig(),
      });

      if (!fileInfo.filename && detectedFilename) {
        fileInfo.filename = detectedFilename;
      }

      if (!fileInfo.isFile) {
        logs.debug('layout', `URL is not a file: ${rawUrl}`);
        return;
      }

      logs.info('layout', `File URL detected: ${fileInfo.filename} (${fileInfo.size} bytes)`);

      const currentSettings = getSettings();

      await invoke('show_notification_window', {
        data: {
          title: fileInfo.filename,
          body: formatSize(fileInfo.size),
          thumbnail: null,
          url: rawUrl,
          compact: currentSettings.compactNotifications,
          downloadLabel: $t('notification.downloadButton'),
          dismissLabel: $t('notification.dismissButton'),
          isFile: true,
          fileInfo: fileInfo,
        },
        position: currentSettings.notificationPosition,
        monitor: currentSettings.notificationMonitor,
        offset: currentSettings.notificationOffset,
      });
    } catch (err) {
      logs.warn('layout', `Failed to check file URL: ${err}`);
    }
  }

  async function handleDetectedUrl(rawUrl: string) {
    const url = cleanUrl(rawUrl);

    if (!appWindow) return;
    const isVisible = await appWindow.isVisible();
    const isFocused = await appWindow.isFocused();

    if (isVisible && isFocused) {
      toast.info(`📋 ${$t('clipboard.detected')}`);
      goto(`/?url=${encodeURIComponent(url)}`);
      return;
    }

    if (!$settings.notificationsEnabled) {
      return;
    }

    const fetchingToastId = toast.loading(
      $t('clipboard.fetchingInfo') || 'Fetching media info...',
      url.length > 50 ? url.substring(0, 50) + '...' : url
    );

    const currentSettings = getSettings();
    const isChannel = isLikelyChannel(url);
    const isPlaylist =
      !isChannel && isLikelyPlaylist(url, { ignoreMixes: currentSettings.ignoreMixes });

    try {
      if (isChannel && !isAndroid()) {
        const { info: channelInfo } = await resolveUrl(url, {
          cookies_from_browser: currentSettings.cookiesFromBrowser || null,
          custom_cookies: currentSettings.customCookies || null,
          proxy: convertProxyConfig(getProxyConfig()),
          youtube_player_client: currentSettings.usePlayerClientForExtraction
            ? currentSettings.youtubePlayerClient
            : null,
          flat_playlist: true,
        });

        const channelName =
          channelInfo.channel || channelInfo.uploader || channelInfo.title || 'Channel';
        const handle = channelInfo.channelId ? `@${channelInfo.channelId}` : '';
        const totalCount = channelInfo.playlistCount ?? channelInfo.entries?.length ?? 0;

        logs.info('layout', `Channel info: name=${channelName}, videos=${totalCount}`);

        if (totalCount > 0) {
          logs.info(
            'layout',
            `Showing channel notification: ${channelName} (${totalCount} videos)`
          );

          mediaCache.setPreview(url, {
            title: channelName || undefined,
            thumbnail: channelInfo.thumbnail || undefined,
            author: handle || undefined,
          });

          dismissToast(fetchingToastId);
          await maybeSendClipboardSystemNotification(
            `clipboard:${url}`,
            'Comine • Clipboard',
            `${channelName} (${totalCount} videos)`
          );
          await invoke('show_notification_window', {
            data: {
              title: channelName,
              body: `${totalCount} videos${handle ? ` • ${handle}` : ''}`,
              thumbnail: channelInfo.thumbnail,
              url: url,
              compact: currentSettings.compactNotifications,
              downloadLabel: $t('notification.downloadButton'),
              dismissLabel: $t('notification.dismissButton'),
              isChannel: true,
            },
            position: currentSettings.notificationPosition,
            monitor: currentSettings.notificationMonitor,
            offset: currentSettings.notificationOffset,
          });
          return;
        }
      }

      if (isPlaylist && !isAndroid()) {
        const { info: playlistInfo } = await resolveUrl(url, {
          cookies_from_browser: currentSettings.cookiesFromBrowser || null,
          custom_cookies: currentSettings.customCookies || null,
          proxy: convertProxyConfig(getProxyConfig()),
          youtube_player_client: currentSettings.usePlayerClientForExtraction
            ? currentSettings.youtubePlayerClient
            : null,
          flat_playlist: true,
        });
        const totalCount = playlistInfo.playlistCount ?? playlistInfo.entries?.length ?? 0;
        logs.info(
          'layout',
          `Playlist info: isPlaylist=${playlistInfo.isPlaylist}, title=${playlistInfo.title}, count=${totalCount}`
        );

        if (playlistInfo.isPlaylist && totalCount > 0) {
          logs.info(
            'layout',
            `Showing playlist notification: ${playlistInfo.title} (${totalCount} items}`
          );

          mediaCache.setPreview(url, {
            title: playlistInfo.title || undefined,
            thumbnail: playlistInfo.thumbnail || undefined,
            author: playlistInfo.uploader || undefined,
          });

          dismissToast(fetchingToastId);
          await maybeSendClipboardSystemNotification(
            `clipboard:${url}`,
            'Comine • Clipboard',
            `${playlistInfo.title || $t('playlist.notification.detected')} (${totalCount} ${$t('playlist.videos')})`
          );
          await invoke('show_notification_window', {
            data: {
              title: playlistInfo.title || $t('playlist.notification.detected'),
              body: `${totalCount} ${$t('playlist.videos')}`,
              thumbnail: null,
              url: url,
              compact: currentSettings.compactNotifications,
              downloadLabel: $t('notification.downloadButton'),
              dismissLabel: $t('notification.dismissButton'),
              isPlaylist: true,
            },
            position: currentSettings.notificationPosition,
            monitor: currentSettings.notificationMonitor,
            offset: currentSettings.notificationOffset,
          });
          return;
        }
      }

      const { info: videoInfo } = await resolveUrl(url, {
        cookies_from_browser: currentSettings.cookiesFromBrowser || null,
        custom_cookies: currentSettings.customCookies || null,
        proxy: convertProxyConfig(getProxyConfig()),
        youtube_player_client: currentSettings.usePlayerClientForExtraction
          ? currentSettings.youtubePlayerClient
          : null,
      });

      const originalThumbnailUrl = videoInfo.thumbnail || getQuickThumbnail(url);

      let durationStr = '';
      const duration = videoInfo.duration ? Number(videoInfo.duration) : 0;
      if (duration) {
        const mins = Math.floor(duration / 60);
        const secs = Math.floor(duration % 60);
        durationStr = ` • ${mins}:${secs.toString().padStart(2, '0')}`;
      }

      const isTwitter = /(?:twitter\.com|x\.com)/i.test(url);
      const authorDisplay =
        isTwitter && videoInfo.channelId
          ? `@${videoInfo.channelId}`
          : videoInfo.uploader || videoInfo.channel || '';

      mediaCache.setPreview(url, {
        title: videoInfo.title || undefined,
        thumbnail: originalThumbnailUrl || undefined,
        author: authorDisplay || undefined,
        duration: duration,
      });

      dismissToast(fetchingToastId);

      await maybeSendClipboardSystemNotification(
        `clipboard:${url}`,
        'Comine • Clipboard',
        `${videoInfo.title || $t('notification.mediaDetected')}${authorDisplay ? ` • ${authorDisplay}` : ''}${durationStr}`
      );
      await invoke('show_notification_window', {
        data: {
          title: videoInfo.title || $t('notification.mediaDetected'),
          body: `${authorDisplay}${durationStr}`,
          thumbnail: originalThumbnailUrl,
          url: url,
          compact: currentSettings.compactNotifications,
          downloadLabel: $t('notification.downloadButton'),
          dismissLabel: $t('notification.dismissButton'),
        },
        position: currentSettings.notificationPosition,
        monitor: currentSettings.notificationMonitor,
        offset: currentSettings.notificationOffset,
      });
    } catch (err) {
      console.error('Failed to get video info:', err);
      dismissToast(fetchingToastId);
      const currentSettings = getSettings();
      const quickThumbnail = getQuickThumbnail(url);

      await maybeSendClipboardSystemNotification(
        `clipboard:${url}`,
        'Comine • Clipboard',
        $t('notification.clickToDownload')
      );
      await invoke('show_notification_window', {
        data: {
          title: $t('notification.mediaDetected'),
          body: $t('notification.clickToDownload'),
          thumbnail: quickThumbnail,
          url: url,
          compact: currentSettings.compactNotifications,
          downloadLabel: $t('notification.downloadButton'),
          dismissLabel: $t('notification.dismissButton'),
        },
        position: currentSettings.notificationPosition,
        monitor: currentSettings.notificationMonitor,
        offset: currentSettings.notificationOffset,
      });
    }
  }

  async function downloadFromClipboard() {
    try {
      const text = await readText();
      if (text && isValidMediaUrl(text, $settings.clipboardPatterns || [])) {
        goto(`/?url=${encodeURIComponent(text)}`);
        if (appWindow) {
          await appWindow.show();
          await appWindow.setFocus();
        }
      } else {
        toast.warning($t('clipboard.noValidUrl'));
      }
    } catch (err) {
      toast.error($t('clipboard.error'));
    }
  }

  async function minimizeWindow() {
    if (appWindow) await appWindow.minimize();
  }

  async function maximizeWindow() {
    if (appWindow) await appWindow.toggleMaximize();
  }

  async function closeWindow() {
    if (appWindow) await appWindow.close();
  }

  interface NavItemConfig {
    path: string;
    icon: IconName;
    labelKey: string;
    badge?: number;
  }

  let mainNavItems = $derived<NavItemConfig[]>([
    { path: '/', icon: 'download2', labelKey: 'nav.download' },
    {
      path: '/downloads',
      icon: 'history',
      labelKey: 'nav.downloads',
      badge: $activeDownloadsCount > 0 ? $activeDownloadsCount : undefined,
    },
    { path: '/settings', icon: 'settings', labelKey: 'nav.settings' },
  ]);

  const secondaryNavItems: NavItemConfig[] = [
    { path: '/info', icon: 'info', labelKey: 'nav.info' },
    { path: '/logs', icon: 'text', labelKey: 'nav.logs' },
  ];

  let allNavItems = $derived<NavItemConfig[]>([...mainNavItems, ...secondaryNavItems]);

  let currentPath = $derived($page.url.pathname);

  let sidebarNavEl: HTMLElement | null = $state(null);
  let sidebarNavIndicatorStyle = $state('');
  let sidebarNavIndicatorVisible = $state(false);
  const sidebarNavItemEls = new Map<string, HTMLElement>();
  let sidebarNavIndicatorRaf: number | null = null;

  function registerSidebarNavItem(node: HTMLElement, path: string) {
    sidebarNavItemEls.set(path, node);
    queueSidebarNavIndicatorUpdate();
    return {
      destroy() {
        sidebarNavItemEls.delete(path);
        queueSidebarNavIndicatorUpdate();
      },
    };
  }

  function queueSidebarNavIndicatorUpdate() {
    if (isMobile) return;
    if (sidebarNavIndicatorRaf !== null) cancelAnimationFrame(sidebarNavIndicatorRaf);
    sidebarNavIndicatorRaf = requestAnimationFrame(() => {
      sidebarNavIndicatorRaf = null;
      updateSidebarNavIndicator();
    });
  }

  function updateSidebarNavIndicator() {
    if (isMobile || !sidebarNavEl) {
      sidebarNavIndicatorVisible = false;
      sidebarNavIndicatorStyle = '';
      return;
    }

    const activeEl = sidebarNavItemEls.get(currentPath);
    if (!activeEl) {
      sidebarNavIndicatorVisible = false;
      sidebarNavIndicatorStyle = '';
      return;
    }

    const containerRect = sidebarNavEl.getBoundingClientRect();
    const activeRect = activeEl.getBoundingClientRect();
    const top = Math.round(activeRect.top - containerRect.top);
    const height = Math.round(activeRect.height);

    sidebarNavIndicatorVisible = true;
    sidebarNavIndicatorStyle = `transform: translateY(${top}px); height: ${height}px;`;
  }

  $effect(() => {
    currentPath;
    isMobile;
    allNavItems;
    queueSidebarNavIndicatorUpdate();
  });
</script>

{#if isNotificationWindow || currentPath.startsWith('/notification')}
  {@render children()}
{:else}
  <AccentProvider />
  <BackgroundProvider />
  <SurfaceProvider />
  <div
    class="app"
    class:mobile={isMobile}
    style="--window-tint: {$settings.backgroundType === 'oled' ? 0 : $settings.windowTint / 100};"
  >
    {#if !isMobile}
      <div class="titlebar" data-tauri-drag-region>
        <div class="titlebar-spacer"></div>
        <div class="titlebar-brand" data-tauri-drag-region>
          {#if isDownloading}
            <Icon name="download" size={13} />
            <span class="titlebar-speed">{totalDownloadSpeed}</span>
          {:else}
            <svg class="titlebar-icon" viewBox="0 0 1024 1024" fill="currentColor">
              <path
                fill-rule="evenodd"
                clip-rule="evenodd"
                d="M300.29 223.05L612.418 0L844 298.937L799.054 760.396L472.441 1024L158 592.095L300.29 223.05ZM754.854 722.285C700.283 629.788 671.5 524.355 671.5 416.959V323.5L464.5 633.5L754.854 722.285Z"
              />
            </svg>
            <span class="titlebar-text">comine</span>
          {/if}
        </div>
        <div class="window-controls" data-tauri-drag-region="false">
          <button class="titlebar-btn" onclick={minimizeWindow} use:tooltip={$t('window.minimize')}>
            <Icon name="minimize" size={16} />
          </button>
          <button class="titlebar-btn" onclick={maximizeWindow} use:tooltip={$t('window.maximize')}>
            <Icon name="maximize" size={12} />
          </button>
          <button
            class="titlebar-btn close-btn"
            onclick={closeWindow}
            use:tooltip={$t('window.close')}
          >
            <Icon name="close" size={16} />
          </button>
        </div>
      </div>
    {/if}

    <div class="main-container">
      {#if !isMobile}
        <aside class="sidebar" data-tauri-drag-region>
          <nav class="sidebar-nav" bind:this={sidebarNavEl} data-tauri-drag-region>
            {#if sidebarNavIndicatorVisible}
              <div class="sidebar-nav-active-indicator" style={sidebarNavIndicatorStyle}></div>
            {/if}
            {#each allNavItems as item}
              <NavItem
                href={item.path}
                icon={item.icon}
                title={$t(item.labelKey)}
                active={currentPath === item.path}
                badge={item.badge}
                register={(node) => registerSidebarNavItem(node, item.path)}
              />
            {/each}
          </nav>

          <div class="sidebar-bottom" data-tauri-drag-region>
            <NavItem href="https://t.me/comineapp" icon="telegram" title="Telegram" external />
            <NavItem href="https://discord.gg/8sfk33Kr2A" icon="discord" title="Discord" external />
            <NavItem
              href="https://github.com/nichind/comine"
              icon="github"
              title="GitHub"
              external
            />
          </div>
        </aside>
      {/if}

      <main class="content-area">
        {@render children()}
      </main>
    </div>

    {#if isMobile}
      <nav class="bottom-bar-container">
        <div class="bottom-bar" class:show-labels={$settings.showMobileNavLabels}>
          {#each allNavItems as item}
            {@const isActive = currentPath === item.path}
            <a href={item.path} class="bottom-bar-item" class:active={isActive}>
              <div class="bottom-bar-icon" class:active={isActive}>
                <Icon name={item.icon} size={22} />
                {#if item.badge}
                  <span class="badge">{item.badge}</span>
                {/if}
              </div>
              {#if $settings.showMobileNavLabels}
                <span class="bottom-bar-label">{$t(item.labelKey)}</span>
              {/if}
            </a>
          {/each}
        </div>
      </nav>
    {/if}
  </div>

  <Toast />
  <NotificationPopup />
{/if}

<style>
  :global(*) {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }

  :global(html),
  :global(body) {
    height: 100%;
    width: 100%;
    overflow: hidden;
  }

  :global(body) {
    font-family:
      'Jost',
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      Roboto,
      sans-serif;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(h1) {
    font-family: 'Funnel Display', 'Jost', sans-serif;
    line-height: 1;
  }

  :global(.page) {
    animation: fadeIn 0.2s ease-out;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  :global(.spotlight) {
    --spotlight-x: 50%;
    --spotlight-y: 50%;
    position: relative;
    overflow: hidden;
  }

  :global(.spotlight::before) {
    content: '';
    position: absolute;
    inset: 0;
    background: radial-gradient(
      circle 150px at var(--spotlight-x) var(--spotlight-y),
      rgba(255, 255, 255, 0.15),
      transparent 60%
    );
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.2s;
  }

  :global(.spotlight:hover::before) {
    opacity: 1;
  }

  :global(.spotlight-border) {
    --spotlight-x: 50%;
    --spotlight-y: 50%;
    position: relative;
  }

  :global(.spotlight-border::before) {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    padding: 1px;
    background: radial-gradient(
      circle 100px at var(--spotlight-x) var(--spotlight-y),
      rgba(255, 255, 255, 0.5),
      transparent 50%
    );
    -webkit-mask:
      linear-gradient(#fff 0 0) content-box,
      linear-gradient(#fff 0 0);
    mask:
      linear-gradient(#fff 0 0) content-box,
      linear-gradient(#fff 0 0);
    -webkit-mask-composite: xor;
    mask-composite: exclude;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.3s;
  }

  :global(.spotlight-border:hover::before) {
    opacity: 1;
  }

  .app {
    --page-padding-inline: 16px;
    --page-padding-inline-compact: 8px;
    background: rgba(19, 19, 19, var(--window-tint, 0.48));
    height: 100%;
    width: 100%;
    color: white;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
    display: flex;
    flex-direction: column;
    position: relative;
    overflow: hidden;
  }

  .app::before {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(
      to bottom,
      rgba(0, 0, 0, 1) 0%,
      rgba(0, 0, 0, 0) 50%,
      rgba(0, 0, 0, 1) 100%
    );
    opacity: 0;
    pointer-events: none;
  }

  .titlebar {
    height: 30px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0;
    position: relative;
    z-index: 10;
    user-select: none;
    flex-shrink: 0;
  }

  .titlebar-spacer {
    width: 84px;
  }

  .titlebar-brand {
    display: flex;
    align-items: center;
    gap: 6px;
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
  }

  @keyframes shimmer {
    0%,
    100% {
      background-position: 200% center;
    }
    50% {
      background-position: -200% center;
    }
  }

  .titlebar-icon {
    width: 13px;
    height: 13px;
    flex-shrink: 0;
    fill: rgba(255, 255, 255, 0.7);
    animation: icon-shimmer 3s ease-in-out infinite;
  }

  @keyframes icon-shimmer {
    0%,
    40%,
    60%,
    100% {
      fill: rgba(255, 255, 255, 0.7);
    }
    50% {
      fill: rgba(255, 255, 255, 1);
    }
  }

  .titlebar-text {
    font-family: 'Funnel Display', 'Jost', sans-serif;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.5px;
    background: linear-gradient(
      90deg,
      rgba(255, 255, 255, 0.7) 0%,
      rgba(255, 255, 255, 0.7) 40%,
      rgba(255, 255, 255, 1) 50%,
      rgba(255, 255, 255, 0.7) 60%,
      rgba(255, 255, 255, 0.7) 100%
    );
    background-size: 200% 100%;
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
    animation: shimmer 3s ease-in-out infinite;
  }

  .titlebar-speed {
    font-family: 'Jost', sans-serif;
    font-size: 12px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.8);
    letter-spacing: 0.3px;
  }

  .titlebar-brand :global(svg:first-child:not(.titlebar-icon)) {
    color: var(--accent, #6366f1);
    animation: download-pulse 1.5s ease-in-out infinite;
  }

  @keyframes download-pulse {
    0%,
    100% {
      opacity: 0.7;
    }
    50% {
      opacity: 1;
    }
  }

  .window-controls {
    display: flex;
    padding-right: 1px;
    gap: 0;
  }

  .titlebar-btn {
    width: 36px;
    height: 28px;
    border: none;
    background: transparent;
    color: #e1e1e1;
    cursor: pointer;
    border-radius: var(--radius-sm, 6px);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s ease;
    padding: 0;
  }

  .titlebar-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: #ffffff;
  }

  .titlebar-btn.close-btn:hover {
    background: #ef4444;
    color: #ffffff;
  }

  .main-container {
    flex: 1;
    display: flex;
    position: relative;
    z-index: 1;
    overflow: hidden;
  }

  .sidebar {
    width: 56px;
    background: rgba(255, 255, 255, 0);
    border-right: 1px solid rgba(255, 255, 255, 0);
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }

  .sidebar-nav {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 0 0 8px;
    gap: 4px;
    position: relative;
  }

  .sidebar-nav-active-indicator {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    border-radius: 0 8px 8px 0;
    background: rgba(255, 255, 255, 0.14);
    border-left: 2px solid rgba(255, 255, 255, 0.18);
    transition:
      transform 220ms cubic-bezier(0.2, 0.9, 0.2, 1),
      height 220ms cubic-bezier(0.2, 0.9, 0.2, 1),
      opacity 180ms ease;
    will-change: transform, height;
    z-index: 0;
    pointer-events: none;
  }

  .sidebar-bottom {
    padding: 8px 0;
    border-top: 1px solid rgba(255, 255, 255, 0);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .content-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    position: relative;
    overflow: hidden;
  }

  .bottom-bar-container {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    display: flex;
    justify-content: center;
    padding: 12px;
    padding-bottom: max(12px, env(safe-area-inset-bottom, 12px));
    z-index: 100;
    pointer-events: none;
  }

  .bottom-bar {
    display: flex;
    justify-content: center;
    align-items: center;
    width: 92%;
    max-width: 420px;
    height: 64px;
    background: rgba(20, 20, 24, 0.85);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 24px;
    padding: 0 8px;
    box-shadow:
      0 8px 32px rgba(0, 0, 0, 0.4),
      0 2px 8px rgba(0, 0, 0, 0.2),
      inset 0 1px 0 rgba(255, 255, 255, 0.05);
    pointer-events: auto;
  }

  .bottom-bar.show-labels {
    height: 68px;
    padding: 0 4px;
  }

  .bottom-bar-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    flex: 1 1 0;
    min-width: 0;
    padding: 8px 0;
    color: rgba(255, 255, 255, 0.5);
    text-decoration: none;
    border-radius: 16px;
    transition: all 0.25s cubic-bezier(0.34, 1.56, 0.64, 1);
    position: relative;
    -webkit-tap-highlight-color: transparent;
  }

  .bottom-bar-item:active {
    transform: scale(0.92);
    transition: transform 0.1s ease;
  }

  .bottom-bar-item:hover {
    color: rgba(255, 255, 255, 0.8);
  }

  .bottom-bar-item.active {
    color: #ffffff;
  }

  .bottom-bar-icon {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 32px;
    border-radius: var(--radius-lg, 12px);
    transition: all 0.25s cubic-bezier(0.34, 1.56, 0.64, 1);
  }

  .bottom-bar-icon.active {
    background: var(--accent, #6366f1);
    box-shadow:
      0 4px 12px color-mix(in srgb, var(--accent, #6366f1) 40%, transparent),
      0 0 0 1px rgba(255, 255, 255, 0.1);
    transform: scale(1.05);
  }

  .bottom-bar-label {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.3px;
    opacity: 0.9;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 64px;
    text-align: center;
    transition: opacity 0.2s ease;
  }

  .bottom-bar-item .badge {
    position: absolute;
    top: -2px;
    right: -2px;
    background: var(--accent, #6366f1);
    color: white;
    font-size: 9px;
    font-weight: 700;
    min-width: 16px;
    height: 16px;
    border-radius: var(--radius, 8px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 4px;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3);
    border: 2px solid rgba(20, 20, 24, 0.9);
  }

  .app.mobile {
    border-radius: 0;
    border: none;
  }

  .app.mobile .main-container {
    flex: 1;
    min-height: 0;
  }

  .app.mobile .content-area {
    padding: 0 0 0 8px;
    padding-top: env(safe-area-inset-top, 24px);
  }

  .app.mobile :global(.page) {
    padding: 0 !important;
    padding-bottom: 24px !important;
  }

  .app.mobile :global(.page h1) {
    font-size: 28px !important;
  }

  .app.mobile :global(.page-header) {
    padding-top: 8px;
  }

  :global(*::-webkit-scrollbar) {
    width: 6px;
    height: 6px;
  }

  :global(*::-webkit-scrollbar-track) {
    background: transparent;
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: rgba(255, 255, 255, 0.15);
    border-radius: 3px;
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: rgba(255, 255, 255, 0.25);
  }

  :global(*::-webkit-scrollbar-thumb:active) {
    background: rgba(255, 255, 255, 0.35);
  }

  :global(*::-webkit-scrollbar-corner) {
    background: transparent;
  }

  :global(.has-thumb-accent) {
    --accent: var(--thumb-accent) !important;
    --accent-light: var(--thumb-accent-light) !important;
    --accent-dark: var(--thumb-accent-dark) !important;
    --accent-alpha: var(--thumb-accent-alpha) !important;
    --accent-alpha-hover: var(--thumb-accent-alpha-hover) !important;
    --accent-bg: var(--thumb-accent-alpha) !important;
    --accent-bg-hover: var(--thumb-accent-alpha-hover) !important;
  }
</style>
