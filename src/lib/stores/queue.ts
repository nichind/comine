import { writable, derived, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event';
import { load, type Store } from '@tauri-apps/plugin-store';
import { logs } from './logs';
import { toast } from '$lib/components/Toast.svelte';
import { translate } from '$lib/i18n';
import { formatSpeed, formatTime } from '$lib/utils/format';
import { getSettings, settingsReady } from '$lib/stores/settings';
import { history } from '$lib/stores/history';
import { appStats } from '$lib/stores/stats';
import type {
  DownloadOptions as OrchestratorDownloadOptions,
  DownloadRequest,
  Job,
  JobControl,
  JobEvent,
  JobStatus,
  ProxyConfig as OrchestratorProxyConfig,
  ResolveResult,
  ClipRange,
} from '$lib/bindings';

export type DownloadStatus =
  | 'pending'
  | 'fetching-info'
  | 'downloading'
  | 'processing'
  | 'converting'
  | 'completed'
  | 'failed'
  | 'paused';

export type QueueItemSource = 'ytdlp' | 'file' | 'convert';

export interface PlaylistGroup {
  playlistId: string;
  playlistTitle: string;
  items: QueueItem[];
}

export interface GroupedDownloads {
  groups: PlaylistGroup[];
  singles: QueueItem[];
}

export interface PrefetchedInfo {
  title?: string;
  author?: string;
  thumbnail?: string;
  duration?: number;
}

export interface QueueAddOptions {
  videoQuality?: string;
  downloadMode?: 'auto' | 'audio' | 'mute';
  audioQuality?: string;

  convertToMp4?: boolean;
  remux?: boolean;
  clearMetadata?: boolean;
  dontShowInHistory?: boolean;

  useAria2?: boolean;
  ignoreMixes?: boolean;

  cookiesFromBrowser?: string;
  customCookies?: string;

  sponsorBlock?: boolean;
  sponsorBlockSkipSponsors?: boolean;
  sponsorBlockSkipIntros?: boolean;
  sponsorBlockSkipSelfPromo?: boolean;
  sponsorBlockSkipInteraction?: boolean;

  chapters?: boolean;
  embedSubtitles?: boolean;
  subtitleLanguages?: string;
  embedThumbnail?: boolean;
  outputTemplate?: string;

  // Used by track builder / clip downloads; currently not mapped into orchestrator request.
  clipRanges?: ClipRange[];

  prefetchedInfo?: PrefetchedInfo;
}

export interface QueueItem {
  id: string;
  jobId?: string;
  url: string;
  status: DownloadStatus;
  statusMessage: string;
  title: string;
  author: string;
  authorUrl?: string;
  thumbnail: string;
  duration: number;
  filesize: number;
  extension: string;
  filePath: string;
  progress: number;
  speed: string;
  speedBps?: number;
  eta: string;
  error?: string;
  addedAt: number;
  type: 'video' | 'audio' | 'image' | 'file';
  priority: number;
  options?: QueueAddOptions;
  source: QueueItemSource;
  downloadedBytes?: number;
  totalBytes?: number;
  backendName?: string;

  playlistId?: string;
  playlistTitle?: string;
  playlistIndex?: number;
}

interface QueueState {
  items: QueueItem[];
  isPaused: boolean;
}

let queueStore: Store | null = null;
let saveDebounceTimer: ReturnType<typeof setTimeout> | null = null;
const SAVE_DEBOUNCE_MS = 500;

function isTerminalStatus(status: DownloadStatus): boolean {
  return status === 'completed' || status === 'failed';
}

function isActiveStatus(status: DownloadStatus): boolean {
  return !isTerminalStatus(status);
}

function dedupeByJobId(items: QueueItem[]): QueueItem[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    if (!item.jobId) return true;
    if (seen.has(item.jobId)) return false;
    seen.add(item.jobId);
    return true;
  });
}

function waitForSettingsReady(): Promise<void> {
  if (get(settingsReady)) return Promise.resolve();
  return new Promise<void>((resolve) => {
    const unsub = settingsReady.subscribe((ready) => {
      if (ready) {
        unsub();
        resolve();
      }
    });
  });
}

function jobStatusToDownloadStatus(status: JobStatus): DownloadStatus {
  switch (status.type) {
    case 'queued':
      return 'pending';
    case 'resolving':
      return 'fetching-info';
    case 'downloading':
      return 'downloading';
    case 'postProcessing':
      return 'processing';
    case 'paused':
      return 'paused';
    case 'completed':
      return 'completed';
    case 'failed':
      return 'failed';
    case 'cancelled':
      return 'failed';
    default:
      return 'pending';
  }
}

function statusMessageFor(status: DownloadStatus): string {
  switch (status) {
    case 'pending':
      return translate('downloads.queue.waiting') || 'Waiting';
    case 'fetching-info':
      return translate('downloads.status.fetchingInfo') || 'Fetching info';
    case 'downloading':
      return translate('downloads.status.downloading') || 'Downloading';
    case 'processing':
      return translate('downloads.status.processing') || 'Processing';
    case 'converting':
      return translate('downloads.status.converting') || 'Converting';
    case 'paused':
      return translate('downloads.queue.paused') || 'Paused';
    default:
      return status;
  }
}

