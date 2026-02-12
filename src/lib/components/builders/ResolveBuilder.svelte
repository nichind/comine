<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '$lib/i18n';
  import { getSettings, type DownloadMode } from '$lib/stores/settings';
  import { deps } from '$lib/stores/deps';
  import type { MediaPreview } from '$lib/stores/mediaCache';
  import type { VideoFormat } from '$lib/bindings/VideoFormat';
  import type { ClipRange } from '$lib/bindings/ClipRange';
  import type { Storyboard } from '$lib/bindings/Storyboard';
  import type {
    MediaItemData,
    ViewMode,
    MediaItemSettings,
  } from '$lib/components/media/MediaGrid.svelte';
  import ItemHeader from '$lib/components/resolve/ItemHeader.svelte';
  import ViewHeader from '$lib/components/resolve/ViewHeader.svelte';
  import FilenamePreview from '$lib/components/resolve/FilenamePreview.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { getPlatformName, getPlatformIcon } from '$lib/components/resolve/utils';
  import {
    FormatSection,
    OptionsSection,
    CollectionSection,
    FileSection,
    GalleryMetadata,
    MusicMetadata,
    SeriesMetadata,
    TorrentSection,
    HttpInfoSection,
    CloudInfoSection,
    TagsSection,
  } from '$lib/components/resolve/sections';
  import { queue, type QueueAddOptions } from '$lib/stores/queue';
  import { navigation } from '$lib/stores/navigation';
  import { toast } from '$lib/components/ui/Toast.svelte';
  import { formatSize } from '$lib/utils/format';
  import { useResolveSession } from '$lib/components/resolve/useResolveSession.svelte';

  interface Props {
    url: string;
    cookiesFromBrowser?: string;
    customCookies?: string;
    onBack?: () => void;
    onOpenItem?: (item: MediaItemData) => void;
    onOpenChannel?: (
      channelUrl: string,
      previewData?: { name?: string; thumbnail?: string }
    ) => void;
    prefetchedInfo?: MediaPreview;
  }

  let {
    url,
    cookiesFromBrowser = '',
    customCookies = '',
    onBack,
    onOpenItem,
    onOpenChannel,
    prefetchedInfo,
  }: Props = $props();

  const session = useResolveSession(() => ({
    url,
    cookiesFromBrowser,
    customCookies,
    prefetchedInfo,
  }));

  session.onSelectionUpdate((ids: Set<string>) => {
    selectedIds = ids;
  });

  let info = $derived(session.info);
  let loading = $derived(session.loading);
  let error = $derived(session.error);

  let viewMode = $state<ViewMode>('list');
  let selectedIds = $state<Set<string>>(new Set());
  let perItemSettings = $state<Map<string, Partial<MediaItemSettings>>>(new Map());
  let bulkMode = $state<DownloadMode | null>(null);
  let searchQuery = $state('');
  let showCollectionOptions = $state(false);
  let usePlaylistFolder = $state(true);

  let selectedFileIds = $state<Set<string>>(new Set());
  let lastFileCount = $state(0);

  $effect(() => {
    const files = info?.files;
    if (!files || files.length === 0) return;
    if (files.length !== lastFileCount) {
      lastFileCount = files.length;
      selectedFileIds = new Set(files.map((f, i) => String(f.index ?? i)));
    }
  });

  let selectedVideo = $state('best');
  let selectedAudio = $state('best');

  let outputTemplate = $state('%(title)s.%(ext)s');
  let embedSubs = $state(false);
  let subLangs = $state('en,ru');
  let embedChapters = $state(true);
  let embedThumbnail = $state(true);
  let embedMetadata = $state(true);
  let skipSponsors = $state(false);
  let skipIntros = $state(false);
  let skipSelfPromo = $state(false);
  let skipInteraction = $state(false);

  let clipRanges = $state<ClipRange[]>([]);

  let settingsSynced = false;
  $effect(() => {
    if (url && !settingsSynced) {
      const appSettings = getSettings();
      usePlaylistFolder = appSettings.usePlaylistFolders ?? true;
      outputTemplate = appSettings.ytdlpAdvanced?.outputTemplate || '%(title)s.%(ext)s';
      embedChapters = appSettings.chapters ?? true;
      embedThumbnail = appSettings.embedThumbnail ?? true;
      embedMetadata = !(appSettings.clearMetadata ?? false);
      embedSubs = appSettings.embedSubtitles ?? false;
      subLangs = appSettings.subtitleLanguages ?? 'en,ru';
      skipSponsors = appSettings.sponsorBlockSkipSponsors ?? false;
      skipIntros = appSettings.sponsorBlockSkipIntros ?? false;
      skipSelfPromo = appSettings.sponsorBlockSkipSelfPromo ?? false;
      skipInteraction = appSettings.sponsorBlockSkipInteraction ?? false;
      settingsSynced = true;
    }
  });

  $effect(() => {
    url;
    settingsSynced = false;
  });

  let hasYtdlp = $derived($deps.ytdlp?.installed ?? false);

  let isCollection = $derived(!!(info?.entries && info.entries.length > 0));
  let hasFormats = $derived(!!(info?.formats && info.formats.length > 0));
  let hasFiles = $derived(!!(info?.files && info.files.length > 0));
  let showTrackUI = $derived(!isCollection && (loading || hasFormats || !!prefetchedInfo));
  let isFileMode = $derived(!isCollection && !showTrackUI && !loading && hasFiles);

  let totalCount = $derived(
    info?.pagination?.totalCount ?? info?.playlistCount ?? info?.entries?.length ?? 0
  );
  let realEntries = $derived(info?.entries ?? []);

  function normalizeExternalUrl(url: string): string {
    if (url.startsWith('//')) return `https:${url}`;
    if (url.startsWith('http://')) return url.replace(/^http:\/\//, 'https://');
    return url;
  }

  function parseStoryboardsFromFormats(
    formats: VideoFormat[],
    videoDuration: number | null
  ): Storyboard[] | null {
    const storyboards: Storyboard[] = [];

    for (const f of formats) {
      const formatId = f.formatId || '';
      const formatNote = f.formatNote || '';
      const ext = f.ext || '';
      const fragments = f.fragments ?? null;
      const protocol = f.protocol || '';

      const isStoryboard =
        formatId.startsWith('sb') ||
        formatNote.toLowerCase().includes('storyboard') ||
        formatId.toLowerCase().includes('storyboard') ||
        (ext === 'mhtml' && !!fragments) ||
        (protocol === 'mhtml' && !!fragments) ||
        formatNote.toLowerCase().includes('sprite') ||
        (formatId.includes('thumb') && !!fragments);

      if (!isStoryboard) continue;
      if (!fragments || fragments.length === 0) continue;

      const width = f.width ?? 160;
      const height = f.height ?? 90;
      const cols = f.columns ?? 5;
      const rows = f.rows ?? 5;

      let totalFragmentsDuration = 0;
      for (const frag of fragments) {
        const d = typeof frag?.duration === 'number' ? frag.duration : 0;
        totalFragmentsDuration += d;
      }

      const avgFragmentDuration =
        fragments.length > 0 && totalFragmentsDuration > 0
          ? totalFragmentsDuration / fragments.length
          : 5.0;

      let templateUrl: string | null = null;
      let fragmentCount = fragments.length;

      if (fragments.length >= 2) {
        const url0 = fragments[0]?.url ?? '';
        const url1 = fragments[1]?.url ?? '';
        const match0 = url0.match(/(\d+)(\.[a-z]+)$/i);
        const match1 = url1.match(/(\d+)(\.[a-z]+)$/i);

        if (match0 && match1 && match0[2] === match1[2]) {
          const num0 = parseInt(match0[1]);
          const num1 = parseInt(match1[1]);
          if (num1 === num0 + 1) {
            templateUrl = url0.replace(/(\d+)(\.[a-z]+)$/i, '$M$2');
          }
        }
      }

      if (!templateUrl && fragments[0]?.url) {
        const fragUrl = fragments[0].url;
        if (/\/M\d+\.(jpg|webp|png)/i.test(fragUrl)) {
          templateUrl = fragUrl.replace(/\/M(\d+)\.(jpg|webp|png)/i, '/M$M.$2');
        }
      }

      let finalUrl =
        templateUrl ?? (f as VideoFormat & { url?: string }).url ?? fragments[0]?.url ?? '';
      finalUrl = normalizeExternalUrl(finalUrl);

      const isTemplateUrl = finalUrl.includes('$M');
      if (isTemplateUrl && videoDuration && videoDuration > 0 && avgFragmentDuration > 0) {
        fragmentCount = Math.ceil(videoDuration / avgFragmentDuration);
      }

      storyboards.push({
        url: finalUrl,
        width: Math.max(1, width),
        height: Math.max(1, height),
        cols: Math.max(1, cols),
        rows: Math.max(1, rows),
        fragmentCount,
        fragmentDuration: avgFragmentDuration,
      });
    }

    if (storyboards.length === 0) return null;
    storyboards.sort((a, b) => b.width * b.height - a.width * a.height);
    return storyboards;
  }

  let displayDuration = $derived.by(() => {
    if (info?.duration) return Number(info.duration);
    if (prefetchedInfo?.duration) return prefetchedInfo.duration;

    if (info?.formats && info.formats.length > 0) {
      let maxDuration = 0;
      for (const f of info.formats) {
        const d = (f as VideoFormat & { duration?: number }).duration;
        if (typeof d === 'number' && d > maxDuration) maxDuration = d;
      }
      if (maxDuration > 0) return maxDuration;
    }
    return null;
  });

  let displayStoryboards = $derived.by(() => {
    let videoDuration: number | null = null;
    if (info?.duration) videoDuration = Number(info.duration);
    else if (prefetchedInfo?.duration) videoDuration = prefetchedInfo.duration;
    else if (info?.formats && info.formats.length > 0) {
      let maxDur = 0;
      for (const f of info.formats) {
        const d = (f as VideoFormat & { duration?: number }).duration;
        if (typeof d === 'number' && d > maxDur) maxDur = d;
      }
      if (maxDur > 0) videoDuration = maxDur;
    }

    let sbs: Storyboard[] = [];
    if (info?.storyboards && info.storyboards.length > 0) {
      sbs = info.storyboards.map((sb: Storyboard) => {
        let sbUrl = sb.url;
        if (!sbUrl.includes('$M')) sbUrl = sbUrl.replace(/(\d+)(\.[a-z]+)$/i, '$M$2');
        sbUrl = normalizeExternalUrl(sbUrl);

        const isTemplateUrl = sbUrl.includes('$M');
        let fragmentCount = sb.fragmentCount;
        if (isTemplateUrl && videoDuration && videoDuration > 0 && sb.fragmentDuration > 0) {
          fragmentCount = Math.ceil(videoDuration / sb.fragmentDuration);
        }
        return { ...sb, url: sbUrl, fragmentCount };
      });
    } else {
      const parsed = parseStoryboardsFromFormats(info?.formats ?? [], videoDuration);
      if (parsed) sbs = parsed;
    }

    const withCoverage = sbs.map((sb) => {
      const coverage = sb.fragmentCount * sb.fragmentDuration;
      const coversFullVideo = !videoDuration || coverage >= videoDuration * 0.9;
      const quality = sb.width * sb.height;
      return { sb, coverage, coversFullVideo, quality };
    });

    withCoverage.sort((a, b) => {
      if (a.coversFullVideo !== b.coversFullVideo) return a.coversFullVideo ? -1 : 1;
      if (a.coversFullVideo && b.coversFullVideo) return b.quality - a.quality;
      if (a.coverage !== b.coverage) return b.coverage - a.coverage;
      return b.quality - a.quality;
    });

    return withCoverage.map((x) => x.sb);
  });

  let displayTitle = $derived(info?.title || prefetchedInfo?.title || null);
  let displayAuthor = $derived(info?.uploader || info?.channel || prefetchedInfo?.author || null);
  let displayThumbnail = $derived(info?.thumbnail || prefetchedInfo?.thumbnail || null);

  let estimatedSize = $derived.by(() => {
    const formats = info?.formats;
    if (!formats || formats.length === 0) return '';

    let totalSize = 0;
    const vidFmt =
      selectedVideo === 'best'
        ? formats
            .filter((f: VideoFormat) => !!f.vcodec && f.vcodec !== 'none')
            .sort((a: VideoFormat, b: VideoFormat) => {
              const aH = parseInt(a.resolution?.split('x')[1] || '0') || 0;
              const bH = parseInt(b.resolution?.split('x')[1] || '0') || 0;
              return bH - aH;
            })[0]
        : selectedVideo !== 'none'
          ? formats.find((f: VideoFormat) => f.formatId === selectedVideo)
          : null;

    const audFmt =
      selectedAudio === 'best'
        ? formats
            .filter(
              (f: VideoFormat) =>
                !!f.acodec && f.acodec !== 'none' && (!f.vcodec || f.vcodec === 'none')
            )
            .sort((a: VideoFormat, b: VideoFormat) => (b.abr ?? 0) - (a.abr ?? 0))[0]
        : selectedAudio !== 'none'
          ? formats.find((f: VideoFormat) => f.formatId === selectedAudio)
          : null;

    const vidIsMuxed =
      vidFmt &&
      !!vidFmt.vcodec &&
      vidFmt.vcodec !== 'none' &&
      !!vidFmt.acodec &&
      vidFmt.acodec !== 'none';

    if (vidFmt) totalSize += Number(vidFmt.filesize ?? vidFmt.filesizeApprox ?? 0);
    if (audFmt && !vidIsMuxed) totalSize += Number(audFmt.filesize ?? audFmt.filesizeApprox ?? 0);
    return totalSize > 0 ? formatSize(totalSize) : '';
  });

  function buildFormatString(): { formatString: string; downloadMode: 'auto' | 'audio' | 'mute' } {
    let formatString: string;
    let downloadMode: 'auto' | 'audio' | 'mute' = 'auto';

    if (!hasYtdlp) {
      if (selectedVideo === 'none') {
        downloadMode = 'audio';
        formatString = selectedAudio === 'best' ? '' : selectedAudio;
      } else if (selectedVideo === 'best') {
        formatString = '';
      } else {
        formatString = selectedVideo;
      }
    } else if (selectedVideo === 'none' && selectedAudio === 'none') {
      formatString = 'bestvideo+bestaudio/best';
    } else if (selectedVideo === 'best') {
      if (selectedAudio === 'best') {
        formatString = 'bestvideo+bestaudio/best';
      } else if (selectedAudio === 'none') {
        formatString = 'bestvideo';
        downloadMode = 'mute';
      } else {
        formatString = `bestvideo+${selectedAudio}`;
      }
    } else if (selectedVideo === 'none') {
      if (selectedAudio === 'best') {
        const formats = info?.formats ?? [];
        const bestM4a = formats.find((f) => {
          const hasAudio = !!f.acodec && f.acodec !== 'none';
          const hasVideo = !!f.vcodec && f.vcodec !== 'none';
          return hasAudio && !hasVideo && f.ext === 'm4a';
        });
        formatString = bestM4a?.formatId ?? 'bestaudio';
        downloadMode = 'audio';
      } else {
        formatString = selectedAudio;
        downloadMode = 'audio';
      }
    } else {
      const vidFmt = info?.formats?.find((f) => f.formatId === selectedVideo);
      const isMuxed =
        vidFmt &&
        !!vidFmt.vcodec &&
        vidFmt.vcodec !== 'none' &&
        !!vidFmt.acodec &&
        vidFmt.acodec !== 'none';

      if (isMuxed) {
        formatString = selectedVideo;
      } else if (selectedAudio === 'best') {
        formatString = `${selectedVideo}+bestaudio`;
      } else if (selectedAudio === 'none') {
        formatString = selectedVideo;
        downloadMode = 'mute';
      } else {
        formatString = `${selectedVideo}+${selectedAudio}`;
      }
    }

    return { formatString, downloadMode };
  }

  function getEffectiveSettings(id: string): MediaItemSettings {
    const appDefault = 'auto';
    const entry = info?.entries?.find((e) => e.id === id);
    const isMusic = entry?.isMusic ?? false;
    const globalDefault: DownloadMode = bulkMode ?? (isMusic ? 'audio' : appDefault);
    const override = perItemSettings.get(id) ?? {};

    return {
      downloadMode: override.downloadMode ?? globalDefault,
      skipSponsors: override.skipSponsors ?? skipSponsors,
      skipIntros: override.skipIntros ?? skipIntros,
      skipSelfPromo: override.skipSelfPromo ?? skipSelfPromo,
      skipInteraction: override.skipInteraction ?? skipInteraction,
      embedChapters: override.embedChapters ?? embedChapters,
      embedThumbnail: override.embedThumbnail ?? embedThumbnail,
      embedMetadata: override.embedMetadata ?? embedMetadata,
      embedSubs: override.embedSubs ?? embedSubs,
      subLangs: override.subLangs ?? subLangs,
    };
  }

  function updateItemSettings(id: string, update: Partial<MediaItemSettings>) {
    const newMap = new Map(perItemSettings);
    const existing = newMap.get(id) ?? {};
    newMap.set(id, { ...existing, ...update });
    perItemSettings = newMap;
  }

  function handleOpenItem(item: MediaItemData) {
    const entry = info?.entries?.find((e) => e.id === item.id);
    if (!entry) return;

    if (onOpenItem) {
      onOpenItem(item);
    } else {
      navigation.openVideo(entry.url, {
        title: entry.title ?? undefined,
        thumbnail: entry.thumbnail ?? undefined,
        author: entry.uploader ?? undefined,
        duration:
          typeof entry.duration === 'bigint'
            ? Number(entry.duration)
            : (entry.duration ?? undefined),
      });
    }
  }

  function handleGridScroll(scrollTop: number, scrollHeight: number, clientHeight: number) {
    const threshold = 500;
    if (scrollHeight - scrollTop - clientHeight < threshold) {
      session.fetchMore();
    }
  }

  async function handleDownload() {
    if (!info) return;

    if (isFileMode && info.files) {
      const selectedFiles = info.files.filter((_f, i) =>
        selectedFileIds.has(String(_f.index ?? i))
      );
      if (selectedFiles.length === 0) {
        toast.error($t('playlist.noSelection'));
        return;
      }

      const entries = selectedFiles.map((f) => ({
        url: f.url ?? info!.url,
        title: f.filename ?? undefined,
        thumbnail: f.thumbnail ?? undefined,
      }));

      queue.addPlaylist(
        entries,
        {
          playlistId: info.id ?? 'files',
          playlistTitle: info.title ?? 'Files',
          usePlaylistFolder: true,
        },
        {}
      );

      toast.success(
        $t('playlist.notification.downloadStarted').replace('{count}', String(entries.length))
      );
      onBack?.();
      return;
    }

    if (isCollection) {
      if (selectedIds.size === 0) {
        toast.error($t('playlist.noSelection'));
        return;
      }

      const selectedEntries = (info.entries ?? [])
        .filter((e) => selectedIds.has(e.id))
        .map((e) => {
          const s = getEffectiveSettings(e.id);
          return {
            url: e.url,
            title: e.title ?? undefined,
            thumbnail: e.thumbnail ?? undefined,
            author: e.uploader ?? undefined,
            duration:
              typeof e.duration === 'bigint' ? Number(e.duration) : (e.duration ?? undefined),

            downloadMode: s.downloadMode,

            sponsorBlock: s.skipSponsors || s.skipIntros || s.skipSelfPromo || s.skipInteraction,
            sponsorBlockSkipSponsors: s.skipSponsors,
            sponsorBlockSkipIntros: s.skipIntros,
            sponsorBlockSkipSelfPromo: s.skipSelfPromo,
            sponsorBlockSkipInteraction: s.skipInteraction,

            chapters: s.embedChapters,
            embedThumbnail: s.embedThumbnail,
            embedMetadata: s.embedMetadata,
            embedSubtitles: s.embedSubs,
            subtitleLanguages: s.subLangs,

            clearMetadata: !s.embedMetadata,
          };
        });

      if (selectedEntries.length === 0) return;

      queue.addPlaylist(
        selectedEntries,
        {
          playlistId: info.id ?? 'unknown',
          playlistTitle: info.title ?? 'Playlist',
          usePlaylistFolder,
        },
        {
          videoQuality: 'best',
        }
      );

      toast.success(
        $t('playlist.notification.downloadStarted').replace(
          '{count}',
          String(selectedEntries.length)
        )
      );
      onBack?.();
      return;
    }

    try {
      const { formatString, downloadMode } = buildFormatString();
      const sponsorBlock = skipSponsors || skipIntros || skipSelfPromo || skipInteraction;
      const duration = displayDuration ?? 0;
      const hasClips =
        clipRanges.length > 0 &&
        !(
          clipRanges.length === 1 &&
          clipRanges[0].start <= 0.5 &&
          clipRanges[0].end >= duration - 0.5
        );

      const options: QueueAddOptions = {
        videoQuality: formatString || undefined,
        downloadMode,
        outputTemplate: outputTemplate !== '%(title)s.%(ext)s' ? outputTemplate : undefined,
        embedSubtitles: embedSubs,
        subtitleLanguages: embedSubs ? subLangs : undefined,
        chapters: embedChapters,
        embedThumbnail,
        clearMetadata: !embedMetadata,
        sponsorBlock,
        sponsorBlockSkipSponsors: skipSponsors,
        sponsorBlockSkipIntros: skipIntros,
        sponsorBlockSkipSelfPromo: skipSelfPromo,
        sponsorBlockSkipInteraction: skipInteraction,
        clipRanges: hasClips ? clipRanges : undefined,
        cookiesFromBrowser: cookiesFromBrowser || undefined,
        customCookies: customCookies || undefined,
        prefetchedInfo: {
          title: displayTitle ?? undefined,
          thumbnail: displayThumbnail ?? undefined,
          author: displayAuthor ?? undefined,
          duration: displayDuration ?? undefined,
        },
      };

      await queue.add(info.url, options);
      toast.success($t('download.started'));
      onBack?.();
    } catch (err) {
      console.error('Download error:', err);
    }
  }

  function handleCancel() {
    session.cancel();
    onBack?.();
  }

  $effect(() => {
    if (url) {
      untrack(() => session.load());
    }
  });
</script>

<div class="resolve-builder">
  <ViewHeader
    platformName={getPlatformName(info, url)}
    platformIcon={getPlatformIcon(info)}
    {onBack}
    {estimatedSize}
    selectionCount={isCollection ? selectedIds.size : isFileMode ? selectedFileIds.size : undefined}
    totalCount={isCollection
      ? totalCount
      : isFileMode
        ? (info?.fileCount ?? info?.files?.length ?? 0)
        : undefined}
    {loading}
    downloadDisabled={!!error}
    onDownload={handleDownload}
    onCancel={loading ? handleCancel : undefined}
  />

  {#if error}
    <div class="error-state">
      <Icon name="warning" size={18} />
      <span>{$t('download.tracks.error')}</span>
      <button class="retry-btn" onclick={() => session.load()}>
        <Icon name="restart" size={14} />
      </button>
    </div>
  {:else}
    <div class="content-scroll">
      <ItemHeader {info} {loading} {onOpenChannel} currentUrl={url} {prefetchedInfo}>
        {#if !isCollection && !isFileMode}
          <FilenamePreview
            bind:template={outputTemplate}
            title={displayTitle}
            author={displayAuthor}
            {url}
            {loading}
          />
        {/if}
      </ItemHeader>

      {#if info?.torrent}
        <TorrentSection torrent={info.torrent} />
      {/if}
      {#if info?.gallery}
        <GalleryMetadata gallery={info.gallery} />
      {/if}
      {#if info?.music}
        <MusicMetadata music={info.music} />
      {/if}
      {#if info?.series}
        <SeriesMetadata series={info.series} />
      {/if}
      {#if info?.http}
        <HttpInfoSection http={info.http} filesize={info.filesize} mimeType={info.mimeType} />
      {/if}
      {#if info?.cloud}
        <CloudInfoSection cloud={info.cloud} />
      {/if}

      {#if hasFormats && !isCollection}
        <FormatSection
          formats={info?.formats ?? []}
          bind:selectedVideo
          bind:selectedAudio
          {loading}
          duration={displayDuration}
          storyboard={displayStoryboards?.[0]}
          chapters={info?.chapters}
          bind:clipRanges
        />
      {/if}

      {#if isCollection && info?.entries}
        <CollectionSection
          entries={realEntries}
          bind:selectedIds
          {perItemSettings}
          {totalCount}
          {loading}
          showSponsorBlock={hasYtdlp}
          bind:bulkMode
          bind:searchQuery
          bind:viewMode
          bind:usePlaylistFolder
          bind:skipSponsors
          bind:skipIntros
          bind:skipSelfPromo
          bind:skipInteraction
          bind:embedChapters
          bind:embedThumbnail
          bind:embedMetadata
          bind:embedSubs
          bind:subLangs
          onUpdateSettings={updateItemSettings}
          onOpenItem={handleOpenItem}
          onScroll={handleGridScroll}
          getDefaultSettings={(item) => getEffectiveSettings(item.id)}
        />
      {/if}

      {#if isFileMode && info?.files}
        <FileSection
          files={info.files}
          fileCount={info.fileCount}
          totalSize={info.filesize ? Number(info.filesize) : null}
          bind:selectedFileIds
        />
      {/if}

      {#if showTrackUI && !isCollection}
        <OptionsSection
          bind:skipSponsors
          bind:skipIntros
          bind:skipSelfPromo
          bind:skipInteraction
          bind:embedChapters
          bind:embedThumbnail
          bind:embedMetadata
          bind:embedSubs
          bind:subLangs
          showSponsorBlock={hasYtdlp}
          {loading}
        />
      {/if}

      {#if info?.tags?.length || info?.categories?.length}
        <TagsSection tags={info?.tags} categories={info?.categories} />
      {/if}
    </div>
  {/if}
</div>

<style>
  .resolve-builder {
    display: flex;
    flex-direction: column;
    height: 100%;
    color: var(--text-1, white);
  }

  .content-scroll {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 16px 4px 0 0;
    margin-bottom: 12px;
    flex: 1;
    margin-top: 4px;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .error-state {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px;
    color: rgba(239, 68, 68, 0.9);
    background: rgba(239, 68, 68, 0.1);
    border-radius: var(--radius, 8px);
    margin-top: 12px;
  }

  .retry-btn {
    margin-left: auto;
    padding: 6px;
    background: rgba(255, 255, 255, 0.06);
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
  }

  .retry-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  @media (max-width: 600px) {
    .content-scroll {
      gap: 16px;
    }
  }
</style>
