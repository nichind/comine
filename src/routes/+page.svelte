<script lang="ts">
  import { onMount } from 'svelte';
  import { readText } from '@tauri-apps/plugin-clipboard-manager';
  import { invoke } from '@tauri-apps/api/core';
  import { replaceState } from '$app/navigation';
  import { page } from '$app/stores';
  import { t } from '$lib/i18n';
  import { deps } from '$lib/stores/deps';
  import { queue, activeDownloadsCount } from '$lib/stores/queue';
  import { logs } from '$lib/stores/logs';
  import { navigation, currentView, previousView, canGoBack } from '$lib/stores/navigation';
  import { toast } from '$lib/components/Toast.svelte';
  import SetupBanner from '$lib/components/SetupBanner.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import Chip from '$lib/components/Chip.svelte';
  import Input from '$lib/components/Input.svelte';
  import SettingButton from '$lib/components/SettingButton.svelte';
  import OptionModal from '$lib/components/OptionModal.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import Checkbox from '$lib/components/Checkbox.svelte';
  import Divider from '$lib/components/Divider.svelte';
  import CollapsibleBlock from '$lib/components/CollapsibleBlock.svelte';
  import PlaylistBuilder, {
    type PlaylistSelection,
    type SelectedEntry,
    type EntrySettings,
    type PlaylistEntry,
  } from '$lib/components/PlaylistBuilder.svelte';
  import TrackBuilder, { type TrackSelection } from '$lib/components/TrackBuilder.svelte';
  import ChannelBuilder, { type ChannelSelection } from '$lib/components/ChannelBuilder.svelte';
  import { type ChannelEntry, mediaCache } from '$lib/stores/mediaCache';
  import ViewStack, { type ViewInstance } from '$lib/components/ViewStack.svelte';
  import type { IconName } from '$lib/components/Icon.svelte';
  import Toggle from '$lib/components/Toggle.svelte';

  import { isAndroid } from '$lib/utils/android';
  import {
    settings,
    settingsReady,
    updateSetting,
    updateSettings,
    defaultYtDlpAdvanced,
    type YtDlpAdvancedSettings as YtDlpAdvancedSettingsType,
    type CustomPreset,
    type VideoQuality,
    type DownloadMode,
    type AudioQuality,
    getProxyConfig,
  } from '$lib/stores/settings';
  import {
    cleanUrl,
    isLikelyPlaylist,
    isLikelyChannel,
    isValidMediaUrl,
    isDirectFileUrl,
  } from '$lib/utils/format';

  let url = $state('');
  let status = $state('');
  let androidReady = $state(false);

  let lastYtmAutoSwitchUrl = $state('');
  let trackSelection = $state<TrackSelection | null>(null);

  let scrollMaskStyle = $state('');
  const MASK_SIZE = 25;

  function handleViewScroll(event: Event) {
    const container = event.target as HTMLDivElement;
    if (!container) return;

    const { scrollTop, scrollHeight, clientHeight } = container;
    const maxScroll = scrollHeight - clientHeight;

    if (maxScroll <= 0) {
      scrollMaskStyle = '';
      return;
    }

    const topProgress = Math.min(scrollTop / MASK_SIZE, 1);
    const bottomProgress = Math.min((maxScroll - scrollTop) / MASK_SIZE, 1);

    const topFade =
      topProgress > 0 ? `transparent, black ${MASK_SIZE * topProgress}px` : 'black, black 0px';
    const bottomFade =
      bottomProgress > 0
        ? `black calc(100% - ${MASK_SIZE * bottomProgress}px), transparent`
        : 'black 100%, black 100%';

    scrollMaskStyle = `mask-image: linear-gradient(to bottom, ${topFade}, ${bottomFade}); -webkit-mask-image: linear-gradient(to bottom, ${topFade}, ${bottomFade});`;
  }

  let ytdlpInstalled = $derived($deps.ytdlp?.installed ?? false);
  let luxInstalled = $derived($deps.lux?.installed ?? false);
  let aria2Installed = $derived($deps.aria2?.installed ?? false);
  let ffmpegInstalled = $derived($deps.ffmpeg?.installed ?? false);

  function checkIsYouTubeUrl(urlStr: string): boolean {
    if (!urlStr.trim()) return false;
    const u = urlStr.toLowerCase();
    return u.includes('youtube.com') || u.includes('youtu.be');
  }

  function checkIsPlaylistUrl(urlStr: string): boolean {
    if (!urlStr.trim()) return false;
    return isLikelyPlaylist(urlStr.trim(), { ignoreMixes: $settings.ignoreMixes });
  }

  function checkIsChannelUrl(urlStr: string): boolean {
    if (!urlStr.trim()) return false;
    return isLikelyChannel(urlStr.trim());
  }

  type PlatformIcon =
    | 'platform_youtube'
    | 'platform_youtube_music'
    | 'platform_bilibili'
    | 'platform_tiktok'
    | 'platform_twitter'
    | 'platform_instagram'
    | 'platform_twitch'
    | 'platform_vimeo'
    | 'platform_facebook'
    | 'platform_weibo'
    | 'platform_generic';

  function getPlatformIcon(urlStr: string): PlatformIcon {
    if (!urlStr.trim()) return 'platform_generic';
    const u = urlStr.toLowerCase();

    if (u.includes('music.youtube.com')) return 'platform_youtube_music';
    if (u.includes('youtube.com') || u.includes('youtu.be')) return 'platform_youtube';
    if (u.includes('bilibili.com') || u.includes('b23.tv')) return 'platform_bilibili';
    if (u.includes('tiktok.com') || u.includes('douyin.com') || u.includes('iesdouyin.com'))
      return 'platform_tiktok';
    if (u.includes('twitter.com') || u.includes('x.com')) return 'platform_twitter';
    if (u.includes('instagram.com')) return 'platform_instagram';
    if (u.includes('twitch.tv')) return 'platform_twitch';
    if (u.includes('vimeo.com')) return 'platform_vimeo';
    if (u.includes('facebook.com') || u.includes('fb.watch')) return 'platform_facebook';
    if (u.includes('weibo.com')) return 'platform_weibo';

    return 'platform_generic';
  }

  let isYouTubeUrl = $derived(checkIsYouTubeUrl(url));
  let isPlaylistUrl = $derived(checkIsPlaylistUrl(url));
  let isChannelUrl = $derived(checkIsChannelUrl(url));
  let isVideoUrl = $derived(!!url.trim());
  let platformIcon = $derived(getPlatformIcon(url));

  $effect(() => {
    const urlParam = $page.url.searchParams.get('url');
    const openPlaylist = $page.url.searchParams.get('openPlaylist') === '1';
    const openChannel = $page.url.searchParams.get('openChannel') === '1';
    const openFormat = $page.url.searchParams.get('openFormat') === '1';

    if (urlParam) {
      url = urlParam;
      if (openFormat) {
        navigation.openVideo(urlParam);
      } else if (openPlaylist) {
        navigation.openPlaylist(urlParam);
      } else if (openChannel) {
        navigation.openChannel(urlParam);
      }
      if (typeof window !== 'undefined') {
        replaceState(window.location.pathname, {});
      }
    }
  });

  $effect(() => {
    if ($settings.youtubeMusicAudioOnly && url && /music\.youtube\.com/i.test(url)) {
      if (url !== lastYtmAutoSwitchUrl && downloadMode !== 'audio') {
        downloadMode = 'audio';
        if (selectedPreset !== 'music') {
          selectedPreset = 'music';
        }
        lastYtmAutoSwitchUrl = url;
      }
    }
    if (!url.trim()) {
      lastYtmAutoSwitchUrl = '';
    }
  });

  let canDownload = $derived(
    isAndroid() ? androidReady : $deps.ytdlp?.installed || $deps.lux?.installed
  );

  let isDownloading = $derived($activeDownloadsCount > 0);

  let downloadOptionsExpanded = $state(true);
  let mediaSettingsExpanded = $state(false);
  let downloaderExpanded = $state(false);

  let selectedPreset = $state($settings.selectedPreset ?? 'best');
  let videoQuality = $state<VideoQuality>($settings.defaultVideoQuality ?? 'max');
  let downloadMode = $state<DownloadMode>($settings.defaultDownloadMode ?? 'auto');
  let audioQuality = $state<AudioQuality>($settings.defaultAudioQuality ?? 'best');
  let convertToMp4 = $state($settings.convertToMp4 ?? false);
  let remux = $state($settings.remux ?? true);
  let clearMetadata = $state($settings.clearMetadata ?? false);
  let dontShowInHistory = $state($settings.dontShowInHistory ?? false);
  let usePlaylistFolders = $state($settings.usePlaylistFolders ?? true);
  let useAria2 = $state($settings.useAria2 ?? false);
  let ignoreMixes = $state($settings.ignoreMixes ?? true);
  let cookiesFromBrowser = $state($settings.cookiesFromBrowser ?? '');
  let customCookies = $state($settings.customCookies ?? '');
  let sponsorBlock = $state($settings.sponsorBlock ?? false);
  let sponsorBlockSkipSponsors = $state($settings.sponsorBlockSkipSponsors ?? true);
  let sponsorBlockSkipIntros = $state($settings.sponsorBlockSkipIntros ?? false);
  let sponsorBlockSkipSelfPromo = $state($settings.sponsorBlockSkipSelfPromo ?? false);
  let sponsorBlockSkipInteraction = $state($settings.sponsorBlockSkipInteraction ?? false);
  let chapters = $state($settings.chapters ?? true);
  let embedSubtitles = $state($settings.embedSubtitles ?? false);
  let subtitleLanguages = $state($settings.subtitleLanguages ?? 'en,ru');
  let embedThumbnail = $state($settings.embedThumbnail ?? true);
  let downloadSpeedLimit = $state($settings.downloadSpeedLimit ?? 0);
  let ytdlpAdvanced = $state<YtDlpAdvancedSettingsType>(
    $settings.ytdlpAdvanced ?? { ...defaultYtDlpAdvanced }
  );

  $effect(() => {
    if (!$settingsReady) return;

    selectedPreset = $settings.selectedPreset ?? 'best';
    videoQuality = $settings.defaultVideoQuality ?? 'max';
    const isYtmUrl = url && /music\.youtube\.com/i.test(url);
    if (!($settings.youtubeMusicAudioOnly && isYtmUrl)) {
      downloadMode = $settings.defaultDownloadMode ?? 'auto';
    }
    audioQuality = $settings.defaultAudioQuality ?? 'best';
    convertToMp4 = $settings.convertToMp4 ?? false;
    remux = $settings.remux ?? true;
    clearMetadata = $settings.clearMetadata ?? false;
    dontShowInHistory = $settings.dontShowInHistory ?? false;
    usePlaylistFolders = $settings.usePlaylistFolders ?? true;
    useAria2 = $settings.useAria2 ?? false;
    ignoreMixes = $settings.ignoreMixes ?? true;
    cookiesFromBrowser = $settings.cookiesFromBrowser ?? '';
    customCookies = $settings.customCookies ?? '';
    sponsorBlock = $settings.sponsorBlock ?? false;
    sponsorBlockSkipSponsors = $settings.sponsorBlockSkipSponsors ?? true;
    sponsorBlockSkipIntros = $settings.sponsorBlockSkipIntros ?? false;
    sponsorBlockSkipSelfPromo = $settings.sponsorBlockSkipSelfPromo ?? false;
    sponsorBlockSkipInteraction = $settings.sponsorBlockSkipInteraction ?? false;
    chapters = $settings.chapters ?? true;
    embedSubtitles = $settings.embedSubtitles ?? false;
    subtitleLanguages = $settings.subtitleLanguages ?? 'en,ru';
    embedThumbnail = $settings.embedThumbnail ?? true;
    downloadSpeedLimit = $settings.downloadSpeedLimit ?? 0;
    ytdlpAdvanced = $settings.ytdlpAdvanced ?? { ...defaultYtDlpAdvanced };
  });

  $effect(() => {
    if ($settingsReady && $deps.hasCheckedAll) {
      if (!aria2Installed && useAria2) {
        useAria2 = false;
        saveSettings();
      }
      if (!ffmpegInstalled) {
        let needsSave = false;
        if (remux) {
          remux = false;
          needsSave = true;
        }
        if (convertToMp4) {
          convertToMp4 = false;
          needsSave = true;
        }
        if (needsSave) {
          saveSettings();
        }
      }
    }
  });

  function saveSettings() {
    updateSettings({
      selectedPreset,
      defaultVideoQuality: videoQuality,
      defaultDownloadMode: downloadMode,
      defaultAudioQuality: audioQuality,
      convertToMp4,
      remux,
      clearMetadata,
      dontShowInHistory,
      usePlaylistFolders,
      useAria2,
      ignoreMixes,
      cookiesFromBrowser,
      customCookies,
      sponsorBlock,
      sponsorBlockSkipSponsors,
      sponsorBlockSkipIntros,
      sponsorBlockSkipSelfPromo,
      sponsorBlockSkipInteraction,
      chapters,
      embedSubtitles,
      subtitleLanguages,
      embedThumbnail,
      downloadSpeedLimit,
      ytdlpAdvanced,
    });
  }

  let videoQualityModalOpen = $state(false);
  let downloadModeModalOpen = $state(false);
  let audioQualityModalOpen = $state(false);
  let cookiesModalOpen = $state(false);
  let customCookiesModalOpen = $state(false);
  let customCookiesInput = $state('');
  let speedLimitModalOpen = $state(false);
  let customSpeedInput = $state('');

  let createPresetModalOpen = $state(false);
  let newPresetName = $state('');

  const browserOptions = [
    { value: '', label: $t('download.options.noCookies') },
    { value: 'chrome', label: 'Chrome' },
    { value: 'firefox', label: 'Firefox' },
    { value: 'edge', label: 'Edge' },
    { value: 'brave', label: 'Brave' },
    { value: 'opera', label: 'Opera' },
    { value: 'vivaldi', label: 'Vivaldi' },
    { value: 'safari', label: 'Safari' },
    { value: 'custom', label: $t('download.options.customCookies') },
  ];

  function getBrowserLabel(browser: string): string {
    const option = browserOptions.find((o) => o.value === browser);
    return option?.label ?? browser;
  }

  const videoQualityOptions: { value: VideoQuality; label: string }[] = [
    { value: 'max', label: $t('download.quality.max') },
    { value: '4k', label: '4K' },
    { value: '1440p', label: '1440p' },
    { value: '1080p', label: '1080p' },
    { value: '720p', label: '720p' },
    { value: '480p', label: '480p' },
    { value: '360p', label: '360p' },
    { value: '240p', label: '240p' },
  ];

  const downloadModeOptions: { value: DownloadMode; label: string }[] = [
    { value: 'auto', label: $t('download.mode.auto') },
    { value: 'audio', label: $t('download.mode.audio') },
    { value: 'mute', label: $t('download.mode.mute') },
  ];

  const audioQualityOptions: { value: AudioQuality; label: string }[] = [
    { value: 'best', label: $t('download.audio.best') },
    { value: '320', label: '320 kbps' },
    { value: '256', label: '256 kbps' },
    { value: '192', label: '192 kbps' },
    { value: '128', label: '128 kbps' },
    { value: '96', label: '96 kbps' },
  ];

  const builtInPresets: { id: string; label: string; icon: IconName }[] = [
    { id: 'best', label: $t('download.options.bestVideo'), icon: 'video' },
    { id: 'music', label: $t('download.options.music'), icon: 'music' },
    { id: 'small', label: $t('download.options.smallVideo'), icon: 'weight' },
  ];

  let allPresets = $derived([
    ...builtInPresets,
    ...($settings.customPresets ?? []).map((p) => ({
      id: p.id,
      label: p.label,
      icon: 'star' as IconName,
    })),
  ]);

  function applyPreset(preset: string) {
    selectedPreset = preset;

    const customPreset = $settings.customPresets?.find((p) => p.id === preset);
    if (customPreset) {
      videoQuality = customPreset.videoQuality;
      downloadMode = customPreset.downloadMode;
      audioQuality = customPreset.audioQuality;
      remux = customPreset.remux;
      convertToMp4 = customPreset.convertToMp4;
      clearMetadata = customPreset.clearMetadata;
      dontShowInHistory = customPreset.dontShowInHistory;
      useAria2 = customPreset.useAria2;
      ignoreMixes = customPreset.ignoreMixes;
      cookiesFromBrowser = customPreset.cookiesFromBrowser;
      sponsorBlock = customPreset.sponsorBlock ?? false;
      chapters = customPreset.chapters ?? true;
      embedSubtitles = customPreset.embedSubtitles ?? false;
      subtitleLanguages = customPreset.subtitleLanguages ?? 'en,ru';
      embedThumbnail = customPreset.embedThumbnail ?? true;
      if (customPreset.outputTemplate) {
        updateSetting('ytdlpAdvanced', {
          ...($settings.ytdlpAdvanced ?? {}),
          outputTemplate: customPreset.outputTemplate,
        });
      }
      saveSettings();
      return;
    }

    switch (preset) {
      case 'best':
        videoQuality = 'max';
        downloadMode = 'auto';
        audioQuality = 'best';
        remux = true;
        convertToMp4 = false;
        clearMetadata = false;
        sponsorBlock = false;
        chapters = true;
        embedSubtitles = false;
        embedThumbnail = true;
        break;
      case 'small':
        videoQuality = '480p';
        downloadMode = 'auto';
        audioQuality = '192';
        remux = true;
        convertToMp4 = true;
        clearMetadata = false;
        sponsorBlock = false;
        chapters = true;
        embedSubtitles = false;
        embedThumbnail = false;
        break;
      case 'music':
        videoQuality = 'max';
        downloadMode = 'audio';
        audioQuality = 'best';
        remux = true;
        convertToMp4 = false;
        clearMetadata = false;
        sponsorBlock = false;
        chapters = false;
        embedSubtitles = false;
        embedThumbnail = true;
        break;
    }
    saveSettings();
  }

  function createCustomPreset() {
    if (!newPresetName.trim()) {
      toast.error($t('download.options.presetNameRequired'));
      return;
    }

    const id = `custom-${Date.now()}`;
    const newPreset: CustomPreset = {
      id,
      label: newPresetName.trim(),
      videoQuality,
      downloadMode,
      audioQuality,
      remux,
      convertToMp4,
      clearMetadata,
      dontShowInHistory,
      useAria2,
      ignoreMixes,
      cookiesFromBrowser,
      sponsorBlock,
      chapters,
      embedSubtitles,
      subtitleLanguages,
      embedThumbnail,
      outputTemplate: $settings.ytdlpAdvanced?.outputTemplate,
    };

    const updatedPresets = [...($settings.customPresets ?? []), newPreset];
    updateSetting('customPresets', updatedPresets);

    selectedPreset = id;
    newPresetName = '';
    createPresetModalOpen = false;
    toast.success($t('download.options.presetCreated'));
  }

  function deletePreset(presetId: string) {
    const updatedPresets = ($settings.customPresets ?? []).filter((p) => p.id !== presetId);
    updateSetting('customPresets', updatedPresets);

    if (selectedPreset === presetId) {
      selectedPreset = 'custom';
    }
    toast.info($t('download.options.presetDeleted'));
  }

  function getLabel(options: { value: string; label: string }[], value: string): string {
    return options.find((o) => o.value === value)?.label ?? value;
  }

  function handleCheckboxChange(key: keyof typeof $settings, value: boolean) {
    switch (key) {
      case 'convertToMp4':
        convertToMp4 = value;
        break;
      case 'remux':
        remux = value;
        break;
      case 'clearMetadata':
        clearMetadata = value;
        break;
      case 'dontShowInHistory':
        dontShowInHistory = value;
        break;
      case 'useAria2':
        useAria2 = value;
        break;
      case 'ignoreMixes':
        ignoreMixes = value;
        break;
      case 'sponsorBlock':
        sponsorBlock = value;
        break;
      case 'sponsorBlockSkipSponsors':
        sponsorBlockSkipSponsors = value;
        break;
      case 'sponsorBlockSkipIntros':
        sponsorBlockSkipIntros = value;
        break;
      case 'sponsorBlockSkipSelfPromo':
        sponsorBlockSkipSelfPromo = value;
        break;
      case 'sponsorBlockSkipInteraction':
        sponsorBlockSkipInteraction = value;
        break;
      case 'chapters':
        chapters = value;
        break;
      case 'embedSubtitles':
        embedSubtitles = value;
        break;
      case 'embedThumbnail':
        embedThumbnail = value;
        break;
    }
    selectedPreset = 'custom';
    saveSettings();
  }

  function handleOptionChange(type: 'video' | 'audio' | 'mode', value: string) {
    switch (type) {
      case 'video':
        videoQuality = value as VideoQuality;
        break;
      case 'audio':
        audioQuality = value as AudioQuality;
        break;
      case 'mode':
        downloadMode = value as DownloadMode;
        break;
    }
    selectedPreset = 'custom';
    saveSettings();
  }

  onMount(async () => {
    if (isAndroid()) {
      androidReady = true;

      if (!url.trim()) {
        try {
          const clipboardText = await readText();
          if (clipboardText && isValidMediaUrl(clipboardText, $settings.clipboardPatterns || [])) {
            url = cleanUrl(clipboardText);
            toast.info(`📋 ${$t('clipboard.detected')}`);
          }
        } catch (err) {}
      }
    }
  });

  function handleBack() {
    navigation.pop();
    url = '';
  }

  let backLabel = $derived(() => {
    const prev = $previousView;
    if (!prev) return undefined;

    switch (prev.type) {
      case 'home':
        return $t('nav.download');
      case 'playlist':
        const title = prev.cachedData?.title;
        if (title) {
          return title.length > 20 ? title.slice(0, 20) + '…' : title;
        }
        return $t('playlist.title');
      case 'video':
        return prev.cachedData?.title?.slice(0, 20) + '…' || $t('download.tracks.title');
      case 'channel':
        const channelName = prev.cachedData?.title;
        if (channelName) {
          return channelName.length > 20 ? channelName.slice(0, 20) + '…' : channelName;
        }
        return $t('channel.title');
      default:
        return undefined;
    }
  });

  function handleVideoDownload(selection: TrackSelection) {
    const downloadUrl = $currentView.url;
    if (!downloadUrl) return;

    logs.info('download', `Using custom tracks: ${selection.formatString}`);

    const hasSponsorBlock = selection.sponsorblock && selection.sponsorblock.length > 0;
    const sponsorCategories = selection.sponsorblock ?? [];

    const queueId = queue.add(downloadUrl, {
      videoQuality: selection.formatString,
      downloadMode: selection.downloadMode,
      audioQuality: 'best',
      convertToMp4,
      remux,
      clearMetadata: selection.embedMetadata === false ? true : clearMetadata,
      dontShowInHistory,
      useAria2,
      ignoreMixes,
      cookiesFromBrowser,
      customCookies,
      sponsorBlock: hasSponsorBlock ? true : sponsorBlock,
      sponsorBlockSkipSponsors: hasSponsorBlock
        ? sponsorCategories.includes('sponsor')
        : sponsorBlockSkipSponsors,
      sponsorBlockSkipIntros: hasSponsorBlock
        ? sponsorCategories.includes('intro') || sponsorCategories.includes('outro')
        : sponsorBlockSkipIntros,
      sponsorBlockSkipSelfPromo: hasSponsorBlock
        ? sponsorCategories.includes('selfpromo')
        : sponsorBlockSkipSelfPromo,
      sponsorBlockSkipInteraction: hasSponsorBlock
        ? sponsorCategories.includes('interaction')
        : sponsorBlockSkipInteraction,
      chapters: selection.embedChapters ?? chapters,
      embedSubtitles: selection.embedSubs ?? embedSubtitles,
      subtitleLanguages: selection.subLangs ?? subtitleLanguages,
      embedThumbnail: selection.embedThumbnail ?? embedThumbnail,
      outputTemplate: selection.outputTemplate,
      clipRanges: selection.clipRanges,
      prefetchedInfo: {
        title: selection.title,
        author: selection.author,
        thumbnail: selection.thumbnail,
        duration: selection.duration,
      },
    });

    if (queueId) {
      logs.info('download', `Added to queue with ID: ${queueId}`);
    }
    navigation.pop();
    const displayTitle =
      selection.title && selection.title.length > 40
        ? selection.title.slice(0, 40) + '…'
        : selection.title || 'Download';
    toast.info($t('downloads.started').replace('{title}', displayTitle));
    url = '';
  }

  function handlePlaylistDownload(selection: PlaylistSelection) {
    logs.info(
      'playlist',
      `Downloading ${selection.entries.length} items from playlist: ${selection.playlistInfo.title}`
    );

    const globalOptions = {
      videoQuality,
      audioQuality,
      convertToMp4,
      remux,
      clearMetadata,
      dontShowInHistory,
      useAria2,
      ignoreMixes,
      cookiesFromBrowser,
      customCookies,
      sponsorBlock,
      chapters,
      embedSubtitles,
      subtitleLanguages,
      embedThumbnail,
    };

    const queueEntries = selection.entries.map((e) => ({
      url: e.entry.url,
      title: e.entry.title,
      thumbnail: e.entry.thumbnail ?? undefined,
      author: e.entry.uploader ?? undefined,
      duration: e.entry.duration ?? undefined,
      downloadMode: e.settings.downloadMode,
      sponsorBlock:
        e.settings.skipSponsors ||
        e.settings.skipIntros ||
        e.settings.skipSelfPromo ||
        e.settings.skipInteraction,
      sponsorBlockSkipSponsors: e.settings.skipSponsors,
      sponsorBlockSkipIntros: e.settings.skipIntros,
      sponsorBlockSkipSelfPromo: e.settings.skipSelfPromo,
      sponsorBlockSkipInteraction: e.settings.skipInteraction,
      chapters: e.settings.embedChapters,
      embedSubtitles: e.settings.embedSubs,
      subtitleLanguages: e.settings.subLangs,
      embedThumbnail: e.settings.embedThumbnail,
      clearMetadata: e.settings.embedMetadata === false,
    }));

    queue.addPlaylist(
      queueEntries,
      {
        playlistId: selection.playlistInfo.id,
        playlistTitle: selection.playlistInfo.title,
        usePlaylistFolder: selection.playlistInfo.usePlaylistFolder,
      },
      globalOptions
    );

    toast.success(
      $t('playlist.notification.downloadStarted').replace(
        '{count}',
        selection.entries.length.toString()
      )
    );
    navigation.goHome();
    url = '';
  }

  function handleOpenPlaylistItem(entry: PlaylistEntry) {
    navigation.openVideo(entry.url, {
      title: entry.title,
      thumbnail: entry.thumbnail ?? undefined,
      author: entry.uploader ?? undefined,
      duration: entry.duration ?? undefined,
    });
  }

  function handleChannelDownload(selection: ChannelSelection) {
    logs.info(
      'channel',
      `Downloading ${selection.entries.length} items from channel: ${selection.channelInfo.name}`
    );

    const globalOptions = {
      videoQuality,
      audioQuality,
      convertToMp4,
      remux,
      clearMetadata,
      dontShowInHistory,
      useAria2,
      ignoreMixes,
      cookiesFromBrowser,
      customCookies,
      sponsorBlock,
      chapters,
      embedSubtitles,
      subtitleLanguages,
      embedThumbnail,
    };

    const queueEntries = selection.entries.map((e) => ({
      url: e.entry.url,
      title: e.entry.title,
      thumbnail: e.entry.thumbnail ?? undefined,
      author: selection.channelInfo.name,
      duration: e.entry.duration ?? undefined,
      downloadMode: e.settings.downloadMode,
      sponsorBlock:
        e.settings.skipSponsors ||
        e.settings.skipIntros ||
        e.settings.skipSelfPromo ||
        e.settings.skipInteraction,
      sponsorBlockSkipSponsors: e.settings.skipSponsors,
      sponsorBlockSkipIntros: e.settings.skipIntros,
      sponsorBlockSkipSelfPromo: e.settings.skipSelfPromo,
      sponsorBlockSkipInteraction: e.settings.skipInteraction,
      chapters: e.settings.embedChapters,
      embedSubtitles: e.settings.embedSubs,
      subtitleLanguages: e.settings.subLangs,
      embedThumbnail: e.settings.embedThumbnail,
      clearMetadata: e.settings.embedMetadata === false,
    }));

    queue.addPlaylist(
      queueEntries,
      {
        playlistId: selection.channelInfo.id,
        playlistTitle: selection.channelInfo.name,
        usePlaylistFolder: selection.channelInfo.useChannelFolder,
      },
      globalOptions
    );

    toast.success(
      $t('playlist.notification.downloadStarted').replace(
        '{count}',
        selection.entries.length.toString()
      )
    );
    navigation.goHome();
    url = '';
  }

  function handleOpenChannelItem(entry: ChannelEntry) {
    navigation.openVideo(entry.url, {
      title: entry.title,
      thumbnail: entry.thumbnail ?? undefined,
      duration: entry.duration ?? undefined,
    });
  }

  function handleOpenChannelFromVideo(
    channelUrl: string,
    previewData?: { name?: string; thumbnail?: string }
  ) {
    navigation.openChannel(channelUrl, {
      title: previewData?.name,
      thumbnail: previewData?.thumbnail,
    });
  }

  async function quickDownload() {
    if (!url.trim()) {
      status = `⚠️ ${$t('download.placeholder')}`;
      return;
    }

    const downloadUrl = url.trim();
    logs.info('download', `Quick download: ${downloadUrl}`);

    const fileCheck = isDirectFileUrl(downloadUrl);
    if (fileCheck.isFile) {
      logs.info('download', `Direct file URL detected: ${fileCheck.filename}`);
      queue.addFile({
        url: downloadUrl,
        filename: fileCheck.filename || 'download',
      });
      toast.info($t('downloads.started').replace('{title}', fileCheck.filename || 'File'));
      url = '';
      return;
    }

    if (isAndroid()) {
      if (!androidReady) {
        status = '⚠️ yt-dlp is initializing, please wait...';
        return;
      }
    } else {
      if (!$deps.ytdlp?.installed) {
        status = '⚠️ Please install yt-dlp first';
        return;
      }
    }

    const cachedPreview = mediaCache.getPreview(downloadUrl);
    const prefetchedInfo = cachedPreview
      ? {
          title: cachedPreview.title,
          author: cachedPreview.author,
          thumbnail: cachedPreview.thumbnail,
          duration: cachedPreview.duration,
        }
      : undefined;

    if (prefetchedInfo?.title) {
      logs.debug('download', `Using cached preview info: ${prefetchedInfo.title}`);
    }

    const queueId = queue.add(downloadUrl, {
      videoQuality,
      downloadMode: downloadMode as 'auto' | 'audio' | 'mute',
      audioQuality,
      convertToMp4,
      remux,
      clearMetadata,
      dontShowInHistory,
      useAria2,
      ignoreMixes,
      cookiesFromBrowser,
      customCookies,
      sponsorBlock,
      chapters,
      embedSubtitles,
      subtitleLanguages,
      embedThumbnail,
      prefetchedInfo,
    });

    if (queueId) {
      logs.info('download', `Added to queue with ID: ${queueId}`);
      let displayTitle = prefetchedInfo?.title;
      if (!displayTitle) {
        try {
          displayTitle = new URL(downloadUrl).hostname;
        } catch {
          displayTitle = 'Download';
        }
      } else if (displayTitle.length > 40) {
        displayTitle = displayTitle.slice(0, 40) + '…';
      }
      toast.info($t('downloads.started').replace('{title}', displayTitle));
    }
    url = '';
  }

  function openAdvancedView() {
    if (!url.trim()) return;

    if (isChannelUrl) {
      navigation.openChannel(url.trim());
    } else if (isPlaylistUrl) {
      navigation.openPlaylist(url.trim());
    } else {
      navigation.openVideo(url.trim());
    }
  }