function filenameExt(path: string): string {
  const normalized = path.replace(/\\/g, '/');
  const base = normalized.split('/').pop() || '';
  const idx = base.lastIndexOf('.');
  if (idx === -1) return '';
  return base.slice(idx + 1).toLowerCase();
}

function jobToQueuePatch(job: Job): Partial<QueueItem> {
  const status = jobStatusToDownloadStatus(job.status);

  let filePath = '';
  if (job.status.type === 'completed') {
    filePath = job.status.data.output_path;
  }

  const extension = filePath ? filenameExt(filePath) : '';

  const speed = job.speed ? formatSpeed(Number(job.speed)) : '';
  const eta = job.eta ? formatTime(Number(job.eta)) : '';

  return {
    status,
    statusMessage: statusMessageFor(status),
    progress: Number.isFinite(job.progress) ? Math.max(0, Math.min(100, job.progress)) : 0,
    downloadedBytes: Number(job.downloadedBytes),
    totalBytes: job.totalBytes ? Number(job.totalBytes) : undefined,
    speed,
    eta,
    error:
      job.status.type === 'failed'
        ? job.status.data.error
        : job.lastError
          ? job.lastError
          : undefined,
    filePath,
    extension,
    title: job.title ?? undefined,
    thumbnail: job.thumbnail ?? undefined,
  };
}

function buildProxyConfigFromSettings(): OrchestratorProxyConfig | null {
  const s = getSettings();

  // For 'none' mode, disable proxy entirely
  if (s.proxyMode === 'none') {
    return { enabled: false, url: null, username: null, password: null };
  }

  // For 'custom' mode, use the custom URL
  if (s.proxyMode === 'custom') {
    const url = s.customProxyUrl?.trim();
    if (!url) {
      // Custom mode but no URL - let backend try system proxy
      return { enabled: true, url: null, username: null, password: null };
    }
    return {
      enabled: true,
      url,
      username: null,
      password: null,
    };
  }

  // For 'system' mode, set enabled=true with null URL so backend detects system proxy
  return { enabled: true, url: null, username: null, password: null };
}

function buildOrchestratorOptions(ui: QueueAddOptions | undefined): OrchestratorDownloadOptions {
  const s = getSettings();

  const embedThumbnail = ui?.embedThumbnail ?? s.embedThumbnail ?? true;
  const embedMetadata = !(ui?.clearMetadata ?? s.clearMetadata ?? false);
  const embedSubtitles = ui?.embedSubtitles ?? s.embedSubtitles ?? false;

  const cookies_from_browser = ui?.cookiesFromBrowser?.trim() ? ui.cookiesFromBrowser.trim() : null;
  const custom_cookies = ui?.customCookies?.trim() ? ui.customCookies.trim() : null;

  const sponsorCategories: string[] = [];
  if (ui?.sponsorBlock) {
    if (ui.sponsorBlockSkipSponsors) sponsorCategories.push('sponsor');
    if (ui.sponsorBlockSkipIntros) sponsorCategories.push('intro');
    if (ui.sponsorBlockSkipSelfPromo) sponsorCategories.push('selfpromo');
    if (ui.sponsorBlockSkipInteraction) sponsorCategories.push('interaction');
    if (sponsorCategories.length === 0) sponsorCategories.push('sponsor');
  }

  return {
    cookiesFromBrowser: cookies_from_browser,
    customCookies: custom_cookies,
    proxy: buildProxyConfigFromSettings(),
    speedLimit: s.downloadSpeedLimit ? BigInt(s.downloadSpeedLimit * 1024) : null,
    embedThumbnail: Boolean(embedThumbnail),
    embedMetadata: Boolean(embedMetadata),
    embedSubtitles: Boolean(embedSubtitles),
    subtitleLangs: ui?.subtitleLanguages?.trim() ? ui.subtitleLanguages.trim() : null,
    sponsorblockRemove: sponsorCategories.length > 0 ? sponsorCategories.join(',') : null,
    youtubePlayerClient: s.youtubePlayerClient.trim() || null,
    aria2Connections: s.aria2Connections ?? 8,
    aria2Splits: s.aria2Splits ?? 8,
    maxRetries: 3,
    clipRanges: ui?.clipRanges ?? null,
  };
}

function videoQualityToMaxHeight(quality: string | undefined): number | null {
  if (!quality || quality === 'max') return null;
  const match = quality.match(/(\d+)/);
  if (match) {
    const height = parseInt(match[1], 10);
    if (quality === '4k') return 2160;
    return height;
  }
  return null;
}

