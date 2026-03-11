<script lang="ts">
  import { onMount, onDestroy, type Snippet } from 'svelte';

  import { browser } from '$app/environment';
  import { getCurrentWindow, type Window as TauriWindow } from '@tauri-apps/api/window';
  import { listen, emit } from '@tauri-apps/api/event';
  import { readText } from '@tauri-apps/plugin-clipboard-manager';
  import { attachLogger } from '@tauri-apps/plugin-log';
  import { invoke } from '@tauri-apps/api/core';
  import { isPermissionGranted, sendNotification } from '@tauri-apps/plugin-notification';

  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import Icon, { type IconName } from '$lib/components/ui/Icon.svelte';
  import NavItem from '$lib/components/layout/NavItem.svelte';
  import Toast from '$lib/components/ui/Toast.svelte';
  import BackgroundProvider from '$lib/components/providers/BackgroundProvider.svelte';
  import AccentProvider from '$lib/components/providers/AccentProvider.svelte';
  import SurfaceProvider from '$lib/components/providers/SurfaceProvider.svelte';
  import { toast } from '$lib/components/ui/Toast.svelte';
  import MediaPlayer from '$lib/components/player/MediaPlayer.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { t } from '$lib/i18n';
  import {
    initSettings,
    settings,
    settingsReady,
    type CloseBehavior,
    getSettings,
  } from '$lib/stores/settings';
  import { history } from '$lib/stores/history';
  import { queue, activeDownloadsCount } from '$lib/stores/queue';
  import { deps } from '$lib/stores/deps';
  import { logs, type LogLevel } from '$lib/stores/logs';
  import { mediaCache } from '$lib/stores/mediaCache';
  import { clearColorCache } from '$lib/utils/color';
  import { isTypingTarget } from '$lib/utils/keyboard';
  import { cleanUrl, isHttpUrl } from '$lib/utils/urlUtils';
  import { formatSpeed } from '$lib/utils/format';

  import {
    isAndroid,
    isMobile as isMobilePlatform,
    onShareIntent,
    onNavigateTo,
    setupAndroidLogHandler,
  } from '$lib/utils/android';
  import {
    startUpdateChecker,
    stopUpdateChecker,
    clearDismissedVersionIfUpdated,
  } from '$lib/stores/updates';
  import { listenForDepUpdates, stopDepUpdateListener } from '$lib/stores/deps';
  import { navigation } from '$lib/stores/navigation';
  import NotificationPopup from '$lib/components/layout/NotificationPopup.svelte';
  import { initRemoteSync } from '$lib/composables/remoteSync';
  import { setupExtensionBridge } from '$lib/composables/extensionBridge';
  import {
    downloadFromClipboard,
    setupClipboardWatcher,
    cleanupClipboardListeners,
  } from '$lib/composables/clipboardHandler';

  let { children }: { children: Snippet } = $props();

  let totalDownloadSpeed = $derived.by(() => {
    let totalBps = 0;
    for (const item of $queue.items) {
      if (item.status === 'downloading' && item.speedBps) {
        totalBps += item.speedBps;
      }
    }
    return totalBps > 0 ? formatSpeed(totalBps) : '';
  });

  let isDownloading = $derived($activeDownloadsCount > 0 && totalDownloadSpeed !== '');

  let resolvedControlsStyle = $derived.by(() => {
    const style = $settings.windowControlsStyle;
    if (style !== 'auto') return style;
    const ua = navigator.userAgent.toLowerCase();
    return ua.includes('mac') ? 'macos' : 'windows';
  });

  let isNotificationWindow = $derived(
    browser && window.location.pathname.startsWith('/notification')
  );

  let appWindow: TauriWindow | null = $state(null);

  let isMobile = $derived(
    $settings.navigationStyle === 'navbar' ||
      ($settings.navigationStyle === 'auto' && isMobilePlatform())
  );

  let hasShownTrayNotification = false;
  let isWindowHidden = $state(false);

  // Single cleanup array replaces 12+ separate unlistenFn variables
  const cleanups: (() => void)[] = [];

  function waitForSettings(): Promise<void> {
    if ($settingsReady) return Promise.resolve();
    return new Promise<void>((resolve) => {
      const unsub = settingsReady.subscribe((ready) => {
        if (ready) {
          unsub();
          resolve();
        }
      });
    });
  }

  async function focusAppWindow(): Promise<void> {
    if (!appWindow) return;
    try {
      await appWindow.unminimize();
      await appWindow.show();
      await appWindow.setFocus();
    } catch (e) {
      logs.warn('layout', `Failed to show/focus window: ${e}`);
    }
  }

  function setMediaPreview(
    url: string,
    metadata?: { title?: string | null; thumbnail?: string | null; uploader?: string | null } | null
  ): void {
    if (!metadata?.title && !metadata?.thumbnail && !metadata?.uploader) return;
    mediaCache.setPreview(url, {
      title: metadata.title || undefined,
      thumbnail: metadata.thumbnail || undefined,
      author: metadata.uploader || undefined,
    });
  }

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
        const idx = Math.max(0, pages.indexOf(currentPath));
        const next = (idx + (e.shiftKey ? pages.length - 1 : 1)) % pages.length;
        goto(pages[next]);
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
    cleanups.push(() => window.removeEventListener('keydown', handleKeyDown));
  }

  let diskSpaceWarningShown = false;

  async function autoStartExtensionServer() {
    await waitForSettings();

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

    await waitForSettings();

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
    const isNotificationRoute = window.location.pathname.startsWith('/notification');

    const splash = document.getElementById('splash-screen');
    if (splash) {
      if (isNotificationRoute) {
        splash.remove();
        ensureAppRootVisible();
      } else {
        if (!isMobilePlatform()) {
          try {
            emit('frontend-ready');
          } catch (e) {}
        }

        const icon = document.getElementById('splash-icon');
        const startShrink = () => {
          if (!icon) {
            splash.remove();
            ensureAppRootVisible();
            return;
          }
          icon.classList.add('animate');
          setTimeout(() => ensureAppRootVisible(), 500);
          const onEnd = () => {
            icon.removeEventListener('animationend', onEnd);
            splash.remove();
          };
          icon.addEventListener('animationend', onEnd);
          setTimeout(() => {
            splash.remove();
            ensureAppRootVisible();
          }, 900);
        };

        let started = false;
        let splashUnlisten: (() => void) | null = null;
        listen('window-shown', () => {
          if (started) return;
          started = true;
          splashUnlisten?.();
          requestAnimationFrame(() => startShrink());
        }).then((fn) => {
          if (started) {
            fn();
          } else {
            splashUnlisten = fn;
          }
        });
        setTimeout(() => {
          if (started) return;
          started = true;
          splashUnlisten?.();
          startShrink();
        }, 300);
      }
    } else {
      ensureAppRootVisible();
    }

    if (isNotificationRoute) {
      return;
    }

    appWindow = getCurrentWindow();

    initSettings();
    queue.init();

    autoStartExtensionServer();

    setTimeout(async () => {
      await deps.checkAll();
      if (!isMobilePlatform()) {
        await deps.autoInstallBundle();
        checkDiskSpace();
      }
    }, 1500);

    if (!isMobile) {
      queueSidebarNavIndicatorUpdate();
    }

    setupListeners();

    if (!isMobilePlatform()) {
      setupKeyboardShortcuts();
    }

    setupLogForwarding();

    if (!isMobilePlatform()) {
      setupClipboardWatcher();
    }

    startUpdateChecker();
    listenForDepUpdates();
    clearDismissedVersionIfUpdated();
    initRemoteSyncComposable();

    if (isAndroid()) {
      cleanups.push(onShareIntent(handleAndroidShareIntent));
      cleanups.push(onNavigateTo(handleAndroidNavigateTo));

      setupAndroidLogHandler((level, source, message) => {
        logs.log(level, source, message);
      });
      logs.info('system', 'Android log forwarding initialized');
    }
  });

  const LOG_LEVELS: LogLevel[] = ['info', 'error', 'warn', 'info', 'debug', 'trace'];
  function levelNumberToLogLevel(level: number): LogLevel {
    return LOG_LEVELS[level] ?? 'info';
  }

  // Matches: [date][time][target][LEVEL] ...  OR  module::sub::[LEVEL] ...
  const LOG_PREFIX_RE =
    /^(?:\[\d{4}-\d{2}-\d{2}\]\[\d{2}:\d{2}:\d{2}\])?(?:\[([^\]]+)\]\[([A-Z]+)\]\s*)?/;

  async function setupLogForwarding() {
    try {
      const detach = await attachLogger(({ level, message }) => {
        const levelStr = levelNumberToLogLevel(level);
        const m = message.match(LOG_PREFIX_RE);
        let source = 'rust';
        let msg = message;

        if (m && m[0].length > 0) {
          msg = message.substring(m[0].length).trim();
          if (m[1]) source = m[1].split('::').pop()?.split(' ').pop() || 'rust';
        } else {
          const colonIdx = message.indexOf('::');
          if (colonIdx > 0 && colonIdx < 40) {
            source = message.substring(0, colonIdx).split('_').pop() || 'rust';
            msg = message
              .substring(colonIdx + 2)
              .replace(/^\[[A-Z]+\]\s*/, '')
              .trim();
          }
        }

        logs.log(levelStr, source, msg);
      });
      cleanups.push(detach);
      logs.info('system', 'Backend log forwarding initialized');
    } catch (e) {
      console.error('Failed to attach logger:', e);
    }
  }

  async function setupListeners() {
    const ulClose = await listen('close-requested', async () => {
      await handleCloseRequest();
    });
    cleanups.push(ulClose);

    const ulTray = await listen('tray-download-clipboard', async () => {
      await downloadFromClipboard();
    });
    cleanups.push(ulTray);

    const ulNotifDl = await listen<string>('notification-download', async (event) => {
      const url = cleanUrl(event.payload);
      if (url) {
        goto(`/?url=${encodeURIComponent(url)}`);
      }
    });
    cleanups.push(ulNotifDl);

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

    const ulNotifStartDl = await listen<NotificationPayload>(
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
          await focusAppWindow();
          setMediaPreview(url, metadata);

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
          await focusAppWindow();
          setMediaPreview(url, metadata);

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
          await focusAppWindow();
          setMediaPreview(url, metadata);

          navigation.openVideo(url, {
            title: metadata?.title || undefined,
            thumbnail: metadata?.thumbnail || undefined,
            author: metadata?.uploader || undefined,
          });
          await goto('/');
          return;
        }

        if (!isMobilePlatform()) {
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

    cleanups.push(ulNotifStartDl);

    // Download status toasts (moved from queue/eventHandler to separate UI from state)
    const ulDlStatus = await listen<{
      url: string;
      status: string;
      error?: string;
      title?: string;
    }>('download-status-changed', (event) => {
      const { status: dlStatus, error } = event.payload;
      if (dlStatus === 'completed') {
        toast.success($t('downloads.status.completed'));
      } else if (dlStatus === 'failed' && error) {
        toast.error(`Download failed: ${error}`);
      }
    });
    cleanups.push(ulDlStatus);

    const ulPodcastStarted = await listen<{ historyId: string }>('podcast-generation-started', () => {
      toast.info('Generating podcast...');
    });
    cleanups.push(ulPodcastStarted);

    const ulPodcastProgress = await listen<{
      historyId: string;
      step: string;
      progress: number;
      error: string | null;
    }>('podcast-generation-progress', (event) => {
      const { step, error } = event.payload;
      if (step === 'fetching_transcript') {
        toast.info('Podcast: fetching transcript...');
      } else if (step === 'generating_script') {
        toast.info('Podcast: generating script...');
      } else if (step === 'narrating') {
        toast.info('Podcast: narrating...');
      } else if (step === 'mastering') {
        toast.info('Podcast: mastering audio...');
      } else if (step === 'complete') {
        toast.success('Podcast generated!');
      } else if (step === 'failed') {
        toast.error('Podcast generation failed: ' + error);
      }
    });
    cleanups.push(ulPodcastProgress);

    const ulPodcastCompleted = await listen<{ historyId: string; podcastPath: string }>(
      'podcast-generation-completed',
      () => {
        toast.success('Podcast ready!');
      },
    );
    cleanups.push(ulPodcastCompleted);

    const ulPodcastFailed = await listen<{ historyId: string; error: string }>(
      'podcast-generation-failed',
      (event) => {
        toast.error('Podcast failed: ' + event.payload.error);
      },
    );
    cleanups.push(ulPodcastFailed);

    const ulShown = await listen('window-shown', () => {
      onWindowShown();
    });
    cleanups.push(ulShown);

    const ulHidden = await listen('window-hidden', () => {
      isWindowHidden = true;
    });
    cleanups.push(ulHidden);

    const ulDeepLink = await listen<string>('deep-link-url', async (event) => {
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
            await appWindow.unminimize();
          } catch {}
        }
        await focusAppWindow();
      }
    });

    cleanups.push(ulDeepLink);

    const extBridgeCleanup = await setupExtensionBridge({
      getAppWindow: () => appWindow,
      onWindowShown,
    });
    cleanups.push(extBridgeCleanup);

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
    if (sidebarNavIndicatorRaf !== null) {
      cancelAnimationFrame(sidebarNavIndicatorRaf);
      sidebarNavIndicatorRaf = null;
    }
    cleanupClipboardListeners();
    stopUpdateChecker();
    stopDepUpdateListener();
    for (const fn of cleanups) fn();
    cleanups.length = 0;
    queue.cleanup();
    history.cleanup();
  });

  async function initRemoteSyncComposable() {
    await waitForSettings();

    const { show } = await import('$lib/components/layout/NotificationPopup.svelte');
    const cleanup = await initRemoteSync({
      showNotification: show,
    });
    if (cleanup) cleanups.push(cleanup);
  }

  function handleAndroidShareIntent(rawUrl: string) {
    const url = cleanUrl(rawUrl);
    logs.info('layout', `Android share intent received: ${url}`);
    if (url) {
      goto(`/?url=${encodeURIComponent(url)}`);
      toast.info($t('clipboard.detected'));
    }
  }

  function handleAndroidNavigateTo(path: string) {
    if (path) goto(`/${path}`);
  }

  async function releaseMemoryOnHide() {
    logs.info('layout', 'Window hidden - flushing caches and releasing memory');

    await mediaCache.unload();
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
    // Skip during initial splash animation — splash logic handles first reveal
    if (document.getElementById('splash-screen')) return;
    logs.info('layout', 'Window restored - loading caches from disk');
    ensureAppRootVisible();
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

        await releaseMemoryOnHide();
        await appWindow.destroy();
        break;
    }
  }

  function ensureAppRootVisible() {
    const appRoot = document.getElementById('app-root');
    if (appRoot) {
      appRoot.classList.add('is-visible');
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

  $effect(() => {
    $settings.compactSidebar;
    // Wait for CSS transition to finish before recalculating indicator position
    const timer = setTimeout(() => queueSidebarNavIndicatorUpdate(), 250);
    return () => clearTimeout(timer);
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
    class:macos-window={resolvedControlsStyle === 'macos'}
    style="--window-tint: {$settings.backgroundType === 'oled' ? 0 : $settings.windowTint / 100};"
  >
    {#if !isMobile}
      <div
        class="titlebar"
        class:titlebar-macos={resolvedControlsStyle === 'macos'}
        data-tauri-drag-region
      >
        {#if resolvedControlsStyle === 'macos'}
          <div class="traffic-lights" data-tauri-drag-region="false">
            <button
              class="traffic-light traffic-close"
              onclick={closeWindow}
              aria-label={$t('window.close')}
              use:tooltip={$t('window.close')}
            >
              <svg viewBox="0 0 12 12"
                ><path
                  d="M3.172 3.172a.5.5 0 0 1 .707 0L6 5.293l2.121-2.121a.5.5 0 1 1 .707.707L6.707 6l2.121 2.121a.5.5 0 0 1-.707.707L6 6.707 3.879 8.828a.5.5 0 1 1-.707-.707L5.293 6 3.172 3.879a.5.5 0 0 1 0-.707Z"
                  fill="currentColor"
                /></svg
              >
            </button>
            <button
              class="traffic-light traffic-minimize"
              onclick={minimizeWindow}
              aria-label={$t('window.minimize')}
              use:tooltip={$t('window.minimize')}
            >
              <svg viewBox="0 0 12 12"
                ><rect x="2" y="5.5" width="8" height="1" rx=".5" fill="currentColor" /></svg
              >
            </button>
            <button
              class="traffic-light traffic-maximize"
              onclick={maximizeWindow}
              aria-label={$t('window.maximize')}
              use:tooltip={$t('window.maximize')}
            >
              <svg viewBox="0 0 12 12"
                ><path
                  d="M3.5 8.5V5a1.5 1.5 0 0 1 1.5-1.5h3.5"
                  stroke="currentColor"
                  stroke-width="1"
                  fill="none"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                /><path
                  d="M8.5 3.5 3.5 8.5"
                  stroke="currentColor"
                  stroke-width="1"
                  stroke-linecap="round"
                /></svg
              >
            </button>
          </div>
        {:else}
          <div class="titlebar-spacer"></div>
        {/if}
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
        {#if resolvedControlsStyle === 'macos'}
          <div class="titlebar-spacer"></div>
        {:else}
          <div class="window-controls" data-tauri-drag-region="false">
            <button
              class="titlebar-btn"
              onclick={minimizeWindow}
              use:tooltip={$t('window.minimize')}
            >
              <Icon name="minimize" size={16} />
            </button>
            <button
              class="titlebar-btn"
              onclick={maximizeWindow}
              use:tooltip={$t('window.maximize')}
            >
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
        {/if}
      </div>
    {/if}

    <div class="main-container">
      {#if !isMobile}
        <aside class="sidebar" class:compact={$settings.compactSidebar} data-tauri-drag-region>
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
                compact={$settings.compactSidebar}
                register={(node) => registerSidebarNavItem(node, item.path)}
              />
            {/each}
          </nav>

          <div class="sidebar-bottom" data-tauri-drag-region>
            <NavItem
              href="https://t.me/comineapp"
              icon="telegram"
              title="Telegram"
              external
              compact={$settings.compactSidebar}
            />
            <NavItem
              href="https://discord.gg/8sfk33Kr2A"
              icon="discord"
              title="Discord"
              external
              compact={$settings.compactSidebar}
            />
            <NavItem
              href="https://github.com/nichind/comine"
              icon="github"
              title="GitHub"
              external
              compact={$settings.compactSidebar}
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
  <MediaPlayer />
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
    --page-padding-inline: 14px;
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

  .app.macos-window {
    border-radius: 12px;
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
    transition: fill 0.3s ease;
  }

  .titlebar-brand:hover .titlebar-icon {
    fill: rgba(255, 255, 255, 1);
  }

  .titlebar-text {
    font-family: 'Funnel Display', 'Jost', sans-serif;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.5px;
    color: rgba(255, 255, 255, 0.7);
    transition: color 0.3s ease;
  }

  .titlebar-brand:hover .titlebar-text {
    color: rgba(255, 255, 255, 1);
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

  .titlebar-macos {
    justify-content: space-between;
  }

  .traffic-lights {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 14px;
    height: 100%;
  }

  .traffic-light {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: none;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    transition: filter 0.15s ease;
    color: rgba(0, 0, 0, 0.5);
  }

  .traffic-light svg {
    width: 8px;
    height: 8px;
    opacity: 0;
    transition: opacity 0.1s ease;
    flex-shrink: 0;
  }

  .traffic-lights:hover .traffic-light svg {
    opacity: 1;
  }

  .traffic-close {
    background: #ff5f57;
  }

  .traffic-close:hover {
    filter: brightness(0.85);
  }

  .traffic-minimize {
    background: #ffbd2e;
  }

  .traffic-minimize:hover {
    filter: brightness(0.85);
  }

  .traffic-maximize {
    background: #28c840;
  }

  .traffic-maximize:hover {
    filter: brightness(0.85);
  }

  .traffic-light:active {
    filter: brightness(0.7);
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
    transition: width 0.2s ease;
  }

  .sidebar.compact {
    width: 46px;
  }

  .sidebar-nav {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 0 0 8px;
    gap: 4px;
    position: relative;
    transition: gap 0.2s ease;
  }

  .sidebar.compact .sidebar-nav {
    gap: 2px;
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
    z-index: 0;
    pointer-events: none;
  }

  .sidebar.compact .sidebar-nav-active-indicator {
    border-radius: 0 6px 6px 0;
  }

  .sidebar-bottom {
    padding: 8px 0;
    border-top: 1px solid rgba(255, 255, 255, 0);
    display: flex;
    flex-direction: column;
    gap: 4px;
    transition:
      padding 0.2s ease,
      gap 0.2s ease;
  }

  .sidebar.compact .sidebar-bottom {
    padding: 6px 0;
    gap: 2px;
  }

  .sidebar.compact :global(.nav-item) {
    width: 46px;
    height: 42px;
  }

  .sidebar.compact :global(.nav-item .badge) {
    bottom: 4px;
    right: 4px;
    min-width: 14px;
    height: 14px;
    font-size: 9px;
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
    --mobile-nav-clearance: 96px;
    padding: 0 0 0 0;
    padding-top: env(safe-area-inset-top, 24px);
  }

  .app.mobile :global(.page-shell) {
    padding: 0 0 24px var(--page-padding-inline-compact) !important;
  }

  .app.mobile :global(.page-shell.no-padding) {
    padding-left: 0 !important;
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
