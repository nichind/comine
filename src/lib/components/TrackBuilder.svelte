<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '$lib/i18n';
  import { getProxyConfig, getSettings, settings } from '$lib/stores/settings';
  import ThumbnailGlow from './ThumbnailGlow.svelte';
  import { logs } from '$lib/stores/logs';
  import { deps } from '$lib/stores/deps';
  import { portal } from '$lib/actions/portal';
  import Skeleton from './Skeleton.svelte';
  import Icon from './Icon.svelte';
  import type { IconName } from './Icon.svelte';
  import Select from './Select.svelte';
  import Checkbox from './Checkbox.svelte';
  import Chip from './Chip.svelte';
  import CollapsibleBlock from './CollapsibleBlock.svelte';
  import ClipRangeSelector from './ClipRangeSelector.svelte';
  import { formatSize, formatDuration, extractYouTubeVideoId } from '$lib/utils/format';
  import {
    mediaCache,
    type VideoInfo as UnifiedVideoInfo,
    type VideoFormat as UnifiedVideoFormat,
    type SponsorBlockSegment,
  } from '$lib/stores/mediaCache';
  import { resolveUrl, convertProxyConfig } from '$lib/backend/mediaBackend';
  import type { Chapter, Storyboard, VideoFormat as BindingVideoFormat } from '$lib/bindings';

  type VideoFormat = UnifiedVideoFormat;

  interface VideoInfo {
    title: string;
    author: string | null;
    thumbnail: string | null;
    duration: number | null;
    formats: VideoFormat[];
    viewCount?: number | null;
    likeCount?: number | null;
    description?: string | null;
    uploadDate?: string | null;
    channelUrl?: string | null;
    channelId?: string | null;
    storyboards?: Storyboard[] | null;
    chapters?: Chapter[] | null;
    sponsorSegments?: SponsorBlockSegment[] | null;
  }

  function normalizeExternalUrl(url: string): string {
    if (url.startsWith('//')) return `https:${url}`;
    if (url.startsWith('http://')) return url.replace(/^http:\/\//, 'https://');
    return url;
  }

  function normalizeStoryboardUrl(url: string): string {
    return normalizeExternalUrl(url);
  }

  function parseStoryboardsFromFormats(formats: BindingVideoFormat[]): Storyboard[] | null {
    const storyboards: Storyboard[] = [];

    for (const f of formats) {
      const formatId = f.formatId || '';
      const formatNote = f.formatNote || '';
      const ext = f.ext || '';

      const fragments = f.fragments ?? null;

      const isStoryboard =
        formatId.startsWith('sb') ||
        formatNote.toLowerCase().includes('storyboard') ||
        (ext === 'mhtml' && !!fragments);

      if (!isStoryboard) continue;
      if (!fragments || fragments.length === 0) continue;

      const firstFrag = fragments[0];

      const width = 160;
      const height = 90;
      const cols = f.columns ?? 10;
      const rows = f.rows ?? 10;

      const fragmentDuration =
        typeof firstFrag?.duration === 'number' && Number.isFinite(firstFrag.duration)
          ? firstFrag.duration
          : 2.0;

      const urlRaw =
        typeof (f as any).url === 'string'
          ? ((f as any).url as string)
          : typeof firstFrag?.url === 'string'
            ? (firstFrag.url as string)
            : '';
      if (!urlRaw) continue;

      storyboards.push({
        url: normalizeStoryboardUrl(urlRaw),
        width: Math.max(1, width),
        height: Math.max(1, height),
        cols: Math.max(1, cols),
        rows: Math.max(1, rows),
        fragmentCount: fragments.length,
        fragmentDuration,
      });
    }

    if (storyboards.length === 0) return null;
    storyboards.sort((a, b) => b.width * b.height - a.width * a.height);
    return storyboards;
  }

  export interface TrackSelection {
    formatString: string;
    downloadMode: 'auto' | 'audio' | 'mute';
    title?: string;
    author?: string;
    thumbnail?: string;
    duration?: number;
    embedSubs?: boolean;
    subLangs?: string;
    embedChapters?: boolean;
    sponsorblock?: string[];
    embedThumbnail?: boolean;
    embedMetadata?: boolean;
    outputTemplate?: string;
    clipRanges?: { id: string; start: number; end: number }[];
  }

  export interface PrefetchedInfo {
    title?: string;
    thumbnail?: string;
    author?: string;
    duration?: number;
  }

  export interface DefaultSettings {
    sponsorBlock?: boolean;
    sponsorBlockSkipSponsors?: boolean;
    sponsorBlockSkipIntros?: boolean;
    sponsorBlockSkipSelfPromo?: boolean;
    sponsorBlockSkipInteraction?: boolean;
    chapters?: boolean;
    embedSubtitles?: boolean;
    subtitleLanguages?: string;
    embedThumbnail?: boolean;
    clearMetadata?: boolean;
  }

  interface Props {
    url: string;
    cookiesFromBrowser?: string;
    customCookies?: string;
    defaults?: DefaultSettings;
    ondownload?: (selection: TrackSelection) => void;
    onback?: () => void;
    onopenchannel?: (
      channelUrl: string,
      previewData?: { name?: string; thumbnail?: string }
    ) => void;
    showHeader?: boolean;
    backLabel?: string;
    prefetchedInfo?: PrefetchedInfo;
  }

  let {
    url,
    cookiesFromBrowser = '',
    customCookies = '',
    defaults,
    ondownload,
    onback,
    onopenchannel,
    showHeader = false,
    backLabel,
    prefetchedInfo,
  }: Props = $props();

  const CACHE_TTL = 10 * 60 * 1000;

  function getInitialState() {
    const uiState = mediaCache.getUIState(url);
    const cachedFormats = mediaCache.getFormats(url);
    const cachedVideoInfo = mediaCache.getVideoInfo(url);

    let info: VideoInfo | null = null;
    if (cachedVideoInfo && cachedFormats) {
      info = {
        title: cachedVideoInfo.title,
        author: cachedVideoInfo.author,
        thumbnail: cachedVideoInfo.thumbnail,
        duration: cachedVideoInfo.duration,
        viewCount: cachedVideoInfo.viewCount,
        likeCount: cachedVideoInfo.likeCount,
        description: cachedVideoInfo.description,
        uploadDate: cachedVideoInfo.uploadDate,
        channelUrl: cachedVideoInfo.channelUrl,
        channelId: cachedVideoInfo.channelId,
        chapters: cachedVideoInfo.chapters ?? null,
        storyboards: cachedVideoInfo.storyboards ?? null,
        sponsorSegments: cachedVideoInfo.sponsorSegments ?? null,
        formats: cachedFormats,
      };
    }

    const hasFullData = !!info;

    return {
      info,
      selectedVideo: uiState?.selectedVideo ?? 'best',
      selectedAudio: uiState?.selectedAudio ?? 'best',
      loading: !hasFullData,
      lastLoadedUrl: hasFullData ? url : '',
      fromCache: hasFullData,
    };
  }

  function formatCount(num: number | null | undefined): string {
    if (!num) return '';
    if (num >= 1_000_000) return (num / 1_000_000).toFixed(1).replace(/\.0$/, '') + 'M';
    if (num >= 1_000) return (num / 1_000).toFixed(1).replace(/\.0$/, '') + 'K';
    return num.toString();
  }

  function formatUploadDate(dateStr: string | null | undefined): string {
    if (!dateStr || dateStr.length !== 8) return '';
    const year = dateStr.slice(0, 4);
    const month = dateStr.slice(4, 6);
    const day = dateStr.slice(6, 8);
    const date = new Date(`${year}-${month}-${day}`);
    return date.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
  }

  function cleanDescription(desc: string | null | undefined): string {
    if (!desc) return '';
    return desc.trim().replace(/\n{3,}/g, '\n\n');
  }

  function getCodec(codec: string | null): string {
    if (!codec || codec === 'none') return '';
    if (codec.startsWith('avc1')) return 'H.264';
    if (codec.startsWith('av01')) return 'AV1';
    if (codec.startsWith('vp9') || codec.startsWith('vp09')) return 'VP9';
    if (codec.startsWith('hev1') || codec.startsWith('hvc1')) return 'H.265';
    if (codec.startsWith('mp4a')) return 'AAC';
    if (codec.startsWith('opus')) return 'Opus';
    return codec.split('.')[0];
  }

  function getFmtSize(f: VideoFormat): string {
    const size = f.filesize ?? f.filesizeApprox;
    if (size) return formatSize(Number(size));
    return '';
  }

  function makeVideoLabel(f: VideoFormat): string {
    const parts: string[] = [];

    if (f.resolution) {
      const resParts = f.resolution.split('x');
      if (resParts.length >= 2 && resParts[1]) {
        const height = parseInt(resParts[1]);
        if (!isNaN(height) && height > 0) {
          parts.push(height + 'p');
        } else {
          parts.push(f.resolution); // Use as-is if can't parse
        }
      } else if (resParts.length === 1 && resParts[0]) {
        const clean = resParts[0].replace('p', '');
        const height = parseInt(clean);
        if (!isNaN(height) && height > 0) {
          parts.push(height + 'p');
        } else {
          parts.push(f.resolution);
        }
      } else {
        parts.push('?');
      }
    } else if (f.formatNote) {
      parts.push(f.formatNote);
    } else {
      parts.push('?');
    }

    if (f.fps && f.fps > 30) parts.push(`${Math.round(f.fps)}fps`);
    const codec = getCodec(f.vcodec);
    if (codec) parts.push(codec);
    const size = getFmtSize(f);
    if (size) parts.push(size);
    return parts.join(' · ');
  }

  function makeAudioLabel(f: VideoFormat): string {
    const parts: string[] = [];
    if (f.abr) parts.push(`${Math.round(f.abr)} kbps`);
    const codec = getCodec(f.acodec);
    if (codec) parts.push(codec);
    const size = getFmtSize(f);
    if (size) parts.push(size);
    return parts.join(' · ') || 'audio';
  }

  const initialState = getInitialState();
  let loading = $state(initialState.loading);
  let error = $state<string | null>(null);
  let info = $state<VideoInfo | null>(initialState.info);
  let processedThumbnail = $state<string | null>(
    initialState.info?.thumbnail ? normalizeExternalUrl(initialState.info.thumbnail) : null
  );
  let lastLoadedUrl = $state(initialState.lastLoadedUrl);

  let destroyed = false;
  let thumbnailError = $state(false);
  let refreshedMissingChannelForUrl = $state<string | null>(null);

  let selectedVideo = $state<string>(initialState.selectedVideo);
  let selectedAudio = $state<string>(initialState.selectedAudio);

  let showDescription = $state(false);

  const initialDefaults = untrack(() => ({
    embedSubtitles: defaults?.embedSubtitles ?? false,
    subtitleLanguages: defaults?.subtitleLanguages ?? 'en,ru',
    sponsorBlock: defaults?.sponsorBlock ?? false,
    sponsorBlockSkipSponsors: defaults?.sponsorBlockSkipSponsors ?? true,
    sponsorBlockSkipIntros: defaults?.sponsorBlockSkipIntros ?? false,
    sponsorBlockSkipSelfPromo: defaults?.sponsorBlockSkipSelfPromo ?? false,
    sponsorBlockSkipInteraction: defaults?.sponsorBlockSkipInteraction ?? false,
    chapters: defaults?.chapters ?? true,
    embedThumbnail: defaults?.embedThumbnail ?? true,
    clearMetadata: defaults?.clearMetadata ?? false,
  }));

  let embedSubs = $state(initialDefaults.embedSubtitles);
  let subLangs = $state(initialDefaults.subtitleLanguages);

  let skipSponsors = $state(
    initialDefaults.sponsorBlock ? initialDefaults.sponsorBlockSkipSponsors : false
  );
  let skipIntros = $state(
    initialDefaults.sponsorBlock ? initialDefaults.sponsorBlockSkipIntros : false
  );
  let skipSelfPromo = $state(
    initialDefaults.sponsorBlock ? initialDefaults.sponsorBlockSkipSelfPromo : false
  );
  let skipInteraction = $state(
    initialDefaults.sponsorBlock ? initialDefaults.sponsorBlockSkipInteraction : false
  );

  let embedChapters = $state(initialDefaults.chapters);
  let embedThumbnail = $state(initialDefaults.embedThumbnail);
  let embedMetadata = $state(!initialDefaults.clearMetadata);

  const initialOutputTemplate = $settings.ytdlpAdvanced?.outputTemplate || '%(title)s.%(ext)s';
  let outputTemplate = $state(initialOutputTemplate);
  let isEditingFilename = $state(false);

  interface ClipRange {
    id: string;
    start: number;
    end: number;
  }
  let clipRanges = $state<ClipRange[]>([]);

  let sponsorSegments = $state<SponsorBlockSegment[] | null>(
    initialState.info?.sponsorSegments ?? null
  );
  let sponsorSegmentsLoading = $state(false);

  let showMoreOptions = $state(false);

  type PresetId = 'best' | 'music' | 'custom' | string;
  let selectedPreset = $state<PresetId>('best');
  let isYouTubeMusic = $derived(url.includes('music.youtube.com'));
  let didInitialPreset = $state(false);

  let hasYtdlp = $derived($deps.ytdlp?.installed ?? false);
  let platformName = $derived.by(() => {
    if (url.includes('bilibili.com') || url.includes('b23.tv')) return 'Bilibili';
    if (url.includes('douyin.com')) return 'Douyin';
    if (url.includes('iqiyi.com')) return 'iQIYI';
    if (url.includes('youku.com')) return 'Youku';
    if (url.includes('qq.com')) return 'Tencent';
    if (url.includes('mgtv.com')) return 'MGTV';
    if (url.includes('weibo.com')) return 'Weibo';
    if (url.includes('kuaishou.com')) return 'Kuaishou';
    return 'Video';
  });
  let isYtdlp = $derived(hasYtdlp);

  const presets = $derived.by(() => {
    const available: { id: PresetId; label: string; icon: IconName }[] = [];

    available.push({ id: 'best', label: $t('download.tracks.presetBest'), icon: 'video' });

    const resolutionCounts = new Map<number, number>();
    if (videoFormats.length > 0) {
      videoFormats.forEach((f) => {
        let height = 0;

        if (f.resolution) {
          const parts = f.resolution.split('x');
          if (parts.length >= 2) {
            height = parseInt(parts[1]);
          } else if (parts.length === 1) {
            height = parseInt(f.resolution.replace('p', ''));
          }
        }

        if (height === 0 && f.formatNote) {
          const match = f.formatNote.match(/(\d+)p/);
          if (match) {
            height = parseInt(match[1]);
          }
        }

        if (height > 0) {
          resolutionCounts.set(height, (resolutionCounts.get(height) || 0) + 1);
        }
      });
    }

    const sortedResolutions = Array.from(resolutionCounts.keys())
      .sort((a, b) => b - a)
      .slice(0, 3);

    sortedResolutions.forEach((height) => {
      available.push({
        id: `${height}p`,
        label: `${height}p`,
        icon: 'video' as IconName,
      });
    });

    if (audioFormats.length > 0 || videoFormats.some((f) => f.hasAudio)) {
      available.push({ id: 'music', label: $t('download.tracks.presetMusic'), icon: 'music' });
    }

    return available;
  });

  function applyPreset(preset: PresetId) {
    selectedPreset = preset;

    if (preset === 'best') {
      selectedVideo = 'best';
      selectedAudio = 'best';
    } else if (preset === 'music') {
      selectedVideo = 'none';
      selectedAudio = 'best';
    } else if (preset.endsWith('p')) {
      const height = parseInt(preset.slice(0, -1));
      selectedVideo = findVideoByHeight(height) || 'best';
      selectedAudio = 'best';
    }
  }

  function findVideoByHeight(targetHeight: number): string | null {
    if (!videoFormats.length) return null;
    const match = videoFormats.find((f) => {
      const h = parseInt(f.resolution?.split('x')[1] || '0') || 0;
      return h === targetHeight;
    });
    return match?.formatId || null;
  }

  $effect(() => {
    if (
      !loading &&
      info &&
      isYouTubeMusic &&
      $settings.youtubeMusicAudioOnly &&
      !didInitialPreset
    ) {
      didInitialPreset = true;
      applyPreset('music');
    }
  });

  function markCustomPreset() {
    selectedPreset = 'custom';
  }
  $effect(() => {
    if (selectedVideo === 'none' && selectedAudio === 'none') {
      selectedAudio = 'best';
    }
  });

  let hasSeparateStreams = $derived(
    info?.formats?.some((f) => (f.hasVideo && !f.hasAudio) || (f.hasAudio && !f.hasVideo)) ?? false
  );

  let hasMuxedFormats = $derived(info?.formats?.some((f) => f.hasVideo && f.hasAudio) ?? false);

  let useDualSelectors = $derived(hasSeparateStreams);

  let videoFormats = $derived(
    info?.formats
      .filter((f) => f.hasVideo)
      .sort((a, b) => {
        const aH = parseInt(a.resolution?.split('x')[1] || '0') || 0;
        const bH = parseInt(b.resolution?.split('x')[1] || '0') || 0;
        return bH - aH;
      }) ?? []
  );

  let audioFormats = $derived(
    info?.formats
      .filter((f) => f.hasAudio && !f.hasVideo)
      .sort((a, b) => (b.abr ?? 0) - (a.abr ?? 0)) ?? []
  );

  let muxedFormats = $derived(
    info?.formats
      .filter((f) => f.hasVideo && f.hasAudio)
      .sort((a, b) => {
        const aH = parseInt(a.resolution?.split('x')[1] || '0') || 0;
        const bH = parseInt(b.resolution?.split('x')[1] || '0') || 0;
        return bH - aH;
      }) ?? []
  );

  let selectedVideoIsMuxed = $derived(
    selectedVideo !== 'best' && selectedVideo !== 'none'
      ? (info?.formats.find((f) => f.formatId === selectedVideo)?.hasAudio ?? false)
      : false
  );

  let muxedOptions = $derived([
    { value: 'best', label: $t('download.tracks.best') },
    ...muxedFormats.map((f) => ({ value: f.formatId, label: makeVideoLabel(f) })),
  ]);

  let selectedMuxed = $state('best');

  function getBestAudioFormat(options?: { preferM4a?: boolean }) {
    if (audioFormats.length === 0) return null;

    if (options?.preferM4a) {
      const bestM4a = audioFormats.find((f) => f.ext === 'm4a');
      return bestM4a ?? audioFormats[0];
    }

    return audioFormats[0];
  }

  let bestVideoDetails = $derived(() => {
    if (videoFormats.length === 0) return '';
    const best = videoFormats[0];
    return makeVideoLabel(best);
  });

  let bestAudioDetails = $derived(() => {
    if (audioFormats.length === 0) return '';
    const best = getBestAudioFormat({ preferM4a: selectedVideo === 'none' });
    if (!best) return '';
    return makeAudioLabel(best);
  });

  let videoOptions = $derived(
    loading
      ? [{ value: 'best', label: $t('download.tracks.loading') }]
      : [
          {
            value: 'best',
            label: bestVideoDetails()
              ? `${$t('download.tracks.bestQuality')} (${bestVideoDetails()})`
              : $t('download.tracks.bestQuality'),
          },
          { value: 'none', label: $t('download.tracks.noVideo') },
          ...videoFormats.map((f) => ({ value: f.formatId, label: makeVideoLabel(f) })),
        ]
  );

  let audioOptions = $derived(
    loading
      ? [{ value: 'best', label: $t('download.tracks.loading') }]
      : [
          {
            value: 'best',
            label: bestAudioDetails()
              ? `${$t('download.tracks.bestQuality')} (${bestAudioDetails()})`
              : $t('download.tracks.bestQuality'),
            disabled: selectedVideoIsMuxed,
          },
          ...(selectedVideo !== 'none'
            ? [
                {
                  value: 'none',
                  label: $t('download.tracks.noAudio'),
                  disabled: selectedVideoIsMuxed,
                },
              ]
            : []),
          ...audioFormats.map((f) => ({
            value: f.formatId,
            label: makeAudioLabel(f),
            disabled: selectedVideoIsMuxed,
          })),
        ]
  );

  let videoOptionsWithValidation = $derived(
    loading
      ? [{ value: 'best', label: $t('download.tracks.loading') }]
      : [
          {
            value: 'best',
            label: bestVideoDetails()
              ? `${$t('download.tracks.bestQuality')} (${bestVideoDetails()})`
              : $t('download.tracks.bestQuality'),
          },
          ...(selectedAudio !== 'none'
            ? [{ value: 'none', label: $t('download.tracks.noVideo') }]
            : []),
          ...videoFormats.map((f) => ({ value: f.formatId, label: makeVideoLabel(f) })),
        ]
  );

  let displayTitle = $derived(info?.title ?? prefetchedInfo?.title ?? '');
  let displayAuthor = $derived(info?.author ?? prefetchedInfo?.author ?? '');
  let displayDuration = $derived(info?.duration ?? prefetchedInfo?.duration ?? null);
  let displayThumbnail = $derived(
    processedThumbnail || info?.thumbnail || prefetchedInfo?.thumbnail
  );

  let lastThumbnailSrc = $state<string | null>(null);
  $effect(() => {
    const next = displayThumbnail ?? null;
    if (next !== lastThumbnailSrc) {
      lastThumbnailSrc = next;
      thumbnailError = false;
    }
  });

  let estimatedSize = $derived(() => {
    let total = 0;
    let hasEstimate = false;

    if (selectedVideo !== 'none') {
      if (selectedVideo === 'best' && videoFormats.length > 0) {
        const size = videoFormats[0].filesize ?? videoFormats[0].filesizeApprox;
        if (size) {
          total += Number(size);
          hasEstimate = true;
        }
      } else {
        const fmt = videoFormats.find((f) => f.formatId === selectedVideo);
        if (fmt) {
          const size = fmt.filesize ?? fmt.filesizeApprox;
          if (size) {
            total += Number(size);
            hasEstimate = true;
          }
        }
      }
    }

    if (selectedAudio !== 'none') {
      if (selectedAudio === 'best' && audioFormats.length > 0) {
        const best = getBestAudioFormat({ preferM4a: selectedVideo === 'none' });
        const size = best?.filesize ?? best?.filesizeApprox;
        if (size) {
          total += Number(size);
          hasEstimate = true;
        }
      } else {
        const fmt = audioFormats.find((f) => f.formatId === selectedAudio);
        if (fmt) {
          const size = fmt.filesize ?? fmt.filesizeApprox;
          if (size) {
            total += Number(size);
            hasEstimate = true;
          }
        }
      }
    }

    return hasEstimate ? formatSize(total) : null;
  });

  let predictedExtension = $derived.by(() => {
    if (selectedVideo === 'none') {
      if (selectedAudio === 'best') {
        const best = getBestAudioFormat({ preferM4a: true });
        return best?.ext ?? 'm4a';
      }
      const fmt = audioFormats.find((f) => f.formatId === selectedAudio);
      return fmt?.ext ?? 'm4a';
    }

    let videoFmt: VideoFormat | undefined;
    if (selectedVideo === 'best') {
      videoFmt = videoFormats[0];
    } else {
      videoFmt = videoFormats.find((f) => f.formatId === selectedVideo);
    }

    if (videoFmt?.hasAudio) {
      return videoFmt.ext ?? 'mp4';
    }

    if (selectedAudio === 'none') {
      return videoFmt?.ext ?? 'mp4';
    }

    let audioFmt: VideoFormat | undefined;
    if (selectedAudio === 'best') {
      audioFmt = getBestAudioFormat({ preferM4a: false }) ?? undefined;
    } else {
      audioFmt = audioFormats.find((f) => f.formatId === selectedAudio);
    }

    const videoExt = videoFmt?.ext ?? 'mp4';
    const audioExt = audioFmt?.ext ?? 'm4a';

    const mp4Compatible = ['mp4', 'm4a', 'm4v', 'mov'];
    if (mp4Compatible.includes(videoExt) && mp4Compatible.includes(audioExt)) {
      return 'mp4';
    }

    if (videoExt === 'webm' && audioExt === 'webm') {
      return 'webm';
    }

    return 'mkv';
  });

  function sanitizeFilename(name: string): string {
    return name.replace(/[<>:"/\\|?*]/g, '_').trim();
  }

  let selectedVideoFormat = $derived.by(() => {
    if (selectedVideo === 'none') return null;
    if (selectedVideo === 'best') return videoFormats[0] ?? null;
    return videoFormats.find((f) => f.formatId === selectedVideo) ?? null;
  });

  let selectedAudioFormat = $derived.by(() => {
    if (selectedAudio === 'none') return null;
    if (selectedAudio === 'best')
      return getBestAudioFormat({ preferM4a: selectedVideo === 'none' }) ?? null;
    return audioFormats.find((f) => f.formatId === selectedAudio) ?? null;
  });

  let filenamePreview = $derived.by(() => {
    const template = outputTemplate || '%(title)s.%(ext)s';
    const ext = predictedExtension;
    const title = sanitizeFilename(displayTitle || 'video');
    const uploader = sanitizeFilename(displayAuthor || 'Unknown');
    const id = url.match(/(?:v=|\/)([\w-]{11})(?:\?|&|$)/)?.[1] ?? 'unknown';
    const uploadDate = info?.uploadDate ?? '';
    const duration = displayDuration ?? 0;
    const viewCount = info?.viewCount ?? 0;
    const likeCount = info?.likeCount ?? 0;
    const resolution = selectedVideoFormat?.resolution ?? 'unknown';
    const fps = selectedVideoFormat?.fps ?? 0;
    const vcodec = selectedVideoFormat?.vcodec ?? 'unknown';
    const acodec = selectedAudioFormat?.acodec ?? 'unknown';

    return template
      .replace(/%\(ext\)s/g, ext)
      .replace(/%\(title\)s/g, title)
      .replace(/%\(uploader\)s/g, uploader)
      .replace(/%\(channel\)s/g, uploader)
      .replace(/%\(id\)s/g, id)
      .replace(/%\(upload_date\)s/g, uploadDate)
      .replace(/%\(duration\)s/g, String(duration))
      .replace(/%\(duration_string\)s/g, formatDuration(duration))
      .replace(/%\(view_count\)s/g, String(viewCount))
      .replace(/%\(like_count\)s/g, String(likeCount))
      .replace(/%\(resolution\)s/g, resolution)
      .replace(/%\(fps\)s/g, String(fps))
      .replace(/%\(vcodec\)s/g, vcodec)
      .replace(/%\(acodec\)s/g, acodec)
      .replace(/%\([^)]+\)s/g, '_');
  });

  function buildSelection(): TrackSelection {
    let formatString: string;
    let downloadMode: 'auto' | 'audio' | 'mute' = 'auto';

    if (!isYtdlp) {
      if (!useDualSelectors) {
        formatString = selectedMuxed === 'best' ? '' : selectedMuxed;
      } else {
        if (selectedVideo === 'none') {
          downloadMode = 'audio';
          formatString = selectedAudio === 'best' ? '' : selectedAudio;
        } else if (selectedVideo === 'best') {
          formatString = '';
        } else {
          formatString = selectedVideo;
        }
      }
    } else if (!useDualSelectors) {
      formatString = selectedMuxed === 'best' ? 'best' : selectedMuxed;
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
        const best = getBestAudioFormat({ preferM4a: true });
        formatString = best?.formatId ?? 'bestaudio';
        downloadMode = 'audio';
      } else {
        formatString = selectedAudio;
        downloadMode = 'audio';
      }
    } else {
      if (selectedVideoIsMuxed) {
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

    const sponsorblockCategories: string[] = [];
    if (isYtdlp) {
      if (skipSponsors) sponsorblockCategories.push('sponsor');
      if (skipIntros) sponsorblockCategories.push('intro', 'outro');
      if (skipSelfPromo) sponsorblockCategories.push('selfpromo');
      if (skipInteraction) sponsorblockCategories.push('interaction');
    }

    return {
      formatString,
      downloadMode,
      title: displayTitle || undefined,
      author: displayAuthor || undefined,
      thumbnail: displayThumbnail ?? undefined,
      duration: displayDuration ?? undefined,
      embedSubs,
      subLangs: embedSubs ? subLangs : undefined,
      embedChapters,
      sponsorblock: sponsorblockCategories.length > 0 ? sponsorblockCategories : undefined,
      embedThumbnail,
      embedMetadata,
      outputTemplate: outputTemplate !== initialOutputTemplate ? outputTemplate : undefined,
      clipRanges:
        clipRanges.length > 0 &&
        !(
          clipRanges.length === 1 &&
          clipRanges[0].start <= 0.5 &&
          clipRanges[0].end >= (displayDuration ?? 0) - 0.5
        )
          ? clipRanges.map((r) => ({ id: r.id, start: r.start, end: r.end }))
          : undefined,
    };
  }

  function handleDownload() {
    if (loading || error) return;
    ondownload?.(buildSelection());
  }

  function getNormalizedChannelUrl(): string | null {
    const raw = info?.channelUrl;
    if (!raw) return null;
    return normalizeExternalUrl(raw);
  }

  function handleOpenChannel() {
    const channelUrl = getNormalizedChannelUrl();
    if (!channelUrl || !onopenchannel) return;

    onopenchannel(channelUrl, {
      name: info?.author ?? undefined,
      thumbnail: undefined,
    });
  }

  let canOpenChannel = $derived(!!getNormalizedChannelUrl() && !!onopenchannel);

  $effect(() => {
    if (!onopenchannel) return;
    if (loading) return;
    if (!info) return;
    if (getNormalizedChannelUrl()) return;
    if (refreshedMissingChannelForUrl === url) return;

    refreshedMissingChannelForUrl = url;
    loadInfo({ silent: true });
  });

  $effect(() => {
    if (url && url !== lastLoadedUrl && !info) {
      loadInfo();
    }
  });

  function saveToCache() {
    if (url) {
      mediaCache.setUIState(url, {
        selectedVideo,
        selectedAudio,
        scrollTop: 0,
      });

      if (info) {
        mediaCache.setVideoInfo(url, {
          title: info.title,
          author: info.author,
          thumbnail: info.thumbnail,
          duration: info.duration,
          viewCount: info.viewCount ?? null,
          likeCount: info.likeCount ?? null,
          uploadDate: info.uploadDate ?? null,
          description: info.description ?? null,
          channelUrl: info.channelUrl ?? null,
          channelId: info.channelId ?? null,
          chapters: info.chapters ?? null,
          storyboards: info.storyboards ?? null,
          sponsorSegments: null,
        });

        mediaCache.setFormats(url, info.formats);
      }
    }
  }

  $effect(() => {
    return () => {
      destroyed = true;
      saveToCache();
      info = null;
      processedThumbnail = null;
    };
  });

  async function processThumbnail(thumbUrl: string) {
    if (!thumbUrl || destroyed) return;
    processedThumbnail = normalizeExternalUrl(thumbUrl);
  }

  function updateCachedSponsorSegments(segments: SponsorBlockSegment[]) {
    const cachedInfo = mediaCache.getVideoInfo(url);
    if (cachedInfo) {
      mediaCache.setVideoInfo(url, {
        ...cachedInfo,
        sponsorSegments: segments,
      });
    }
  }

  async function fetchSponsorBlockSegments(videoUrl: string) {
    const videoId = extractYouTubeVideoId(videoUrl);
    if (!videoId) {
      logs.debug('tracks', 'Not a YouTube video, skipping SponsorBlock fetch');
      return;
    }

    sponsorSegmentsLoading = true;
    try {
      const categories = JSON.stringify([
        'sponsor',
        'selfpromo',
        'interaction',
        'intro',
        'outro',
        'preview',
        'music_offtopic',
        'filler',
        'poi_highlight',
      ]);
      const response = await fetch(
        `https://sponsor.ajay.app/api/skipSegments?videoID=${videoId}&categories=${encodeURIComponent(categories)}`
      );

      if (response.status === 404) {
        sponsorSegments = [];
        updateCachedSponsorSegments([]);
        logs.debug('tracks', `No SponsorBlock segments for video: ${videoId}`);
        return;
      }

      if (!response.ok) {
        throw new Error(`SponsorBlock API error: ${response.status}`);
      }

      const data = (await response.json()) as SponsorBlockSegment[];
      sponsorSegments = data;
      updateCachedSponsorSegments(data);
      logs.info('tracks', `Loaded ${data.length} SponsorBlock segments for video: ${videoId}`);
    } catch (err) {
      logs.warn('tracks', `Failed to fetch SponsorBlock segments: ${err}`);
      sponsorSegments = null;
    } finally {
      sponsorSegmentsLoading = false;
    }
  }

  async function loadInfo(options?: { silent?: boolean }) {
    if (destroyed) return;
    const silent = options?.silent ?? false;
    const hadInfo = !!info;
    if (!silent || !hadInfo) {
      loading = true;
    }
    error = null;
    thumbnailError = false;
    if (!silent || !hadInfo) {
      processedThumbnail = null;
    }
    lastLoadedUrl = url;

    try {
      logs.info('tracks', `Fetching info for: ${url}`);

      let loadedInfo: VideoInfo;

      const currentSettings = getSettings();
      const resolveResult = await resolveUrl(url, {
        cookies_from_browser: cookiesFromBrowser || null,
        custom_cookies: customCookies || null,
        proxy: convertProxyConfig(getProxyConfig()),
        youtube_player_client: currentSettings.usePlayerClientForExtraction
          ? currentSettings.youtubePlayerClient || null
          : null,
      });

      if (destroyed) return;

      const raw = resolveResult.info;
      const storyboards = raw.storyboards ?? parseStoryboardsFromFormats(raw.formats ?? []);
      const normalizedChannelUrl = raw.channelUrl ? normalizeExternalUrl(raw.channelUrl) : null;

      let thumbnail = raw.thumbnail ? normalizeExternalUrl(raw.thumbnail) : null;
      const formatsArray = raw.formats ?? [];

      loadedInfo = {
        title: raw.title || url,
        author: raw.uploader || raw.channel || null,
        thumbnail: thumbnail,
        duration: raw.duration ? Number(raw.duration) : null,
        viewCount: raw.viewCount ? Number(raw.viewCount) : null,
        likeCount: raw.likeCount ? Number(raw.likeCount) : null,
        description: raw.description ?? null,
        uploadDate: raw.uploadDate ?? null,
        channelUrl: raw.channelUrl ?? null,
        channelId: raw.channelId ?? null,
        storyboards: storyboards,
        chapters: raw.chapters ?? null,
        sponsorSegments: null,
        formats: formatsArray.map((f) => ({
          formatId: f.formatId || '',
          ext: f.ext || '',
          resolution: f.resolution || null,
          fps: f.fps ? Number(f.fps) : null,
          vcodec: f.vcodec || null,
          acodec: f.acodec || null,
          filesize: f.filesize ? Number(f.filesize) : null,
          filesizeApprox: f.filesizeApprox ? Number(f.filesizeApprox) : null,
          tbr: f.tbr ? Number(f.tbr) : null,
          vbr: f.vbr ? Number(f.vbr) : null,
          abr: f.abr ? Number(f.abr) : null,
          asr: f.asr ? Number(f.asr) : null,
          formatNote: f.formatNote || null,
          hasVideo: f.hasVideo,
          hasAudio: f.hasAudio,
          quality: f.quality ? Number(f.quality) : null,
          rows: f.rows ?? null,
          columns: f.columns ?? null,
          fragments: f.fragments ?? null,
        })),
      };

      if (
        !loadedInfo.channelUrl &&
        loadedInfo.channelId &&
        (url.includes('bilibili.com') || url.includes('b23.tv'))
      ) {
        loadedInfo.channelUrl = `https://space.bilibili.com/${loadedInfo.channelId}`;
      }

      info = loadedInfo;
      saveToCache();

      if (info.thumbnail) processThumbnail(info.thumbnail);

      if (!sponsorSegments || sponsorSegments.length === 0) {
        fetchSponsorBlockSegments(url);
      }

      logs.info(
        'tracks',
        `Loaded: ${info.title}, ${videoFormats.length} video, ${audioFormats.length} audio`
      );
    } catch (e) {
      if (destroyed) return;
      error = String(e);
      logs.error('tracks', `Failed: ${e}`);
    } finally {
      if (!destroyed) {
        loading = false;
      }
    }
  }
</script>

{#if showHeader && $settings.builderThumbnailGlow}
  <ThumbnailGlow thumbnailUrl={displayThumbnail} enabled={showHeader} />
{/if}

<div class="track-builder" class:full-bleed={showHeader}>
  {#if showHeader}
    <div class="view-header">
      <button class="back-btn" onclick={onback}>
        <Icon name="alt_arrow_rigth" size={16} class="rotate-180" />
        <span>{backLabel || $t('common.back')}</span>
      </button>
      <div class="header-badge">
        <Icon name="play" size={12} />
        <span>{platformName}</span>
      </div>
      <div class="header-spacer"></div>
      {#if estimatedSize()}
        <span class="size-estimate">~{estimatedSize()}</span>
      {/if}
      <button class="header-download-btn" onclick={handleDownload} disabled={loading || !!error}>
        <Icon name="download" size={16} />
        <span>{$t('common.download')}</span>
      </button>
    </div>
  {:else}
    <div class="yt-badge">
      <Icon name="play" size={14} />
      <span>{platformName}</span>
    </div>
  {/if}

  <div class="card">
    {#if error}
      <div class="error-state">
        <Icon name="warning" size={18} />
        <span>{$t('download.tracks.error')}</span>
        <button class="retry-btn" onclick={() => loadInfo()}>
          <Icon name="restart" size={14} />
        </button>
      </div>
    {:else if showHeader}
      <div class="content-scroll">
        <div class="video-header">
          <div class="video-thumb-container">
            {#if displayThumbnail && !thumbnailError}
              <img
                src={displayThumbnail}
                alt=""
                class="video-thumb"
                decoding="async"
                onload={(e) => {
                  const src = (e.currentTarget as HTMLImageElement).getAttribute('src');
                  if (src && src === displayThumbnail) thumbnailError = false;
                }}
                onerror={(e) => {
                  const src = (e.currentTarget as HTMLImageElement).getAttribute('src');
                  if (src && src === displayThumbnail) thumbnailError = true;
                }}
              />
              {#if info?.duration}
                <span class="thumb-duration">{formatDuration(info.duration)}</span>
              {/if}
            {:else if loading}
              <Skeleton width="100%" height="auto" class="video-thumb" />
            {:else}
              <div class="video-thumb empty"><Icon name="video" size={32} /></div>
            {/if}
          </div>
          <div class="video-info">
            {#if loading && !displayTitle}
              <Skeleton width="90%" height="14px" />
              <Skeleton width="50%" height="10px" />
              <div class="stats-skel">
                <Skeleton width="60px" height="12px" />
                <Skeleton width="60px" height="12px" />
                <Skeleton width="60px" height="12px" />
              </div>
            {:else if displayTitle || info}
              <h1 class="video-title">{displayTitle}</h1>

              <div class="video-meta">
                {#if displayAuthor}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <span
                    class="channel"
                    class:clickable={canOpenChannel}
                    onclick={canOpenChannel ? handleOpenChannel : undefined}
                  >
                    <span class="channel-avatar-placeholder"><Icon name="user" size={12} /></span>
                    {displayAuthor}
                    {#if canOpenChannel}
                      <Icon name="alt_arrow_rigth" size={12} class="channel-arrow" />
                    {/if}
                  </span>
                {/if}
                {#if displayDuration && !info?.viewCount}
                  <span class="meta-item"
                    ><Icon name="clock" size={14} />{formatDuration(displayDuration)}</span
                  >
                {/if}
                {#if loading && !info}
                  <Skeleton width="50px" height="14px" class="meta-item" />
                  <Skeleton width="50px" height="14px" class="meta-item" />
                  <Skeleton width="50px" height="14px" class="meta-item" />
                {:else}
                  {#if info?.viewCount}<span class="meta-item"
                      ><Icon name="eye_line_duotone" size={14} />{formatCount(info.viewCount)}</span
                    >{/if}
                  {#if info?.likeCount}<span class="meta-item"
                      ><Icon name="heart" size={14} />{formatCount(info.likeCount)}</span
                    >{/if}
                  {#if info?.uploadDate}<span class="meta-item"
                      ><Icon name="date" size={14} />{formatUploadDate(info.uploadDate)}</span
                    >{/if}
                {/if}
              </div>

              {#if loading && !filenamePreview}
                <Skeleton width="100%" height="18px" style="max-width: 280px; margin-top: 4px;" />
              {:else}
                <div class="filename-row">
                  <Icon name="file_text" size={12} />
                  <div class="filename-toggle">
                    <span class="filename-preview-text">{filenamePreview}</span>
                    <input
                      type="text"
                      class="filename-template-input"
                      bind:value={outputTemplate}
                      placeholder="%(title)s.%(ext)s"
                    />
                  </div>
                </div>
              {/if}

              {#if loading && !info?.description}
                <Skeleton width="100%" height="32px" radius="6px" style="margin-top: 8px;" />
              {:else if cleanDescription(info?.description)}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div class="desc-preview" onclick={() => (showDescription = true)}>
                  <p class="desc-text">{cleanDescription(info?.description)}</p>
                  <span class="desc-more">...more</span>
                </div>
              {/if}
            {:else}
              <Skeleton width="90%" height="14px" />
              <Skeleton width="50%" height="10px" />
              <Skeleton width="100%" height="32px" radius="6px" style="margin-top: 8px;" />
              <Skeleton width="100%" height="18px" style="max-width: 280px; margin-top: 4px;" />
            {/if}
          </div>
        </div>

        <div class="quality-section">
          <span class="section-label-sm">{$t('download.tracks.quality')}</span>
          <div class="presets-row">
            {#if loading && presets.length <= 1}
              <Skeleton pill width="80px" height="32px" />
              <Skeleton pill width="80px" height="32px" />
              <Skeleton pill width="80px" height="32px" />
              <Skeleton pill width="80px" height="32px" />
            {:else}
              {#each presets as preset}
                <Chip
                  icon={preset.icon}
                  selected={selectedPreset === preset.id}
                  onclick={() => applyPreset(preset.id)}
                >
                  {preset.label}
                </Chip>
              {/each}
            {/if}
          </div>
          {#if loading && !info}
            <div class="quality-row">
              <div class="quality-select">
                <Skeleton width="60px" height="12px" style="margin-bottom: 6px;" />
                <Skeleton width="100%" height="36px" radius="8px" />
              </div>
              <div class="quality-select">
                <Skeleton width="60px" height="12px" style="margin-bottom: 6px;" />
                <Skeleton width="100%" height="36px" radius="8px" />
              </div>
            </div>
          {:else if useDualSelectors}
            <div class="quality-row">
              <div class="quality-select">
                <span class="select-label">{$t('download.tracks.video')}</span>
                <Select
                  bind:value={selectedVideo}
                  options={videoOptionsWithValidation}
                  disabled={loading}
                  onchange={markCustomPreset}
                />
              </div>
              <div class="quality-select" class:disabled={selectedVideoIsMuxed}>
                <span class="select-label">
                  {$t('download.tracks.audio')}
                  {#if selectedVideoIsMuxed}<span class="dimmed"> (included)</span>{/if}
                </span>
                <Select
                  bind:value={selectedAudio}
                  options={audioOptions}
                  disabled={loading || selectedVideoIsMuxed}
                  onchange={markCustomPreset}
                />
              </div>
            </div>
          {:else}
            <div class="quality-row">
              <div class="quality-select single">
                <span class="select-label">{$t('download.tracks.quality')}</span>
                <Select
                  bind:value={selectedMuxed}
                  options={muxedOptions}
                  disabled={loading}
                  onchange={markCustomPreset}
                />
              </div>
            </div>
          {/if}

          {#if loading && !displayDuration}
            <Skeleton width="100%" height="48px" radius="8px" style="margin-top: 12px;" />
          {:else if displayDuration && displayDuration > 0}
            <ClipRangeSelector
              duration={displayDuration}
              bind:ranges={clipRanges}
              disabled={loading}
              storyboard={info?.storyboards?.[0]}
              chapters={info?.chapters}
              {sponsorSegments}
            />
          {/if}
        </div>
        <div class="options-sections">
          {#if isYtdlp}
            <CollapsibleBlock title="SponsorBlock" badge="yt-dlp" expanded={true}>
              <div class="option-grid">
                <Checkbox bind:checked={skipSponsors} label={$t('download.tracks.skipSponsors')} />
                <Checkbox bind:checked={skipIntros} label={$t('download.tracks.skipIntros')} />
                <Checkbox
                  bind:checked={skipSelfPromo}
                  label={$t('download.tracks.skipSelfPromo')}
                />
                <Checkbox
                  bind:checked={skipInteraction}
                  label={$t('download.tracks.skipInteraction')}
                />
              </div>
            </CollapsibleBlock>
          {/if}

          <CollapsibleBlock title={$t('download.tracks.embedOptions')} expanded={true}>
            <div class="option-grid">
              <Checkbox bind:checked={embedChapters} label={$t('download.tracks.embedChapters')} />
              <Checkbox
                bind:checked={embedThumbnail}
                label={$t('download.tracks.embedThumbnail')}
              />
              <Checkbox bind:checked={embedMetadata} label={$t('download.tracks.embedMetadata')} />
            </div>
          </CollapsibleBlock>

          <CollapsibleBlock title={$t('download.tracks.subtitles')} expanded={true}>
            <div class="subs-row">
              <Checkbox bind:checked={embedSubs} label={$t('download.tracks.embedSubs')} />
              {#if embedSubs}
                <input type="text" class="lang-input" bind:value={subLangs} placeholder="en" />
              {/if}
            </div>
          </CollapsibleBlock>
        </div>
      </div>
    {:else}
      <div class="main-row">
        <div class="left">
          {#if displayThumbnail && !thumbnailError}
            <img
              src={displayThumbnail}
              alt=""
              class="thumb"
              decoding="async"
              onload={(e) => {
                const src = (e.currentTarget as HTMLImageElement).getAttribute('src');
                if (src && src === displayThumbnail) thumbnailError = false;
              }}
              onerror={(e) => {
                const src = (e.currentTarget as HTMLImageElement).getAttribute('src');
                if (src && src === displayThumbnail) thumbnailError = true;
              }}
            />
          {:else if loading}
            <Skeleton width="72px" height="72px" radius="8px" />
          {:else}
            <div class="thumb empty"><Icon name="video" size={20} /></div>
          {/if}
          <div class="info">
            {#if loading && !displayTitle}
              <Skeleton width="90%" height="14px" />
              <Skeleton width="50%" height="10px" />
            {:else if displayTitle || info}
              <span class="title-row">
                <span class="title">{displayTitle}</span>
                <button
                  class="copy-link-btn"
                  onclick={() => {
                    navigator.clipboard.writeText(url);
                    import('$lib/components/Toast.svelte').then((m) =>
                      m.toast.success($t('common.copied'))
                    );
                  }}
                  title={$t('common.copyLink')}
                >
                  <Icon name="link" size={12} />
                </button>
              </span>
              <span class="meta">
                {#if displayAuthor}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <span
                    class="channel-link"
                    class:clickable={canOpenChannel}
                    onclick={canOpenChannel ? handleOpenChannel : undefined}
                  >
                    {displayAuthor}
                    {#if canOpenChannel}
                      <Icon name="alt_arrow_rigth" size={10} class="link-arrow" />
                    {/if}
                  </span>
                {/if}
                {#if displayDuration}
                  {#if displayAuthor}
                    ·
                  {/if}
                  {formatDuration(displayDuration)}
                {/if}
              </span>
              {#if info?.viewCount || info?.likeCount}
                <div class="stats-row">
                  {#if info?.viewCount}
                    <span class="stat">
                      <Icon name="eye_line_duotone" size={12} />
                      {formatCount(info.viewCount)}
                    </span>
                  {/if}
                  {#if info?.likeCount}
                    <span class="stat">
                      <Icon name="heart" size={12} />
                      {formatCount(info.likeCount)}
                    </span>
                  {/if}
                </div>
              {/if}
            {:else}
              <Skeleton width="90%" height="14px" />
              <Skeleton width="50%" height="10px" />
            {/if}
          </div>
        </div>

        <div class="right">
          <div class="select-group">
            <span class="select-label">{$t('download.tracks.video')}</span>
            <Select
              bind:value={selectedVideo}
              options={videoOptionsWithValidation}
              disabled={loading}
            />
          </div>
          <div class="select-group">
            <span class="select-label">{$t('download.tracks.audio')}</span>
            <Select bind:value={selectedAudio} options={audioOptions} disabled={loading} />
          </div>
        </div>
      </div>

      <div class="extras-row">
        <button class="more-btn" onclick={() => (showMoreOptions = !showMoreOptions)}>
          <Icon name={showMoreOptions ? 'chevron_up' : 'chevron_down'} size={14} />
          <span>{$t('download.tracks.moreOptions')}</span>
        </button>
      </div>

      {#if showMoreOptions}
        <div class="options-sections">
          <CollapsibleBlock title={$t('download.tracks.outputFilename')} expanded={true}>
            <div class="filename-row in-block">
              <div class="filename-toggle">
                <span class="filename-preview-text">{filenamePreview}</span>
                <input
                  type="text"
                  class="filename-template-input"
                  bind:value={outputTemplate}
                  placeholder="%(title)s.%(ext)s"
                />
              </div>
            </div>
          </CollapsibleBlock>

          {#if isYtdlp}
            <CollapsibleBlock title="SponsorBlock" badge="yt-dlp" expanded={true}>
              <div class="option-grid">
                <Checkbox bind:checked={skipSponsors} label={$t('download.tracks.skipSponsors')} />
                <Checkbox bind:checked={skipIntros} label={$t('download.tracks.skipIntros')} />
                <Checkbox
                  bind:checked={skipSelfPromo}
                  label={$t('download.tracks.skipSelfPromo')}
                />
                <Checkbox
                  bind:checked={skipInteraction}
                  label={$t('download.tracks.skipInteraction')}
                />
              </div>
            </CollapsibleBlock>
          {/if}

          <CollapsibleBlock title={$t('download.tracks.subtitles')} expanded={true}>
            <div class="subs-row">
              <Checkbox bind:checked={embedSubs} label={$t('download.tracks.embedSubs')} />
              {#if embedSubs}
                <input
                  type="text"
                  class="lang-input"
                  bind:value={subLangs}
                  placeholder="en"
                  title={$t('download.tracks.subLangsHint')}
                />
              {/if}
            </div>
          </CollapsibleBlock>

          <CollapsibleBlock title={$t('download.tracks.embedOptions')} expanded={true}>
            <div class="option-grid">
              <Checkbox bind:checked={embedChapters} label={$t('download.tracks.embedChapters')} />
              <Checkbox
                bind:checked={embedThumbnail}
                label={$t('download.tracks.embedThumbnail')}
              />
              <Checkbox bind:checked={embedMetadata} label={$t('download.tracks.embedMetadata')} />
            </div>
          </CollapsibleBlock>
        </div>
      {/if}

      {#if !showHeader && ondownload}
        <div class="footer-actions">
          {#if estimatedSize()}
            <span class="size-estimate">~{estimatedSize()}</span>
          {/if}
          <button
            class="download-btn footer-download"
            onclick={handleDownload}
            disabled={loading || !!error}
          >
            <Icon name="download" size={18} />
            <span>{$t('common.download')}</span>
          </button>
        </div>
      {/if}
    {/if}
  </div>
</div>

{#if showDescription && info?.description}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="desc-modal-overlay" use:portal onclick={() => (showDescription = false)}>
    <div class="desc-modal" onclick={(e) => e.stopPropagation()}>
      <div class="desc-modal-header">
        <span class="desc-modal-title">{$t('download.tracks.description')}</span>
        <button class="desc-modal-close" onclick={() => (showDescription = false)}>
          <Icon name="close" size={16} />
        </button>
      </div>
      <div class="desc-modal-content">
        {cleanDescription(info.description)}
      </div>
    </div>
  </div>
{/if}

<style>
  .track-builder {
    display: flex;
    flex-direction: column;
    gap: 6px;
    animation: fadeIn 0.2s ease-out;
  }

  .track-builder.full-bleed {
    padding: 0 8px 0 0;
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  @media (max-width: 480px) {
    .track-builder.full-bleed {
      height: auto;
      min-height: 100%;
    }
  }

  .track-builder.full-bleed .view-header {
    position: sticky;
    top: 0;
    z-index: 10;
    margin: 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .track-builder.full-bleed .card {
    background: transparent;
    border: none;
    border-radius: 0;
    flex: 1;
    overflow-y: auto;
  }

  .track-builder.full-bleed .yt-badge {
    display: none;
  }

  .view-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    flex-shrink: 0;
    flex-wrap: wrap;
    min-width: 0;
  }

  .back-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.6);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    min-width: 0;
    flex-shrink: 1;
  }

  .back-btn span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 120px;
  }

  .back-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .header-badge {
    display: none;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    background: rgba(255, 0, 0, 0.12);
    border-radius: var(--radius-sm, 6px);
    color: #ff6b6b;
    font-size: 11px;
    font-weight: 600;
  }

  @media (min-width: 400px) {
    .header-badge {
      display: inline-flex;
    }
  }

  .header-spacer {
    flex: 1;
    min-width: 8px;
  }

  .size-estimate {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    font-weight: 500;
    flex-shrink: 0;
  }

  .header-download-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius, 8px);
    color: white;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    flex-shrink: 0;
  }

  .header-download-btn span {
    display: none;
  }

  @media (min-width: 360px) {
    .header-download-btn span {
      display: inline;
    }
  }

  .header-download-btn:hover:not(:disabled) {
    background: var(--accent, #6366f1);
    border-color: var(--accent, #6366f1);
  }

  .header-download-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .content-scroll {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-top: 8px;
  }

  .video-header {
    display: flex;
    gap: 16px;
    align-items: flex-start;
  }

  .video-thumb-container {
    position: relative;
    flex-shrink: 0;
    width: 280px;
    border-radius: var(--radius-lg, 12px);
    overflow: hidden;
  }

  .video-thumb {
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    background: rgba(255, 255, 255, 0.04);
    border-radius: var(--radius-lg, 12px);
  }

  .video-thumb.empty {
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.2);
  }

  .thumb-duration {
    position: absolute;
    bottom: 8px;
    right: 8px;
    padding: 3px 6px;
    background: rgba(0, 0, 0, 0.8);
    border-radius: var(--radius-sm, 4px);
    font-size: 12px;
    font-weight: 500;
    color: white;
  }

  .video-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .video-title {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: white;
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .video-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.6);
  }

  .video-meta .channel,
  .video-meta .meta-item {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .video-meta .channel {
    color: rgba(255, 255, 255, 0.9);
    font-weight: 500;
    gap: 6px;
    transition: color 0.15s ease;
  }

  .video-meta .channel.clickable {
    cursor: pointer;
  }

  .video-meta .channel.clickable:hover {
    color: var(--accent);
  }

  .video-meta .channel :global(.channel-arrow) {
    opacity: 0;
    width: 0;
    overflow: hidden;
    transition:
      opacity 0.15s ease,
      width 0.15s ease,
      transform 0.15s ease;
    color: rgba(255, 255, 255, 0.5);
  }

  .video-meta .channel.clickable:hover :global(.channel-arrow) {
    opacity: 1;
    width: 12px;
    transform: translateX(2px);
    color: var(--accent);
  }

  .channel-avatar-placeholder {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.5);
  }

  .video-meta .channel::after {
    content: '•';
    margin-left: 4px;
    color: rgba(255, 255, 255, 0.4);
  }

  .video-meta .meta-item::after {
    content: '•';
    margin-left: 4px;
    color: rgba(255, 255, 255, 0.4);
  }

  .video-meta .meta-item:last-child::after {
    display: none;
  }

  .desc-preview {
    padding: 8px 10px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: var(--radius, 8px);
    cursor: pointer;
    transition: background 0.15s;
  }

  .desc-preview:hover {
    background: rgba(255, 255, 255, 0.06);
  }

  .desc-text {
    margin: 0;
    font-size: 11px;
    line-height: 1.4;
    color: rgba(255, 255, 255, 0.6);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .desc-more {
    display: inline-block;
    margin-top: 2px;
    font-size: 11px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.8);
  }

  .presets-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .filename-row {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 4px 0;
    border-radius: var(--radius, 8px);
    color: rgba(255, 255, 255, 0.4);
    transition: color 0.15s ease;
  }

  .filename-row:hover,
  .filename-row:focus-within {
    color: var(--accent, #6366f1);
  }

  .filename-row.in-block {
    padding: 8px 10px;
  }

  .filename-toggle {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 18px;
    overflow: hidden;
  }

  .filename-preview-text {
    display: block;
    color: rgba(255, 255, 255, 0.55);
    font-size: 11px;
    font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
    white-space: normal;
    word-break: break-all;
    line-height: 1.4;
    opacity: 1;
    transform: translateY(0);
    transition:
      opacity 0.15s ease,
      transform 0.15s ease;
    pointer-events: none;
  }

  .filename-template-input {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    padding: 0;
    background: transparent;
    border: none;
    color: white;
    font-size: 11px;
    font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
    opacity: 0;
    transform: translateY(4px);
    transition:
      opacity 0.15s ease,
      transform 0.15s ease;
  }

  .filename-template-input:focus {
    outline: none;
  }

  .filename-template-input::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }

  .filename-toggle:hover .filename-preview-text,
  .filename-toggle:focus-within .filename-preview-text {
    opacity: 0;
    transform: translateY(-4px);
  }

  .filename-toggle:hover .filename-template-input,
  .filename-toggle:focus-within .filename-template-input {
    opacity: 1;
    transform: translateY(0);
  }

  .filename-row::after {
    content: 'template';
    position: absolute;
    right: 10px;
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: rgba(255, 255, 255, 0.2);
    opacity: 0;
    transition: opacity 0.15s ease;
    pointer-events: none;
  }

  .filename-row:hover::after,
  .filename-row:focus-within::after {
    opacity: 1;
  }

  .quality-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow: hidden;
  }

  .section-label-sm {
    font-size: 11px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.5);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .quality-row {
    display: flex;
    gap: 12px;
  }

  .quality-select {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .quality-select.single {
    max-width: 400px;
  }

  .quality-select.disabled {
    opacity: 0.5;
    pointer-events: none;
  }

  .quality-select .select-label {
    font-size: 11px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.5);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .quality-select .select-label .dimmed {
    font-size: 10px;
    opacity: 0.6;
    font-weight: 400;
    text-transform: none;
  }

  .desc-modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    animation: fadeIn 0.15s ease-out;
  }

  .desc-modal {
    width: 90%;
    max-width: 500px;
    max-height: 70vh;
    background: #1a1a24;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-lg, 12px);
    display: flex;
    flex-direction: column;
    animation: slideUp 0.2s ease-out;
  }

  @keyframes slideUp {
    from {
      opacity: 0;
      transform: translateY(20px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .desc-modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .desc-modal-title {
    font-size: 14px;
    font-weight: 600;
    color: white;
  }

  .desc-modal-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    background: rgba(255, 255, 255, 0.06);
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    transition: all 0.15s;
  }

  .desc-modal-close:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .desc-modal-content {
    padding: 16px;
    font-size: 13px;
    line-height: 1.6;
    color: rgba(255, 255, 255, 0.75);
    white-space: pre-wrap;
    word-break: break-word;
    overflow-y: auto;
  }

  .download-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 20px;
    background: var(--accent, #6366f1);
    border: none;
    border-radius: var(--radius, 8px);
    color: white;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .download-btn:hover:not(:disabled) {
    background: var(--accent-hover, #5558e3);
    transform: translateY(-1px);
  }

  .download-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .footer-actions {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
    display: flex;
    justify-content: flex-end;
  }

  .footer-download {
    padding: 12px 24px;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .yt-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: rgba(255, 0, 0, 0.15);
    border-radius: var(--radius-sm, 6px);
    color: #ff6b6b;
    font-size: 12px;
    font-weight: 600;
    width: fit-content;
  }

  .card {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: var(--radius-lg, 12px);
    padding: 0;
  }

  .error-state {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px;
    color: rgba(239, 68, 68, 0.9);
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

  .main-row {
    display: flex;
    gap: 14px;
  }

  .left {
    display: flex;
    gap: 10px;
    flex: 1;
    min-width: 0;
  }

  .thumb {
    width: 72px;
    height: 72px;
    border-radius: var(--radius, 8px);
    object-fit: cover;
    flex-shrink: 0;
    background: rgba(255, 255, 255, 0.04);
  }

  .thumb.empty {
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.2);
  }

  .info {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
    min-width: 0;
    flex: 1;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .title {
    font-size: 13px;
    font-weight: 600;
    color: white;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    line-height: 1.3;
  }

  .copy-link-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    background: rgba(255, 255, 255, 0.08);
    border: none;
    border-radius: var(--radius-sm, 4px);
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .copy-link-btn:hover {
    background: rgba(255, 255, 255, 0.15);
    color: rgba(255, 255, 255, 0.9);
  }

  .meta {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.45);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta .channel-link {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    transition: color 0.15s ease;
  }

  .meta .channel-link.clickable {
    cursor: pointer;
  }

  .meta .channel-link.clickable:hover {
    color: var(--accent);
  }

  .meta .channel-link :global(.link-arrow) {
    opacity: 0;
    transition:
      opacity 0.15s ease,
      transform 0.15s ease;
  }

  .meta .channel-link.clickable:hover :global(.link-arrow) {
    opacity: 1;
    transform: translateX(2px);
  }

  .stats-row {
    display: flex;
    gap: 10px;
    margin-top: 2px;
  }

  .stat {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    color: rgba(255, 255, 255, 0.4);
  }

  .right {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
  }

  .select-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 160px;
  }

  .select-label {
    font-size: 10px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .extras-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }

  .subs-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .lang-input {
    width: 80px;
    padding: 4px 8px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-sm, 4px);
    color: white;
    font-size: 12px;
    font-family: inherit;
  }

  .lang-input:focus {
    outline: none;
    border-color: var(--accent, #6366f1);
  }

  .more-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 4px);
    color: rgba(255, 255, 255, 0.5);
    font-size: 11px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .more-btn:hover {
    color: white;
    background: rgba(255, 255, 255, 0.06);
  }

  .options-sections {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 12px;
  }

  .option-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 6px 16px;
  }

  .stats-skel {
    display: flex;
    gap: 12px;
    margin-top: 8px;
  }

  @media (max-width: 560px) {
    .main-row {
      flex-direction: column;
      gap: 12px;
    }

    .left {
      flex-direction: row;
      gap: 10px;
    }

    .thumb {
      width: 64px;
      height: 64px;
    }

    .right {
      flex-direction: row;
      width: 100%;
    }

    .select-group {
      flex: 1;
      width: auto;
      min-width: 0;
    }

    .extras-row {
      flex-wrap: wrap;
      gap: 6px;
    }

    .more-btn {
      order: 0;
      margin-left: auto;
    }

    .option-grid {
      grid-template-columns: 1fr;
      gap: 6px;
    }

    .subs-row {
      flex-direction: column;
      align-items: flex-start;
      gap: 6px;
    }

    .lang-input {
      width: 100%;
      max-width: 120px;
    }

    .video-header {
      flex-direction: column;
    }

    .video-thumb-container {
      width: 100%;
      max-width: 320px;
    }

    .video-title {
      font-size: 15px;
    }

    .quality-row {
      flex-direction: column;
      gap: 8px;
    }
  }

  :global(.rotate-180) {
    transform: rotate(180deg);
  }
</style>