function buildFormatString(
  audioOnly: boolean,
  preferredVideoCodec: string | undefined,
  preferredAudioCodec: string | undefined
): string {
  const audioCodecFilter: Record<string, string> = {
    opus: '[acodec=opus]',
    aac: '[acodec^=mp4a]',
    mp3: '[acodec^=mp3]',
    vorbis: '[acodec=vorbis]',
  };

  const videoCodecFilter: Record<string, string> = {
    h264: '[vcodec^=avc]',
    h265: '[vcodec^=hev]',
    vp9: '[vcodec^=vp9]',
    av1: '[vcodec^=av01]',
  };

  if (audioOnly) {
    const audioFilter =
      preferredAudioCodec && preferredAudioCodec !== 'any'
        ? audioCodecFilter[preferredAudioCodec] || ''
        : '';
    return audioFilter ? `bestaudio${audioFilter}/bestaudio/best` : 'bestaudio/best';
  }

  const videoFilter =
    preferredVideoCodec && preferredVideoCodec !== 'any'
      ? videoCodecFilter[preferredVideoCodec] || ''
      : '';
  const audioFilter =
    preferredAudioCodec && preferredAudioCodec !== 'any'
      ? audioCodecFilter[preferredAudioCodec] || ''
      : '';

  if (videoFilter || audioFilter) {
    const preferredVideo = `bestvideo${videoFilter}`;
    const preferredAudio = `bestaudio${audioFilter}`;
    return `${preferredVideo}+${preferredAudio}/${preferredVideo}+bestaudio/bestvideo+${preferredAudio}/bestvideo+bestaudio/best`;
  }

  return 'bestvideo+bestaudio/best';
}

async function buildDownloadRequest(
  url: string,
  ui: QueueAddOptions | undefined,
  filename?: string
): Promise<DownloadRequest> {
  const s = getSettings();
  const mode = ui?.downloadMode ?? s.defaultDownloadMode ?? 'auto';
  const audioOnly = mode === 'audio';

  const baseDir = audioOnly && s.useAudioPath ? s.audioPath : s.downloadPath;

  let directory = baseDir || '';
  if (!directory) {
    try {
      directory = await invoke<string>('get_default_download_dir');
    } catch {
      directory = '';
    }
  }

  const maxHeight = videoQualityToMaxHeight(ui?.videoQuality);
  const format = buildFormatString(audioOnly, s.preferredVideoCodec, s.preferredAudioCodec);

  return {
    url,
    backend: null,
    quality: {
      format,
      maxHeight: maxHeight,
      preferCodec: null,
      audioOnly: audioOnly,
      audioFormat: null,
    },
    output: {
      directory,
      filenameTemplate: ui?.outputTemplate ?? null,
      filename: filename ?? null,
    },
    options: buildOrchestratorOptions(ui),
    postProcess: [],
  };
}