</script>

<div class="page">
  <ViewStack>
    {#snippet children({ views, currentId, isActive })}
      {#each views as view (view.id)}
        {#key view.id}
          {@const active = isActive(view.id)}
          <div
            class="view-container"
            class:active
            onscroll={handleViewScroll}
            style={active ? scrollMaskStyle : ''}
          >
            {#if view.type === 'home'}
              <div class="page-header">
                <h1>{$t('app.name')}</h1>
                <p class="subtitle">{$t('download.subtitle')}</p>
              </div>

              <Divider my={20} />

              <div class="page-content">
                <div class="url-input-wrapper">
                  {#if url.trim() && canDownload && (isVideoUrl || isPlaylistUrl || isChannelUrl)}
                    <button
                      class="input-badge"
                      class:playlist={isPlaylistUrl}
                      class:channel={isChannelUrl}
                      onclick={openAdvancedView}
                      title={isChannelUrl ? 'Channel' : isPlaylistUrl ? 'Playlist' : 'Video'}
                    >
                      <Icon name={platformIcon} size={14} />
                      {#if isPlaylistUrl || isChannelUrl}
                        <Icon
                          name={isChannelUrl ? 'user' : 'playlist'}
                          size={10}
                          class="type-indicator"
                        />
                      {/if}
                    </button>
                  {:else}
                    <Icon name="link" size={18} />
                  {/if}
                  <input
                    bind:value={url}
                    placeholder={$t('download.placeholder')}
                    class="url-input"
                    disabled={!canDownload}
                  />
                  {#if url.trim() && canDownload && (isVideoUrl || isPlaylistUrl || isChannelUrl)}
                    <button
                      class="customize-btn"
                      onclick={openAdvancedView}
                      title={$t('download.customizeDownload')}
                    >
                      <Icon name="alt_arrow_rigth" size={18} />
                    </button>
                  {/if}
                  <button
                    class="download-btn"
                    onclick={quickDownload}
                    disabled={!canDownload || !url.trim()}
                  >
                    <Icon name="download" size={20} />
                  </button>
                </div>

                <SetupBanner />

                <div class="settings-blocks">
                  <CollapsibleBlock
                    title={$t('download.blocks.general')}
                    icon="settings"
                    description={$t('download.blocks.generalDesc')}
                    bind:expanded={downloadOptionsExpanded}
                  >
                    <div class="options-group">
                      <span class="group-label">{$t('download.options.presets')}</span>
                      <div class="options-row">
                        {#each allPresets as preset}
                          <Chip
                            selected={selectedPreset === preset.id}
                            icon={preset.icon}
                            onclick={() => applyPreset(preset.id)}
                          >
                            {preset.label}
                            {#if preset.id.startsWith('custom-')}
                              <button
                                class="preset-delete"
                                onclick={(e) => {
                                  e.stopPropagation();
                                  deletePreset(preset.id);
                                }}
                                title={$t('common.delete')}
                              >
                                <Icon name="close" size={12} />
                              </button>
                            {/if}
                          </Chip>
                        {/each}
                        <Chip icon="add" onclick={() => (createPresetModalOpen = true)}
                          >{$t('download.options.createNew')}</Chip
                        >
                      </div>
                    </div>

                    <div class="options-group">
                      <span class="group-label">{$t('download.blocks.quality')}</span>
                      <div class="options-row">
                        <SettingButton
                          label={$t('download.options.videoQuality')}
                          value={getLabel(videoQualityOptions, videoQuality)}
                          onclick={() => (videoQualityModalOpen = true)}
                        />
                        <SettingButton
                          label={$t('download.options.downloadMode')}
                          value={getLabel(downloadModeOptions, downloadMode)}
                          onclick={() => (downloadModeModalOpen = true)}
                        />
                        <SettingButton
                          label={$t('download.options.audioQuality')}
                          value={getLabel(audioQualityOptions, audioQuality)}
                          onclick={() => (audioQualityModalOpen = true)}
                        />
                      </div>
                    </div>

                    <div class="options-group">
                      <span class="group-label">{$t('download.options.postProcessing')}</span>
                      <div class="checkbox-grid">
                        <Checkbox
                          checked={convertToMp4}
                          label={$t('download.options.convertToMp4')}
                          onchange={(val) => handleCheckboxChange('convertToMp4', val)}
                          disabled={!ffmpegInstalled}
                        />
                        <Checkbox
                          checked={remux}
                          label={$t('download.options.remux')}
                          onchange={(val) => handleCheckboxChange('remux', val)}
                          disabled={!ffmpegInstalled}
                        />
                        <Checkbox
                          checked={clearMetadata}
                          label={$t('download.options.clearMetadata')}
                          onchange={(val) => handleCheckboxChange('clearMetadata', val)}
                        />
                      </div>
                    </div>
                  </CollapsibleBlock>

                  <CollapsibleBlock
                    title={$t('download.blocks.youtubeOptions')}
                    icon="video"
                    description="SponsorBlock, chapters, thumbnails, and subtitles"
                    bind:expanded={mediaSettingsExpanded}
                  >
                    <div class="options-group">
                      <div class="group-header">
                        <span class="group-label">SponsorBlock</span>
                        <Toggle
                          checked={sponsorBlock}
                          onchange={(val) => handleCheckboxChange('sponsorBlock', val)}
                        />
                      </div>
                      {#if sponsorBlock}
                        <div class="checkbox-grid">
                          <Checkbox
                            checked={sponsorBlockSkipSponsors}
                            label={$t('download.tracks.skipSponsors')}
                            onchange={(val) =>
                              handleCheckboxChange('sponsorBlockSkipSponsors', val)}
                          />
                          <Checkbox
                            checked={sponsorBlockSkipIntros}
                            label={$t('download.tracks.skipIntros')}
                            onchange={(val) => handleCheckboxChange('sponsorBlockSkipIntros', val)}
                          />
                          <Checkbox
                            checked={sponsorBlockSkipSelfPromo}
                            label={$t('download.tracks.skipSelfPromo')}
                            onchange={(val) =>
                              handleCheckboxChange('sponsorBlockSkipSelfPromo', val)}
                          />
                          <Checkbox
                            checked={sponsorBlockSkipInteraction}
                            label={$t('download.tracks.skipInteraction')}
                            onchange={(val) =>
                              handleCheckboxChange('sponsorBlockSkipInteraction', val)}
                          />
                        </div>
                      {/if}
                    </div>

                    <div class="options-group">
                      <span class="group-label">{$t('download.tracks.embedOptions')}</span>
                      <div class="checkbox-grid">
                        <Checkbox
                          checked={chapters}
                          label={$t('download.tracks.embedChapters')}
                          onchange={(val) => handleCheckboxChange('chapters', val)}
                        />
                        <Checkbox
                          checked={embedThumbnail}
                          label={$t('download.tracks.embedThumbnail')}
                          onchange={(val) => handleCheckboxChange('embedThumbnail', val)}
                        />
                      </div>
                    </div>

                    <div class="options-group">
                      <div class="group-header">
                        <span class="group-label">{$t('download.tracks.subtitles')}</span>
                        <Toggle
                          checked={embedSubtitles}
                          onchange={(val) => handleCheckboxChange('embedSubtitles', val)}
                        />
                      </div>
                      {#if embedSubtitles}
                        <div class="subtitle-row">
                          <span class="subtitle-hint">{$t('download.tracks.subLangsHint')}</span>
                          <input
                            type="text"
                            class="lang-input"
                            value={subtitleLanguages}
                            placeholder="en,ru,es"
                            onchange={(e) => {
                              subtitleLanguages = (e.target as HTMLInputElement).value;
                              saveSettings();
                            }}
                          />
                        </div>
                      {/if}
                    </div>
                  </CollapsibleBlock>

                  <CollapsibleBlock
                    title={$t('download.blocks.downloader')}
                    icon="tuning2"
                    description={$t('download.blocks.advancedDesc')}
                    bind:expanded={downloaderExpanded}
                  >
                    <div class="options-group">
                      <span class="group-label">{$t('download.blocks.authentication')}</span>
                      <div class="setting-row">
                        <span class="setting-desc">{$t('download.blocks.authenticationHint')}</span>
                        <SettingButton
                          label={$t('download.options.cookies')}
                          value={cookiesFromBrowser
                            ? getBrowserLabel(cookiesFromBrowser)
                            : $t('download.options.noCookies')}
                          onclick={() => (cookiesModalOpen = true)}
                        />
                      </div>
                    </div>

                    <div class="options-group">
                      <div class="group-header">
                        <span class="group-label">{$t('download.options.useAria2')}</span>
                        <Toggle
                          checked={useAria2}
                          onchange={(val) => handleCheckboxChange('useAria2', val)}
                          disabled={!aria2Installed}
                        />
                      </div>
                      {#if !aria2Installed}
                        <span class="hint-text">{$t('download.blocks.aria2NotInstalled')}</span>
                      {:else if useAria2}
                        <span class="hint-text success">{$t('download.blocks.aria2Active')}</span>
                      {:else}
                        <span class="hint-text">{$t('download.blocks.aria2Hint')}</span>
                      {/if}
                    </div>

                    <div class="options-group">
                      <span class="group-label">{$t('settings.downloads.downloadSpeedLimit')}</span>
                      <div class="speed-chips">
                        <Chip
                          selected={downloadSpeedLimit === 0}
                          onclick={() => {
                            downloadSpeedLimit = 0;
                            saveSettings();
                          }}
                        >
                          {$t('settings.downloads.unlimited')}
                        </Chip>
                        <Chip
                          selected={downloadSpeedLimit === 5}
                          onclick={() => {
                            downloadSpeedLimit = 5;
                            saveSettings();
                          }}
                        >
                          5 MB/s
                        </Chip>
                        <Chip
                          selected={downloadSpeedLimit === 10}
                          onclick={() => {
                            downloadSpeedLimit = 10;
                            saveSettings();
                          }}
                        >
                          10 MB/s
                        </Chip>
                        <Chip
                          selected={downloadSpeedLimit === 25}
                          onclick={() => {
                            downloadSpeedLimit = 25;
                            saveSettings();
                          }}
                        >
                          25 MB/s
                        </Chip>
                        <Chip
                          selected={![0, 5, 10, 25].includes(downloadSpeedLimit)}
                          onclick={() => (speedLimitModalOpen = true)}
                        >
                          {![0, 5, 10, 25].includes(downloadSpeedLimit)
                            ? `${downloadSpeedLimit} MB/s`
                            : $t('download.options.custom')}
                        </Chip>
                      </div>
                    </div>

                    <div class="options-group">
                      <span class="group-label">{$t('download.options.other')}</span>
                      <div class="checkbox-grid">
                        <Checkbox
                          checked={ignoreMixes}
                          label={$t('download.options.ignoreMixes')}
                          onchange={(val) => handleCheckboxChange('ignoreMixes', val)}
                        />
                        <Checkbox
                          checked={usePlaylistFolders}
                          label={$t('download.options.usePlaylistFolders')}
                          onchange={(val) => handleCheckboxChange('usePlaylistFolders', val)}
                        />
                        <Checkbox
                          checked={dontShowInHistory}
                          label={$t('download.options.dontShowInHistory')}
                          onchange={(val) => handleCheckboxChange('dontShowInHistory', val)}
                        />
                      </div>
                    </div>
                  </CollapsibleBlock>
                </div>

                {#if status}
                  <p class="status">{status}</p>
                {/if}
              </div>
            {:else if view.type === 'video'}
              <TrackBuilder
                url={view.url ?? ''}
                {cookiesFromBrowser}
                {customCookies}
                defaults={{
                  sponsorBlock,
                  sponsorBlockSkipSponsors,
                  sponsorBlockSkipIntros,
                  sponsorBlockSkipSelfPromo,
                  sponsorBlockSkipInteraction,
                  chapters,
                  embedSubtitles,
                  subtitleLanguages,
                  embedThumbnail,
                  clearMetadata,
                }}
                showHeader={true}
                onback={handleBack}
                ondownload={handleVideoDownload}
                onopenchannel={handleOpenChannelFromVideo}
                backLabel={backLabel()}
                prefetchedInfo={view.cachedData}
              />
            {:else if view.type === 'playlist'}
              <PlaylistBuilder
                url={view.url ?? ''}
                {cookiesFromBrowser}
                {customCookies}
                defaultDownloadMode={downloadMode}
                defaults={{
                  sponsorBlock,
                  sponsorBlockSkipSponsors,
                  sponsorBlockSkipIntros,
                  sponsorBlockSkipSelfPromo,
                  sponsorBlockSkipInteraction,
                  chapters,
                  embedSubtitles,
                  subtitleLanguages,
                  embedThumbnail,
                  clearMetadata,
                }}
                showHeader={true}
                onback={handleBack}
                ondownload={handlePlaylistDownload}
                onopenitem={handleOpenPlaylistItem}
                backLabel={backLabel()}
                prefetchedInfo={view.cachedData}
              />
            {:else if view.type === 'channel'}
              <ChannelBuilder
                url={view.url ?? ''}
                {cookiesFromBrowser}
                {customCookies}
                defaults={{
                  downloadMode,
                  sponsorBlock,
                  sponsorBlockSkipSponsors,
                  sponsorBlockSkipIntros,
                  sponsorBlockSkipSelfPromo,
                  sponsorBlockSkipInteraction,
                  chapters,
                  embedSubtitles,
                  subtitleLanguages,
                  embedThumbnail,
                  clearMetadata,
                }}
                showHeader={true}
                onback={handleBack}
                ondownload={handleChannelDownload}
                onopenitem={handleOpenChannelItem}
                backLabel={backLabel()}
                prefetchedInfo={view.cachedData}
              />
            {/if}
          </div>
        {/key}
      {/each}
    {/snippet}
  </ViewStack>
</div>

<OptionModal
  bind:open={videoQualityModalOpen}
  title={$t('download.options.videoQuality')}
  options={videoQualityOptions}
  bind:value={videoQuality}
  columns={4}
  onselect={(val) => handleOptionChange('video', val)}
/>

<OptionModal
  bind:open={downloadModeModalOpen}
  title={$t('download.options.downloadMode')}
  options={downloadModeOptions}
  bind:value={downloadMode}
  columns={3}
  onselect={(val) => handleOptionChange('mode', val)}
/>

<OptionModal
  bind:open={audioQualityModalOpen}
  title={$t('download.options.audioQuality')}
  options={audioQualityOptions}
  bind:value={audioQuality}
  columns={4}
  onselect={(val) => handleOptionChange('audio', val)}
/>

<OptionModal
  bind:open={cookiesModalOpen}
  title={$t('download.options.cookies')}
  description={$t('download.options.cookiesDescription')}
  options={browserOptions}
  bind:value={cookiesFromBrowser}
  columns={3}
  onselect={async (val) => {
    cookiesFromBrowser = val;
    saveSettings();
    if (val === 'custom') {
      customCookiesInput = customCookies;
      customCookiesModalOpen = true;
    } else if (val === '') {
      customCookies = '';
      saveSettings();
      try {
        await invoke('clear_cookies');
      } catch (e) {
        console.warn('Failed to clear cookies:', e);
      }
    }
  }}
/>

<Modal bind:open={customCookiesModalOpen} title={$t('download.options.customCookies')}>
  <p class="modal-desc">{$t('download.options.customCookiesDescription')}</p>
  <textarea
    class="cookies-textarea"
    bind:value={customCookiesInput}
    placeholder={$t('download.options.customCookiesPlaceholder')}
    rows="10"
  ></textarea>

  {#snippet actions()}
    <button class="modal-btn" onclick={() => (customCookiesModalOpen = false)}>
      {$t('common.cancel')}
    </button>
    <button
      class="modal-btn primary"
      onclick={() => {
        customCookies = customCookiesInput;
        saveSettings();
        customCookiesModalOpen = false;
      }}
    >
      {$t('common.save')}
    </button>
  {/snippet}
</Modal>

<Modal bind:open={createPresetModalOpen} title={$t('download.options.createPreset')}>
  <p class="modal-desc">{$t('download.options.createPresetDescription')}</p>
  <Input bind:value={newPresetName} placeholder={$t('download.options.presetNamePlaceholder')} />

  <div class="preset-summary">
    <span class="summary-label">{$t('download.options.currentSettings')}:</span>
    <div class="summary-items">
      <span class="summary-item">{getLabel(videoQualityOptions, videoQuality)}</span>
      <span class="summary-item">{getLabel(downloadModeOptions, downloadMode)}</span>
      <span class="summary-item">{getLabel(audioQualityOptions, audioQuality)}</span>
      {#if remux}<span class="summary-item">Remux</span>{/if}
      {#if convertToMp4}<span class="summary-item">MP4</span>{/if}
      {#if useAria2}<span class="summary-item">aria2</span>{/if}
      {#if sponsorBlock}<span class="summary-item">SponsorBlock</span>{/if}
      {#if chapters}<span class="summary-item">Chapters</span>{/if}
      {#if embedSubtitles}<span class="summary-item">Subtitles</span>{/if}
      {#if embedThumbnail}<span class="summary-item">Thumbnail</span>{/if}
    </div>
  </div>

  {#snippet actions()}
    <button
      class="modal-btn"
      onclick={() => {
        createPresetModalOpen = false;
        newPresetName = '';
      }}
    >
      {$t('common.cancel')}
    </button>
    <button class="modal-btn primary" onclick={createCustomPreset} disabled={!newPresetName.trim()}>
      {$t('common.create')}
    </button>
  {/snippet}
</Modal>

<Modal bind:open={speedLimitModalOpen} title={$t('settings.downloads.downloadSpeedLimit')}>
  <p class="modal-desc">{$t('settings.downloads.downloadSpeedLimitDescription')}</p>
  <div class="speed-input-wrapper">
    <input
      type="number"
      class="speed-input"
      bind:value={customSpeedInput}
      placeholder="0"
      min="0"
      max="100"
    />
    <span class="speed-unit">MB/s</span>
  </div>

  {#snippet actions()}
    <button
      class="modal-btn"
      onclick={() => {
        speedLimitModalOpen = false;
        customSpeedInput = '';
      }}
    >
      {$t('common.cancel')}
    </button>
    <button
      class="modal-btn primary"
      onclick={() => {
        const val = parseInt(customSpeedInput) || 0;
        downloadSpeedLimit = Math.max(0, Math.min(100, val));
        saveSettings();
        speedLimitModalOpen = false;
        customSpeedInput = '';
      }}
    >
      {$t('common.apply')}
    </button>
  {/snippet}
</Modal>

<style>
  .page {
    padding: 0;
    height: 100%;
    position: relative;
    overflow: hidden;
  }

  .view-container {
    position: absolute;
    top: 0;
    left: 0;
    right: 4px;
    bottom: 4px;
    visibility: hidden;
    opacity: 0;
    pointer-events: none;
    overflow-x: hidden;
    overflow-y: auto;
    padding: 0 6px 0 18px;
    &:not(.active) * {
      transition: none !important;
      animation: none !important;
    }
  }

  @media (min-width: 481px) {
    .view-container {
      padding-bottom: 12px;
    }
  }

  @media (max-width: 480px) {
    .view-container {
      padding: 0 4px 120px 0;
    }
  }

  .view-container.active {
    visibility: visible;
    opacity: 1;
    pointer-events: auto;
    z-index: 1;
  }

  .page-header {
    margin-bottom: 0;
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

  .page-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .url-input-wrapper {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 4px 4px 16px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: var(--radius-lg, 12px);
    transition: all 0.2s;
  }

  .url-input-wrapper:focus-within {
    background: rgba(255, 255, 255, 0.08);
    border-color: var(--accent-alpha, rgba(99, 102, 241, 0.4));
  }

  .url-input-wrapper > :global(svg) {
    color: rgba(255, 255, 255, 0.4);
    flex-shrink: 0;
  }

  .url-input {
    flex: 1;
    padding: 8px 0;
    font-size: var(--text-md, 14px);
    background: transparent;
    border: none;
    color: white;
    outline: none;
  }

  .url-input::placeholder {
    color: rgba(255, 255, 255, 0.4);
  }

  .download-btn {
    width: 44px;
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.1);
    border: none;
    border-radius: var(--radius, 10px);
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    transition: all 0.15s;
    flex-shrink: 0;
  }

  .download-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .download-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.15);
    color: white;
  }

  .settings-blocks {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .options-group {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .group-label {
    font-size: var(--text-sm, 12px);
    font-weight: 500;
    color: rgba(255, 255, 255, 0.5);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .group-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }

  .setting-desc {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    flex: 1;
    min-width: 150px;
  }

  .subtitle-row {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    margin-top: 6px;
  }

  .subtitle-hint {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .lang-input {
    width: 120px;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-sm, 6px);
    color: white;
    font-size: 12px;
    font-family: inherit;
  }

  .lang-input:focus {
    outline: none;
    border-color: var(--accent, #6366f1);
  }

  .hint-text {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    margin-top: 4px;
  }

  .hint-text.success {
    color: rgb(34, 197, 94);
  }

  .options-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .speed-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .speed-input-wrapper {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .speed-input {
    width: 100px;
    padding: 10px 12px;
    font-size: var(--text-md, 14px);
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: var(--radius, 8px);
    color: white;
    outline: none;
    transition: border-color 0.15s;
  }

  .speed-input:focus {
    border-color: var(--accent-alpha-hover, rgba(99, 102, 241, 0.5));
  }

  .speed-input::placeholder {
    color: rgba(255, 255, 255, 0.4);
  }

  .speed-input::-webkit-inner-spin-button,
  .speed-input::-webkit-outer-spin-button {
    -webkit-appearance: none;
    appearance: none;
    margin: 0;
  }
  .speed-input[type='number'] {
    -moz-appearance: textfield;
    appearance: textfield;
  }

  .speed-unit {
    font-size: var(--text-md, 14px);
    color: rgba(255, 255, 255, 0.6);
  }

  .checkbox-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 10px;
    align-items: start;
  }

  @media (max-width: 600px) {
    .checkbox-grid {
      grid-template-columns: 1fr;
    }
  }

  .status {
    padding: 12px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 8px);
    color: rgba(255, 255, 255, 0.8);
    font-size: var(--text-md, 14px);
  }

  .modal-desc {
    font-size: var(--text-base, 13px);
    color: rgba(255, 255, 255, 0.6);
    margin: 0 0 12px 0;
    line-height: 1.5;
  }

  .cookies-textarea {
    width: 100%;
    min-width: 400px;
    padding: 12px;
    font-family: monospace;
    font-size: var(--text-sm, 12px);
    background: rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 8px);
    color: rgba(255, 255, 255, 0.9);
    resize: vertical;
  }

  .cookies-textarea::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }

  .cookies-textarea:focus {
    outline: none;
    border-color: var(--accent-alpha-hover, rgba(99, 102, 241, 0.5));
  }

  .modal-btn {
    padding: 8px 16px;
    font-size: var(--text-md, 14px);
    font-weight: 500;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: var(--radius, 8px);
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    transition: all 0.15s;
  }

  .modal-btn:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .modal-btn.primary {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.2));
    border-color: var(--accent-alpha-hover, rgba(99, 102, 241, 0.3));
    color: var(--accent, rgba(129, 140, 248, 1));
  }

  .modal-btn.primary:hover {
    background: var(--accent-alpha-hover, rgba(99, 102, 241, 0.3));
  }

  .modal-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .preset-delete {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    margin-left: 6px;
    padding: 0;
    background: rgba(255, 255, 255, 0.1);
    border: none;
    border-radius: 50%;
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    transition: all 0.15s;
  }

  .preset-delete:hover {
    background: rgba(239, 68, 68, 0.3);
    color: #ef4444;
  }

  .preset-summary {
    margin-top: 16px;
    padding: 12px;
    background: rgba(255, 255, 255, 0.05);
    border-radius: var(--radius, 8px);
  }

  .summary-label {
    display: block;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    margin-bottom: 8px;
  }

  .summary-items {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .summary-item {
    padding: 4px 8px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-sm, 4px);
    font-size: 12px;
    color: rgba(255, 255, 255, 0.7);
  }

  .input-badge {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 6px;
    background: rgba(255, 0, 0, 0.15);
    color: #ff6666;
    border: none;
    border-radius: var(--radius, 8px);
    flex-shrink: 0;
    cursor: pointer;
    overflow: hidden;
    animation: badge-expand 0.2s ease-out forwards;
    transition:
      background 0.15s ease,
      transform 0.1s ease;
  }

  .input-badge:hover {
    background: rgba(255, 0, 0, 0.25);
    transform: scale(1.05);
  }

  .input-badge :global(.type-indicator) {
    position: absolute;
    bottom: 2px;
    right: 2px;
    opacity: 0.9;
  }

  @keyframes badge-expand {
    from {
      max-width: 0;
      padding: 6px 0;
      opacity: 0;
    }
    to {
      max-width: 40px;
      padding: 6px;
      opacity: 1;
    }
  }

  .input-badge.playlist {
    background: rgba(255, 165, 0, 0.15);
    color: #ffaa44;
  }

  .input-badge.playlist:hover {
    background: rgba(255, 165, 0, 0.25);
  }

  .input-badge.channel {
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
  }

  .input-badge.channel:hover {
    background: rgba(239, 68, 68, 0.25);
  }

  .customize-btn {
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-alpha, rgba(99, 102, 241, 0.15));
    color: var(--accent, rgb(99, 102, 241));
    border: none;
    border-radius: var(--radius, 8px);
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .customize-btn:hover {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.25));
    color: var(--accent, rgb(99, 102, 241));
  }

  :global(.rotate-180) {
    transform: rotate(180deg);
  }
</style>
