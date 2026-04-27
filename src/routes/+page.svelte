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
  import { toast } from '$lib/components/ui/Toast.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Chip from '$lib/components/ui/Chip.svelte';
  import Input from '$lib/components/ui/Input.svelte';
  import SettingButton from '$lib/components/settings/SettingButton.svelte';
  import OptionModal from '$lib/components/ui/OptionModal.svelte';
  import Modal from '$lib/components/ui/Modal.svelte';
  import Checkbox from '$lib/components/ui/Checkbox.svelte';
  import CollapsibleBlock from '$lib/components/layout/CollapsibleBlock.svelte';
  import PageShell from '$lib/components/layout/PageShell.svelte';
  import PageHeader from '$lib/components/layout/PageHeader.svelte';
  import { edgeMask } from '$lib/actions/edgeMask';
  import ResolveBuilder from '$lib/components/builders/ResolveBuilder.svelte';
  import { mediaCache } from '$lib/stores/mediaCache';
  import ViewStack, { type ViewInstance } from '$lib/components/layout/ViewStack.svelte';
  import type { IconName } from '$lib/components/ui/Icon.svelte';
  import { getPlatformIconFromUrl } from '$lib/components/resolve/utils';
  import Toggle from '$lib/components/ui/Toggle.svelte';
  import TorrentSearch from '$lib/components/resolve/TorrentSearch.svelte';
  import TorrentDetail from '$lib/components/resolve/TorrentDetail.svelte';

  import { isAndroid, isMobile } from '$lib/utils/android';
  import {
    settings,
    settingsReady,
    updateSetting,
    updateSettings,
    type CustomPreset,
    type VideoQuality,
    type DownloadMode,
    type AudioQuality,
  } from '$lib/stores/settings';
  import {
    cleanUrl,
    isLikelyPlaylist,
    isLikelyChannel,
    isValidMediaUrl,
    isDirectFileUrl,
  } from '$lib/utils/urlUtils';

  let url = $state('');
  let status = $state('');
  let androidReady = $state(false);

  let lastYtmAutoSwitchUrl = $state('');

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

  let isYouTubeUrl = $derived(checkIsYouTubeUrl(url));
  let isPlaylistUrl = $derived(checkIsPlaylistUrl(url));
  let isChannelUrl = $derived(checkIsChannelUrl(url));
  let isVideoUrl = $derived(!!url.trim());
  let platformIcon = $derived(getPlatformIconFromUrl(url));

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
      if (url !== lastYtmAutoSwitchUrl && $settings.defaultDownloadMode !== 'audio') {
        updateSettings({ defaultDownloadMode: 'audio', selectedPreset: 'music' });
        lastYtmAutoSwitchUrl = url;
      }
    }
    if (!url.trim()) {
      lastYtmAutoSwitchUrl = '';
    }
  });

  let canDownload = $derived(isAndroid() ? androidReady : true);

  let isDownloading = $derived($activeDownloadsCount > 0);

  let downloadOptionsExpanded = $state(true);
  let mediaSettingsExpanded = $state(false);
  let downloaderExpanded = $state(false);

  $effect(() => {
    if ($settingsReady && $deps.hasCheckedAll) {
    }
  });

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

  const isMobilePlatform = isMobile();

  const browserOptions = isMobilePlatform
    ? [
        { value: '', label: $t('download.options.noCookies') },
        { value: 'custom', label: $t('download.options.customCookies') },
      ]
    : [
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

  $effect(() => {
    if (!isMobilePlatform) return;
    if ($settings.cookiesFromBrowser && $settings.cookiesFromBrowser !== 'custom') {
      updateSetting('cookiesFromBrowser', '');
    }
  });

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
    const customPreset = $settings.customPresets?.find((p) => p.id === preset);
    if (customPreset) {
      updateSettings({
        selectedPreset: preset,
        defaultVideoQuality: customPreset.videoQuality,
        defaultDownloadMode: customPreset.downloadMode,
        defaultAudioQuality: customPreset.audioQuality,
        remux: customPreset.remux,
        convertToMp4: customPreset.convertToMp4,
        clearMetadata: customPreset.clearMetadata,
        dontShowInHistory: customPreset.dontShowInHistory,
        useAria2: customPreset.useAria2,
        ignoreMixes: customPreset.ignoreMixes,
        cookiesFromBrowser: customPreset.cookiesFromBrowser,
        sponsorBlock: customPreset.sponsorBlock ?? false,
        chapters: customPreset.chapters ?? true,
        embedSubtitles: customPreset.embedSubtitles ?? false,
        subtitleLanguages: customPreset.subtitleLanguages ?? 'en,ru',
        embedThumbnail: customPreset.embedThumbnail ?? true,
      });
      if (customPreset.outputTemplate) {
        updateSetting('ytdlpAdvanced', {
          ...($settings.ytdlpAdvanced ?? {}),
          outputTemplate: customPreset.outputTemplate,
        });
      }
      return;
    }

    switch (preset) {
      case 'best':
        updateSettings({
          selectedPreset: 'best',
          defaultVideoQuality: 'max',
          defaultDownloadMode: 'auto',
          defaultAudioQuality: 'best',
          remux: true,
          convertToMp4: false,
          clearMetadata: false,
          sponsorBlock: false,
          chapters: true,
          embedSubtitles: false,
          embedThumbnail: true,
        });
        break;
      case 'small':
        updateSettings({
          selectedPreset: 'small',
          defaultVideoQuality: '480p',
          defaultDownloadMode: 'auto',
          defaultAudioQuality: '192',
          remux: true,
          convertToMp4: true,
          clearMetadata: false,
          sponsorBlock: false,
          chapters: true,
          embedSubtitles: false,
          embedThumbnail: false,
        });
        break;
      case 'music':
        updateSettings({
          selectedPreset: 'music',
          defaultVideoQuality: 'max',
          defaultDownloadMode: 'audio',
          defaultAudioQuality: 'best',
          remux: true,
          convertToMp4: false,
          clearMetadata: false,
          sponsorBlock: false,
          chapters: false,
          embedSubtitles: false,
          embedThumbnail: true,
        });
        break;
    }
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
      videoQuality: $settings.defaultVideoQuality ?? 'max',
      downloadMode: $settings.defaultDownloadMode ?? 'auto',
      audioQuality: $settings.defaultAudioQuality ?? 'best',
      remux: $settings.remux ?? true,
      convertToMp4: $settings.convertToMp4 ?? false,
      clearMetadata: $settings.clearMetadata ?? false,
      dontShowInHistory: $settings.dontShowInHistory ?? false,
      useAria2: $settings.useAria2 ?? false,
      ignoreMixes: $settings.ignoreMixes ?? true,
      cookiesFromBrowser: $settings.cookiesFromBrowser ?? '',
      sponsorBlock: $settings.sponsorBlock ?? false,
      chapters: $settings.chapters ?? true,
      embedSubtitles: $settings.embedSubtitles ?? false,
      subtitleLanguages: $settings.subtitleLanguages ?? 'en,ru',
      embedThumbnail: $settings.embedThumbnail ?? true,
      outputTemplate: $settings.ytdlpAdvanced?.outputTemplate,
    };

    const updatedPresets = [...($settings.customPresets ?? []), newPreset];
    updateSettings({ customPresets: updatedPresets, selectedPreset: id });

    newPresetName = '';
    createPresetModalOpen = false;
    toast.success($t('download.options.presetCreated'));
  }

  function deletePreset(presetId: string) {
    const updatedPresets = ($settings.customPresets ?? []).filter((p) => p.id !== presetId);
    updateSettings({
      customPresets: updatedPresets,
      ...($settings.selectedPreset === presetId ? { selectedPreset: 'custom' } : {}),
    });
    toast.info($t('download.options.presetDeleted'));
  }

  function getLabel(options: { value: string; label: string }[], value: string): string {
    return options.find((o) => o.value === value)?.label ?? value;
  }

  function handleCheckboxChange(key: keyof typeof $settings, value: boolean) {
    updateSettings({ [key]: value, selectedPreset: 'custom' } as Partial<typeof $settings>);
  }

  function handleOptionChange(type: 'video' | 'audio' | 'mode', value: string) {
    const keyMap = {
      video: 'defaultVideoQuality',
      audio: 'defaultAudioQuality',
      mode: 'defaultDownloadMode',
    } as const;
    updateSettings({ [keyMap[type]]: value, selectedPreset: 'custom' } as Partial<
      typeof $settings
    >);
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

  function handleOpenChannelFromVideo(
    channelUrl: string,
    previewData?: { name?: string; thumbnail?: string }
  ) {
    navigation.openChannel(channelUrl, {
      title: previewData?.name,
      thumbnail: previewData?.thumbnail,
    });
  }

  function looksLikeUrl(input: string): boolean {
    return (
      /^(https?:|magnet:|ftp:)/i.test(input) || /^[a-z0-9][-a-z0-9]*\.[a-z]{2,}/i.test(input)
    );
  }

  async function quickDownload() {
    if (!url.trim()) {
      status = `⚠️ ${$t('download.placeholder')}`;
      return;
    }

    const downloadUrl = url.trim();

    // If input doesn't look like a URL, treat as torrent search query
    if (!looksLikeUrl(downloadUrl)) {
      navigation.openTorrentSearch(downloadUrl);
      return;
    }

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
      videoQuality: $settings.defaultVideoQuality ?? 'max',
      downloadMode: ($settings.defaultDownloadMode ?? 'auto') as 'auto' | 'audio' | 'mute',
      audioQuality: $settings.defaultAudioQuality ?? 'best',
      convertToMp4: $settings.convertToMp4 ?? false,
      remux: $settings.remux ?? true,
      clearMetadata: $settings.clearMetadata ?? false,
      dontShowInHistory: $settings.dontShowInHistory ?? false,
      useAria2: $settings.useAria2 ?? false,
      ignoreMixes: $settings.ignoreMixes ?? true,
      cookiesFromBrowser: $settings.cookiesFromBrowser ?? '',
      customCookies: $settings.customCookies ?? '',
      sponsorBlock: $settings.sponsorBlock ?? false,
      chapters: $settings.chapters ?? true,
      embedSubtitles: $settings.embedSubtitles ?? false,
      subtitleLanguages: $settings.subtitleLanguages ?? 'en,ru',
      embedThumbnail: $settings.embedThumbnail ?? true,
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

<PageShell scrollMode="custom" noPadding>
  <ViewStack>
    {#snippet children({ views, currentId, isActive })}
      {#each views as view (view.id)}
        {#key view.id}
          {@const active = isActive(view.id)}
          <div class="view-container" class:active use:edgeMask>
            {#if view.type === 'home'}
              <PageHeader title={$t('app.name')} subtitle={$t('download.subtitle')} />

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
                    onfocus={() => invoke('set_url_input_focused', { focused: true })}
                    onblur={() => invoke('set_url_input_focused', { focused: false })}
                    onkeydown={(e) => { if (e.key === 'Enter' && url.trim()) quickDownload(); }}
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
                    class="search-torrent-btn"
                    onclick={() => navigation.openTorrentSearch(url.trim() || undefined)}
                    title={$t('torrentSearch.title')}
                  >
                    <Icon name="search" size={18} />
                  </button>
                  <button class="download-btn" onclick={quickDownload} disabled={!url.trim()}>
                    <Icon name="download" size={20} />
                  </button>
                </div>

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
                            selected={$settings.selectedPreset === preset.id}
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
                          value={getLabel(
                            videoQualityOptions,
                            $settings.defaultVideoQuality ?? 'max'
                          )}
                          onclick={() => (videoQualityModalOpen = true)}
                        />
                        <SettingButton
                          label={$t('download.options.downloadMode')}
                          value={getLabel(
                            downloadModeOptions,
                            $settings.defaultDownloadMode ?? 'auto'
                          )}
                          onclick={() => (downloadModeModalOpen = true)}
                        />
                        <SettingButton
                          label={$t('download.options.audioQuality')}
                          value={getLabel(
                            audioQualityOptions,
                            $settings.defaultAudioQuality ?? 'best'
                          )}
                          onclick={() => (audioQualityModalOpen = true)}
                        />
                      </div>
                    </div>

                    <div class="options-group">
                      <span class="group-label">{$t('download.options.postProcessing')}</span>
                      <div class="checkbox-grid">
                        <Checkbox
                          checked={$settings.convertToMp4 ?? false}
                          label={$t('download.options.convertToMp4')}
                          onchange={(val) => handleCheckboxChange('convertToMp4', val)}
                        />
                        <Checkbox
                          checked={$settings.remux ?? true}
                          label={$t('download.options.remux')}
                          onchange={(val) => handleCheckboxChange('remux', val)}
                        />
                        <Checkbox
                          checked={$settings.clearMetadata ?? false}
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
                          checked={$settings.sponsorBlock ?? false}
                          onchange={(val) => handleCheckboxChange('sponsorBlock', val)}
                        />
                      </div>
                      {#if $settings.sponsorBlock}
                        <div class="checkbox-grid">
                          <Checkbox
                            checked={$settings.sponsorBlockSkipSponsors ?? true}
                            label={$t('download.tracks.skipSponsors')}
                            onchange={(val) =>
                              handleCheckboxChange('sponsorBlockSkipSponsors', val)}
                          />
                          <Checkbox
                            checked={$settings.sponsorBlockSkipIntros ?? false}
                            label={$t('download.tracks.skipIntros')}
                            onchange={(val) => handleCheckboxChange('sponsorBlockSkipIntros', val)}
                          />
                          <Checkbox
                            checked={$settings.sponsorBlockSkipSelfPromo ?? false}
                            label={$t('download.tracks.skipSelfPromo')}
                            onchange={(val) =>
                              handleCheckboxChange('sponsorBlockSkipSelfPromo', val)}
                          />
                          <Checkbox
                            checked={$settings.sponsorBlockSkipInteraction ?? false}
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
                          checked={$settings.chapters ?? true}
                          label={$t('download.tracks.embedChapters')}
                          onchange={(val) => handleCheckboxChange('chapters', val)}
                        />
                        <Checkbox
                          checked={$settings.embedThumbnail ?? true}
                          label={$t('download.tracks.embedThumbnail')}
                          onchange={(val) => handleCheckboxChange('embedThumbnail', val)}
                        />
                      </div>
                    </div>

                    <div class="options-group">
                      <div class="group-header">
                        <span class="group-label">{$t('download.tracks.subtitles')}</span>
                        <Toggle
                          checked={$settings.embedSubtitles ?? false}
                          onchange={(val) => handleCheckboxChange('embedSubtitles', val)}
                        />
                      </div>
                      {#if $settings.embedSubtitles}
                        <div class="subtitle-row">
                          <span class="subtitle-hint">{$t('download.tracks.subLangsHint')}</span>
                          <input
                            type="text"
                            class="lang-input"
                            value={$settings.subtitleLanguages ?? 'en,ru'}
                            placeholder="en,ru,es"
                            onchange={(e) => {
                              updateSetting(
                                'subtitleLanguages',
                                (e.target as HTMLInputElement).value
                              );
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
                          value={$settings.cookiesFromBrowser
                            ? getBrowserLabel($settings.cookiesFromBrowser)
                            : $t('download.options.noCookies')}
                          onclick={() => (cookiesModalOpen = true)}
                        />
                      </div>
                    </div>

                    <div class="options-group">
                      <div class="group-header">
                        <span class="group-label">{$t('download.options.useAria2')}</span>
                        <Toggle
                          checked={$settings.useAria2 ?? false}
                          onchange={(val) => handleCheckboxChange('useAria2', val)}
                        />
                      </div>
                      {#if !aria2Installed}
                        <span class="hint-text">{$t('download.blocks.aria2NotInstalled')}</span>
                      {:else if $settings.useAria2}
                        <span class="hint-text success">{$t('download.blocks.aria2Active')}</span>
                      {:else}
                        <span class="hint-text">{$t('download.blocks.aria2Hint')}</span>
                      {/if}
                    </div>

                    <div class="options-group">
                      <span class="group-label">{$t('settings.downloads.downloadSpeedLimit')}</span>
                      <div class="speed-chips">
                        <Chip
                          selected={($settings.downloadSpeedLimit ?? 0) === 0}
                          onclick={() => updateSetting('downloadSpeedLimit', 0)}
                        >
                          {$t('settings.downloads.unlimited')}
                        </Chip>
                        <Chip
                          selected={$settings.downloadSpeedLimit === 5}
                          onclick={() => updateSetting('downloadSpeedLimit', 5)}
                        >
                          5 MB/s
                        </Chip>
                        <Chip
                          selected={$settings.downloadSpeedLimit === 10}
                          onclick={() => updateSetting('downloadSpeedLimit', 10)}
                        >
                          10 MB/s
                        </Chip>
                        <Chip
                          selected={$settings.downloadSpeedLimit === 25}
                          onclick={() => updateSetting('downloadSpeedLimit', 25)}
                        >
                          25 MB/s
                        </Chip>
                        <Chip
                          selected={![0, 5, 10, 25].includes($settings.downloadSpeedLimit ?? 0)}
                          onclick={() => (speedLimitModalOpen = true)}
                        >
                          {![0, 5, 10, 25].includes($settings.downloadSpeedLimit ?? 0)
                            ? `${$settings.downloadSpeedLimit} MB/s`
                            : $t('download.options.custom')}
                        </Chip>
                      </div>
                    </div>

                    <div class="options-group">
                      <span class="group-label">{$t('download.options.other')}</span>
                      <div class="checkbox-grid">
                        <Checkbox
                          checked={$settings.ignoreMixes ?? true}
                          label={$t('download.options.ignoreMixes')}
                          onchange={(val) => handleCheckboxChange('ignoreMixes', val)}
                        />
                        <Checkbox
                          checked={$settings.usePlaylistFolders ?? true}
                          label={$t('download.options.usePlaylistFolders')}
                          onchange={(val) => handleCheckboxChange('usePlaylistFolders', val)}
                        />
                        <Checkbox
                          checked={$settings.dontShowInHistory ?? false}
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
            {:else if view.type === 'video' || view.type === 'playlist' || view.type === 'channel'}
              <ResolveBuilder
                url={view.url ?? ''}
                cookiesFromBrowser={$settings.cookiesFromBrowser ?? ''}
                customCookies={$settings.customCookies ?? ''}
                onBack={handleBack}
                onOpenChannel={handleOpenChannelFromVideo}
                prefetchedInfo={view.cachedData}
              />
            {:else if view.type === 'torrent-search'}
              <TorrentSearch
                initialQuery={view.torrentQuery}
                savedState={view.torrentSearchState}
                onBack={() => navigation.pop()}
              />
            {:else if view.type === 'torrent-detail'}
              <TorrentDetail
                result={view.torrentResult}
                onBack={() => navigation.pop()}
              />
            {/if}
          </div>
        {/key}
      {/each}
    {/snippet}
  </ViewStack>
</PageShell>

<OptionModal
  bind:open={videoQualityModalOpen}
  title={$t('download.options.videoQuality')}
  options={videoQualityOptions}
  value={$settings.defaultVideoQuality ?? 'max'}
  columns={4}
  onselect={(val) => handleOptionChange('video', val)}
/>

<OptionModal
  bind:open={downloadModeModalOpen}
  title={$t('download.options.downloadMode')}
  options={downloadModeOptions}
  value={$settings.defaultDownloadMode ?? 'auto'}
  columns={3}
  onselect={(val) => handleOptionChange('mode', val)}
/>

<OptionModal
  bind:open={audioQualityModalOpen}
  title={$t('download.options.audioQuality')}
  options={audioQualityOptions}
  value={$settings.defaultAudioQuality ?? 'best'}
  columns={4}
  onselect={(val) => handleOptionChange('audio', val)}
/>

<OptionModal
  bind:open={cookiesModalOpen}
  title={$t('download.options.cookies')}
  description={$t('download.options.cookiesDescription')}
  options={browserOptions}
  value={$settings.cookiesFromBrowser ?? ''}
  columns={3}
  onselect={async (val) => {
    updateSetting('cookiesFromBrowser', val);
    if (val === 'custom') {
      customCookiesInput = $settings.customCookies ?? '';
      customCookiesModalOpen = true;
    } else if (val === '') {
      updateSetting('customCookies', '');
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
        updateSetting('customCookies', customCookiesInput);
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
      <span class="summary-item"
        >{getLabel(videoQualityOptions, $settings.defaultVideoQuality ?? 'max')}</span
      >
      <span class="summary-item"
        >{getLabel(downloadModeOptions, $settings.defaultDownloadMode ?? 'auto')}</span
      >
      <span class="summary-item"
        >{getLabel(audioQualityOptions, $settings.defaultAudioQuality ?? 'best')}</span
      >
      {#if $settings.remux}<span class="summary-item">Remux</span>{/if}
      {#if $settings.convertToMp4}<span class="summary-item">MP4</span>{/if}
      {#if $settings.useAria2}<span class="summary-item">aria2</span>{/if}
      {#if $settings.sponsorBlock}<span class="summary-item">SponsorBlock</span>{/if}
      {#if $settings.chapters}<span class="summary-item">Chapters</span>{/if}
      {#if $settings.embedSubtitles}<span class="summary-item">Subtitles</span>{/if}
      {#if $settings.embedThumbnail}<span class="summary-item">Thumbnail</span>{/if}
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
        const val = Math.max(0, Math.min(100, parseInt(customSpeedInput) || 0));
        updateSetting('downloadSpeedLimit', val);
        speedLimitModalOpen = false;
        customSpeedInput = '';
      }}
    >
      {$t('common.apply')}
    </button>
  {/snippet}
</Modal>

<style>
  .view-container {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    visibility: hidden;
    opacity: 0;
    pointer-events: none;
    overflow-x: hidden;
    overflow-y: scroll;
    padding: 0 4px 0 var(--page-padding-inline);
    &:not(.active) * {
      transition: none !important;
      animation: none !important;
    }
  }

  @media (max-width: 480px) {
    .view-container {
      padding: 0 4px 0 8px;
    }
  }

  :global(.app.mobile) .view-container {
    padding-bottom: var(--mobile-nav-clearance, 0px);
  }

  .view-container.active {
    visibility: visible;
    opacity: 1;
    pointer-events: auto;
    z-index: 1;
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

  .search-torrent-btn {
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.5);
    border: none;
    border-radius: var(--radius, 8px);
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .search-torrent-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  :global(.rotate-180) {
    transform: rotate(180deg);
  }
</style>