function createQueueStore() {
  const { subscribe, update } = writable<QueueState>({
    items: [],
    isPaused: false,
  });

  let unlisten: UnlistenFn | null = null;

  async function ensureStoreLoaded() {
    if (queueStore) return;
    queueStore = await load('queue.json', { autoSave: false, defaults: {} });
  }

  async function loadQueue() {
    try {
      await ensureStoreLoaded();
      const savedItems = (await queueStore!.get<QueueItem[]>('items')) || [];
      update((s) => ({ ...s, items: Array.isArray(savedItems) ? savedItems : [] }));

      // Sync with backend state on startup
      const jobs = await invoke<Job[]>('get_jobs');

      update((state) => {
        const byJobId = new Map<string, Job>();
        for (const j of jobs) byJobId.set(j.id, j);

        const nextItems: QueueItem[] = state.items.map((item) => {
          if (!item.jobId) return item;
          const job = byJobId.get(item.jobId);
          if (!job) {
            // Backend forgot about the job (e.g. older queue.json from UI). Mark interrupted.
            if (!isTerminalStatus(item.status)) {
              return {
                ...item,
                status: 'failed' as DownloadStatus,
                statusMessage: 'Interrupted',
                error: 'Interrupted',
              };
            }
            return item;
          }

          const patch = jobToQueuePatch(job);
          return {
            ...item,
            ...patch,
            title: patch.title ?? item.title,
            thumbnail: patch.thumbnail ?? item.thumbnail,
          };
        });

        // Any active item without a jobId cannot be reconciled with the backend and will never progress.
        // Mark these as interrupted so they don't block new downloads.
        for (let i = 0; i < nextItems.length; i++) {
          const item = nextItems[i];
          if (!item.jobId && isActiveStatus(item.status)) {
            nextItems[i] = {
              ...item,
              status: 'failed',
              statusMessage: 'Interrupted',
              error: item.error ?? 'Interrupted',
            };
          }
        }

        const knownJobIds = new Set(nextItems.map((i) => i.jobId).filter(Boolean) as string[]);
        for (const job of jobs) {
          if (knownJobIds.has(job.id)) continue;
          const patch = jobToQueuePatch(job);

          // If we have a pending UI item for this URL that never got a jobId (race / restart), attach it.
          const mergeIdx = nextItems.findIndex(
            (i) => !i.jobId && i.url === job.request.url && isActiveStatus(i.status)
          );

          if (mergeIdx !== -1) {
            nextItems[mergeIdx] = {
              ...nextItems[mergeIdx],
              jobId: job.id,
              ...patch,
              title: patch.title ?? nextItems[mergeIdx].title,
              thumbnail: patch.thumbnail ?? nextItems[mergeIdx].thumbnail,
            };
          } else {
            nextItems.unshift({
              id: crypto.randomUUID(),
              jobId: job.id,
              url: job.request.url,
              status: patch.status ?? 'pending',
              statusMessage: patch.statusMessage ?? '',
              title: (patch.title ?? job.request.url) || job.request.url,
              author: '',
              thumbnail: patch.thumbnail ?? '',
              duration: 0,
              filesize: patch.totalBytes ?? 0,
              extension: patch.extension ?? '',
              filePath: patch.filePath ?? '',
              progress: patch.progress ?? 0,
              speed: patch.speed ?? '',
              eta: patch.eta ?? '',
              error: patch.error,
              addedAt: Number(job.createdAt),
              type: job.request.quality.audioOnly ? 'audio' : 'video',
              priority: 0,
              options: undefined,
              source: 'ytdlp',
              downloadedBytes: patch.downloadedBytes,
              totalBytes: patch.totalBytes,
            });
          }
        }

        const deduped = dedupeByJobId(nextItems);
        saveQueueState(deduped);
        return { ...state, items: deduped };
      });
    } catch (e) {
      logs.error('queue', `Failed to load queue: ${e}`);
    }
  }

  function saveQueueState(items: QueueItem[]) {
    if (!queueStore) return;
    if (saveDebounceTimer) clearTimeout(saveDebounceTimer);

    saveDebounceTimer = setTimeout(async () => {
      try {
        await queueStore!.set('items', items);
        await queueStore!.save();
      } catch (e) {
        logs.error('queue', `Failed to save queue: ${e}`);
      }
    }, SAVE_DEBOUNCE_MS);
  }

  async function setupListener() {
    if (unlisten) return; // Already listening

    // Listen for global job events from Rust Orchestrator
    unlisten = await listen<JobEvent>('job-event', (event) => {
      const payload = event.payload;

      update((state) => {
        const jobId =
          payload.type === 'added'
            ? payload.data.job.id
            : 'job_id' in payload.data
              ? payload.data.job_id
              : undefined;
        if (!jobId) return state;

        const index = state.items.findIndex((i) => i.jobId === jobId);
        const newItems = [...state.items];

        const prevStatus = index !== -1 ? state.items[index].status : undefined;
        const wasFinished = prevStatus ? isTerminalStatus(prevStatus) : false;

        const applyPatchToItem = (patch: Partial<QueueItem>) => {
          if (index === -1) return;
          newItems[index] = {
            ...newItems[index],
            ...patch,
            title: patch.title ?? newItems[index].title,
            thumbnail: patch.thumbnail ?? newItems[index].thumbnail,
          };
        };

        switch (payload.type) {
          case 'added': {
            // If the job was created elsewhere (another window / persisted restore), add it to UI.
            if (index !== -1) {
              const patch = jobToQueuePatch(payload.data.job);
              applyPatchToItem(patch);
              break;
            }
            const job = payload.data.job;
            const patch = jobToQueuePatch(job);

            // Avoid duplicates when the UI inserted a placeholder item before we had a jobId.
            const mergeIdx = newItems.findIndex(
              (i) => !i.jobId && i.url === job.request.url && isActiveStatus(i.status)
            );

            if (mergeIdx !== -1) {
              newItems[mergeIdx] = {
                ...newItems[mergeIdx],
                jobId: job.id,
                ...patch,
                title: patch.title ?? newItems[mergeIdx].title,
                thumbnail: patch.thumbnail ?? newItems[mergeIdx].thumbnail,
              };
            } else {
              newItems.unshift({
                id: crypto.randomUUID(),
                jobId: job.id,
                url: job.request.url,
                status: patch.status ?? 'pending',
                statusMessage: patch.statusMessage ?? '',
                title: (patch.title ?? job.request.url) || job.request.url,
                author: '',
                thumbnail: patch.thumbnail ?? '',
                duration: 0,
                filesize: patch.totalBytes ?? 0,
                extension: patch.extension ?? '',
                filePath: patch.filePath ?? '',
                progress: patch.progress ?? 0,
                speed: patch.speed ?? '',
                eta: patch.eta ?? '',
                error: patch.error,
                addedAt: Number(job.createdAt),
                type: job.request.quality.audioOnly ? 'audio' : 'video',
                priority: 0,
                options: undefined,
                source: 'ytdlp',
                downloadedBytes: patch.downloadedBytes,
                totalBytes: patch.totalBytes,
              });
            }
            break;
          }

          case 'started':
            applyPatchToItem({
              status: 'downloading',
              statusMessage: translate('downloads.status.starting') || 'Starting',
              backendName: payload.data.backend,
            });
            break;

          case 'progress': {
            const progress = Math.max(0, Math.min(100, Number(payload.data.progress)));
            const speedBps = payload.data.speed ? Number(payload.data.speed) : undefined;
            const speed =
              typeof speedBps === 'number' && Number.isFinite(speedBps)
                ? formatSpeed(speedBps)
                : '';
            const eta = payload.data.eta ? formatTime(Number(payload.data.eta)) : '';
            const status = progress >= 95 ? 'processing' : 'downloading';
            const statusMessage =
              progress >= 95
                ? translate('downloads.status.processing')
                : translate('downloads.status.downloading');

            applyPatchToItem({
              status: status as DownloadStatus,
              statusMessage,
              progress,
              downloadedBytes: Number(payload.data.downloaded_bytes),
              totalBytes: payload.data.total_bytes ? Number(payload.data.total_bytes) : undefined,
              speedBps,
              speed,
              eta,
            });

            // Emit for notification popup
            if (index !== -1) {
              emit('download-progress-parsed', {
                url: newItems[index].url,
                progress,
                speed,
                eta,
                status,
                statusMessage,
              });
            }
            break;
          }

          case 'statusChanged': {
            const nextStatus = jobStatusToDownloadStatus(payload.data.status);
            applyPatchToItem({ status: nextStatus, statusMessage: statusMessageFor(nextStatus) });
            break;
          }

          case 'paused':
            applyPatchToItem({
              status: 'paused',
              statusMessage: translate('downloads.queue.paused'),
            });
            break;

          case 'resumed':
            applyPatchToItem({
              status: 'pending',
              statusMessage: translate('downloads.queue.waiting'),
            });
            break;

          case 'cancelled':
            applyPatchToItem({ status: 'failed', statusMessage: 'Cancelled', error: 'Cancelled' });

            // Emit for notification popup
            if (index !== -1) {
              emit('download-status-changed', {
                url: newItems[index].url,
                status: 'cancelled',
              });
            }

            // Stats: count as a finished (failed) download.
            if (!wasFinished) {
              appStats.trackDownload(0, false);
            }
            break;

          case 'failed':
            applyPatchToItem({
              status: 'failed',
              statusMessage: payload.data.error || 'Failed',
              error: payload.data.error,
            });
            toast.error(`Download failed: ${payload.data.error}`);

            // Emit for notification popup
            if (index !== -1) {
              emit('download-status-changed', {
                url: newItems[index].url,
                status: 'failed',
                error: payload.data.error,
              });
            }

            // Stats: count as a finished (failed) download.
            if (!wasFinished) {
              appStats.trackDownload(0, false);
            }
            break;

          case 'completed': {
            const ext = filenameExt(payload.data.output_path);
            const filesize = payload.data.filesize ? Number(payload.data.filesize) : undefined;
            applyPatchToItem({
              status: 'completed',
              progress: 100,
              statusMessage: 'Finished',
              filePath: payload.data.output_path,
              extension: ext,
              title: payload.data.title ?? undefined,
              thumbnail: payload.data.thumbnail ?? undefined,
              filesize: filesize,
              totalBytes: filesize,
            });

            // Emit for notification popup
            if (index !== -1) {
              const item = newItems[index];
              emit('download-status-changed', {
                url: item.url,
                status: 'completed',
                filePath: payload.data.output_path,
                title: item.title,
              });

              if (!item.options?.dontShowInHistory) {
                history
                  .add({
                    url: item.url,
                    title: item.title,
                    author: item.author,
                    authorUrl: item.authorUrl,
                    thumbnail: item.thumbnail,
                    extension: item.extension,
                    size: item.filesize,
                    duration: item.duration,
                    filePath: item.filePath,
                    type: item.type,
                    playlistId: item.playlistId,
                    playlistTitle: item.playlistTitle,
                    playlistIndex: item.playlistIndex,
                    downloadSource: item.backendName,
                  })
                  .catch(() => undefined);
              }

              // Stats: count as a finished (successful) download.
              if (!wasFinished) {
                const sizeBytes = item.filesize ?? item.totalBytes ?? 0;
                const sizeMb = sizeBytes > 0 ? sizeBytes / (1024 * 1024) : 0;
                appStats.trackDownload(sizeMb, true);
              }
            }

            toast.success(translate('downloads.status.completed'));
            break;
          }
        }

        const deduped = dedupeByJobId(newItems);
        saveQueueState(deduped);
        return { ...state, items: deduped };
      });
    });
  }

  const pauseAll = () => {
    update((s) => ({ ...s, isPaused: true }));
    const state = get({ subscribe });
    for (const item of state.items) {
      if (
        item.jobId &&
        (item.status === 'pending' ||
          item.status === 'downloading' ||
          item.status === 'processing' ||
          item.status === 'fetching-info')
      ) {
        invokeControl(item.jobId, 'pause');
      }
    }
  };

  const resumeAll = () => {
    update((s) => ({ ...s, isPaused: false }));
    const state = get({ subscribe });
    for (const item of state.items) {
      if (item.jobId && item.status === 'paused') {
        invokeControl(item.jobId, 'resume');
      }
    }
  };

  const cancelAll = () => {
    const state = get({ subscribe });
    for (const item of state.items) {
      if (item.jobId && isActiveStatus(item.status)) {
        invokeControl(item.jobId, 'cancel');
      }
    }
  };

  function findItemById(state: QueueState, itemId: string): QueueItem | undefined {
    return state.items.find((i) => i.id === itemId);
  }

  function invokeControl(jobId: string, action: JobControl) {
    invoke('control_job', { jobId, action }).catch((e) => {
      logs.error('queue', `Failed to control job (${action}): ${e}`);
    });
  }

  function enqueueUrl(
    url: string,
    options?: QueueAddOptions,
    extras?: {
      playlistId?: string;
      playlistTitle?: string;
      playlistIndex?: number;
      filename?: string;
      source?: QueueItemSource;
    }
  ): string | null {
    const state = get({ subscribe });
    if (state.items.some((i) => i.url === url && isActiveStatus(i.status))) {
      logs.info('queue', `Ignored enqueue for already-active URL: ${url}`);
      toast.info(translate('downloads.queue.alreadyQueued') || 'Already in queue');
      return null;
    }

    const id = crypto.randomUUID();
    const prefetched = options?.prefetchedInfo;
    const type: QueueItem['type'] = options?.downloadMode === 'audio' ? 'audio' : 'video';

    const newItem: QueueItem = {
      id,
      url,
      status: 'pending',
      statusMessage: translate('downloads.queue.waiting') || 'Waiting',
      title: prefetched?.title || url,
      author: prefetched?.author || '',
      thumbnail: prefetched?.thumbnail || '',
      duration:
        typeof prefetched?.duration === 'number' && Number.isFinite(prefetched.duration)
          ? prefetched.duration
          : 0,
      filesize: 0,
      extension: '',
      filePath: '',
      progress: 0,
      speed: '',
      eta: '',
      addedAt: Date.now(),
      type,
      priority: 0,
      options,
      source: extras?.source ?? 'ytdlp',
      jobId: undefined,
      playlistId: extras?.playlistId,
      playlistTitle: extras?.playlistTitle,
      playlistIndex: extras?.playlistIndex,
    };

    update((s) => {
      const items = [newItem, ...s.items];
      saveQueueState(items);
      return { ...s, items };
    });

    void (async () => {
      await waitForSettingsReady();

      const request = await buildDownloadRequest(url, options, extras?.filename);
      const outDir = request.output.directory?.trim();
      if (!outDir) {
        const msg =
          translate('downloads.errors.noDownloadPath') ||
          'Set a download folder in Settings before downloading.';
        logs.warn('queue', `Cannot start job without output directory. url=${url}`);
        update((s) => {
          const items = s.items.map((i) =>
            i.id === id
              ? {
                  ...i,
                  status: 'failed' as DownloadStatus,
                  statusMessage: 'Missing download folder',
                  error: msg,
                }
              : i
          );
          saveQueueState(items);
          return { ...s, items };
        });
        toast.error(msg);
        return;
      }

      // Start the download job IMMEDIATELY - don't wait for resolve
      invoke<string>('start_job', { request })
        .then((jobId) => {
          update((s) => {
            const withJobId = s.items.map((i) => (i.id === id ? { ...i, jobId } : i));
            const deduped = dedupeByJobId(withJobId);
            saveQueueState(deduped);
            return { ...s, items: deduped };
          });
        })
        .catch((err) => {
          const msg = String(err);
          logs.error('queue', `Failed to start job: ${msg}`);
          update((s) => {
            const items = s.items.map((i) =>
              i.id === id
                ? {
                    ...i,
                    status: 'failed' as DownloadStatus,
                    statusMessage: 'Failed to start',
                    error: msg,
                  }
                : i
            );
            saveQueueState(items);
            return { ...s, items };
          });
          toast.error(msg);
        });

      // Resolve info in parallel for UI display (title, thumbnail, etc.)
      // This does NOT block the download - it just enriches the UI
      if (!prefetched?.title && !prefetched?.thumbnail && !prefetched?.duration) {
        update((s) => {
          const items = s.items.map((i) =>
            i.id === id
              ? {
                  ...i,
                  status: 'fetching-info' as DownloadStatus,
                  statusMessage: translate('downloads.status.fetchingInfo') || 'Fetching info',
                }
              : i
          );
          saveQueueState(items);
          return { ...s, items };
        });

        try {
          const resolved = await invoke<ResolveResult>('resolve_url', { url });
          const info = resolved.info;
          update((s) => {
            const items = s.items.map((i) => {
              if (i.id !== id) return i;
              const parsedDuration = info.duration !== null ? Number(info.duration) : null;
              const safeDuration =
                parsedDuration !== null && Number.isFinite(parsedDuration)
                  ? parsedDuration
                  : i.duration;
              return {
                ...i,
                title: info.title ?? i.title,
                thumbnail: info.thumbnail ?? i.thumbnail,
                author: info.uploader ?? info.channel ?? i.author,
                authorUrl: info.channelUrl ?? i.authorUrl,
                duration: safeDuration,
                filesize: info.filesize !== null ? Number(info.filesize) : i.filesize,
                // Don't override status if download already started/progressing
                status: i.status === 'fetching-info' ? ('pending' as DownloadStatus) : i.status,
                statusMessage:
                  i.status === 'fetching-info'
                    ? translate('downloads.queue.waiting') || 'Waiting'
                    : i.statusMessage,
              };
            });
            saveQueueState(items);
            return { ...s, items };
          });
        } catch (e) {
          logs.warn('queue', `Failed to resolve URL info (${url}): ${e}`);
          // Don't mark as failed - the download may still succeed
          update((s) => {
            const items = s.items.map((i) =>
              i.id === id && i.status === 'fetching-info'
                ? {
                    ...i,
                    status: 'pending' as DownloadStatus,
                    statusMessage: translate('downloads.queue.waiting') || 'Waiting',
                  }
                : i
            );
            saveQueueState(items);
            return { ...s, items };
          });
        }
      }
    })();

    return id;
  }

  return {
    subscribe,
    init: () => {
      void loadQueue();
      void setupListener();
    },
    cleanup: () => {
      if (unlisten) {
        unlisten();
        unlisten = null;
      }
      if (saveDebounceTimer) {
        clearTimeout(saveDebounceTimer);
        saveDebounceTimer = null;
      }
    },

    add: (url: string, options?: QueueAddOptions) => {
      return enqueueUrl(url, options);
    },

    addFile: (args: { url: string; filename: string; size?: number; mimeType?: string }) => {
      return enqueueUrl(args.url, undefined, { filename: args.filename, source: 'file' });
    },

    addPlaylist: (
      entries: Array<
        {
          url: string;
          title?: string;
          thumbnail?: string;
          author?: string;
          duration?: number;
        } & QueueAddOptions
      >,
      meta: { playlistId: string; playlistTitle: string; usePlaylistFolder?: boolean },
      globalOptions?: QueueAddOptions
    ) => {
      entries.forEach((entry, idx) => {
        const safeDuration =
          typeof entry.duration === 'number' && Number.isFinite(entry.duration)
            ? entry.duration
            : undefined;
        const merged: QueueAddOptions = {
          ...globalOptions,
          ...entry,
          prefetchedInfo: {
            title: entry.title,
            author: entry.author,
            thumbnail: entry.thumbnail,
            duration: safeDuration,
          },
        };
        enqueueUrl(entry.url, merged, {
          playlistId: meta.playlistId,
          playlistTitle: meta.playlistTitle,
          playlistIndex: idx + 1,
        });
      });
    },

    cancel: async (itemId: string) => {
      const state = get({ subscribe });
      const item = findItemById(state, itemId);
      if (!item) return;

      // Handle conversions separately (they don't have jobId)
      if (item.source === 'convert') {
        try {
          await invoke('cancel_conversion', { jobId: item.id });
          update((s) => {
            const items = s.items.map((i) =>
              i.id === itemId && i.source === 'convert'
                ? {
                    ...i,
                    status: 'failed' as DownloadStatus,
                    statusMessage: 'Cancelled',
                    error: 'Cancelled by user',
                  }
                : i
            );
            saveQueueState(items);
            return { ...s, items };
          });
          // Remove from queue after delay
          setTimeout(() => {
            update((s) => {
              const items = s.items.filter((i) => !(i.id === itemId && i.source === 'convert'));
              saveQueueState(items);
              return { ...s, items };
            });
          }, 2000);
        } catch (err) {
          logs.error('queue', `Failed to cancel conversion: ${err}`);
        }
        return;
      }

      if (!item.jobId) return;
      invokeControl(item.jobId, 'cancel');
    },

    retry: (itemId: string) => {
      const state = get({ subscribe });
      const item = findItemById(state, itemId);
      if (!item?.jobId) return;
      invokeControl(item.jobId, 'retry');
    },

    pauseItem: (itemId: string) => {
      const state = get({ subscribe });
      const item = findItemById(state, itemId);
      if (!item?.jobId) return;
      invokeControl(item.jobId, 'pause');
    },

    resumeItem: (itemId: string) => {
      const state = get({ subscribe });
      const item = findItemById(state, itemId);
      if (!item?.jobId) return;
      invokeControl(item.jobId, 'resume');
    },

    pausePlaylist: (playlistId: string) => {
      const state = get({ subscribe });
      for (const item of state.items) {
        if (item.playlistId === playlistId && item.jobId && isActiveStatus(item.status)) {
          invokeControl(item.jobId, 'pause');
        }
      }
    },

    resumePlaylist: (playlistId: string) => {
      const state = get({ subscribe });
      for (const item of state.items) {
        if (item.playlistId === playlistId && item.jobId && item.status === 'paused') {
          invokeControl(item.jobId, 'resume');
        }
      }
    },

    cancelPlaylist: (playlistId: string) => {
      const state = get({ subscribe });
      for (const item of state.items) {
        if (item.playlistId === playlistId && item.jobId && isActiveStatus(item.status)) {
          invokeControl(item.jobId, 'cancel');
        }
      }
    },

    pause: pauseAll,

    resume: resumeAll,

    cancelAll,

    togglePause: () => {
      const paused = get({ subscribe }).isPaused;
      if (paused) resumeAll();
      else pauseAll();
    },

    moveToTop: (itemId: string) => {
      update((s) => {
        const idx = s.items.findIndex((i) => i.id === itemId);
        if (idx === -1) return s;
        const item = s.items[idx];
        const items = [item, ...s.items.slice(0, idx), ...s.items.slice(idx + 1)];
        saveQueueState(items);
        return { ...s, items };
      });
    },

    clearFinished: () => {
      // Only clears completed items, keeps failed items for retry
      update((s) => {
        const items = s.items.filter((i) => i.status !== 'completed');
        saveQueueState(items);
        return { ...s, items };
      });
    },

    clearCompleted: () => {
      // Alias for clearFinished - only clears completed, keeps failed
      update((s) => {
        const items = s.items.filter((i) => i.status !== 'completed');
        saveQueueState(items);
        return { ...s, items };
      });
    },

    clearFailed: () => {
      // Only clears failed items
      update((s) => {
        const items = s.items.filter((i) => i.status !== 'failed');
        saveQueueState(items);
        return { ...s, items };
      });
    },

    clearAll: () => {
      // Clears both completed and failed items
      update((s) => {
        const items = s.items.filter((i) => isActiveStatus(i.status));
        saveQueueState(items);
        return { ...s, items };
      });
    },

    retryAllFailed: () => {
      const state = get({ subscribe });
      for (const item of state.items) {
        if (item.status === 'failed' && item.jobId) {
          invokeControl(item.jobId, 'retry');
        }
      }
    },

    addConversion: (args: {
      id: string;
      title: string;
      author?: string;
      thumbnail?: string;
      duration?: number;
      url?: string;
      targetFormat: string;
      audioOnly: boolean;
    }): string => {
      const safeDuration =
        typeof args.duration === 'number' && Number.isFinite(args.duration) ? args.duration : 0;
      const newItem: QueueItem = {
        id: args.id,
        url: args.url || `convert://${args.id}`,
        status: 'converting',
        statusMessage: `Converting to ${args.targetFormat.toUpperCase()}`,
        title: args.title,
        author: args.author || '',
        thumbnail: args.thumbnail || '',
        duration: safeDuration,
        filesize: 0,
        extension: args.targetFormat,
        filePath: '',
        progress: 0,
        speed: '',
        eta: '',
        addedAt: Date.now(),
        type: args.audioOnly ? 'audio' : 'video',
        priority: 0,
        source: 'convert',
      };

      update((s) => {
        const items = [newItem, ...s.items];
        saveQueueState(items);
        return { ...s, items };
      });

      return args.id;
    },

    updateConversion: (
      id: string,
      patch: Partial<
        Pick<
          QueueItem,
          | 'progress'
          | 'speed'
          | 'status'
          | 'statusMessage'
          | 'filePath'
          | 'extension'
          | 'filesize'
          | 'error'
        >
      >
    ) => {
      update((s) => {
        const items = s.items.map((i) =>
          i.id === id && i.source === 'convert' ? { ...i, ...patch } : i
        );
        saveQueueState(items);
        return { ...s, items };
      });
    },

    removeConversion: (id: string) => {
      update((s) => {
        const items = s.items.filter((i) => !(i.id === id && i.source === 'convert'));
        saveQueueState(items);
        return { ...s, items };
      });
    },
  };
}

