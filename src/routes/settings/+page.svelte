<script lang="ts">
  import { t, locale, setLocale, locales, type Locale } from '$lib/i18n';
  import {
    settings,
    updateSetting,
    settingsReady,
    defaultSettings,
    resetSettings,
    type NotificationPosition,
    type NotificationMonitor,
    type BackgroundType,
    type ToastPosition,
    type ProxyMode,
  } from '$lib/stores/settings';
  import { history } from '$lib/stores/history';
  import { deps, type DependencyName } from '$lib/stores/deps';
  import { onMount } from 'svelte';
  import { beforeNavigate } from '$app/navigation';
  import { save, open } from '@tauri-apps/plugin-dialog';
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { writeTextFile, readTextFile, readFile, stat } from '@tauri-apps/plugin-fs';
  import SettingsBlock from '$lib/components/SettingsBlock.svelte';
  import SettingItem from '$lib/components/SettingItem.svelte';
  import Divider from '$lib/components/Divider.svelte';
  import Input from '$lib/components/Input.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import Checkbox from '$lib/components/Checkbox.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import Select from '$lib/components/Select.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import Button from '$lib/components/Button.svelte';
  import ScrollArea from '$lib/components/ScrollArea.svelte';
  import ExtensionIntegrationSettings from '$lib/components/ExtensionIntegrationSettings.svelte';
  import { toast, updateToast, dismissToast } from '$lib/components/Toast.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { isAndroid, isDesktop } from '$lib/utils/android';
  import {
    updateState,
    checkForUpdates,
    downloadAndInstall,
    getCurrentVersion,
  } from '$lib/stores/updates';
  import { saveScrollPosition, getScrollPosition } from '$lib/stores/scroll';

  const ROUTE_PATH = '/settings';

  let scrollAreaRef: ScrollArea | undefined = $state(undefined);

  beforeNavigate(() => {
    const pos = scrollAreaRef?.getScroll() ?? 0;
    saveScrollPosition(ROUTE_PATH, pos);
  });

  let searchQuery = $state('');

  let onAndroid = $state(false);
  let onDesktop = $state(true);
  let platform = $state<'windows' | 'macos' | 'linux' | 'android'>('windows');

  let backgroundVideoInput = $state('');
  let backgroundImageInput = $state('');

  let customProxyInput = $state('');
  let proxyValidationError = $state<string | null>(null);
  let systemProxyStatus = $state<string | null>(null);
  let detectingSystemProxy = $state(false);
  let currentIp = $state<string | null>(null);
  let checkingIp = $state(false);
  let ipProxyUsed = $state(false);

  let sliderDebounceTimers = new Map<string, ReturnType<typeof setTimeout>>();

  function debouncedSliderUpdate<K extends keyof typeof $settings>(
    key: K,
    value: (typeof $settings)[K],
    delay: number = 150
  ) {
    settings.update((s) => ({ ...s, [key]: value }));

    const existingTimer = sliderDebounceTimers.get(key);
    if (existingTimer) {
      clearTimeout(existingTimer);
    }

    sliderDebounceTimers.set(
      key,
      setTimeout(() => {
        updateSetting(key, value);
        sliderDebounceTimers.delete(key);
      }, delay)
    );
  }

  // Track toast IDs for each dependency being installed
  let depToastIds = $state<Map<DependencyName, number>>(new Map());

  // Wrapper function for installing dependencies with toast progress
  async function installDepWithToast(
    depName: DependencyName,
    installFn: () => Promise<boolean>,
    displayName: string
  ) {
    // Show initial progress toast
    const toastId = toast.progress($t('deps.installing', { component: displayName }), 0);
    depToastIds.set(depName, toastId);

    // Start installation
    const success = await installFn();

    // Dismiss the progress toast
    dismissToast(toastId);
    depToastIds.delete(depName);

    // Show result toast
    if (success) {
      toast.success($t('deps.installed', { component: displayName }));
    } else {
      toast.error($t('deps.installFailed', { component: displayName }));
    }
  }

  // Watch install progress and update toasts
  $effect(() => {
    for (const [depName, toastId] of depToastIds) {
      const progress = $deps.installProgressMap.get(depName);
      if (progress) {
        updateToast(toastId, {
          progress: progress.progress,
          subMessage: progress.message || progress.stage,
        });
      }
    }
  });

  onMount(() => {
    onAndroid = isAndroid();
    onDesktop = isDesktop();

    // Detect platform from navigator
    if (onDesktop && typeof navigator !== 'undefined') {
      const userAgent = navigator.userAgent.toLowerCase();
      if (userAgent.includes('mac')) {
        platform = 'macos';
      } else if (userAgent.includes('linux') && !userAgent.includes('android')) {
        platform = 'linux';
      } else {
        platform = 'windows';
      }
    } else {
      platform = 'android';
    }

    backgroundVideoInput = $settings.backgroundVideo || '';
    backgroundImageInput = $settings.backgroundImage || '';

    customProxyInput = $settings.customProxyUrl || '';

    if (onDesktop) {
      detectSystemProxy();
    }

    const savedPosition = getScrollPosition(ROUTE_PATH);
    if (savedPosition > 0) {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          scrollAreaRef?.restoreScroll(savedPosition);
        });
      });
    }
  });

  const settingItems = {
    language: { section: 'general', keywords: ['language', 'язык', 'idioma', 'locale'] },
    startOnBoot: {
      section: 'general',
      keywords: ['start', 'boot', 'startup', 'autostart', 'запуск'],
    },
    startMinimized: {
      section: 'general',
      keywords: ['start', 'minimized', 'tray', 'hidden', 'свёрнуто', 'скрыто'],
    },
    watchClipboard: { section: 'general', keywords: ['clipboard', 'watch', 'paste', 'буфер'] },
    statusPopup: { section: 'general', keywords: ['status', 'popup', 'notification', 'статус'] },
    notificationsEnabled: {
      section: 'notifications',
      keywords: ['notification', 'popup', 'alert', 'уведомление'],
    },
    notificationPosition: {
      section: 'notifications',
      keywords: ['position', 'corner', 'location', 'позиция'],
    },
    notificationMonitor: {
      section: 'notifications',
      keywords: ['monitor', 'screen', 'display', 'монитор'],
    },
    compactNotifications: {
      section: 'notifications',
      keywords: ['compact', 'small', 'mini', 'icon', 'компактный'],
    },
    notificationFancyBackground: {
      section: 'notifications',
      keywords: ['fancy', 'background', 'blur', 'transparent', 'glass', 'фон', 'прозрачный'],
    },
    notificationThumbnailTheming: {
      section: 'notifications',
      keywords: ['thumbnail', 'theming', 'color', 'tint', 'миниатюра', 'цвет'],
    },
    defaultProcessor: {
      section: 'processing',
      keywords: ['processor', 'backend', 'cobalt', 'yt-dlp', 'lux', 'auto', 'download'],
    },
    youtubePlayerClient: {
      section: 'processing',
      keywords: ['youtube', 'player', 'client', 'extractor', 'android', 'tv', 'web', 'mweb', 'fix'],
    },
    usePlayerClientForExtraction: {
      section: 'processing',
      keywords: ['youtube', 'player', 'client', 'extraction', 'info', 'formats', 'playlist'],
    },
    downloadPath: {
      section: 'downloads',
      keywords: ['download', 'path', 'folder', 'directory', 'save', 'location', 'путь', 'папка'],
    },
    useAudioPath: {
      section: 'downloads',
      keywords: ['audio', 'music', 'separate', 'folder', 'path', 'аудио', 'музыка'],
    },
    usePlaylistFolders: {
      section: 'downloads',
      keywords: ['playlist', 'folder', 'subfolder', 'organize', 'плейлист', 'папка'],
    },
    youtubeMusicAudioOnly: {
      section: 'downloads',
      keywords: ['youtube', 'music', 'audio', 'only', 'аудио', 'музыка'],
    },
    embedThumbnail: {
      section: 'downloads',
      keywords: ['thumbnail', 'cover', 'art', 'image', 'embed', 'audio', 'обложка', 'миниатюра'],
    },
    concurrentDownloads: {
      section: 'downloads',
      keywords: [
        'concurrent',
        'parallel',
        'simultaneous',
        'downloads',
        'speed',
        'multiple',
        'queue',
        'параллельные',
        'одновременные',
      ],
    },
    watchClipboardForFiles: {
      section: 'downloads',
      keywords: ['clipboard', 'file', 'detect', 'direct', 'url', 'буфер', 'файл'],
    },
    fileDownloadNotifications: {
      section: 'downloads',
      keywords: ['notification', 'file', 'download', 'alert', 'уведомление', 'файл'],
    },
    aria2Connections: {
      section: 'downloads',
      keywords: ['aria2', 'connections', 'parallel', 'speed', 'соединения'],
    },
    aria2Splits: {
      section: 'downloads',
      keywords: ['aria2', 'splits', 'chunks', 'pieces', 'части'],
    },
    aria2DisableIPv6: {
      section: 'downloads',
      keywords: ['aria2', 'ipv6', 'ipv4', 'network', 'disable', 'сеть'],
    },
    aria2CustomArgs: {
      section: 'downloads',
      keywords: ['aria2', 'custom', 'args', 'arguments', 'advanced', 'аргументы'],
    },
    downloadSpeedLimit: {
      section: 'downloads',
      keywords: ['speed', 'limit', 'throttle', 'bandwidth', 'скорость', 'лимит'],
    },
    autoUpdate: { section: 'app', keywords: ['update', 'auto', 'automatic', 'обновление'] },
    sendStats: {
      section: 'app',
      keywords: ['stats', 'statistics', 'analytics', 'telemetry', 'статистика'],
    },
    background: {
      section: 'app',
      keywords: [
        'background',
        'acrylic',
        'blur',
        'transparency',
        'video',
        'animated',
        'solid',
        'color',
        'image',
        'фон',
      ],
    },
    accentColor: {
      section: 'app',
      keywords: [
        'accent',
        'color',
        'theme',
        'buttons',
        'highlight',
        'rgb',
        'rainbow',
        'акцент',
        'цвет',
        'тема',
      ],
    },
    accentStyle: {
      section: 'app',
      keywords: ['accent', 'style', 'solid', 'gradient', 'glow', 'стиль', 'эффект'],
    },
    toastPosition: {
      section: 'app',
      keywords: [
        'toast',
        'position',
        'corner',
        'location',
        'in-app',
        'notification',
        'внутри',
        'позиция',
      ],
    },
    disableAnimations: {
      section: 'app',
      keywords: ['animation', 'disable', 'motion', 'reduce', 'анимация'],
    },
    sizeUnit: {
      section: 'app',
      keywords: ['size', 'unit', 'kb', 'mb', 'binary', 'decimal', 'bytes', 'размер'],
    },
    showHistoryStats: {
      section: 'app',
      keywords: ['history', 'stats', 'statistics', 'downloads', 'статистика', 'история'],
    },
    thumbnailTheming: {
      section: 'app',
      keywords: [
        'thumbnail',
        'color',
        'theming',
        'progress',
        'dynamic',
        'миниатюра',
        'цвет',
        'тема',
      ],
    },
    builderThumbnailGlow: {
      section: 'app',
      keywords: [
        'thumbnail',
        'glow',
        'theming',
        'builder',
        'track',
        'playlist',
        'channel',
        'dynamic',
        'свечение',
        'фон',
      ],
    },
    ytdlp: {
      section: 'deps',
      keywords: ['yt-dlp', 'ytdlp', 'dependency', 'download', 'зависимость'],
    },
    proxy: {
      section: 'network',
      keywords: ['proxy', 'network', 'http', 'socks', 'vpn', 'прокси', 'сеть'],
    },
    localServer: {
      section: 'localServer',
      keywords: [
        'local',
        'server',
        'extension',
        'browser',
        'localhost',
        'локальный',
        'сервер',
        'расширение',
      ],
    },
    sync: {
      section: 'sync',
      keywords: [
        'sync',
        'remote',
        'extension',
        'browser',
        'pair',
        'pairing',
        'device',
        'chrome',
        'firefox',
        'relay',
        'расширение',
        'синхронизация',
        'устройство',
      ],
    },
  };

  function matchesSearch(settingKey: string): boolean {
    if (!searchQuery.trim()) return true;
    const query = searchQuery.toLowerCase();
    const item = settingItems[settingKey as keyof typeof settingItems];
    if (!item) return true;
    return (
      item.keywords.some((kw) => kw.includes(query)) ||
      $t(`settings.${item.section}.${settingKey === 'language' ? 'language' : settingKey}`)
        .toLowerCase()
        .includes(query)
    );
  }

  function sectionHasMatches(section: string): boolean {
    if (!searchQuery.trim()) return true;
    return Object.entries(settingItems)
      .filter(([_, item]) => item.section === section)
      .some(([key, _]) => matchesSearch(key));
  }

  function isSectionModified(section: string): boolean {
    return Object.entries(settingItems)
      .filter(([_, item]) => item.section === section)
      .some(([key, _]) => {
        if (key === 'language') return $locale !== defaultSettings.language;
        // @ts-ignore
        return $settings[key] !== defaultSettings[key];
      });
  }

  const languageOptions = locales.map((l) => ({
    value: l.code,
    label: l.nativeName,
  }));

  function handleLanguageChange(value: string) {
    setLocale(value as Locale);
    updateSetting('language', value);
  }

  async function handleStartOnBootChange(checked: boolean) {
    try {
      if (checked) {
        await invoke('autostart_enable');
      } else {
        await invoke('autostart_disable');
      }
      updateSetting('startOnBoot', checked);
    } catch (err) {
      console.error('Failed to update autostart:', err);
      toast.error($t('settings.general.autoStartError'));
    }
  }

  function handleStartMinimizedChange(checked: boolean) {
    updateSetting('startMinimized', checked);
  }

  function handleWatchClipboardChange(checked: boolean) {
    updateSetting('watchClipboard', checked);
  }

  function handleStatusPopupChange(checked: boolean) {
    updateSetting('statusPopup', checked);
  }

  function handleAutoUpdateChange(checked: boolean) {
    updateSetting('autoUpdate', checked);
  }

  function handleSendStatsChange(checked: boolean) {
    updateSetting('sendStats', checked);
  }

  function handleBackgroundTypeChange(value: string) {
    updateSetting('backgroundType', value as BackgroundType);
  }

  function handleBackgroundColorChange(value: string) {
    updateSetting('backgroundColor', value);
  }

  function handleBackgroundImageChange(value: string) {
    updateSetting('backgroundImage', value);
  }

  function handleBackgroundVideoChange(value: string) {
    updateSetting('backgroundVideo', value);
  }

  function handleBackgroundBlurChange(value: number) {
    debouncedSliderUpdate('backgroundBlur', value);
  }

  function handleBackgroundOpacityChange(value: number) {
    debouncedSliderUpdate('backgroundOpacity', value);
  }

  function handleWindowTintChange(value: number) {
    debouncedSliderUpdate('windowTint', value);
  }

  function isWindowEffect(type: string): boolean {
    return (
      type === 'acrylic' ||
      type === 'blur' ||
      type === 'mica' ||
      type === 'mica-dark' ||
      type === 'mica-light' ||
      type === 'tabbed' ||
      type === 'tabbed-dark' ||
      type === 'tabbed-light' ||
      type.startsWith('vibrancy-')
    );
  }

  function handleAccentColorChange(value: string) {
    updateSetting('accentColor', value);
  }

  function handleUseSystemAccentChange(checked: boolean) {
    updateSetting('useSystemAccent', checked);
  }

  async function filePathToUrl(filePath: string, _mimeType: string): Promise<string> {
    return convertFileSrc(filePath);
  }

  function formatFileSize(bytes: number): string {
    if (bytes < 1024) return bytes + ' B';
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
    return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
  }

  async function processVideoFile(filePath: string, mimeType: string) {
    const assetUrl = await filePathToUrl(filePath, mimeType);
    backgroundVideoInput = assetUrl;
    handleBackgroundVideoChange(assetUrl);
  }

  async function confirmLargeFile() {
    if (pendingVideoFile) {
      await processVideoFile(pendingVideoFile.path, pendingVideoFile.mimeType);
      pendingVideoFile = null;
    }
    showLargeFileWarning = false;
  }

  function cancelLargeFile() {
    pendingVideoFile = null;
    showLargeFileWarning = false;
  }

  async function pickBackgroundVideo() {
    const result = await open({
      multiple: false,
      filters: [{ name: 'Video', extensions: ['mp4', 'webm', 'mkv', 'mov', 'avi'] }],
    });
    if (result) {
      const filePath = result as string;

      const fileStat = await stat(filePath);
      const fileSize = fileStat.size;

      const ext = filePath.split('.').pop()?.toLowerCase() || 'mp4';
      const mimeType =
        ext === 'webm'
          ? 'video/webm'
          : ext === 'mkv'
            ? 'video/x-matroska'
            : ext === 'mov'
              ? 'video/quicktime'
              : ext === 'avi'
                ? 'video/x-msvideo'
                : 'video/mp4';

      if (fileSize > LARGE_FILE_THRESHOLD) {
        pendingVideoFile = { path: filePath, mimeType, size: fileSize };
        showLargeFileWarning = true;
        return;
      }

      await processVideoFile(filePath, mimeType);
    }
  }

  async function pickBackgroundImage() {
    const result = await open({
      multiple: false,
      filters: [{ name: 'Image', extensions: ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp'] }],
    });
    if (result) {
      const filePath = result as string;
      const ext = filePath.split('.').pop()?.toLowerCase() || 'png';
      const mimeType =
        ext === 'jpg' || ext === 'jpeg'
          ? 'image/jpeg'
          : ext === 'gif'
            ? 'image/gif'
            : ext === 'webp'
              ? 'image/webp'
              : ext === 'bmp'
                ? 'image/bmp'
                : 'image/png';
      const assetUrl = await filePathToUrl(filePath, mimeType);
      backgroundImageInput = assetUrl;
      handleBackgroundImageChange(assetUrl);
    }
  }

  async function pickDownloadPath() {
    const result = await open({
      multiple: false,
      directory: true,
    });
    if (result) {
      updateSetting('downloadPath', result as string);
    }
  }

  async function pickAudioPath() {
    const result = await open({
      multiple: false,
      directory: true,
    });
    if (result) {
      updateSetting('audioPath', result as string);
    }
  }

  function handleDisableAnimationsChange(checked: boolean) {
    updateSetting('disableAnimations', checked);
  }

  function handleSizeUnitChange(value: string) {
    updateSetting('sizeUnit', value as 'binary' | 'decimal');
  }

  function handleCloseBehaviorChange(value: string) {
    updateSetting('closeBehavior', value as 'close' | 'minimize' | 'tray');
  }

  function handleNotificationsEnabledChange(checked: boolean) {
    updateSetting('notificationsEnabled', checked);
  }

  async function handleNotificationPositionChange(value: string) {
    try {
      await invoke('close_all_notifications');
    } catch (err) {
      console.error('Failed to close notifications:', err);
    }
    updateSetting('notificationPosition', value as NotificationPosition);
  }

  function handleNotificationMonitorChange(value: string) {
    updateSetting('notificationMonitor', value as NotificationMonitor);
  }

  function handleToastPositionChange(value: string) {
    updateSetting('toastPosition', value as ToastPosition);
  }

  let backgroundTypeOptions = $derived.by(() => {
    const baseOptions = [
      { value: 'animated', label: $t('settings.app.backgroundAnimated') },
      { value: 'solid', label: $t('settings.app.backgroundSolid') },
      { value: 'oled', label: $t('settings.app.backgroundOled') },
      { value: 'image', label: $t('settings.app.backgroundImage') },
    ];

    if (!onDesktop) return baseOptions;

    if (platform === 'windows') {
      return [
        { value: 'mica', label: $t('settings.app.backgroundMica') },
        { value: 'mica-dark', label: $t('settings.app.backgroundMicaDark') },
        { value: 'mica-light', label: $t('settings.app.backgroundMicaLight') },
        { value: 'acrylic', label: $t('settings.app.backgroundAcrylic') },
        { value: 'blur', label: $t('settings.app.backgroundBlur') },
        { value: 'tabbed', label: $t('settings.app.backgroundTabbed') },
        { value: 'tabbed-dark', label: $t('settings.app.backgroundTabbedDark') },
        { value: 'tabbed-light', label: $t('settings.app.backgroundTabbedLight') },
        ...baseOptions,
      ];
    } else if (platform === 'macos') {
      return [
        { value: 'vibrancy-sidebar', label: $t('settings.app.backgroundVibrancySidebar') },
        { value: 'vibrancy-hud', label: $t('settings.app.backgroundVibrancyHud') },
        { value: 'vibrancy-window', label: $t('settings.app.backgroundVibrancyWindow') },
        { value: 'vibrancy-popover', label: $t('settings.app.backgroundVibrancyPopover') },
        { value: 'vibrancy-menu', label: $t('settings.app.backgroundVibrancyMenu') },
        { value: 'vibrancy-content', label: $t('settings.app.backgroundVibrancyContent') },
        { value: 'vibrancy-under-window', label: $t('settings.app.backgroundVibrancyUnderWindow') },
        ...baseOptions,
      ];
    }

    return baseOptions;
  });

  let sizeUnitOptions = $derived([
    { value: 'binary', label: $t('settings.app.sizeUnitBinary') },
    { value: 'decimal', label: $t('settings.app.sizeUnitDecimal') },
  ]);

  let closeBehaviorOptions = $derived([
    { value: 'tray', label: $t('settings.general.closeBehaviorTray') },
    { value: 'minimize', label: $t('settings.general.closeBehaviorMinimize') },
    { value: 'close', label: $t('settings.general.closeBehaviorClose') },
  ]);

  let notificationPositionOptions = $derived([
    { value: 'bottom-right', label: $t('settings.notifications.positionBottomRight') },
    { value: 'bottom-left', label: $t('settings.notifications.positionBottomLeft') },
    { value: 'bottom-center', label: $t('settings.notifications.positionBottomCenter') },
    { value: 'top-right', label: $t('settings.notifications.positionTopRight') },
    { value: 'top-left', label: $t('settings.notifications.positionTopLeft') },
    { value: 'top-center', label: $t('settings.notifications.positionTopCenter') },
  ]);

  let toastPositionOptions = $derived([
    { value: 'bottom-right', label: $t('settings.notifications.positionBottomRight') },
    { value: 'bottom-left', label: $t('settings.notifications.positionBottomLeft') },
    { value: 'bottom-center', label: $t('settings.notifications.positionBottomCenter') },
    { value: 'top-right', label: $t('settings.notifications.positionTopRight') },
    { value: 'top-left', label: $t('settings.notifications.positionTopLeft') },
    { value: 'top-center', label: $t('settings.notifications.positionTopCenter') },
  ]);

  let notificationMonitorOptions = $derived([
    { value: 'primary', label: $t('settings.notifications.monitorPrimary') },
    { value: 'cursor', label: $t('settings.notifications.monitorCursor') },
  ]);

  let proxyModeOptions = $derived([
    { value: 'none', label: $t('settings.network.proxyModeNone') },
    { value: 'system', label: $t('settings.network.proxyModeSystem') },
    { value: 'custom', label: $t('settings.network.proxyModeCustom') },
  ]);

  let showResetModal = $state(false);
  let showClearHistoryModal = $state(false);
  let importMessage = $state<{ type: 'success' | 'error'; text: string } | null>(null);

  let showLargeFileWarning = $state(false);
  let pendingVideoFile = $state<{ path: string; mimeType: string; size: number } | null>(null);
  const LARGE_FILE_THRESHOLD = 20 * 1024 * 1024;

  async function handleResetSettings() {
    await resetSettings();
    setLocale(defaultSettings.language as Locale);
    showResetModal = false;
  }

  async function handleClearHistory() {
    await history.clear();
    showClearHistoryModal = false;
  }

  async function handleExportHistory() {
    const items = await history.getItems();
    if (items.length === 0) {
      importMessage = { type: 'error', text: $t('settings.data.noHistoryToExport') };
      setTimeout(() => (importMessage = null), 3000);
      return;
    }

    try {
      const filePath = await save({
        defaultPath: `comine-history-${new Date().toISOString().split('T')[0]}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });

      if (filePath) {
        const data = await history.exportData();
        await writeTextFile(filePath, data);
        importMessage = { type: 'success', text: $t('settings.data.exportSuccess') };
        setTimeout(() => (importMessage = null), 3000);
      }
    } catch (err) {
      console.error('Export failed:', err);
      importMessage = { type: 'error', text: $t('settings.data.exportError') };
      setTimeout(() => (importMessage = null), 3000);
    }
  }

  async function handleImportHistory() {
    try {
      const filePath = await open({
        filters: [{ name: 'JSON', extensions: ['json'] }],
        multiple: false,
      });

      if (filePath && typeof filePath === 'string') {
        const text = await readTextFile(filePath);
        const success = await history.importData(text);
        if (success) {
          importMessage = { type: 'success', text: $t('settings.data.importSuccess') };
        } else {
          importMessage = { type: 'error', text: $t('settings.data.importError') };
        }
        setTimeout(() => (importMessage = null), 3000);
      }
    } catch (err) {
      console.error('Import failed:', err);
      importMessage = { type: 'error', text: $t('settings.data.importError') };
      setTimeout(() => (importMessage = null), 3000);
    }
  }

  function undoLanguage() {
    setLocale(defaultSettings.language as Locale);
    updateSetting('language', defaultSettings.language);
  }

  let clearingCache = $state(false);
  let checkingUpdate = $state(false);

  async function handleCheckForUpdates() {
    checkingUpdate = true;
    try {
      const update = await checkForUpdates(true);
      if (update) {
        toast.success($t('settings.app.updateAvailable', { version: update.version }));
      } else {
        toast.info($t('settings.app.noUpdates'));
      }
    } catch (e) {
      toast.error($t('settings.app.updateCheckFailed'));
    } finally {
      checkingUpdate = false;
    }
  }

  async function handleClearCache() {
    if (!onDesktop) return;
    clearingCache = true;
    try {
      // Clear disk cache
      const deleted = await invoke<number>('clear_cache');
      // Clear Rust memory caches
      await invoke('clear_memory_caches');
      // Clear frontend caches
      const { clearAllFrontendCaches } = await import('$lib/stores/viewState');
      clearAllFrontendCaches();

      if (deleted > 0) {
        toast.success($t('settings.data.clearCacheSuccess'));
      } else {
        toast.info($t('settings.data.clearCacheEmpty'));
      }
    } catch (err) {
      console.error('Failed to clear cache:', err);
    } finally {
      clearingCache = false;
    }
  }

  function handleProxyModeChange(value: string) {
    updateSetting('proxyMode', value as ProxyMode);
  }

  function validateProxyUrl(url: string): boolean {
    if (!url.trim()) return true;

    const proxyRegex =
      /^(https?|socks5?):\/\/([a-zA-Z0-9.-]+|\[[a-fA-F0-9:]+\])(:\d{1,5})?(\/.*)?$/;
    return proxyRegex.test(url.trim());
  }

  function handleCustomProxyInput(value: string) {
    customProxyInput = value;

    if (!value.trim()) {
      proxyValidationError = null;
      updateSetting('customProxyUrl', '');
      return;
    }

    if (validateProxyUrl(value)) {
      proxyValidationError = null;
      updateSetting('customProxyUrl', value.trim());
    } else {
      proxyValidationError = $t('settings.network.proxyInvalid');
    }
  }

  async function detectSystemProxy() {
    if (!onDesktop) return;

    detectingSystemProxy = true;
    systemProxyStatus = null;

    try {
      const result = await invoke<{ url: string; source: string; description: string }>(
        'detect_system_proxy'
      );
      if (result?.url && result.url.length > 0) {
        systemProxyStatus = `${result.url} (${result.source})`;
      } else {
        systemProxyStatus = $t('settings.network.noSystemProxy');
      }
    } catch (err) {
      console.error('Failed to detect system proxy:', err);
      systemProxyStatus = $t('settings.network.noSystemProxy');
    } finally {
      detectingSystemProxy = false;
    }
  }

  async function checkIp() {
    checkingIp = true;
    currentIp = null;

    try {
      const result = await invoke<{ ip: string; proxyUsed: boolean; proxySource: string }>(
        'check_ip',
        {
          proxyConfig: {
            mode: $settings.proxyMode,
            customUrl: $settings.customProxyUrl,
            retryWithoutProxy: $settings.retryWithoutProxy,
          },
        }
      );
      currentIp = result.ip;
      ipProxyUsed = result.proxyUsed;
    } catch (err) {
      console.error('Failed to check IP:', err);
      currentIp = $t('settings.network.ipCheckFailed');
    } finally {
      checkingIp = false;
    }
  }
</script>

<div class="page">
  <ScrollArea bind:this={scrollAreaRef}>
    <div class="page-header">
      <h1>{$t('settings.title')}</h1>
      <p class="subtitle">{$t('settings.subtitle')}</p>
    </div>

    <!-- Search Bar (matching downloads page style) -->
    <div class="search-bar">
      <Icon name="search" size={18} />
      <input type="text" placeholder={$t('settings.search.placeholder')} bind:value={searchQuery} />
    </div>

    <div class="settings-content">
      <!-- General Section -->
      {#if sectionHasMatches('general')}
        <SettingsBlock
          title={$t('settings.general.title')}
          icon="settings"
          onResetSection={isSectionModified('general')
            ? async () => {
                undoLanguage();
                if (onDesktop) {
                  await handleStartOnBootChange(defaultSettings.startOnBoot);
                  updateSetting('startMinimized', defaultSettings.startMinimized);
                  updateSetting('watchClipboard', defaultSettings.watchClipboard);
                  updateSetting('statusPopup', defaultSettings.statusPopup);
                  updateSetting('closeBehavior', defaultSettings.closeBehavior);
                }
              }
            : undefined}
        >
          <!-- Language Selector -->
          {#if matchesSearch('language')}
            <SettingItem
              title={$t('settings.general.language')}
              icon="globe"
              value={$locale}
              defaultValue={defaultSettings.language}
              onReset={undoLanguage}
            >
              <div style="width: 200px;">
                <Select options={languageOptions} value={$locale} onchange={handleLanguageChange} />
              </div>
            </SettingItem>
          {/if}

          {#if matchesSearch('startOnBoot') && onDesktop}
            <SettingItem
              title={$t('settings.general.startOnBoot')}
              description={$t('settings.general.startOnBootDescription')}
              icon="run"
              value={$settings.startOnBoot}
              defaultValue={defaultSettings.startOnBoot}
              onReset={async () => {
                await handleStartOnBootChange(defaultSettings.startOnBoot);
              }}
            >
              <Toggle checked={$settings.startOnBoot} onchange={handleStartOnBootChange} />
            </SettingItem>
          {/if}

          {#if matchesSearch('startMinimized') && onDesktop && $settings.startOnBoot}
            <SettingItem
              title={$t('settings.general.startMinimized')}
              description={$t('settings.general.startMinimizedDescription')}
              icon="minimize"
              value={$settings.startMinimized}
              defaultValue={defaultSettings.startMinimized}
              onReset={() => updateSetting('startMinimized', defaultSettings.startMinimized)}
            >
              <Toggle checked={$settings.startMinimized} onchange={handleStartMinimizedChange} />
            </SettingItem>
          {/if}

          {#if matchesSearch('watchClipboard') && onDesktop}
            <SettingItem
              title={$t('settings.general.watchClipboard')}
              description={$t('settings.general.watchClipboardTooltip')}
              icon="clipboard"
              value={$settings.watchClipboard}
              defaultValue={defaultSettings.watchClipboard}
              onReset={() => updateSetting('watchClipboard', defaultSettings.watchClipboard)}
            >
              <Toggle checked={$settings.watchClipboard} onchange={handleWatchClipboardChange} />
            </SettingItem>
          {/if}

          {#if matchesSearch('statusPopup') && onDesktop}
            <SettingItem
              title={$t('settings.general.statusPopup')}
              icon="bell"
              value={$settings.statusPopup}
              defaultValue={defaultSettings.statusPopup}
              onReset={() => updateSetting('statusPopup', defaultSettings.statusPopup)}
            >
              <Toggle checked={$settings.statusPopup} onchange={handleStatusPopupChange} />
            </SettingItem>
          {/if}

          <!-- Close behavior (desktop only) -->
          {#if onDesktop}
            <SettingItem
              title={$t('settings.general.closeBehavior')}
              description={$t('settings.general.closeBehaviorDescription')}
              icon="close"
              value={$settings.closeBehavior}
              defaultValue={defaultSettings.closeBehavior}
              onReset={() => updateSetting('closeBehavior', defaultSettings.closeBehavior)}
            >
              <div style="width: 220px;">
                <Select
                  options={closeBehaviorOptions}
                  value={$settings.closeBehavior}
                  onchange={handleCloseBehaviorChange}
                />
              </div>
            </SettingItem>
          {/if}
        </SettingsBlock>
      {/if}

      <!-- Notifications Section (desktop only - Android uses system notifications) -->
      {#if sectionHasMatches('notifications') && onDesktop}
        <SettingsBlock
          title={$t('settings.notifications.title')}
          icon="bell"
          onResetSection={isSectionModified('notifications')
            ? () => {
                updateSetting('notificationsEnabled', defaultSettings.notificationsEnabled);
                updateSetting('notificationPosition', defaultSettings.notificationPosition);
                updateSetting('notificationMonitor', defaultSettings.notificationMonitor);
                updateSetting('compactNotifications', defaultSettings.compactNotifications);
                updateSetting(
                  'notificationFancyBackground',
                  defaultSettings.notificationFancyBackground
                );
                updateSetting(
                  'notificationThumbnailTheming',
                  defaultSettings.notificationThumbnailTheming
                );
                updateSetting(
                  'notificationCornerDismiss',
                  defaultSettings.notificationCornerDismiss
                );
                updateSetting('notificationOffset', defaultSettings.notificationOffset);
                updateSetting('notificationDuration', defaultSettings.notificationDuration);
                updateSetting('notificationShowProgress', defaultSettings.notificationShowProgress);
              }
            : undefined}
        >
          {#if matchesSearch('notificationsEnabled')}
            <SettingItem
              title={$t('settings.notifications.enabled')}
              description={$t('settings.notifications.enabledTooltip')}
              icon="bell"
              value={$settings.notificationsEnabled}
              defaultValue={defaultSettings.notificationsEnabled}
              onReset={() =>
                updateSetting('notificationsEnabled', defaultSettings.notificationsEnabled)}
            >
              <Toggle
                checked={$settings.notificationsEnabled}
                onchange={handleNotificationsEnabledChange}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('notificationPosition')}
            <SettingItem
              title={$t('settings.notifications.position')}
              description={$t('settings.notifications.positionDescription')}
              icon="widgets"
              value={$settings.notificationPosition}
              defaultValue={defaultSettings.notificationPosition}
              onReset={() =>
                updateSetting('notificationPosition', defaultSettings.notificationPosition)}
            >
              <div style="width: 180px;">
                <Select
                  options={notificationPositionOptions}
                  value={$settings.notificationPosition}
                  onchange={handleNotificationPositionChange}
                />
              </div>
            </SettingItem>
          {/if}

          {#if matchesSearch('notificationMonitor')}
            <SettingItem
              title={$t('settings.notifications.monitor')}
              description={$t('settings.notifications.monitorDescription')}
              icon="cursor"
              value={$settings.notificationMonitor}
              defaultValue={defaultSettings.notificationMonitor}
              onReset={() =>
                updateSetting('notificationMonitor', defaultSettings.notificationMonitor)}
            >
              <div style="width: 180px;">
                <Select
                  options={notificationMonitorOptions}
                  value={$settings.notificationMonitor}
                  onchange={handleNotificationMonitorChange}
                />
              </div>
            </SettingItem>
          {/if}

          {#if matchesSearch('compactNotifications')}
            <SettingItem
              title={$t('settings.notifications.compact')}
              description={$t('settings.notifications.compactTooltip')}
              icon="minimize_square"
              value={$settings.compactNotifications}
              defaultValue={defaultSettings.compactNotifications}
              onReset={() =>
                updateSetting('compactNotifications', defaultSettings.compactNotifications)}
            >
              <Toggle
                checked={$settings.compactNotifications}
                onchange={(checked) => updateSetting('compactNotifications', checked)}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('notificationFancyBackground')}
            <SettingItem
              title={$t('settings.notifications.fancyBackground')}
              description={$t('settings.notifications.fancyBackgroundTooltip')}
              icon="image"
              value={$settings.notificationFancyBackground}
              defaultValue={defaultSettings.notificationFancyBackground}
              onReset={() =>
                updateSetting(
                  'notificationFancyBackground',
                  defaultSettings.notificationFancyBackground
                )}
            >
              <Toggle
                checked={$settings.notificationFancyBackground}
                onchange={(checked) => updateSetting('notificationFancyBackground', checked)}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('notificationThumbnailTheming')}
            <SettingItem
              title={$t('settings.notifications.thumbnailTheming')}
              description={$t('settings.notifications.thumbnailThemingTooltip')}
              icon="image"
              value={$settings.notificationThumbnailTheming}
              defaultValue={defaultSettings.notificationThumbnailTheming}
              onReset={() =>
                updateSetting(
                  'notificationThumbnailTheming',
                  defaultSettings.notificationThumbnailTheming
                )}
            >
              <Toggle
                checked={$settings.notificationThumbnailTheming}
                onchange={(checked) => updateSetting('notificationThumbnailTheming', checked)}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('notificationCornerDismiss')}
            <SettingItem
              title={$t('settings.notifications.cornerDismiss')}
              description={$t('settings.notifications.cornerDismissTooltip')}
              icon="cross_circle"
              value={$settings.notificationCornerDismiss}
              defaultValue={defaultSettings.notificationCornerDismiss}
              onReset={() =>
                updateSetting(
                  'notificationCornerDismiss',
                  defaultSettings.notificationCornerDismiss
                )}
            >
              <Toggle
                checked={$settings.notificationCornerDismiss}
                onchange={(checked) => updateSetting('notificationCornerDismiss', checked)}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('notificationOffset')}
            <SettingItem
              title={$t('settings.notifications.offset')}
              description={$t('settings.notifications.offsetDescription')}
              icon="sort_vertical"
              value={$settings.notificationOffset}
              defaultValue={defaultSettings.notificationOffset}
              onReset={() =>
                updateSetting('notificationOffset', defaultSettings.notificationOffset)}
            >
              <div class="slider-with-value">
                <input
                  type="range"
                  class="blur-slider"
                  min="0"
                  max="200"
                  step="4"
                  value={$settings.notificationOffset}
                  oninput={(e) =>
                    debouncedSliderUpdate(
                      'notificationOffset',
                      parseInt((e.target as HTMLInputElement).value)
                    )}
                />
                <span class="slider-value">{$settings.notificationOffset}px</span>
              </div>
            </SettingItem>
          {/if}

          {#if matchesSearch('notificationDuration')}
            <SettingItem
              title={$t('settings.notifications.duration')}
              description={$t('settings.notifications.durationDescription')}
              icon="clock"
              value={$settings.notificationDuration}
              defaultValue={defaultSettings.notificationDuration}
              onReset={() =>
                updateSetting('notificationDuration', defaultSettings.notificationDuration)}
            >
              <div class="slider-with-value">
                <input
                  type="range"
                  class="blur-slider"
                  min="3"
                  max="30"
                  step="1"
                  value={$settings.notificationDuration}
                  oninput={(e) =>
                    debouncedSliderUpdate(
                      'notificationDuration',
                      parseInt((e.target as HTMLInputElement).value)
                    )}
                />
                <span class="slider-value">{$settings.notificationDuration}s</span>
              </div>
            </SettingItem>
          {/if}

          {#if matchesSearch('notificationShowProgress')}
            <SettingItem
              title={$t('settings.notifications.showProgress')}
              description={$t('settings.notifications.showProgressTooltip')}
              icon="download"
              value={$settings.notificationShowProgress}
              defaultValue={defaultSettings.notificationShowProgress}
              onReset={() =>
                updateSetting('notificationShowProgress', defaultSettings.notificationShowProgress)}
            >
              <Toggle
                checked={$settings.notificationShowProgress}
                onchange={() =>
                  updateSetting('notificationShowProgress', !$settings.notificationShowProgress)}
              />
            </SettingItem>
          {/if}
        </SettingsBlock>
      {/if}

      <!-- Processing Section -->
      {#if sectionHasMatches('processing')}
        <SettingsBlock
          title={$t('settings.processing.title')}
          icon="server"
          onResetSection={isSectionModified('processing')
            ? () => {
                updateSetting('defaultProcessor', defaultSettings.defaultProcessor);
                updateSetting('youtubePlayerClient', defaultSettings.youtubePlayerClient);
                updateSetting(
                  'usePlayerClientForExtraction',
                  defaultSettings.usePlayerClientForExtraction
                );
              }
            : undefined}
        >
          {#if matchesSearch('defaultProcessor')}
            {#snippet processorHint()}
              {#if $settings.defaultProcessor === 'auto'}
                {#if onAndroid}
                  <div class="setting-hint">Uses yt-dlp for all downloads.</div>
                {:else}
                  <div class="setting-hint">
                    Detects Chinese platforms (Bilibili, Douyin) for Lux, others use yt-dlp.
                  </div>
                {/if}
              {:else if $settings.defaultProcessor === 'lux' && !onAndroid}
                <div class="setting-hint">
                  Optimized for Chinese platforms only. Western sites may not work.
                </div>
              {:else}
                <div class="setting-hint">
                  Supports 1000+ sites. Some Chinese platforms work better with Lux.
                </div>
              {/if}
            {/snippet}

            <SettingItem
              title={$t('settings.processing.defaultProcessor')}
              description={$t('settings.processing.defaultProcessorDescription')}
              icon="server"
              value={$settings.defaultProcessor}
              defaultValue={defaultSettings.defaultProcessor}
              onReset={() => updateSetting('defaultProcessor', defaultSettings.defaultProcessor)}
              descriptionSnippet={processorHint}
            >
              <div style="width: 180px;">
                <Select
                  value={$settings.defaultProcessor}
                  onchange={(val) => updateSetting('defaultProcessor', val as any)}
                  options={onAndroid
                    ? [
                        { value: 'auto', label: $t('settings.processing.auto') },
                        { value: 'yt-dlp', label: 'yt-dlp' },
                      ]
                    : [
                        { value: 'auto', label: $t('settings.processing.auto') },
                        { value: 'yt-dlp', label: 'yt-dlp' },
                        { value: 'lux', label: 'Lux' },
                      ]}
                />
              </div>
            </SettingItem>
          {/if}

          <!-- YouTube Player Client -->
          {#if matchesSearch('youtubePlayerClient')}
            <SettingItem
              title={$t('settings.processing.youtubePlayerClient')}
              description={$t('settings.processing.youtubePlayerClientDescription')}
              icon="tuning"
              value={$settings.youtubePlayerClient}
              defaultValue={defaultSettings.youtubePlayerClient}
              onReset={() =>
                updateSetting('youtubePlayerClient', defaultSettings.youtubePlayerClient)}
            >
              <div style="width: 220px;">
                <Input
                  value={$settings.youtubePlayerClient}
                  placeholder="tv,android_sdkless"
                  oninput={(e) =>
                    updateSetting('youtubePlayerClient', (e.target as HTMLInputElement).value)}
                />
              </div>
            </SettingItem>
          {/if}

          <!-- Use Player Client For Extraction -->
          {#if matchesSearch('usePlayerClientForExtraction')}
            <SettingItem
              title={$t('settings.processing.usePlayerClientForExtraction')}
              description={$t('settings.processing.usePlayerClientForExtractionDescription')}
              icon="link"
              value={$settings.usePlayerClientForExtraction}
              defaultValue={defaultSettings.usePlayerClientForExtraction}
              onReset={() =>
                updateSetting(
                  'usePlayerClientForExtraction',
                  defaultSettings.usePlayerClientForExtraction
                )}
            >
              <Toggle
                checked={$settings.usePlayerClientForExtraction}
                onchange={(checked) => updateSetting('usePlayerClientForExtraction', checked)}
              />
            </SettingItem>
          {/if}

          <!-- Extraction Player Client -->
          {#if matchesSearch('extractionPlayerClient') && !$settings.usePlayerClientForExtraction}
            <SettingItem
              title={$t('settings.processing.extractionPlayerClient')}
              description={$t('settings.processing.extractionPlayerClientDescription')}
              icon="tuning"
              value={$settings.extractionPlayerClient}
              defaultValue={defaultSettings.extractionPlayerClient}
              onReset={() =>
                updateSetting('extractionPlayerClient', defaultSettings.extractionPlayerClient)}
            >
              <div style="width: 220px;">
                <Input
                  value={$settings.extractionPlayerClient}
                  placeholder="android_sdkless,web_safari"
                  oninput={(e) =>
                    updateSetting('extractionPlayerClient', (e.target as HTMLInputElement).value)}
                />
              </div>
            </SettingItem>
          {/if}
        </SettingsBlock>
      {/if}

      <!-- Downloads Section -->
      {#if sectionHasMatches('downloads')}
        <SettingsBlock
          title={$t('settings.downloads.title')}
          icon="download"
          onResetSection={isSectionModified('downloads')
            ? () => {
                updateSetting('downloadPath', defaultSettings.downloadPath);
                updateSetting('useAudioPath', defaultSettings.useAudioPath);
                updateSetting('audioPath', defaultSettings.audioPath);
                updateSetting('usePlaylistFolders', defaultSettings.usePlaylistFolders);
                updateSetting('youtubeMusicAudioOnly', defaultSettings.youtubeMusicAudioOnly);
                updateSetting('embedThumbnail', defaultSettings.embedThumbnail);
                updateSetting('concurrentDownloads', defaultSettings.concurrentDownloads);
                updateSetting('watchClipboardForFiles', defaultSettings.watchClipboardForFiles);
                updateSetting(
                  'fileDownloadNotifications',
                  defaultSettings.fileDownloadNotifications
                );
                updateSetting('aria2Connections', defaultSettings.aria2Connections);
                updateSetting('aria2Splits', defaultSettings.aria2Splits);
                updateSetting('downloadSpeedLimit', defaultSettings.downloadSpeedLimit);
              }
            : undefined}
        >
          {#if matchesSearch('downloadPath')}
            <SettingItem
              title={$t('settings.downloads.downloadPath')}
              description={$t('settings.downloads.downloadPathDescription')}
              icon="folder"
              value={$settings.downloadPath}
              defaultValue={defaultSettings.downloadPath}
              onReset={() => updateSetting('downloadPath', defaultSettings.downloadPath)}
            >
              <button class="path-btn" onclick={pickDownloadPath}>
                <Icon name="folder" size={16} />
                <span class="path-text"
                  >{$settings.downloadPath || $t('settings.downloads.defaultPath')}</span
                >
              </button>
            </SettingItem>
          {/if}

          {#if matchesSearch('useAudioPath')}
            <SettingItem
              title={$t('settings.downloads.useAudioPath')}
              description={$t('settings.downloads.useAudioPathTooltip')}
              icon="music"
              value={$settings.useAudioPath}
              defaultValue={defaultSettings.useAudioPath}
              onReset={() => updateSetting('useAudioPath', defaultSettings.useAudioPath)}
            >
              <Toggle
                checked={$settings.useAudioPath}
                onchange={(checked) => updateSetting('useAudioPath', checked)}
              />
            </SettingItem>

            {#if $settings.useAudioPath}
              <div class="setting-sub-row">
                <div class="setting-label-group">
                  <span class="setting-label" style="font-size: 13px;"
                    >{$t('settings.downloads.audioPath')}</span
                  >
                </div>
                <div class="setting-controls">
                  <button class="path-btn" onclick={pickAudioPath}>
                    <Icon name="folder" size={16} />
                    <span class="path-text"
                      >{$settings.audioPath || $t('settings.downloads.defaultPath')}</span
                    >
                  </button>
                </div>
              </div>
            {/if}
          {/if}

          {#if matchesSearch('usePlaylistFolders')}
            <SettingItem
              title={$t('settings.downloads.usePlaylistFolders')}
              description={$t('settings.downloads.usePlaylistFoldersTooltip')}
              icon="playlist"
              value={$settings.usePlaylistFolders}
              defaultValue={defaultSettings.usePlaylistFolders}
              onReset={() =>
                updateSetting('usePlaylistFolders', defaultSettings.usePlaylistFolders)}
            >
              <Toggle
                checked={$settings.usePlaylistFolders}
                onchange={(checked) => updateSetting('usePlaylistFolders', checked)}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('youtubeMusicAudioOnly')}
            <SettingItem
              title={$t('settings.downloads.youtubeMusicAudioOnly')}
              description={$t('settings.downloads.youtubeMusicAudioOnlyTooltip')}
              icon="headphones"
              value={$settings.youtubeMusicAudioOnly}
              defaultValue={defaultSettings.youtubeMusicAudioOnly}
              onReset={() =>
                updateSetting('youtubeMusicAudioOnly', defaultSettings.youtubeMusicAudioOnly)}
            >
              <Toggle
                checked={$settings.youtubeMusicAudioOnly}
                onchange={(checked) => updateSetting('youtubeMusicAudioOnly', checked)}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('embedThumbnail')}
            <SettingItem
              title={$t('settings.downloads.embedThumbnail')}
              description={$t('settings.downloads.embedThumbnailTooltip')}
              icon="image"
              value={$settings.embedThumbnail}
              defaultValue={defaultSettings.embedThumbnail}
              onReset={() => updateSetting('embedThumbnail', defaultSettings.embedThumbnail)}
            >
              <Toggle
                checked={$settings.embedThumbnail}
                onchange={(checked) => updateSetting('embedThumbnail', checked)}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('concurrentDownloads')}
            <SettingItem
              title={$t('settings.downloads.concurrentDownloads')}
              description={$t('settings.downloads.concurrentDownloadsDescription')}
              icon="queue"
              value={$settings.concurrentDownloads}
              defaultValue={defaultSettings.concurrentDownloads}
              onReset={() =>
                updateSetting('concurrentDownloads', defaultSettings.concurrentDownloads)}
            >
              <div class="slider-with-value">
                <input
                  type="range"
                  class="blur-slider"
                  min="1"
                  max="5"
                  step="1"
                  value={$settings.concurrentDownloads}
                  oninput={(e) =>
                    debouncedSliderUpdate(
                      'concurrentDownloads',
                      parseInt((e.target as HTMLInputElement).value)
                    )}
                />
                <span class="slider-value">{$settings.concurrentDownloads}</span>
              </div>
            </SettingItem>
          {/if}

          <!-- Watch Clipboard for File URLs (desktop only) -->
          {#if matchesSearch('watchClipboardForFiles') && onDesktop}
            <SettingItem
              title={$t('settings.downloads.watchClipboardForFiles')}
              description={$t('settings.downloads.watchClipboardForFilesTooltip')}
              icon="clipboard"
              value={$settings.watchClipboardForFiles}
              defaultValue={defaultSettings.watchClipboardForFiles}
              onReset={() =>
                updateSetting('watchClipboardForFiles', defaultSettings.watchClipboardForFiles)}
            >
              <Toggle
                checked={$settings.watchClipboardForFiles}
                onchange={(checked) => updateSetting('watchClipboardForFiles', checked)}
              />
            </SettingItem>
          {/if}

          <!-- File Download Notifications (desktop only) -->
          {#if matchesSearch('fileDownloadNotifications') && onDesktop}
            <SettingItem
              title={$t('settings.downloads.fileDownloadNotifications')}
              description={$t('settings.downloads.fileDownloadNotificationsTooltip')}
              icon="bell"
              value={$settings.fileDownloadNotifications}
              defaultValue={defaultSettings.fileDownloadNotifications}
              onReset={() =>
                updateSetting(
                  'fileDownloadNotifications',
                  defaultSettings.fileDownloadNotifications
                )}
            >
              <Toggle
                checked={$settings.fileDownloadNotifications}
                onchange={(checked) => updateSetting('fileDownloadNotifications', checked)}
              />
            </SettingItem>
          {/if}

          <!-- Aria2 Connections -->
          {#if matchesSearch('aria2Connections')}
            <SettingItem
              title={$t('settings.downloads.aria2Connections')}
              description={$t('settings.downloads.aria2ConnectionsDescription')}
              icon="link"
              value={$settings.aria2Connections}
              defaultValue={defaultSettings.aria2Connections}
              onReset={() => updateSetting('aria2Connections', defaultSettings.aria2Connections)}
            >
              <div class="slider-with-value">
                <input
                  type="range"
                  class="blur-slider"
                  min="1"
                  max="16"
                  step="1"
                  value={$settings.aria2Connections}
                  oninput={(e) =>
                    debouncedSliderUpdate(
                      'aria2Connections',
                      parseInt((e.target as HTMLInputElement).value)
                    )}
                />
                <span class="slider-value">{$settings.aria2Connections}</span>
              </div>
            </SettingItem>
          {/if}

          <!-- Aria2 Splits -->
          {#if matchesSearch('aria2Splits')}
            <SettingItem
              title={$t('settings.downloads.aria2Splits')}
              description={$t('settings.downloads.aria2SplitsDescription')}
              icon="pie"
              value={$settings.aria2Splits}
              defaultValue={defaultSettings.aria2Splits}
              onReset={() => updateSetting('aria2Splits', defaultSettings.aria2Splits)}
            >
              <div class="slider-with-value">
                <input
                  type="range"
                  class="blur-slider"
                  min="1"
                  max="16"
                  step="1"
                  value={$settings.aria2Splits}
                  oninput={(e) =>
                    debouncedSliderUpdate(
                      'aria2Splits',
                      parseInt((e.target as HTMLInputElement).value)
                    )}
                />
                <span class="slider-value">{$settings.aria2Splits}</span>
              </div>
            </SettingItem>
          {/if}

          <!-- Aria2 Disable IPv6 -->
          {#if matchesSearch('aria2DisableIPv6')}
            <SettingItem
              title={$t('settings.downloads.aria2DisableIPv6')}
              description={$t('settings.downloads.aria2DisableIPv6Description')}
              icon="globe"
              value={$settings.aria2DisableIPv6}
              defaultValue={defaultSettings.aria2DisableIPv6}
              onReset={() => updateSetting('aria2DisableIPv6', defaultSettings.aria2DisableIPv6)}
            >
              <Toggle
                checked={$settings.aria2DisableIPv6}
                onchange={(checked) => updateSetting('aria2DisableIPv6', checked)}
              />
            </SettingItem>
          {/if}

          <!-- Aria2 Custom Args -->
          {#if matchesSearch('aria2CustomArgs')}
            <SettingItem
              title={$t('settings.downloads.aria2CustomArgs')}
              description={$t('settings.downloads.aria2CustomArgsDescription')}
              icon="code"
              value={$settings.aria2CustomArgs}
              defaultValue={defaultSettings.aria2CustomArgs}
              onReset={() => updateSetting('aria2CustomArgs', defaultSettings.aria2CustomArgs)}
            >
              <div style="width: 220px;">
                <Input
                  value={$settings.aria2CustomArgs}
                  placeholder={$t('settings.downloads.aria2CustomArgsPlaceholder')}
                  oninput={(e) =>
                    updateSetting('aria2CustomArgs', (e.target as HTMLInputElement).value)}
                />
              </div>
            </SettingItem>
          {/if}

          <!-- Download Speed Limit -->
          {#if matchesSearch('downloadSpeedLimit')}
            <SettingItem
              title={$t('settings.downloads.downloadSpeedLimit')}
              description={$t('settings.downloads.downloadSpeedLimitDescription')}
              icon="tuning"
              value={$settings.downloadSpeedLimit}
              defaultValue={defaultSettings.downloadSpeedLimit}
              onReset={() =>
                updateSetting('downloadSpeedLimit', defaultSettings.downloadSpeedLimit)}
            >
              <div class="slider-with-value">
                <input
                  type="range"
                  class="blur-slider"
                  min="0"
                  max="100"
                  step="1"
                  value={$settings.downloadSpeedLimit}
                  oninput={(e) =>
                    debouncedSliderUpdate(
                      'downloadSpeedLimit',
                      parseInt((e.target as HTMLInputElement).value)
                    )}
                />
                <span class="slider-value speed-limit-value"
                  >{$settings.downloadSpeedLimit === 0
                    ? $t('settings.downloads.unlimited')
                    : `${$settings.downloadSpeedLimit} ${$settings.sizeUnit === 'binary' ? 'MiB/s' : 'MB/s'}`}</span
                >
              </div>
            </SettingItem>
          {/if}

          <!-- Bypass Proxy for File Downloads -->
          {#if matchesSearch('bypassProxyForDownloads') && $settings.proxyMode !== 'none'}
            <SettingItem
              title={$t('settings.downloads.bypassProxyForDownloads')}
              description={$t('settings.downloads.bypassProxyForDownloadsDescription')}
              icon="download"
              value={$settings.bypassProxyForDownloads}
              defaultValue={defaultSettings.bypassProxyForDownloads}
              onReset={() =>
                updateSetting('bypassProxyForDownloads', defaultSettings.bypassProxyForDownloads)}
            >
              <Toggle
                checked={$settings.bypassProxyForDownloads}
                onchange={(checked) => updateSetting('bypassProxyForDownloads', checked)}
              />
            </SettingItem>
          {/if}
        </SettingsBlock>
      {/if}

      <!-- Network Section (proxy settings) - Desktop only -->
      {#if sectionHasMatches('network') && onDesktop}
        <SettingsBlock
          title={$t('settings.network.title')}
          icon="globe"
          onResetSection={isSectionModified('network')
            ? () => {
                updateSetting('proxyMode', defaultSettings.proxyMode);
                updateSetting('customProxyUrl', defaultSettings.customProxyUrl);
                updateSetting('retryWithoutProxy', defaultSettings.retryWithoutProxy);
              }
            : undefined}
        >
          {#if matchesSearch('proxy')}
            <!-- Proxy Mode -->
            <SettingItem
              title={$t('settings.network.proxyMode')}
              description={$t('settings.network.proxyModeDescription')}
              icon="globe"
              value={$settings.proxyMode}
              defaultValue={defaultSettings.proxyMode}
              onReset={() => updateSetting('proxyMode', defaultSettings.proxyMode)}
            >
              <div style="width: 180px;">
                <Select
                  options={proxyModeOptions}
                  value={$settings.proxyMode}
                  onchange={handleProxyModeChange}
                />
              </div>
            </SettingItem>

            <!-- System proxy status (shown when mode is 'system') -->
            {#if $settings.proxyMode === 'system' && onDesktop}
              <div class="setting-sub-row proxy-status">
                <div class="proxy-status-content">
                  {#if detectingSystemProxy}
                    <span class="proxy-detecting">
                      <span class="btn-spinner"></span>
                      {$t('settings.network.detectingProxy')}
                    </span>
                  {:else if systemProxyStatus}
                    <span class="proxy-detected">
                      <Icon name="check" size={14} />
                      {systemProxyStatus}
                    </span>
                  {:else}
                    <span class="proxy-none">
                      <Icon name="warning" size={14} />
                      {$t('settings.network.noSystemProxy')}
                    </span>
                  {/if}
                </div>
                <button
                  class="dep-btn"
                  onclick={detectSystemProxy}
                  use:tooltip={$t('settings.network.recheckProxy')}
                >
                  <Icon name="refresh" size={16} />
                </button>
              </div>
            {/if}

            <!-- Custom proxy URL (shown when mode is 'custom') -->
            {#if $settings.proxyMode === 'custom'}
              <SettingItem
                title={$t('settings.network.customProxyUrl')}
                description={$t('settings.network.customProxyUrlDescription')}
                icon="link"
                value={$settings.customProxyUrl}
                defaultValue={defaultSettings.customProxyUrl}
                onReset={() => {
                  customProxyInput = defaultSettings.customProxyUrl;
                  handleCustomProxyInput(defaultSettings.customProxyUrl);
                }}
              >
                <div class="proxy-input-group" style="width: 250px;">
                  <div class="proxy-input-wrapper" class:error={proxyValidationError}>
                    <Input
                      value={customProxyInput}
                      oninput={(e) => handleCustomProxyInput((e.target as HTMLInputElement).value)}
                      placeholder={$t('settings.network.customProxyUrlPlaceholder')}
                    />
                  </div>
                </div>
              </SettingItem>

              {#if proxyValidationError}
                <div class="setting-sub-row proxy-error">
                  <span class="error-text">
                    <Icon name="warning" size={14} />
                    {proxyValidationError}
                  </span>
                  <span class="error-hint">{$t('settings.network.proxyValidFormats')}</span>
                </div>
              {/if}
            {/if}

            <!-- Retry Without Proxy option (shown when proxy is not 'none') -->
            {#if $settings.proxyMode !== 'none'}
              <SettingItem
                title={$t('settings.network.retryWithoutProxy')}
                description={$t('settings.network.retryWithoutProxyTooltip')}
                icon="restart"
                value={$settings.retryWithoutProxy}
                defaultValue={defaultSettings.retryWithoutProxy}
                onReset={() =>
                  updateSetting('retryWithoutProxy', defaultSettings.retryWithoutProxy)}
              >
                <Toggle
                  checked={$settings.retryWithoutProxy}
                  onchange={(checked) => updateSetting('retryWithoutProxy', checked)}
                />
              </SettingItem>
            {/if}

            <!-- IP Check (shown when proxy is not 'none') -->
            {#if $settings.proxyMode !== 'none' && onDesktop}
              <SettingItem
                title={$t('settings.network.checkIp')}
                description={$t('settings.network.checkIpDescription')}
                icon="globe"
              >
                <button class="dep-btn" onclick={checkIp} disabled={checkingIp}>
                  {#if checkingIp}
                    <span class="btn-spinner"></span>
                  {:else}
                    <Icon name="globe" size={14} />
                  {/if}
                  {$t('settings.network.checkIpBtn')}
                </button>
              </SettingItem>

              {#if currentIp}
                <div class="setting-sub-row ip-result">
                  <div class="ip-result-content">
                    <span class="ip-address">{currentIp}</span>
                    {#if ipProxyUsed}
                      <span class="ip-badge proxy">
                        <Icon name="check" size={12} />
                        {$t('settings.network.proxyActive')}
                      </span>
                    {:else}
                      <span class="ip-badge direct">
                        <Icon name="warning" size={12} />
                        {$t('settings.network.directConnection')}
                      </span>
                    {/if}
                  </div>
                </div>
              {/if}
            {/if}
          {/if}
        </SettingsBlock>
      {/if}

      <!-- Browser Extension (local + remote) -->
      {#if onDesktop && (sectionHasMatches('localServer') || sectionHasMatches('sync'))}
        <ExtensionIntegrationSettings {searchQuery} />
      {/if}

      <!-- App Section -->
      {#if sectionHasMatches('app')}
        <SettingsBlock
          title={$t('settings.app.title')}
          icon="widgets"
          onResetSection={isSectionModified('app')
            ? () => {
                updateSetting('autoUpdate', defaultSettings.autoUpdate);
                updateSetting('allowPreReleases', defaultSettings.allowPreReleases);
                updateSetting('sendStats', defaultSettings.sendStats);
                updateSetting('backgroundType', defaultSettings.backgroundType);
                updateSetting('backgroundColor', defaultSettings.backgroundColor);
                updateSetting('backgroundVideo', defaultSettings.backgroundVideo);
                updateSetting('backgroundImage', defaultSettings.backgroundImage);
                updateSetting('backgroundBlur', defaultSettings.backgroundBlur);
                updateSetting('backgroundOpacity', defaultSettings.backgroundOpacity);
                updateSetting('accentColor', defaultSettings.accentColor);
                updateSetting('useSystemAccent', defaultSettings.useSystemAccent);
                updateSetting('disableAnimations', defaultSettings.disableAnimations);
                updateSetting('toastPosition', defaultSettings.toastPosition);
                updateSetting('sizeUnit', defaultSettings.sizeUnit);
                updateSetting('showHistoryStats', defaultSettings.showHistoryStats);
                updateSetting('thumbnailTheming', defaultSettings.thumbnailTheming);
              }
            : undefined}
        >
          {#if matchesSearch('autoUpdate')}
            <SettingItem
              title={$t('settings.app.updates')}
              description={$t('settings.app.currentVersion', { version: getCurrentVersion() })}
              icon="download"
            >
              <button class="dep-btn" onclick={handleCheckForUpdates} disabled={checkingUpdate}>
                {#if checkingUpdate}
                  <span class="btn-spinner"></span>
                {:else}
                  <Icon name="download" size={14} />
                {/if}
                {$t('settings.app.checkForUpdates')}
              </button>
            </SettingItem>

            <SettingItem
              title={$t('settings.app.autoUpdate')}
              description={$t('settings.app.autoUpdateDescription')}
              icon="refresh"
              value={$settings.autoUpdate}
              defaultValue={defaultSettings.autoUpdate}
              onReset={() => updateSetting('autoUpdate', defaultSettings.autoUpdate)}
            >
              <Toggle checked={$settings.autoUpdate} onchange={handleAutoUpdateChange} />
            </SettingItem>

            {#if $updateState.available && $updateState.info}
              <div class="setting-sub-row update-available">
                <div class="update-info">
                  <Icon name="download" size={16} />
                  <span
                    >{$t('settings.app.updateAvailable', {
                      version: $updateState.info.version,
                    })}</span
                  >
                  {#if $updateState.info.isPreRelease}
                    <span class="update-badge pre">Pre-release</span>
                  {/if}
                </div>
                <button
                  class="dep-btn primary"
                  onclick={downloadAndInstall}
                  disabled={$updateState.downloading || $updateState.installTriggered}
                >
                  {#if $updateState.downloading}
                    <span class="btn-spinner"></span>
                    {$t('settings.app.downloading')}
                    {$updateState.progress}%
                  {:else if $updateState.installTriggered}
                    <Icon name="check" size={14} />
                    {$t('settings.app.installTriggered')}
                  {:else}
                    {$t('settings.app.installUpdate')}
                  {/if}
                </button>
              </div>

              {#if $updateState.downloading}
                <div class="setting-sub-row update-progress">
                  <div class="update-progress-bar">
                    <div class="update-progress-fill" style="width: {$updateState.progress}%"></div>
                  </div>
                </div>
              {/if}

              {#if $updateState.info.notes}
                <div class="setting-sub-row update-notes">
                  <div class="update-notes-content">
                    <span class="update-notes-label">{$t('settings.app.whatsNew')}</span>
                    <div class="update-notes-text">
                      {@html $updateState.info.notes.replace(/\n/g, '<br>')}
                    </div>
                  </div>
                </div>
              {/if}
            {/if}

            <SettingItem
              title={$t('settings.app.allowPreReleases')}
              description={$t('settings.app.allowPreReleasesTooltip')}
              icon="star"
              value={$settings.allowPreReleases}
              defaultValue={defaultSettings.allowPreReleases}
              onReset={() => updateSetting('allowPreReleases', defaultSettings.allowPreReleases)}
            >
              <Toggle
                checked={$settings.allowPreReleases}
                onchange={(checked) => updateSetting('allowPreReleases', checked)}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('sendStats')}
            <SettingItem
              title={$t('settings.app.sendStats')}
              description={$t('settings.app.sendStatsTooltip')}
              icon="stats"
              value={$settings.sendStats}
              defaultValue={defaultSettings.sendStats}
              onReset={() => updateSetting('sendStats', defaultSettings.sendStats)}
            >
              <Toggle checked={$settings.sendStats} onchange={handleSendStatsChange} />
            </SettingItem>
          {/if}

          {#if matchesSearch('background')}
            <SettingItem
              title={$t('settings.app.background')}
              description={$t('settings.app.backgroundDescription')}
              icon="image"
              value={$settings.backgroundType}
              defaultValue={defaultSettings.backgroundType}
              onReset={() => updateSetting('backgroundType', defaultSettings.backgroundType)}
            >
              <Select
                options={backgroundTypeOptions}
                value={$settings.backgroundType}
                onchange={handleBackgroundTypeChange}
              />
            </SettingItem>

            {#if $settings.backgroundType === 'solid'}
              <div class="setting-sub-row">
                <div class="setting-label-group">
                  <span class="setting-label">{$t('settings.app.backgroundColor')}</span>
                </div>
                <div class="setting-control-group">
                  <input
                    type="color"
                    class="color-picker"
                    value={$settings.backgroundColor}
                    oninput={(e) => handleBackgroundColorChange(e.currentTarget.value)}
                  />
                  <input
                    type="text"
                    class="color-text-input"
                    value={$settings.backgroundColor}
                    oninput={(e) => handleBackgroundColorChange(e.currentTarget.value)}
                    placeholder="#1a1a2e"
                  />
                </div>
              </div>
            {/if}

            {#if $settings.backgroundType === 'animated'}
              <div class="setting-sub-row">
                <div class="setting-label-group">
                  <span class="setting-label">{$t('settings.app.backgroundVideoUrl')}</span>
                  <span class="setting-description"
                    >{$t('settings.app.backgroundVideoUrlDescription')}</span
                  >
                </div>
                <div class="input-with-actions">
                  {#if $settings.backgroundVideo !== defaultSettings.backgroundVideo}
                    <button
                      class="undo-btn"
                      onclick={() => {
                        backgroundVideoInput = defaultSettings.backgroundVideo;
                        handleBackgroundVideoChange(defaultSettings.backgroundVideo);
                      }}
                      use:tooltip={$t('settings.app.resetToDefault')}
                    >
                      <Icon name="undo" size={16} />
                    </button>
                  {/if}
                  <Input
                    bind:value={backgroundVideoInput}
                    oninput={() => handleBackgroundVideoChange(backgroundVideoInput)}
                    placeholder="https://... or C:\path\to\video.mp4"
                  />
                  <button
                    class="picker-btn"
                    onclick={pickBackgroundVideo}
                    use:tooltip={$t('settings.general.browse')}
                  >
                    <Icon name="folder" size={16} />
                  </button>
                </div>
              </div>
            {/if}

            {#if $settings.backgroundType === 'image'}
              <div class="setting-sub-row">
                <div class="setting-label-group">
                  <span class="setting-label">{$t('settings.app.backgroundImageUrl')}</span>
                  <span class="setting-description"
                    >{$t('settings.app.backgroundImageUrlDescription')}</span
                  >
                </div>
                <div class="input-with-actions">
                  {#if $settings.backgroundImage !== defaultSettings.backgroundImage}
                    <button
                      class="undo-btn"
                      onclick={() => {
                        backgroundImageInput = defaultSettings.backgroundImage;
                        handleBackgroundImageChange(defaultSettings.backgroundImage);
                      }}
                      use:tooltip={$t('settings.app.resetToDefault')}
                    >
                      <Icon name="undo" size={16} />
                    </button>
                  {/if}
                  <Input
                    bind:value={backgroundImageInput}
                    oninput={() => handleBackgroundImageChange(backgroundImageInput)}
                    placeholder="https://... or C:\path\to\image.jpg"
                  />
                  <button
                    class="picker-btn"
                    onclick={pickBackgroundImage}
                    use:tooltip={$t('settings.general.browse')}
                  >
                    <Icon name="folder" size={16} />
                  </button>
                </div>
              </div>
            {/if}

            {#if $settings.backgroundType === 'animated' || $settings.backgroundType === 'image'}
              <div class="setting-sub-row">
                <div class="setting-label-group">
                  <span class="setting-label">{$t('settings.app.backgroundBlurAmount')}</span>
                  <span class="setting-description"
                    >{$t('settings.app.backgroundBlurAmountDescription')}</span
                  >
                </div>
                <div class="slider-with-value">
                  <input
                    type="range"
                    class="blur-slider"
                    min="0"
                    max="50"
                    step="1"
                    value={$settings.backgroundBlur}
                    oninput={(e) =>
                      handleBackgroundBlurChange(parseInt((e.target as HTMLInputElement).value))}
                  />
                  <span class="slider-value">{$settings.backgroundBlur}px</span>
                </div>
              </div>
            {/if}

            {#if !isAndroid() && !isWindowEffect($settings.backgroundType)}
              <div class="setting-sub-row">
                <div class="setting-label-group">
                  <span class="setting-label">{$t('settings.app.backgroundOpacity')}</span>
                  <span class="setting-description"
                    >{$t('settings.app.backgroundOpacityDescription')}</span
                  >
                </div>
                <div class="slider-with-value">
                  <input
                    type="range"
                    class="blur-slider"
                    min="0"
                    max="100"
                    step="1"
                    value={$settings.backgroundOpacity}
                    oninput={(e) =>
                      handleBackgroundOpacityChange(parseInt((e.target as HTMLInputElement).value))}
                  />
                  <span class="slider-value">{$settings.backgroundOpacity}%</span>
                </div>
              </div>
            {/if}

            {#if !isAndroid() && $settings.backgroundType !== 'solid' && $settings.backgroundType !== 'oled'}
              <div class="setting-sub-row">
                <div class="setting-label-group">
                  <span class="setting-label">{$t('settings.app.windowTint')}</span>
                  <span class="setting-description">{$t('settings.app.windowTintDescription')}</span
                  >
                </div>
                <div class="slider-with-value">
                  {#if $settings.windowTint !== defaultSettings.windowTint}
                    <button
                      class="slider-reset-btn"
                      onclick={() => updateSetting('windowTint', defaultSettings.windowTint)}
                      use:tooltip={'Reset to default'}
                    >
                      <Icon name="undo" size={14} />
                    </button>
                  {/if}
                  <input
                    type="range"
                    class="blur-slider"
                    min="0"
                    max="100"
                    step="1"
                    value={$settings.windowTint}
                    oninput={(e) =>
                      handleWindowTintChange(parseInt((e.target as HTMLInputElement).value))}
                  />
                  <span class="slider-value">{$settings.windowTint}%</span>
                </div>
              </div>
            {/if}
          {/if}

          {#if matchesSearch('accentColor')}
            <SettingItem
              title={$t('settings.app.accentColor')}
              description={$t('settings.app.accentColorDescription')}
              icon="pen_new"
              value={$settings.accentColor}
              defaultValue={defaultSettings.accentColor}
              onReset={() => updateSetting('accentColor', defaultSettings.accentColor)}
            >
              <div class="color-picker-group">
                <!-- Preset color swatches -->
                <div class="color-presets">
                  {#each ['#6366F1', '#8B5CF6', '#EC4899', '#EF4444', '#F97316', '#EAB308', '#22C55E', '#14B8A6', '#0EA5E9', '#3B82F6'] as color}
                    <button
                      type="button"
                      class="color-swatch"
                      class:active={$settings.accentColor.toUpperCase() === color.toUpperCase()}
                      style="background: {color}"
                      disabled={$settings.useSystemAccent}
                      onclick={() => handleAccentColorChange(color)}
                      title={color}
                    ></button>
                  {/each}
                  <!-- RGB animated color option -->
                  <button
                    type="button"
                    class="color-swatch rgb-swatch"
                    class:active={$settings.accentColor === 'rgb'}
                    disabled={$settings.useSystemAccent}
                    onclick={() => handleAccentColorChange('rgb')}
                    title={$t('settings.app.rgbColor')}
                  ></button>
                </div>
                <input
                  type="color"
                  class="color-picker"
                  value={$settings.accentColor === 'rgb' ? '#6366F1' : $settings.accentColor}
                  disabled={$settings.useSystemAccent || $settings.accentColor === 'rgb'}
                  oninput={(e) => handleAccentColorChange((e.target as HTMLInputElement).value)}
                />
                <input
                  type="text"
                  class="color-text-input"
                  value={$settings.accentColor}
                  disabled={$settings.useSystemAccent}
                  oninput={(e) =>
                    handleAccentColorChange((e.currentTarget as HTMLInputElement).value)}
                  placeholder="#6366F1"
                />
              </div>
            </SettingItem>

            <div class="setting-sub-row">
              <Checkbox
                checked={$settings.useSystemAccent}
                label={$t('settings.app.useSystemAccent')}
                onchange={handleUseSystemAccentChange}
              />
              <span class="setting-hint" style="margin-left: 0; padding-left: 0;"
                >{$t('settings.app.useSystemAccentDescription')}</span
              >
            </div>
          {/if}

          {#if matchesSearch('accentStyle')}
            <SettingItem
              title={$t('settings.app.accentStyle')}
              description={$t('settings.app.accentStyleDescription')}
              icon="pen_new"
              value={$settings.accentStyle}
              defaultValue={defaultSettings.accentStyle}
              onReset={() => updateSetting('accentStyle', defaultSettings.accentStyle)}
            >
              <div class="accent-style-options">
                <button
                  type="button"
                  class="accent-style-btn"
                  class:active={$settings.accentStyle === 'solid'}
                  onclick={() => updateSetting('accentStyle', 'solid')}
                >
                  {$t('settings.app.accentStyleSolid')}
                </button>
                <button
                  type="button"
                  class="accent-style-btn"
                  class:active={$settings.accentStyle === 'gradient'}
                  onclick={() => updateSetting('accentStyle', 'gradient')}
                >
                  {$t('settings.app.accentStyleGradient')}
                </button>
                <button
                  type="button"
                  class="accent-style-btn"
                  class:active={$settings.accentStyle === 'glow'}
                  onclick={() => updateSetting('accentStyle', 'glow')}
                >
                  {$t('settings.app.accentStyleGlow')}
                </button>
              </div>
            </SettingItem>
          {/if}

          {#if matchesSearch('disableAnimations')}
            <SettingItem
              title={$t('settings.app.disableAnimations')}
              icon="stop"
              value={$settings.disableAnimations}
              defaultValue={defaultSettings.disableAnimations}
              onReset={() => updateSetting('disableAnimations', defaultSettings.disableAnimations)}
            >
              <Toggle
                checked={$settings.disableAnimations}
                onchange={handleDisableAnimationsChange}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('toastPosition')}
            <SettingItem
              title={$t('settings.notifications.toastPosition')}
              description={$t('settings.notifications.toastPositionDescription')}
              icon="chat"
              value={$settings.toastPosition}
              defaultValue={defaultSettings.toastPosition}
              onReset={() => updateSetting('toastPosition', defaultSettings.toastPosition)}
            >
              <div style="width: 180px;">
                <Select
                  options={toastPositionOptions}
                  value={$settings.toastPosition}
                  onchange={handleToastPositionChange}
                />
              </div>
            </SettingItem>
          {/if}

          {#if matchesSearch('sizeUnit')}
            <SettingItem
              title={$t('settings.app.sizeUnit')}
              description={$t('settings.app.sizeUnitDescription')}
              icon="weight"
              value={$settings.sizeUnit}
              defaultValue={defaultSettings.sizeUnit}
              onReset={() => updateSetting('sizeUnit', defaultSettings.sizeUnit)}
            >
              <Select
                options={sizeUnitOptions}
                value={$settings.sizeUnit}
                onchange={handleSizeUnitChange}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('showHistoryStats')}
            <SettingItem
              title={$t('settings.app.showHistoryStats')}
              icon="history"
              value={$settings.showHistoryStats}
              defaultValue={defaultSettings.showHistoryStats}
              onReset={() => updateSetting('showHistoryStats', defaultSettings.showHistoryStats)}
            >
              <Toggle
                checked={$settings.showHistoryStats}
                onchange={() => {
                  updateSetting('showHistoryStats', !$settings.showHistoryStats);
                }}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('thumbnailTheming')}
            <SettingItem
              title={$t('settings.app.thumbnailTheming')}
              description={$t('settings.app.thumbnailThemingDescription')}
              icon="pen_new"
              value={$settings.thumbnailTheming}
              defaultValue={defaultSettings.thumbnailTheming}
              onReset={() => updateSetting('thumbnailTheming', defaultSettings.thumbnailTheming)}
            >
              <Toggle
                checked={$settings.thumbnailTheming}
                onchange={() => {
                  updateSetting('thumbnailTheming', !$settings.thumbnailTheming);
                }}
              />
            </SettingItem>
          {/if}

          {#if matchesSearch('builderThumbnailGlow')}
            <SettingItem
              title={$t('settings.app.builderThumbnailGlow')}
              description={$t('settings.app.builderThumbnailGlowDescription')}
              icon="blur"
              value={$settings.builderThumbnailGlow}
              defaultValue={defaultSettings.builderThumbnailGlow}
              onReset={() =>
                updateSetting('builderThumbnailGlow', defaultSettings.builderThumbnailGlow)}
            >
              <Toggle
                checked={$settings.builderThumbnailGlow}
                onchange={() => {
                  updateSetting('builderThumbnailGlow', !$settings.builderThumbnailGlow);
                }}
              />
            </SettingItem>
          {/if}
        </SettingsBlock>
      {/if}

      <!-- Dependencies Section (desktop only - Android uses bundled youtubedl-android) -->
      {#if sectionHasMatches('deps') && onDesktop}
        <SettingsBlock title={$t('settings.deps.title')} icon="package">
          {#if matchesSearch('ytdlp')}
            <SettingItem title="yt-dlp" description={$t('settings.deps.ytdlpDescription')}>
              <div
                class="dep-item"
                style="width: 100%; display: flex; justify-content: space-between; align-items: center;"
              >
                <div class="dep-info" style="display: flex; align-items: center; gap: 8px;">
                  <span class="dep-badge required">{$t('settings.deps.required')}</span>
                  {#if $deps.checking === 'ytdlp'}
                    <span class="dep-status checking">{$t('settings.deps.checking')}</span>
                  {:else if $deps.ytdlp?.installed}
                    <span class="dep-status installed">v{$deps.ytdlp.version}</span>
                  {:else}
                    <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
                  {/if}
                </div>
                <div class="dep-actions">
                  {#if $deps.installingDeps.has('ytdlp')}
                    <button class="dep-btn" disabled>
                      <span class="btn-spinner"></span>
                      {$t('settings.deps.installing')}
                    </button>
                  {:else if $deps.ytdlp?.installed}
                    <button class="dep-btn danger" onclick={() => deps.uninstallYtdlp()}>
                      {$t('settings.deps.uninstall')}
                    </button>
                    <button
                      class="dep-btn"
                      onclick={() =>
                        installDepWithToast('ytdlp', () => deps.installYtdlp(), 'yt-dlp')}
                    >
                      {$t('settings.deps.reinstall')}
                    </button>
                  {:else}
                    <button
                      class="dep-btn primary"
                      onclick={() =>
                        installDepWithToast('ytdlp', () => deps.installYtdlp(), 'yt-dlp')}
                    >
                      {$t('settings.deps.install')}
                    </button>
                  {/if}
                </div>
              </div>
              {#if $deps.installingDeps.has('ytdlp') && $deps.installProgressMap.get('ytdlp')}
                <div class="dep-progress" style="margin-top: 8px;">
                  <div
                    class="dep-progress-bar"
                    style="width: {$deps.installProgressMap.get('ytdlp')?.progress ?? 0}%"
                  ></div>
                </div>
              {/if}
            </SettingItem>
          {/if}

          <!-- ffmpeg -->
          <SettingItem title="ffmpeg" description={$t('settings.deps.ffmpegDescription')}>
            <div
              class="dep-item"
              style="width: 100%; display: flex; justify-content: space-between; align-items: center;"
            >
              <div class="dep-info" style="display: flex; align-items: center; gap: 8px;">
                {#if $deps.checking === 'ffmpeg'}
                  <span class="dep-status checking">{$t('settings.deps.checking')}</span>
                {:else if $deps.ffmpeg?.installed}
                  <span class="dep-status installed">v{$deps.ffmpeg.version}</span>
                {:else}
                  <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
                {/if}
              </div>
              <div class="dep-actions">
                {#if $deps.installingDeps.has('ffmpeg')}
                  <button class="dep-btn" disabled>
                    <span class="btn-spinner"></span>
                    {$t('settings.deps.installing')}
                  </button>
                {:else if $deps.ffmpeg?.installed}
                  <button class="dep-btn danger" onclick={() => deps.uninstallFfmpeg()}>
                    {$t('settings.deps.uninstall')}
                  </button>
                  <button
                    class="dep-btn"
                    onclick={() =>
                      installDepWithToast('ffmpeg', () => deps.installFfmpeg(), 'ffmpeg')}
                  >
                    {$t('settings.deps.reinstall')}
                  </button>
                {:else}
                  <button
                    class="dep-btn primary"
                    onclick={() =>
                      installDepWithToast('ffmpeg', () => deps.installFfmpeg(), 'ffmpeg')}
                  >
                    {$t('settings.deps.install')}
                  </button>
                {/if}
              </div>
            </div>
            {#if $deps.installingDeps.has('ffmpeg') && $deps.installProgressMap.get('ffmpeg')}
              <div class="dep-progress" style="margin-top: 8px;">
                <div
                  class="dep-progress-bar"
                  style="width: {$deps.installProgressMap.get('ffmpeg')?.progress ?? 0}%"
                ></div>
              </div>
            {/if}
          </SettingItem>

          <!-- aria2 -->
          <SettingItem title="aria2" description={$t('settings.deps.aria2Description')}>
            <div
              class="dep-item"
              style="width: 100%; display: flex; justify-content: space-between; align-items: center;"
            >
              <div class="dep-info" style="display: flex; align-items: center; gap: 8px;">
                {#if $deps.checking === 'aria2'}
                  <span class="dep-status checking">{$t('settings.deps.checking')}</span>
                {:else if $deps.aria2?.installed}
                  <span class="dep-status installed">v{$deps.aria2.version}</span>
                {:else}
                  <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
                {/if}
              </div>
              <div class="dep-actions">
                {#if $deps.installingDeps.has('aria2')}
                  <button class="dep-btn" disabled>
                    <span class="btn-spinner"></span>
                    {$t('settings.deps.installing')}
                  </button>
                {:else if $deps.aria2?.installed}
                  <button class="dep-btn danger" onclick={() => deps.uninstallAria2()}>
                    {$t('settings.deps.uninstall')}
                  </button>
                  <button
                    class="dep-btn"
                    onclick={() => installDepWithToast('aria2', () => deps.installAria2(), 'aria2')}
                  >
                    {$t('settings.deps.reinstall')}
                  </button>
                {:else}
                  <button
                    class="dep-btn primary"
                    onclick={() => installDepWithToast('aria2', () => deps.installAria2(), 'aria2')}
                  >
                    {$t('settings.deps.install')}
                  </button>
                {/if}
              </div>
            </div>
            {#if $deps.installingDeps.has('aria2') && $deps.installProgressMap.get('aria2')}
              <div class="dep-progress" style="margin-top: 8px;">
                <div
                  class="dep-progress-bar"
                  style="width: {$deps.installProgressMap.get('aria2')?.progress ?? 0}%"
                ></div>
              </div>
            {/if}
          </SettingItem>

          <!-- deno (JavaScript runtime for yt-dlp) -->
          <SettingItem title="deno" description={$t('settings.deps.denoDescription')}>
            <div
              class="dep-item"
              style="width: 100%; display: flex; justify-content: space-between; align-items: center;"
            >
              <div class="dep-info" style="display: flex; align-items: center; gap: 8px;">
                {#if $deps.checking === 'deno'}
                  <span class="dep-status checking">{$t('settings.deps.checking')}</span>
                {:else if $deps.deno?.installed}
                  <span class="dep-status installed">v{$deps.deno.version}</span>
                {:else}
                  <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
                {/if}
              </div>
              <div class="dep-actions">
                {#if $deps.installingDeps.has('deno')}
                  <button class="dep-btn" disabled>
                    <span class="btn-spinner"></span>
                    {$t('settings.deps.installing')}
                  </button>
                {:else if $deps.deno?.installed}
                  <button class="dep-btn danger" onclick={() => deps.uninstallDeno()}>
                    {$t('settings.deps.uninstall')}
                  </button>
                  <button
                    class="dep-btn"
                    onclick={() => installDepWithToast('deno', () => deps.installDeno(), 'deno')}
                  >
                    {$t('settings.deps.reinstall')}
                  </button>
                {:else}
                  <button
                    class="dep-btn primary"
                    onclick={() => installDepWithToast('deno', () => deps.installDeno(), 'deno')}
                  >
                    {$t('settings.deps.install')}
                  </button>
                {/if}
              </div>
            </div>
            {#if $deps.installingDeps.has('deno') && $deps.installProgressMap.get('deno')}
              <div class="dep-progress" style="margin-top: 8px;">
                <div
                  class="dep-progress-bar"
                  style="width: {$deps.installProgressMap.get('deno')?.progress ?? 0}%"
                ></div>
              </div>
            {/if}
          </SettingItem>

          <!-- quickjs (Lightweight JavaScript runtime for yt-dlp PO tokens) -->
          <SettingItem title="quickjs" description={$t('settings.deps.quickjsDescription')}>
            <div
              class="dep-item"
              style="width: 100%; display: flex; justify-content: space-between; align-items: center;"
            >
              <div class="dep-info" style="display: flex; align-items: center; gap: 8px;">
                {#if $deps.checking === 'quickjs'}
                  <span class="dep-status checking">{$t('settings.deps.checking')}</span>
                {:else if $deps.quickjs?.installed}
                  <span class="dep-status installed">{$t('settings.deps.installed')}</span>
                {:else}
                  <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
                {/if}
              </div>
              <div class="dep-actions">
                {#if $deps.installingDeps.has('quickjs')}
                  <button class="dep-btn" disabled>
                    <span class="btn-spinner"></span>
                    {$t('settings.deps.installing')}
                  </button>
                {:else if $deps.quickjs?.installed}
                  <button class="dep-btn danger" onclick={() => deps.uninstallQuickjs()}>
                    {$t('settings.deps.uninstall')}
                  </button>
                  <button
                    class="dep-btn"
                    onclick={() =>
                      installDepWithToast('quickjs', () => deps.installQuickjs(), 'quickjs')}
                  >
                    {$t('settings.deps.reinstall')}
                  </button>
                {:else}
                  <button
                    class="dep-btn primary"
                    onclick={() =>
                      installDepWithToast('quickjs', () => deps.installQuickjs(), 'quickjs')}
                  >
                    {$t('settings.deps.install')}
                  </button>
                {/if}
              </div>
            </div>
            {#if $deps.installingDeps.has('quickjs') && $deps.installProgressMap.get('quickjs')}
              <div class="dep-progress" style="margin-top: 8px;">
                <div
                  class="dep-progress-bar"
                  style="width: {$deps.installProgressMap.get('quickjs')?.progress ?? 0}%"
                ></div>
              </div>
            {/if}
          </SettingItem>

          <!-- lux -->
          <SettingItem title="lux" description={$t('settings.deps.luxDescription')}>
            <div
              class="dep-item"
              style="width: 100%; display: flex; justify-content: space-between; align-items: center;"
            >
              <div class="dep-info" style="display: flex; align-items: center; gap: 8px;">
                <span class="dep-badge optional">{$t('settings.deps.optional')}</span>
                {#if $deps.checking === 'lux'}
                  <span class="dep-status checking">{$t('settings.deps.checking')}</span>
                {:else if $deps.lux?.installed}
                  <span class="dep-status installed">v{$deps.lux.version}</span>
                {:else}
                  <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
                {/if}
              </div>
              <div class="dep-actions">
                {#if $deps.installingDeps.has('lux')}
                  <button class="dep-btn" disabled>
                    <span class="btn-spinner"></span>
                    {$t('settings.deps.installing')}
                  </button>
                {:else if $deps.lux?.installed}
                  <button class="dep-btn danger" onclick={() => deps.uninstallLux()}>
                    {$t('settings.deps.uninstall')}
                  </button>
                  <button
                    class="dep-btn"
                    onclick={() => installDepWithToast('lux', () => deps.installLux(), 'lux')}
                  >
                    {$t('settings.deps.reinstall')}
                  </button>
                {:else}
                  <button
                    class="dep-btn primary"
                    onclick={() => installDepWithToast('lux', () => deps.installLux(), 'lux')}
                  >
                    {$t('settings.deps.install')}
                  </button>
                {/if}
              </div>
            </div>
            {#if $deps.installingDeps.has('lux') && $deps.installProgressMap.get('lux')}
              <div class="dep-progress" style="margin-top: 8px;">
                <div
                  class="dep-progress-bar"
                  style="width: {$deps.installProgressMap.get('lux')?.progress ?? 0}%"
                ></div>
              </div>
            {/if}
          </SettingItem>

          {#if $deps.error}
            <p class="dep-error">{$deps.error}</p>
          {/if}
        </SettingsBlock>
      {/if}

      <!-- Data Section -->
      {#if !searchQuery.trim() || 'data reset clear export import история сброс очистить экспорт импорт'.includes(searchQuery.toLowerCase())}
        <SettingsBlock title={$t('settings.data.title')} icon="folder">
          <!-- Reset Settings -->
          <SettingItem
            title={$t('settings.data.resetSettings')}
            description={$t('settings.data.resetSettingsDescription')}
            icon="undo"
          >
            <button class="data-btn danger" onclick={() => (showResetModal = true)}>
              <Icon name="undo" size={16} />
              {$t('settings.data.resetSettings')}
            </button>
          </SettingItem>

          <!-- Clear History -->
          <SettingItem
            title={$t('settings.data.clearHistory')}
            description={$t('settings.data.clearHistoryDescription')}
            icon="trash"
          >
            <button class="data-btn danger" onclick={() => (showClearHistoryModal = true)}>
              <Icon name="trash" size={16} />
              {$t('settings.data.clearHistory')}
            </button>
          </SettingItem>

          <!-- Clear Cache -->
          {#if onDesktop}
            <SettingItem
              title={$t('settings.data.clearCache')}
              description={$t('settings.data.clearCacheDescription')}
              icon="trash"
            >
              <button class="data-btn" onclick={handleClearCache} disabled={clearingCache}>
                {#if clearingCache}
                  <span class="btn-spinner"></span>
                {:else}
                  <Icon name="trash" size={16} />
                {/if}
                {$t('settings.data.clearCache')}
              </button>
            </SettingItem>
          {/if}

          <!-- Export History -->
          <SettingItem
            title={$t('settings.data.exportHistory')}
            description={$t('settings.data.exportHistoryDescription')}
            icon="download"
          >
            <button class="data-btn" onclick={handleExportHistory}>
              <Icon name="download" size={16} />
              {$t('settings.data.exportHistory')}
            </button>
          </SettingItem>

          <!-- Import History -->
          <SettingItem
            title={$t('settings.data.importHistory')}
            description={$t('settings.data.importHistoryDescription')}
            icon="move_to_folder"
          >
            <button class="data-btn" onclick={handleImportHistory}>
              <Icon name="move_to_folder" size={16} />
              {$t('settings.data.importHistory')}
            </button>
          </SettingItem>

          <!-- Import Message -->
          {#if importMessage}
            <p
              class="import-message"
              class:success={importMessage.type === 'success'}
              class:error={importMessage.type === 'error'}
            >
              {importMessage.text}
            </p>
          {/if}
        </SettingsBlock>
      {/if}

      <!-- No results -->
      {#if searchQuery.trim() && !sectionHasMatches('general') && !sectionHasMatches('processing') && !sectionHasMatches('app') && !sectionHasMatches('deps') && !'data reset clear export import история сброс очистить экспорт импорт'.includes(searchQuery.toLowerCase())}
        <div class="no-results">
          <Icon name="search" size={32} />
          <p>{$t('settings.noResults')} "{searchQuery}"</p>
        </div>
      {/if}
    </div>
  </ScrollArea>
</div>

<!-- Reset Settings Modal -->
<Modal bind:open={showResetModal} title={$t('settings.data.resetSettings')}>
  <p>{$t('settings.data.resetSettingsConfirm')}</p>

  {#snippet actions()}
    <button class="modal-btn" onclick={() => (showResetModal = false)}>
      {$t('common.cancel')}
    </button>
    <button class="modal-btn danger" onclick={handleResetSettings}>
      {$t('settings.data.resetSettings')}
    </button>
  {/snippet}
</Modal>

<!-- Clear History Modal -->
<Modal bind:open={showClearHistoryModal} title={$t('settings.data.clearHistory')}>
  <p>{$t('settings.data.clearHistoryConfirm')}</p>

  {#snippet actions()}
    <button class="modal-btn" onclick={() => (showClearHistoryModal = false)}>
      {$t('common.cancel')}
    </button>
    <button class="modal-btn danger" onclick={handleClearHistory}>
      {$t('settings.data.clearHistory')}
    </button>
  {/snippet}
</Modal>

<!-- Large File Warning Modal -->
<Modal bind:open={showLargeFileWarning} title={$t('settings.app.largeFileWarningTitle')}>
  <div class="large-file-warning">
    <div class="warning-icon-large">
      <Icon name="warning" size={32} />
    </div>
    <p class="warning-text">{$t('settings.app.largeFileWarningMessage')}</p>
    {#if pendingVideoFile}
      <div class="file-size-info">
        <span class="size-label">{$t('settings.app.fileSize')}:</span>
        <span class="size-value">{formatFileSize(pendingVideoFile.size)}</span>
      </div>
    {/if}
    {#if isAndroid()}
      <p class="warning-hint">{$t('settings.app.largeFileWarningAndroid')}</p>
    {:else}
      <p class="warning-hint">{$t('settings.app.largeFileWarningDesktop')}</p>
    {/if}
  </div>
  {#snippet actions()}
    <div class="warning-modal-actions">
      <Button variant="ghost" onclick={cancelLargeFile}>
        {$t('common.cancel')}
      </Button>
      <Button variant="primary" onclick={confirmLargeFile}>
        {$t('settings.app.useAnyway')}
      </Button>
    </div>
  {/snippet}
</Modal>

<style>
  .page {
    padding: 0 0 0 16px;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .page-header {
    margin-bottom: 16px;
  }

  h1 {
    font-size: 28px;
    font-weight: 700;
    margin-bottom: 6px;
  }

  .subtitle {
    color: rgba(255, 255, 255, 0.6);
    font-size: 14px;
  }

  .settings-content {
    display: flex;
    flex-direction: column;
    gap: 24px;
    margin-top: 24px;
  }

  /* Search Bar */
  .search-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 10px;
    margin-bottom: 16px;
  }

  .search-bar :global(svg) {
    color: rgba(255, 255, 255, 0.4);
    flex-shrink: 0;
  }

  .search-bar input {
    flex: 1;
    background: transparent;
    border: none;
    color: white;
    font-size: 14px;
    outline: none;
  }

  .search-bar input::placeholder {
    color: rgba(255, 255, 255, 0.4);
  }

  /* Sub-rows for nested settings */
  .setting-sub-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 4px 0 4px 36px;
  }

  .setting-sub-row.update-available {
    background: rgba(34, 197, 94, 0.1);
    padding: 12px 16px 12px 36px;
    border-radius: 8px;
    margin-top: 4px;
  }

  .setting-label-group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .setting-label {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.75);
  }

  .setting-description {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.45);
  }

  .setting-controls {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .setting-hint {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.45);
    padding: 2px 0 6px 36px;
    line-height: 1.4;
  }

  /* Reset padding for hints inside SettingItem */
  :global(.setting-item) .setting-hint {
    padding: 0;
    color: rgba(255, 255, 255, 0.5);
  }

  .setting-control-group {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .input-with-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 300px;
  }

  .input-with-actions :global(.input-wrapper) {
    flex: 1;
  }

  .picker-btn {
    width: 30px;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    transition: all 0.15s;
    flex-shrink: 0;
  }

  .picker-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.2);
    color: white;
  }

  .undo-btn {
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    transition: all 0.15s;
    flex-shrink: 0;
    width: 26px;
    height: 26px;
    color: rgba(99, 102, 241, 0.7);
  }

  .undo-btn:hover {
    color: rgba(99, 102, 241, 1);
    background: rgba(99, 102, 241, 0.15);
  }

  .path-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    color: rgba(255, 255, 255, 0.8);
    font-size: 13px;
    cursor: pointer;
    transition: all 0.15s;
    max-width: 280px;
  }

  .path-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.2);
    color: white;
  }

  .path-text {
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 220px;
  }

  .color-picker-group {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .color-presets {
    display: flex;
    gap: 4px;
  }

  .color-swatch {
    width: 24px;
    height: 24px;
    border: 2px solid transparent;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.15s;
    padding: 0;
  }

  .color-swatch:hover:not(:disabled) {
    transform: scale(1.1);
  }

  .color-swatch.active {
    border-color: white;
    box-shadow: 0 0 0 2px rgba(0, 0, 0, 0.3);
  }

  .color-swatch:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  /* RGB animated swatch */
  .color-swatch.rgb-swatch {
    background: linear-gradient(
      90deg,
      #ff0000 0%,
      #ff8000 14%,
      #ffff00 28%,
      #00ff00 42%,
      #00ffff 56%,
      #0000ff 70%,
      #8000ff 84%,
      #ff0080 100%
    );
    background-size: 200% 100%;
    animation: rgb-shift 3s linear infinite;
  }

  @keyframes rgb-shift {
    0% {
      background-position: 0% 50%;
    }
    100% {
      background-position: 200% 50%;
    }
  }

  /* Accent style buttons */
  .accent-style-options {
    display: flex;
    gap: 8px;
  }

  .accent-style-btn {
    padding: 8px 16px;
    font-size: 12px;
    font-weight: 500;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    transition: all 0.15s ease;
    font-family: inherit;
  }

  .accent-style-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .accent-style-btn.active {
    background: var(--accent, #6366f1);
    border-color: var(--accent, #6366f1);
    color: white;
  }

  .color-picker {
    width: 40px;
    height: 32px;
    padding: 2px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.05);
    cursor: pointer;
  }

  .color-picker::-webkit-color-swatch-wrapper {
    padding: 2px;
  }

  .color-picker::-webkit-color-swatch {
    border: none;
    border-radius: 4px;
  }

  .color-picker:disabled,
  .color-text-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .color-text-input {
    width: 90px;
    padding: 6px 10px;
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    color: white;
    outline: none;
    transition: all 0.2s;
  }

  .color-text-input:focus {
    border-color: rgba(99, 102, 241, 0.5);
  }

  .update-progress-bar {
    width: 100%;
    height: 6px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 3px;
    overflow: hidden;
  }

  .update-progress-fill {
    height: 100%;
    background: linear-gradient(90deg, #22c55e, #16a34a);
    border-radius: 3px;
    transition: width 0.2s ease-out;
  }

  .update-info {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #22c55e;
  }

  .update-badge {
    font-size: 11px;
    padding: 2px 6px;
    border-radius: 4px;
    font-weight: 500;
  }

  .update-badge.pre {
    background: rgba(234, 179, 8, 0.2);
    color: #eab308;
  }

  .update-notes {
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }

  .update-notes-content {
    width: 100%;
  }

  .update-notes-label {
    font-size: 13px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.8);
    margin-bottom: 6px;
    display: block;
  }

  .update-notes-text {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.6);
    line-height: 1.5;
    max-height: 150px;
    overflow-y: auto;
    padding-right: 8px;
  }

  .update-notes-text::-webkit-scrollbar {
    width: 4px;
  }

  .update-notes-text::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.2);
    border-radius: 2px;
  }

  .slider-with-value {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 180px;
  }

  .slider-reset-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    background: transparent;
    border: 1px solid transparent;
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    transition: all 0.2s;
    flex-shrink: 0;
  }

  .slider-reset-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.9);
  }

  .blur-slider {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 6px;
    background: rgba(255, 255, 255, 0.15);
    border-radius: 3px;
    outline: none;
    cursor: pointer;
  }

  .blur-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    background: var(--accent, #6366f1);
    border-radius: 50%;
    cursor: pointer;
    transition:
      background 0.15s,
      transform 0.15s;
  }

  .blur-slider::-webkit-slider-thumb:hover {
    background: #818cf8;
    transform: scale(1.1);
  }

  .blur-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    background: var(--accent, #6366f1);
    border: none;
    border-radius: 50%;
    cursor: pointer;
    transition:
      background 0.15s,
      transform 0.15s;
  }

  .blur-slider::-moz-range-thumb:hover {
    background: #818cf8;
    transform: scale(1.1);
  }

  .slider-value {
    font-size: 13px;
    font-family: 'JetBrains Mono', monospace;
    color: rgba(255, 255, 255, 0.7);
    min-width: 40px;
    text-align: right;
  }

  .slider-value.speed-limit-value {
    min-width: 80px;
  }

  .no-results {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 48px 24px;
    color: rgba(255, 255, 255, 0.4);
    text-align: center;
  }

  .no-results p {
    font-size: 14px;
  }

  /* Dependency items */
  .dep-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .dep-info {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .dep-badge {
    font-size: 11px;
    padding: 2px 6px;
    border-radius: 4px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .dep-badge.required {
    color: rgba(251, 191, 36, 0.9);
    background: rgba(251, 191, 36, 0.15);
  }

  .dep-badge.optional {
    color: rgba(156, 163, 175, 0.9);
    background: rgba(156, 163, 175, 0.15);
  }

  .dep-status {
    font-size: 13px;
    padding: 2px 8px;
    border-radius: 4px;
  }

  .dep-status.checking {
    color: rgba(255, 255, 255, 0.5);
    background: rgba(255, 255, 255, 0.05);
  }

  .dep-status.installed {
    color: rgba(34, 197, 94, 0.9);
    background: rgba(34, 197, 94, 0.15);
  }

  .dep-status.not-installed {
    color: rgba(239, 68, 68, 0.9);
    background: rgba(239, 68, 68, 0.15);
  }

  .dep-actions {
    display: flex;
    gap: 8px;
  }

  .dep-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px;
    font-size: 13px;
    font-weight: 500;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    transition: all 0.15s;
  }

  .dep-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.2);
  }

  .dep-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .dep-btn.primary {
    background: rgba(99, 102, 241, 0.2);
    border-color: rgba(99, 102, 241, 0.3);
    color: rgba(99, 102, 241, 0.9);
  }

  .dep-btn.primary:hover:not(:disabled) {
    background: rgba(99, 102, 241, 0.3);
  }

  .dep-btn.danger {
    background: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.2);
    color: rgba(239, 68, 68, 0.8);
  }

  .dep-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.2);
  }

  .btn-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.2);
    border-top-color: rgba(99, 102, 241, 0.8);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .dep-progress {
    margin-top: 8px;
    height: 4px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 2px;
    overflow: hidden;
  }

  .dep-progress-bar {
    height: 100%;
    background: linear-gradient(90deg, rgba(99, 102, 241, 0.8), rgba(139, 92, 246, 0.8));
    transition: width 0.3s ease;
  }

  .dep-error {
    margin-top: 8px;
    font-size: 13px;
    color: rgba(239, 68, 68, 0.9);
  }

  .data-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    font-size: 13px;
    font-weight: 500;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
  }

  .data-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.2);
  }

  .data-btn.danger {
    background: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.25);
    color: rgba(239, 68, 68, 0.9);
  }

  .data-btn.danger:hover {
    background: rgba(239, 68, 68, 0.2);
    border-color: rgba(239, 68, 68, 0.4);
  }

  .import-message {
    font-size: 13px;
    padding: 8px 12px;
    border-radius: 6px;
    margin-top: 8px;
  }

  .import-message.success {
    background: rgba(34, 197, 94, 0.15);
    color: rgba(34, 197, 94, 0.9);
  }

  .import-message.error {
    background: rgba(239, 68, 68, 0.15);
    color: rgba(239, 68, 68, 0.9);
  }

  /* Modal buttons */
  .modal-btn {
    padding: 8px 16px;
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

  .modal-btn.danger {
    background: rgba(239, 68, 68, 0.2);
    border-color: rgba(239, 68, 68, 0.3);
    color: rgba(239, 68, 68, 0.9);
  }

  .modal-btn.danger:hover {
    background: rgba(239, 68, 68, 0.3);
  }

  /* Large file warning modal */
  .large-file-warning {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 8px 0;
    text-align: center;
  }

  .warning-icon-large {
    color: rgba(251, 191, 36, 1);
    background: rgba(251, 191, 36, 0.15);
    padding: 16px;
    border-radius: 50%;
  }

  .warning-text {
    color: rgba(255, 255, 255, 0.85);
    font-size: 14px;
    line-height: 1.5;
    margin: 0;
  }

  .file-size-info {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 10px 16px;
    background: rgba(0, 0, 0, 0.3);
    border-radius: 8px;
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
  }

  .file-size-info .size-label {
    color: rgba(255, 255, 255, 0.6);
    font-size: 12px;
  }

  .file-size-info .size-value {
    color: rgba(251, 191, 36, 1);
    font-size: 16px;
    font-weight: 600;
  }

  .warning-hint {
    color: rgba(255, 255, 255, 0.5);
    font-size: 12px;
    margin: 0;
    max-width: 300px;
  }

  .warning-modal-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .proxy-status-content {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
  }

  .proxy-detecting {
    display: flex;
    align-items: center;
    gap: 8px;
    color: rgba(255, 255, 255, 0.6);
    font-size: 13px;
  }

  .proxy-detected {
    display: flex;
    align-items: center;
    gap: 6px;
    color: rgba(34, 197, 94, 0.9);
    font-size: 13px;
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
  }

  .proxy-none {
    display: flex;
    align-items: center;
    gap: 6px;
    color: rgba(255, 255, 255, 0.5);
    font-size: 13px;
  }

  .proxy-input-group {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 320px;
  }

  .proxy-input-wrapper {
    flex: 1;
  }

  .proxy-input-wrapper.error :global(.input-wrapper) {
    border-color: rgba(239, 68, 68, 0.5);
  }

  .proxy-input-wrapper.error :global(.input-wrapper:focus-within) {
    border-color: rgba(239, 68, 68, 0.8);
    box-shadow: 0 0 0 2px rgba(239, 68, 68, 0.1);
  }

  .proxy-error {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .proxy-error .error-text {
    display: flex;
    align-items: center;
    gap: 6px;
    color: rgba(239, 68, 68, 0.9);
    font-size: 13px;
  }

  .proxy-error .error-hint {
    color: rgba(255, 255, 255, 0.4);
    font-size: 12px;
    margin-left: 20px;
  }

  .ip-result-content {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .ip-address {
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
    font-size: 14px;
    color: rgba(255, 255, 255, 0.9);
    background: rgba(255, 255, 255, 0.05);
    padding: 4px 10px;
    border-radius: 4px;
  }

  .ip-badge {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    padding: 3px 8px;
    border-radius: 4px;
  }

  .ip-badge.proxy {
    background: rgba(34, 197, 94, 0.15);
    color: rgba(34, 197, 94, 0.9);
  }

  .ip-badge.direct {
    background: rgba(251, 191, 36, 0.15);
    color: rgba(251, 191, 36, 0.9);
  }

  /* Mobile responsive styles */
  @media (max-width: 700px) {
    h1 {
      font-size: 24px;
    }

    .slider-with-value {
      min-width: unset;
      width: 100%;
    }

    /* Sub-rows on mobile - stack vertically */
    .setting-sub-row {
      flex-direction: column;
      align-items: flex-start;
      gap: 8px;
      padding-left: 16px;
    }

    .setting-sub-row .setting-controls {
      width: 100%;
    }

    /* Hints on mobile */
    .setting-hint {
      padding-left: 16px;
    }

    /* Dependency items on mobile */
    .dep-item {
      flex-direction: column;
      align-items: flex-start;
      gap: 12px;
    }

    .dep-info {
      flex-wrap: wrap;
    }

    .dep-actions {
      width: 100%;
      justify-content: flex-start;
    }

    /* Proxy section on mobile */
    .proxy-input-group {
      min-width: unset;
      width: 100%;
    }
  }
</style>