export const queue = createQueueStore();

export const isQueuePaused = derived(queue, ($q) => $q.isPaused);

export const activeDownloads = derived(queue, ($q) =>
  $q.items.filter((i) => isActiveStatus(i.status))
);

export const activeDownloadsCount = derived(activeDownloads, ($items) => $items.length);

export const groupedDownloads = derived(queue, ($q): GroupedDownloads => {
  const active = $q.items.filter((i) => isActiveStatus(i.status));
  const byPlaylist = new Map<string, PlaylistGroup>();
  const singles: QueueItem[] = [];

  for (const item of active) {
    if (item.playlistId) {
      const existing = byPlaylist.get(item.playlistId);
      if (existing) {
        existing.items.push(item);
      } else {
        byPlaylist.set(item.playlistId, {
          playlistId: item.playlistId,
          playlistTitle: item.playlistTitle || 'Playlist',
          items: [item],
        });
      }
    } else {
      singles.push(item);
    }
  }

  const groups = Array.from(byPlaylist.values()).map((g) => ({
    ...g,
    items: [...g.items].sort((a, b) => (a.playlistIndex ?? 0) - (b.playlistIndex ?? 0)),
  }));

  return { groups, singles };
});

export const activeConversions = derived(queue, ($q) =>
  $q.items.filter((i) => i.source === 'convert' && isActiveStatus(i.status))
);

export const activeConversionsCount = derived(activeConversions, ($items) => $items.length);

export const failedDownloads = derived(queue, ($q) =>
  $q.items.filter((i) => i.status === 'failed')
);

export const failedDownloadsCount = derived(failedDownloads, ($items) => $items.length);

export const completedDownloads = derived(queue, ($q) =>
  $q.items.filter((i) => i.status === 'completed')
);

export const completedDownloadsCount = derived(completedDownloads, ($items) => $items.length);
