import { writable, derived, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event';
import { stat } from '@tauri-apps/plugin-fs';
import { load } from '@tauri-apps/plugin-store';
import type { Store } from '@tauri-apps/plugin-store';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';
import { history } from './history';
import { logs } from './logs';
import { deps } from './deps';
import { settings, getSettings, getProxyConfig } from './settings';
import { appStats } from './stats';
import { toast } from '$lib/components/Toast.svelte';
import { translate } from '$lib/i18n';
import { decodeJobEvent, type JobEvent } from '$lib/jobs/jobEvent';
import {
  isAndroid,
  waitForAndroidYtDlp,
  startAndroidDownloadJob,
  cancelAndroidJob,
  type AndroidYtDlpJobSettings,
} from '$lib/utils/android';
import { getVideoInfoBackend } from '$lib/backend/mediaBackend';

export type DownloadStatus =
  | 'pending'
  | 'paused'
  | 'fetching-info'
  | 'downloading'
  | 'processing'
  | 'completed'
  | 'failed';

export type QueueItemSource = 'ytdlp' | 'file';

export interface QueueItem {
  id: string;
  url: string;
  status: DownloadStatus;
  statusMessage: string;
  title: string;
  author: string;
  thumbnail: string;
  duration: number;
  filesize: number;
  extension: string;
  filePath: string;
  progress: number;
  speed: string;
  eta: string;
  error?: string;
  addedAt: number;
  type: 'video' | 'audio' | 'image' | 'file';
  priority: number;
  options?: Partial<DownloadOptions>;
  playlistId?: string;
  playlistTitle?: string;
  playlistIndex?: number;
  usePlaylistFolder?: boolean;
  source: QueueItemSource;
  mimeType?: string;
  totalBytes?: number;
  downloadedBytes?: number;
  jobId?: string;
}

export interface PrefetchedInfo {
  title?: string;
  thumbnail?: string;
  author?: string;
  duration?: number;
}

export interface DownloadOptions {
  videoQuality: string;
  downloadMode: 'auto' | 'audio' | 'mute';
  audioQuality: string;
  convertToMp4: boolean;
  remux: boolean;
  clearMetadata: boolean;
  dontShowInHistory: boolean;
  useAria2: boolean;
  ignoreMixes: boolean;
  cookiesFromBrowser: string;
  customCookies: string;
  sponsorBlock: boolean;
  sponsorBlockSkipSponsors?: boolean;
  sponsorBlockSkipIntros?: boolean;
  sponsorBlockSkipSelfPromo?: boolean;
  sponsorBlockSkipInteraction?: boolean;
  chapters: boolean;
  embedSubtitles: boolean;
  subtitleLanguages: string;
  embedThumbnail: boolean;
  prefetchedInfo?: PrefetchedInfo;
  outputTemplate?: string;
  clipRanges?: { start: number; end: number }[];
}

interface QueueState {
  items: QueueItem[];
  currentDownloadId: string | null;
  activeDownloadIds: string[];
  isPaused: boolean;
}

let queueStore: Store | null = null;
let saveDebounceTimer: ReturnType<typeof setTimeout> | null = null;
const SAVE_DEBOUNCE_MS = 500;
const MAX_PERSISTED_FAILED_ITEMS = 50;

function serializeQueueItems(items: QueueItem[]): QueueItem[] {
  const pending = items.filter((item) => item.status === 'pending' || item.status === 'paused');
  const failed = items.filter((item) => item.status === 'failed');

  const limitedFailed = failed.slice(0, MAX_PERSISTED_FAILED_ITEMS);

  return [...pending, ...limitedFailed].map((item) => {
    const cleanOptions = item.options
      ? {
          ...item.options,
          prefetchedInfo: item.options.prefetchedInfo
            ? {
                title: item.options.prefetchedInfo.title,
                author: item.options.prefetchedInfo.author,
                duration: item.options.prefetchedInfo.duration,
                thumbnail: item.options.prefetchedInfo.thumbnail?.startsWith('data:')
                  ? undefined
                  : item.options.prefetchedInfo.thumbnail,
              }
            : undefined,
        }
      : undefined;

    return {
      ...item,
      thumbnail: item.thumbnail?.startsWith('data:') ? '' : item.thumbnail,
      status:
        item.status === 'downloading' || item.status === 'processing'
          ? ('pending' as DownloadStatus)
          : item.status,
      statusMessage:
        item.status === 'failed' ? item.statusMessage : translate('downloads.status.queued'),
      progress: 0,
      speed: '',
      eta: '',
      options: cleanOptions,
    };
  });
}

async function loadQueue(): Promise<QueueItem[]> {
  try {
    queueStore = await load('queue.json', { autoSave: false, defaults: {} });
    const items = await queueStore.get<QueueItem[]>('items');
    if (items && Array.isArray(items)) {
      logs.info('queue', `Loaded ${items.length} queued items from storage`);
      return items;
    }
  } catch (error) {
    logs.error('queue', `Failed to load queue from storage: ${error}`);
  }
  return [];
}

function saveQueue(items: QueueItem[]) {
  if (!queueStore) return;

  if (saveDebounceTimer) {
    clearTimeout(saveDebounceTimer);
  }

  saveDebounceTimer = setTimeout(async () => {
    try {
      const serialized = serializeQueueItems(items);
      await queueStore!.set('items', serialized);
      await queueStore!.save();
      logs.debug('queue', `Saved ${serialized.length} queue items to storage`);
    } catch (error) {
      logs.error('queue', `Failed to save queue: ${error}`);
    }
  }, SAVE_DEBOUNCE_MS);
}

function createQueueStore() {
  const { subscribe, set, update } = writable<QueueState>({
    items: [],
    currentDownloadId: null,
    activeDownloadIds: [],
    isPaused: false,
  });

  let unlisten: UnlistenFn | null = null;
  let unlistenDownloadProgress: UnlistenFn | null = null;
  let notificationPermission: boolean | null = null;

  const jobToItemId = new Map<string, string>();
  const jobWaiters = new Map<
    string,
    {
      resolve: () => void;
      reject: (err: unknown) => void;
    }
  >();

  // Throttle high-frequency progress updates.
  const progressThrottleState = new Map<
    string,
    {
      lastUpdateAt: number;
      lastPercent: number | null;
    }
  >();

  // Throttle high-frequency log lines from job processes.
  const logThrottleState = new Map<
    string,
    {
      lastAt: number;
    }
  >();

  const lastJobStatusMessage = new Map<
    string,
    {
      at: number;
      message: string;
    }
  >();

  const maxProgressMap = new Map<string, number>();

  // Clip downloads frequently use ffmpeg section downloader, which often only emits 100% per section.
  // Track per-section state so the progress bar advances across sections instead of jumping to 95%.
  const clipProgressMap = new Map<
    string,
    {
      totalSections: number;
      completedDestinations: Set<string>;
      destinationOrder: string[];
      destinationIndex: Map<string, number>;
      lastDestination?: string;
    }
  >();

  function formatTimestamp(seconds: number): string {
    if (!Number.isFinite(seconds)) return '';
    const total = Math.max(0, Math.floor(seconds));
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    const secs = total % 60;
    if (hours > 0) return `${hours}:${String(minutes).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
    return `${minutes}:${String(secs).padStart(2, '0')}`;
  }

  function formatClipRange(start: number, end: number): string {
    const a = formatTimestamp(start);
    const b = formatTimestamp(end);
    if (!a || !b) return '';
    return `${a}–${b}`;
  }

  function formatClipStatus(
    base: string,
    clipIndex: number | null,
    total: number,
    rangeLabel?: string
  ): string {
    if (!total || total <= 0) return base;
    const idx = clipIndex && clipIndex > 0 ? clipIndex : null;
    const prefix = idx ? `${base} (clip ${idx}/${total}` : `${base} (clip ?/${total}`;
    if (rangeLabel) return `${prefix} • ${rangeLabel})`;
    return `${prefix})`;
  }

  const videoInfoPromises = new Map<string, Promise<void>>();

  const cancelledIds = new Set<string>();

  const CLEANUP_INTERVAL_MS = 5 * 1000;
  let cleanupInterval: ReturnType<typeof setInterval> | null = null;

  function cleanupMaps() {
    const state = get({ subscribe });
    const activeUrls = new Set(state.items.map((i) => i.url));
    const activeIds = new Set(state.items.map((i) => i.id));

    // Clean up maxProgressMap for URLs no longer in queue
    let cleanedProgress = 0;
    maxProgressMap.forEach((_, url) => {
      if (!activeUrls.has(url)) {
        maxProgressMap.delete(url);
        cleanedProgress++;
      }
    });

    // Clean up clipProgressMap for URLs no longer in queue
    let cleanedClip = 0;
    clipProgressMap.forEach((_, url) => {
      if (!activeUrls.has(url)) {
        clipProgressMap.delete(url);
        cleanedClip++;
      }
    });

    // Clean up videoInfoPromises for items no longer in queue
    let cleanedPromises = 0;
    videoInfoPromises.forEach((_, id) => {
      if (!activeIds.has(id)) {
        videoInfoPromises.delete(id);
        cleanedPromises++;
      }
    });

    // Clean up cancelledIds for items no longer referenced
    let cleanedCancelled = 0;
    cancelledIds.forEach((id) => {
      if (!activeIds.has(id)) {
        cancelledIds.delete(id);
        cleanedCancelled++;
      }
    });

    if (cleanedProgress + cleanedClip + cleanedPromises + cleanedCancelled > 0) {
      logs.debug(
        'queue',
        `Cleaned up ${cleanedProgress} progress entries, ${cleanedClip} clip entries, ${cleanedPromises} promises, ${cleanedCancelled} cancelled IDs`
      );
    }
  }

  function startCleanupInterval() {
    if (cleanupInterval) return;
    cleanupInterval = setInterval(cleanupMaps, CLEANUP_INTERVAL_MS);
  }

  async function ensureNotificationPermission(): Promise<boolean> {
    if (notificationPermission !== null) return notificationPermission;

    try {
      let granted = await isPermissionGranted();
      if (!granted) {
        const permission = await requestPermission();
        granted = permission === 'granted';
      }
      notificationPermission = granted;
      return granted;
    } catch {
      notificationPermission = false;
      return false;
    }
  }

  async function sendDownloadNotification(
    type: 'started' | 'completed' | 'failed',
    title: string,
    body?: string
  ) {
    if (isAndroid()) return;

    const currentSettings = getSettings();
    if (!currentSettings.notificationsEnabled) return;

    const hasPermission = await ensureNotificationPermission();
    if (!hasPermission) return;

    try {
      const icons: Record<string, string> = {
        started: '⬇️',
        completed: '✅',
        failed: '❌',
      };

      sendNotification({
        title: `${icons[type]} ${title}`,
        body: body || '',
      });
    } catch (e) {
      logs.warn('queue', `Failed to send notification: ${e}`);
    }
  }

  async function setupListener() {
    if (unlisten) return;

    logs.debug('queue', 'Setting up job-event listener');

    const formatSpeed = (bps: number | null): string => {
      if (!bps || !Number.isFinite(bps) || bps <= 0) return '';
      const kb = 1024;
      const mb = kb * 1024;
      const gb = mb * 1024;
      if (bps >= gb) return `${(bps / gb).toFixed(2)} GiB/s`;
      if (bps >= mb) return `${(bps / mb).toFixed(2)} MiB/s`;
      if (bps >= kb) return `${(bps / kb).toFixed(1)} KiB/s`;
      return `${bps} B/s`;
    };

    const formatEta = (ms: number | null): string => {
      if (!ms || !Number.isFinite(ms) || ms <= 0) return '';
      const totalSec = Math.floor(ms / 1000);
      const h = Math.floor(totalSec / 3600);
      const m = Math.floor((totalSec % 3600) / 60);
      const s = totalSec % 60;
      if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
      return `${m}:${String(s).padStart(2, '0')}`;
    };

    const INVALID_JOB_EVENT_TOAST_DEBOUNCE_MS = 5_000;
    let lastInvalidJobEventToastAt = 0;

    const PROGRESS_THROTTLE_MS = 150;
    const LOG_THROTTLE_MS = 250;

    type BackendDownloadProgress = {
      url: string;
      progress?: number;
      downloadedBytes?: number;
      totalBytes?: number;
      speedBps?: number;
      message?: string;
    };

    const onFileDownloadProgress = (p: BackendDownloadProgress) => {
      const url = p.url;
      if (!url) return;

      const currentState = get({ subscribe });
      const item = currentState.items.find((i) => i.url === url && i.source === 'file');
      if (!item) return;

      const speed = formatSpeed(typeof p.speedBps === 'number' ? p.speedBps : null);

      const downloaded =
        typeof p.downloadedBytes === 'number' && Number.isFinite(p.downloadedBytes)
          ? Math.max(0, Math.floor(p.downloadedBytes))
          : undefined;
      const total =
        typeof p.totalBytes === 'number' && Number.isFinite(p.totalBytes)
          ? Math.max(0, Math.floor(p.totalBytes))
          : undefined;

      const etaMs =
        downloaded != null && total != null && typeof p.speedBps === 'number' && p.speedBps > 0
          ? Math.max(0, Math.floor(((total - downloaded) / p.speedBps) * 1000))
          : null;
      const eta = formatEta(etaMs);

      const rawProgress =
        typeof p.progress === 'number' && Number.isFinite(p.progress)
          ? Math.max(0, Math.min(100, Math.floor(p.progress)))
          : total != null && total > 0 && downloaded != null
            ? Math.max(0, Math.min(100, Math.floor((downloaded / total) * 100)))
            : null;

      const progress =
        rawProgress != null
          ? (() => {
              const capped = Math.min(rawProgress, 99);
              const prevMax = maxProgressMap.get(url) ?? 0;
              const next = Math.max(prevMax, capped);
              maxProgressMap.set(url, next);
              return next;
            })()
          : null;

      const statusMessage =
        item.statusMessage && item.statusMessage !== translate('downloads.status.downloading')
          ? item.statusMessage
          : translate('downloads.status.downloading');

      update((state) => ({
        ...state,
        items: state.items.map((i) =>
          i.id === item.id
            ? {
                ...i,
                status: progress != null && progress >= 99 ? ('processing' as DownloadStatus) : ('downloading' as DownloadStatus),
                progress: progress != null ? progress : i.progress,
                speed,
                eta,
                statusMessage,
                downloadedBytes: downloaded ?? i.downloadedBytes,
                totalBytes: total ?? i.totalBytes,
              }
            : i
        ),
      }));

      if (progress != null) {
        emit('download-progress-parsed', {
          url,
          progress,
          speed,
          eta,
          status: progress >= 99 ? 'processing' : 'downloading',
          statusMessage: '',
        });
      }
    };

    const onJobEventPayload = async (payload: unknown) => {
      const decoded = decodeJobEvent(payload);
      if (!decoded.ok) {
        logs.warn(
          'queue',
          `Received invalid job-event payload: ${decoded.error}${
            decoded.context ? ` | Context: ${JSON.stringify(decoded.context)}` : ''
          }`
        );

        const now = Date.now();
        if (now - lastInvalidJobEventToastAt >= INVALID_JOB_EVENT_TOAST_DEBOUNCE_MS) {
          lastInvalidJobEventToastAt = now;
          toast.error('Internal error: invalid job event received');
        }

        return;
      }

      const p = decoded.event;
      const jobId = p.job_id;

      const itemId = jobToItemId.get(jobId);
      if (!itemId) return;

      const currentState = get({ subscribe });
      const currentItem = currentState.items.find((i) => i.id === itemId);
      const url = currentItem?.url;

      if (p.type === 'Started') {
        const argsPreview = p.args?.length ? ` ${p.args.slice(0, 6).join(' ')}${p.args.length > 6 ? ' …' : ''}` : '';
        logs.info('job', `Started ${p.title} (${p.command}${argsPreview})`);

        // Give the user something concrete to look at in the popup while the job spins up.
        update((state) => ({
          ...state,
          items: state.items.map((item) =>
            item.id === itemId
              ? {
                  ...item,
                  statusMessage: translate('downloads.status.starting'),
                }
              : item
          ),
        }));
        return;
      }

      if (p.type === 'Log') {
        const raw = (p.level || '').toLowerCase();
        const level = (['trace', 'debug', 'info', 'warn', 'error'] as const).includes(raw as any)
          ? (raw as any)
          : ('info' as const);

        const throttleKey = `${jobId}:${p.step_id}:${level}`;
        const now = Date.now();
        const prev = logThrottleState.get(throttleKey) ?? { lastAt: 0 };

        const shouldThrottle = level === 'trace' || level === 'debug' || level === 'info';
        if (shouldThrottle && now - prev.lastAt < LOG_THROTTLE_MS) {
          return;
        }
        logThrottleState.set(throttleKey, { lastAt: now });

        const source = `job:${jobId}`;
        const message = p.message?.trim?.() ? p.message.trim() : '';
        if (!message) return;
        logs.log(level, source, message);
        return;
      }

      if (p.type === 'Status') {
        const now = Date.now();
        const rawFallback = p.message?.replace(/\s+/g, ' ').trim() || '';
        const keyed = p.key ? translate(p.key) : '';
        const resolved = p.key ? (keyed !== p.key ? keyed : rawFallback) : rawFallback;
        const cleaned = resolved.replace(/\s+/g, ' ').trim();
        if (!cleaned) return;
        const short = cleaned.length > 80 ? `${cleaned.slice(0, 77)}…` : cleaned;

        const last = lastJobStatusMessage.get(jobId);
        const isNew = !last || last.message !== short;
        const uiThrottleOk = !last || now - last.at >= 400;
        if (!isNew || !uiThrottleOk) return;

        lastJobStatusMessage.set(jobId, { at: now, message: short });
        update((state) => ({
          ...state,
          items: state.items.map((item) =>
            item.id === itemId
              ? {
                  ...item,
                  statusMessage: short,
                }
              : item
          ),
        }));
        return;
      }

      if (p.type === 'Progress') {
        const fraction = p.fraction ?? null;
        const rawProgress =
          fraction != null ? Math.max(0, Math.min(100, Math.round(fraction * 100))) : null;

        // Many download backends reset progress between sub-steps (video->audio, merge, post-process).
        // Keep progress monotonic and reserve 100% for the terminal Finished event.
        const progress =
          rawProgress != null
            ? (() => {
                const capped = Math.min(rawProgress, 99);
                if (!url) return capped;
                const prevMax = maxProgressMap.get(url) ?? 0;
                const next = Math.max(prevMax, capped);
                maxProgressMap.set(url, next);
                return next;
              })()
            : null;
        const speed = formatSpeed(p.speed_bps);
        const eta = formatEta(p.eta_ms);

        const now = Date.now();
        const throttleKey = `${jobId}:${p.step_id}`;
        const prev = progressThrottleState.get(throttleKey) ?? {
          lastUpdateAt: 0,
          lastPercent: null,
        };

        const percentChanged = progress != null && progress !== prev.lastPercent;
        const timeOk = now - prev.lastUpdateAt >= PROGRESS_THROTTLE_MS;
        const isTerminalish = progress != null && progress >= 99;

        // Always allow updates when percent changes or near completion.
        if (!percentChanged && !isTerminalish && !timeOk) {
          return;
        }

        progressThrottleState.set(throttleKey, {
          lastUpdateAt: now,
          lastPercent: progress,
        });

        const inferredStatus: DownloadStatus =
          p.phase && p.phase.toLowerCase().includes('processing')
            ? ('processing' as DownloadStatus)
            : ('downloading' as DownloadStatus);

        // If the backend doesn't emit Status events for a while (or for tools that only emit Progress),
        // keep the UI message aligned with the current phase so it doesn't get stuck.
        const statusKey =
          inferredStatus === 'processing'
            ? 'downloads.status.processing'
            : ('downloads.status.downloading' as const);
        const lastMsg = lastJobStatusMessage.get(jobId);
        const shouldRefreshMessage = !lastMsg || now - lastMsg.at >= 2_000;
        const refreshedMessage = shouldRefreshMessage ? translate(statusKey) : null;

        update((state) => ({
          ...state,
          items: state.items.map((item) =>
            item.id === itemId
              ? {
                  ...item,
                  status: progress != null ? (progress >= 99 ? ('processing' as DownloadStatus) : inferredStatus) : inferredStatus,
                  progress: progress != null ? progress : item.progress,
                  speed,
                  eta,
                  statusMessage: refreshedMessage ?? item.statusMessage,
                  downloadedBytes: p.downloaded_bytes ?? undefined,
                  totalBytes: p.total_bytes ?? undefined,
                }
              : item
          ),
        }));

        if (refreshedMessage) {
          lastJobStatusMessage.set(jobId, { at: now, message: refreshedMessage });
        }

        if (url && progress != null) {
          emit('download-progress-parsed', {
            url,
            progress,
            speed,
            eta,
            status: progress >= 99 ? ('processing' as DownloadStatus) : inferredStatus,
            statusMessage: '',
          });
        }
        return;
      }

      if (p.type === 'Artifact') {
        const filePath = p.path;
        if (!filePath) return;

        const extension =
          p.ext?.toString?.()?.toLowerCase?.() ||
          filePath.split('.').pop()?.toLowerCase() ||
          'mp4';

        let filesize =
          typeof p.size_bytes === 'number' && Number.isFinite(p.size_bytes)
            ? Math.max(0, Math.floor(p.size_bytes))
            : 0;

        // On desktop, stat() is reliable; on Android prefer native-provided size.
        if (!isAndroid() && (!filesize || filesize <= 0)) {
          try {
            const fileStat = await stat(filePath);
            filesize = fileStat.size;
          } catch (err) {
            logs.warn('queue', `Could not get file size: ${err}`);
          }
        }

        update((state) => ({
          ...state,
          items: state.items.map((item) =>
            item.id === itemId ? { ...item, filePath, extension, filesize } : item
          ),
        }));
        return;
      }

      if (p.type === 'Failed') {
        update((state) => ({
          ...state,
          items: state.items.map((item) =>
            item.id === itemId
              ? { ...item, status: 'failed' as DownloadStatus, error: p.error || 'Download failed' }
              : item
          ),
        }));
        jobWaiters.get(jobId)?.reject(p.error || 'Download failed');
        jobWaiters.delete(jobId);
        jobToItemId.delete(jobId);
        // Clear throttling state for this job.
        for (const key of progressThrottleState.keys()) {
          if (key.startsWith(`${jobId}:`)) progressThrottleState.delete(key);
        }

        if (url) maxProgressMap.delete(url);
        lastJobStatusMessage.delete(jobId);

        for (const key of logThrottleState.keys()) {
          if (key.startsWith(`${jobId}:`)) logThrottleState.delete(key);
        }
        return;
      }

      if (p.type === 'Cancelled') {
        jobWaiters.get(jobId)?.reject('cancelled');
        jobWaiters.delete(jobId);
        jobToItemId.delete(jobId);
        for (const key of progressThrottleState.keys()) {
          if (key.startsWith(`${jobId}:`)) progressThrottleState.delete(key);
        }

        if (url) maxProgressMap.delete(url);
        lastJobStatusMessage.delete(jobId);

        for (const key of logThrottleState.keys()) {
          if (key.startsWith(`${jobId}:`)) logThrottleState.delete(key);
        }
        return;
      }

      if (p.type === 'Finished') {
        jobWaiters.get(jobId)?.resolve();
        jobWaiters.delete(jobId);
        jobToItemId.delete(jobId);

        for (const key of progressThrottleState.keys()) {
          if (key.startsWith(`${jobId}:`)) progressThrottleState.delete(key);
        }

        if (url) maxProgressMap.delete(url);
        lastJobStatusMessage.delete(jobId);

        for (const key of logThrottleState.keys()) {
          if (key.startsWith(`${jobId}:`)) logThrottleState.delete(key);
        }

        update((state) => ({
          ...state,
          items: state.items.map((item) =>
            item.id === itemId ? { ...item, progress: 100 } : item
          ),
        }));
        return;
      }
    };

    if (isAndroid()) {
      const domHandler = (e: Event) => {
        const detail = (e as CustomEvent).detail;
        void onJobEventPayload(detail);
      };
      window.addEventListener('job-event', domHandler as EventListener);
      unlisten = () => window.removeEventListener('job-event', domHandler as EventListener);
    } else {
      unlisten = await listen<JobEvent>('job-event', async (event) => {
        await onJobEventPayload(event.payload);
      });

      if (!unlistenDownloadProgress) {
        unlistenDownloadProgress = await listen<BackendDownloadProgress>(
          'download-progress',
          (event) => {
            try {
              onFileDownloadProgress(event.payload);
            } catch (e) {
              logs.debug('queue', `download-progress handler error: ${e}`);
            }
          }
        );
      }
    }
  }

  async function processQueue() {
    const state = get({ subscribe });

    if (state.isPaused) {
      logs.debug('queue', 'Queue is paused, skipping processing');
      return;
    }

    const currentSettings = getSettings();
    const maxConcurrent = currentSettings.concurrentDownloads ?? 2;

    const activeCount = state.activeDownloadIds.length;

    if (activeCount >= maxConcurrent) {
      logs.trace('queue', `Already at max concurrent downloads (${activeCount}/${maxConcurrent})`);
      return;
    }

    const pendingItems = state.items
      .filter((item) => item.status === 'pending')
      .sort((a, b) => {
        if (b.priority !== a.priority) return b.priority - a.priority;
        return a.addedAt - b.addedAt;
      });

    const slotsAvailable = maxConcurrent - activeCount;
    const itemsToStart = pendingItems.slice(0, slotsAvailable);

    if (itemsToStart.length === 0) {
      return;
    }

    logs.info(
      'queue',
      `Starting ${itemsToStart.length} download(s), ${activeCount} already active, max ${maxConcurrent}`
    );

    for (const pendingItem of itemsToStart) {
      processDownload(pendingItem);
    }
  }

  async function processFileDownload(pendingItem: QueueItem) {
    const itemId = pendingItem.id;
    const url = pendingItem.url;

    logs.info('queue', `Starting file download: ${pendingItem.title} from ${url}`);

    maxProgressMap.delete(url);

    update((state) => ({
      ...state,
      currentDownloadId: itemId,
      activeDownloadIds: [...state.activeDownloadIds, itemId],
      items: state.items.map((item) =>
        item.id === itemId
          ? {
              ...item,
              status: 'downloading' as DownloadStatus,
              statusMessage: translate('downloads.status.downloading'),
            }
          : item
      ),
    }));

    sendDownloadNotification(
      'started',
      translate('notifications.downloadStarted'),
      pendingItem.title || url
    );

    try {
      const currentSettings = getSettings();
      const proxyConfig = getProxyConfig();

      // Bypass proxy for file downloads if setting is enabled
      const effectiveProxyConfig = currentSettings.bypassProxyForDownloads
        ? { mode: 'none', customUrl: '', retryWithoutProxy: false }
        : proxyConfig;

      let filePath = '';
      let extension = pendingItem.extension;
      let filesize = pendingItem.totalBytes || 0;

      if (isAndroid()) {
        await setupListener();

        const initStart = Date.now();
        await waitForAndroidYtDlp();
        logs.debug('queue', `[Android] yt-dlp ready after ${Date.now() - initStart}ms (file download)`);

        const androidJobSettings = {
          format: 'best',
          playlistFolder: null,
          isAudioOnly: false,
          aria2Connections: currentSettings.aria2Connections ?? 8,
          aria2Splits: currentSettings.aria2Splits ?? 8,
          aria2MinSplitSize: currentSettings.aria2MinSplitSize ?? '1M',
          speedLimit: currentSettings.downloadSpeedLimit ?? 0,
          downloadPath: currentSettings.downloadPath || null,
          youtubePlayerClient: currentSettings.youtubePlayerClient || null,
          // For direct file URLs we want a deterministic filename.
          outputTemplate: pendingItem.title || 'download',
          // Keep post-processing off for files.
          embedThumbnail: false,
          embedChapters: false,
          embedSubtitles: false,
          subtitleLanguages: null,
          sponsorBlock: false,
          sponsorBlockCategories: [],
          clearMetadata: false,
          remux: false,
          convertToMp4: false,
          clipRanges: null,
        };

        const jobId = startAndroidDownloadJob(url, androidJobSettings);
        logs.info('queue', `Android file download job started: jobId=${jobId}`);

        jobToItemId.set(jobId, itemId);
        update((state) => ({
          ...state,
          items: state.items.map((i) => (i.id === itemId ? { ...i, jobId } : i)),
        }));

        await new Promise<void>((resolve, reject) => {
          jobWaiters.set(jobId, { resolve, reject });
        });

        const stateAfter = get({ subscribe });
        const completed = stateAfter.items.find((i) => i.id === itemId);
        filePath = completed?.filePath || '';
        extension = completed?.extension || extension;
        filesize = completed?.filesize || filesize;
      } else {
        logs.info('queue', `Invoking download_file command for ${pendingItem.title}`);

        const result = await invoke<string>('download_file', {
          url: url,
          filename: pendingItem.title,
          downloadPath: currentSettings.downloadPath || '',
          proxyConfig: effectiveProxyConfig,
          connections: currentSettings.aria2Connections,
          splits: currentSettings.aria2Splits,
          minSplitSize: currentSettings.aria2MinSplitSize,
          speedLimit: currentSettings.downloadSpeedLimit,
        });

        logs.info('queue', `download_file returned: ${result}`);

        filePath = result;
        extension = filePath.split('.').pop()?.toLowerCase() || pendingItem.extension;

        filesize = pendingItem.totalBytes || 0;
        try {
          const fileStat = await stat(filePath);
          filesize = fileStat.size;
        } catch (err) {
          logs.warn('queue', `Could not get file size: ${err}`);
        }
      }

      if (!filePath) {
        throw new Error('Download completed but no output file was reported');
      }

      logs.info('queue', `File download completed: ${filePath}`);

      // Check if the file is an image - use the file itself as thumbnail
      const imageExtensions = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg', 'ico'];
      const isImage = imageExtensions.includes(extension.toLowerCase());
      const thumbnail = isImage ? filePath : '';
      const fileType = isImage ? ('image' as const) : ('file' as const);

      update((state) => {
        const newItems = state.items.map((item) =>
          item.id === itemId
            ? {
                ...item,
                status: 'completed' as DownloadStatus,
                progress: 100,
                filePath,
                extension,
                filesize,
                thumbnail,
                type: fileType,
              }
            : item
        );
        saveQueue(newItems);
        return {
          ...state,
          currentDownloadId:
            state.activeDownloadIds.length <= 1
              ? null
              : (state.activeDownloadIds.find((id) => id !== itemId) ?? null),
          activeDownloadIds: state.activeDownloadIds.filter((id) => id !== itemId),
          items: newItems,
        };
      });

      const sizeMb = filesize ? filesize / (1024 * 1024) : 0;
      appStats.trackDownload(sizeMb, true);

      await history.add({
        url: url,
        title: pendingItem.title || 'Downloaded file',
        author: new URL(url).hostname,
        thumbnail: thumbnail,
        extension: extension,
        size: filesize,
        duration: 0,
        filePath: filePath,
        type: fileType,
      });

      emit('download-status-changed', {
        url: url,
        status: 'completed',
        filePath: filePath,
        title: pendingItem.title || 'Downloaded file',
      });

      sendDownloadNotification(
        'completed',
        translate('notifications.downloadComplete'),
        pendingItem.title || 'Download finished'
      );

      toast.success(translate('download.success'));

      setTimeout(() => {
        update((state) => ({
          ...state,
          items: state.items.filter((item) => item.id !== itemId),
        }));
      }, 3000);
    } catch (error) {
      if (cancelledIds.has(itemId)) {
        cancelledIds.delete(itemId);
        return;
      }

      logs.error('queue', `File download failed: ${error}`);

      appStats.trackDownload(0, false);

      update((state) => ({
        ...state,
        currentDownloadId:
          state.activeDownloadIds.length <= 1
            ? null
            : (state.activeDownloadIds.find((id) => id !== itemId) ?? null),
        activeDownloadIds: state.activeDownloadIds.filter((id) => id !== itemId),
        items: state.items.map((item) =>
          item.id === itemId
            ? { ...item, status: 'failed' as DownloadStatus, error: String(error) }
            : item
        ),
      }));

      emit('download-status-changed', {
        url: url,
        status: 'failed',
        error: String(error),
      });

      sendDownloadNotification(
        'failed',
        translate('notifications.downloadFailed'),
        pendingItem.title || String(error)
      );

      toast.error(`${translate('download.error')}: ${error}`);
    } finally {
      maxProgressMap.delete(url);
      processQueue();
    }
  }

  async function processDownload(pendingItem: QueueItem) {
    if (pendingItem.source === 'file') {
      return processFileDownload(pendingItem);
    }

    const itemId = pendingItem.id;
    const url = pendingItem.url;

    logs.info('queue', `Starting download: ${url}`);
    logs.debug(
      'queue',
      `Download options: mode=${pendingItem.options?.downloadMode}, quality=${pendingItem.options?.videoQuality}, cookies=${pendingItem.options?.cookiesFromBrowser || 'none'}`
    );

    maxProgressMap.delete(url);

    update((state) => ({
      ...state,
      currentDownloadId: itemId,
      activeDownloadIds: [...state.activeDownloadIds, itemId],
      items: state.items.map((item) =>
        item.id === itemId
          ? {
              ...item,
              status: 'downloading' as DownloadStatus,
              statusMessage: translate('downloads.status.starting'),
            }
          : item
      ),
    }));

    sendDownloadNotification(
      'started',
      translate('notifications.downloadStarted'),
      pendingItem.title || url
    );

    try {
      // Skip fetching video info if essential data is already present.
      const hasEssentialInfo = pendingItem.title && pendingItem.title !== url && pendingItem.duration > 0;

      // On Android, fetch metadata after starting the job.
      if (!isAndroid()) {
        if (!hasEssentialInfo) {
          logs.debug('queue', `Fetching video info before download for: ${url.slice(0, 50)}...`);
          try {
            await fetchVideoInfo(
              itemId,
              url,
              pendingItem.options?.cookiesFromBrowser,
              pendingItem.options?.customCookies
            );
          } catch (infoError) {
            logs.warn('queue', `Failed to fetch video info (continuing with download): ${infoError}`);
          }
        } else {
          logs.debug('queue', `Skipping video info fetch (already have info): ${pendingItem.title}`);
        }
      }

      let filePath = '';
      let filesize = 0;
      let extension = pendingItem.options?.downloadMode === 'audio' ? 'mp3' : 'mp4';

      if (isAndroid()) {
        const initStart = Date.now();
        await waitForAndroidYtDlp();
        logs.debug('queue', `[Android] yt-dlp ready after ${Date.now() - initStart}ms`);

        const downloadMode = pendingItem.options?.downloadMode ?? 'auto';
        const videoQuality = pendingItem.options?.videoQuality ?? '';

        const isRawFormat =
          videoQuality &&
          (/^\d/.test(videoQuality) ||
            videoQuality.includes('+') ||
            videoQuality.startsWith('best'));

        let format = 'best';
        if (isRawFormat) {
          format = videoQuality;
        } else if (downloadMode === 'audio') {
          format = 'bestaudio[ext=m4a]/bestaudio';
        } else if (downloadMode === 'mute') {
          format = 'bestvideo';
        }

        const isAudioOnly = downloadMode === 'audio';

        const playlistFolder =
          pendingItem.playlistTitle && pendingItem.usePlaylistFolder !== false
            ? pendingItem.playlistTitle
            : null;
        logs.info(
          'queue',
          `Starting Android download: ${url} (format: ${format}, isAudioOnly: ${isAudioOnly}${playlistFolder ? `, folder: ${playlistFolder}` : ''})`
        );

        const currentSettings = getSettings();

        let androidDownloadPath = currentSettings.downloadPath;
        if (isAudioOnly && currentSettings.useAudioPath && currentSettings.audioPath) {
          androidDownloadPath = currentSettings.audioPath;
          logs.info('queue', `[Android] Using separate audio path: ${androidDownloadPath}`);
        }

        // Check if ytdlp advanced settings override global aria2 settings (same as desktop path)
        const ytdlpAdvanced = currentSettings.ytdlpAdvanced;
        const useYtdlpAria2Override = ytdlpAdvanced?.aria2OverrideGlobal ?? false;
        
        // Determine aria2 settings (use yt-dlp specific if override is enabled)
        const aria2Connections = useYtdlpAria2Override
          ? (ytdlpAdvanced?.aria2YtdlpConnections ?? 8)
          : (currentSettings.aria2Connections ?? 8);
        const aria2Splits = useYtdlpAria2Override
          ? (ytdlpAdvanced?.aria2YtdlpSplits ?? 8)
          : (currentSettings.aria2Splits ?? 8);
        const aria2MinSplitSize = useYtdlpAria2Override
          ? (ytdlpAdvanced?.aria2YtdlpMinSplitSize ?? '1M')
          : (currentSettings.aria2MinSplitSize ?? '1M');

        // Build SponsorBlock categories array from individual flags
        const sponsorBlockCategories: string[] = [];
        if (pendingItem.options?.sponsorBlock ?? currentSettings.sponsorBlock) {
          if (pendingItem.options?.sponsorBlockSkipSponsors ?? currentSettings.sponsorBlockSkipSponsors ?? true) {
            sponsorBlockCategories.push('sponsor');
          }
          if (pendingItem.options?.sponsorBlockSkipIntros ?? currentSettings.sponsorBlockSkipIntros ?? false) {
            sponsorBlockCategories.push('intro', 'outro');
          }
          if (pendingItem.options?.sponsorBlockSkipSelfPromo ?? currentSettings.sponsorBlockSkipSelfPromo ?? false) {
            sponsorBlockCategories.push('selfpromo');
          }
          if (pendingItem.options?.sponsorBlockSkipInteraction ?? currentSettings.sponsorBlockSkipInteraction ?? false) {
            sponsorBlockCategories.push('interaction');
          }
        }

        const androidJobSettings: AndroidYtDlpJobSettings = {
          format,
          playlistFolder,
          isAudioOnly,
          aria2Connections,
          aria2Splits,
          aria2MinSplitSize,
          speedLimit: currentSettings.downloadSpeedLimit,
          downloadPath: androidDownloadPath,
          youtubePlayerClient: currentSettings.youtubePlayerClient ?? null,
          outputTemplate: pendingItem.options?.outputTemplate || ytdlpAdvanced?.outputTemplate || null,
          embedThumbnail: isAudioOnly ? (pendingItem.options?.embedThumbnail ?? currentSettings.embedThumbnail) : false,
          embedChapters: pendingItem.options?.chapters ?? currentSettings.chapters,
          embedSubtitles: pendingItem.options?.embedSubtitles ?? currentSettings.embedSubtitles,
          subtitleLanguages: pendingItem.options?.subtitleLanguages ?? currentSettings.subtitleLanguages ?? 'en,ru',
          sponsorBlock: sponsorBlockCategories.length > 0,
          sponsorBlockCategories,
          clearMetadata: pendingItem.options?.clearMetadata ?? false,
          remux: pendingItem.options?.remux ?? true,
          convertToMp4: pendingItem.options?.convertToMp4 ?? false,
          clipRanges: pendingItem.options?.clipRanges ?? null,
        };

        const jobId = startAndroidDownloadJob(url, androidJobSettings);
        logs.info('queue', `Android download job started: jobId=${jobId}`);

        if (!hasEssentialInfo) {
          void fetchVideoInfo(
            itemId,
            url,
            pendingItem.options?.cookiesFromBrowser,
            pendingItem.options?.customCookies
          ).catch((infoError) => {
            logs.debug('queue', `[Android] Video info fetch (post-start) failed: ${infoError}`);
          });
        }

        jobToItemId.set(jobId, itemId);
        update((state) => ({
          ...state,
          items: state.items.map((i) => (i.id === itemId ? { ...i, jobId } : i)),
        }));

        await new Promise<void>((resolve, reject) => {
          jobWaiters.set(jobId, { resolve, reject });
        });

        const stateAfter = get({ subscribe });
        const completed = stateAfter.items.find((i) => i.id === itemId);
        filePath = completed?.filePath || '';
        extension = completed?.extension || extension;
        filesize = completed?.filesize || 0;
      } else {
        const currentSettings = getSettings();
        const isAudioDownload = pendingItem.options?.downloadMode === 'audio';
        let downloadPath = currentSettings.downloadPath || '';

        if (isAudioDownload && currentSettings.useAudioPath && currentSettings.audioPath) {
          downloadPath = currentSettings.audioPath;
          logs.info('queue', `Using separate audio path: ${downloadPath}`);
        }

        logs.debug(
          'queue',
          `Download path decision: isAudio=${isAudioDownload}, useAudioPath=${currentSettings.useAudioPath}, audioPath=${currentSettings.audioPath}, final=${downloadPath}`
        );

        const playlistTitle =
          pendingItem.playlistTitle && pendingItem.usePlaylistFolder !== false
            ? pendingItem.playlistTitle
            : null;

        const proxyConfig = getProxyConfig();

        logs.info('queue', `Starting download job for: ${url}`);

        // Check if ytdlp advanced settings override global aria2 settings
        const ytdlpAdvanced = currentSettings.ytdlpAdvanced;
        const useYtdlpAria2Override = ytdlpAdvanced?.aria2OverrideGlobal ?? false;

        // Determine aria2 settings (use yt-dlp specific if override is enabled)
        const aria2Connections = useYtdlpAria2Override
          ? (ytdlpAdvanced?.aria2YtdlpConnections ?? 8)
          : (currentSettings.aria2Connections ?? 8);
        const aria2Splits = useYtdlpAria2Override
          ? (ytdlpAdvanced?.aria2YtdlpSplits ?? 8)
          : (currentSettings.aria2Splits ?? 8);
        const aria2MinSplitSize = useYtdlpAria2Override
          ? (ytdlpAdvanced?.aria2YtdlpMinSplitSize ?? '1M')
          : (currentSettings.aria2MinSplitSize ?? '1M');
        const aria2DisableIpv6 = useYtdlpAria2Override
          ? (ytdlpAdvanced?.aria2YtdlpDisableIPv6 ?? true)
          : (currentSettings.aria2DisableIPv6 ?? true);
        const aria2CustomArgs = useYtdlpAria2Override
          ? (ytdlpAdvanced?.aria2YtdlpCustomArgs ?? '')
          : (currentSettings.aria2CustomArgs ?? '');

        // Build custom args from advanced settings
        const downloadCustomArgs = ytdlpAdvanced?.downloadCustomArgs ?? '';
        const concurrentFragments = ytdlpAdvanced?.downloadConcurrentFragments ?? 1;
        const retries = ytdlpAdvanced?.downloadRetries ?? 10;
        const fragmentRetries = ytdlpAdvanced?.downloadFragmentRetries ?? 10;

        const downloadPromise = invoke<string>('download_video', {
          url: url,
          videoQuality: pendingItem.options?.videoQuality ?? 'max',
          downloadMode: pendingItem.options?.downloadMode ?? 'auto',
          audioQuality: pendingItem.options?.audioQuality ?? 'best',
          convertToMp4: pendingItem.options?.convertToMp4 ?? false,
          remux: pendingItem.options?.remux ?? true,
          clearMetadata: pendingItem.options?.clearMetadata ?? false,
          useAria2: pendingItem.options?.useAria2 ?? currentSettings.useAria2 ?? true,
          aria2Connections: aria2Connections,
          aria2Splits: aria2Splits,
          aria2MinSplitSize: aria2MinSplitSize,
          aria2DisableIpv6: aria2DisableIpv6,
          aria2CustomArgs: aria2CustomArgs,
          noPlaylist: pendingItem.options?.ignoreMixes ?? true,
          cookiesFromBrowser: pendingItem.options?.cookiesFromBrowser ?? '',
          customCookies: pendingItem.options?.customCookies ?? '',
          downloadPath: downloadPath,
          embedThumbnail:
            isAudioDownload &&
            (pendingItem.options?.embedThumbnail ?? currentSettings.embedThumbnail),
          thumbnailUrlForEmbed: pendingItem.thumbnail || '',
          playlistTitle: playlistTitle,
          proxyConfig: proxyConfig,
          sponsorBlock: pendingItem.options?.sponsorBlock ?? currentSettings.sponsorBlock,
          sponsorBlockSkipSponsors:
            pendingItem.options?.sponsorBlockSkipSponsors ??
            currentSettings.sponsorBlockSkipSponsors ??
            true,
          sponsorBlockSkipIntros:
            pendingItem.options?.sponsorBlockSkipIntros ??
            currentSettings.sponsorBlockSkipIntros ??
            false,
          sponsorBlockSkipSelfPromo:
            pendingItem.options?.sponsorBlockSkipSelfPromo ??
            currentSettings.sponsorBlockSkipSelfPromo ??
            false,
          sponsorBlockSkipInteraction:
            pendingItem.options?.sponsorBlockSkipInteraction ??
            currentSettings.sponsorBlockSkipInteraction ??
            false,
          chapters: pendingItem.options?.chapters ?? currentSettings.chapters,
          embedSubtitles: pendingItem.options?.embedSubtitles ?? currentSettings.embedSubtitles,
          subtitleLanguages:
            pendingItem.options?.subtitleLanguages ?? currentSettings.subtitleLanguages ?? 'en,ru',
          downloadSpeedLimit: currentSettings.downloadSpeedLimit,
          youtubePlayerClient: currentSettings.youtubePlayerClient,
          // Advanced yt-dlp options
          concurrentFragments: concurrentFragments,
          retries: retries,
          fragmentRetries: fragmentRetries,
          downloadCustomArgs: downloadCustomArgs,
          postProcessCustomArgs: ytdlpAdvanced?.postProcessCustomArgs ?? '',
          keepOriginal: ytdlpAdvanced?.postProcessKeepOriginal ?? false,
          // Use per-item output template if provided, otherwise fall back to global settings
          outputTemplate: pendingItem.options?.outputTemplate
            ? pendingItem.options.outputTemplate
            : ytdlpAdvanced?.outputTemplate ?? '',
          restrictFilenames: ytdlpAdvanced?.outputRestrictFilenames ?? false,
          windowsFilenames: ytdlpAdvanced?.outputWindowsFilenames ?? false,
          // Clip ranges for partial downloads
          clipRanges: pendingItem.options?.clipRanges ?? null,
        });

        logs.info(
          'queue',
          `Invoking download_video: downloadMode=${pendingItem.options?.downloadMode}, isAudioDownload=${isAudioDownload}, downloadPath=${downloadPath}, playlistTitle=${playlistTitle}`
        );
        logs.debug(
          'queue',
          `Full invoke params: videoQuality=${pendingItem.options?.videoQuality ?? 'max'}, remux=${pendingItem.options?.remux ?? true}, convertToMp4=${pendingItem.options?.convertToMp4 ?? false}`
        );

        logs.debug('queue', 'Awaiting download invoke...');
        const jobId = await downloadPromise;
        logs.info('queue', `download job started: jobId=${jobId}`);

        jobToItemId.set(jobId, itemId);
        update((state) => ({
          ...state,
          items: state.items.map((i) => (i.id === itemId ? { ...i, jobId } : i)),
        }));

        await new Promise<void>((resolve, reject) => {
          jobWaiters.set(jobId, { resolve, reject });
        });

        const stateAfter = get({ subscribe });
        const completed = stateAfter.items.find((i) => i.id === itemId);
        filePath = completed?.filePath || '';
        extension = completed?.extension || extension;
        filesize = completed?.filesize || 0;
      }

      update((state) => ({
        ...state,
        items: state.items.map((item) =>
          item.id === itemId ? { ...item, filePath, extension, filesize } : item
        ),
      }));

      logs.info('queue', `Download completed: ${url}`);
      logs.debug('queue', `File details: path=${filePath}, size=${filesize}, ext=${extension}`);

      let extractedThumb = '';
      const currentItem = get({ subscribe }).items.find((i) => i.id === itemId);
      if (!currentItem?.thumbnail && filePath && !isAndroid()) {
        try {
          extractedThumb = await invoke<string>('extract_video_thumbnail', { filePath });
          if (extractedThumb) {
            update((state) => ({
              ...state,
              items: state.items.map((item) =>
                item.id === itemId ? { ...item, thumbnail: extractedThumb } : item
              ),
            }));
          }
        } catch {}
      }

      update((state) => {
        const newItems = state.items.map((item) =>
          item.id === itemId
            ? { ...item, status: 'completed' as DownloadStatus, progress: 100 }
            : item
        );
        saveQueue(newItems);
        return {
          ...state,
          currentDownloadId:
            state.activeDownloadIds.length <= 1
              ? null
              : (state.activeDownloadIds.find((id) => id !== itemId) ?? null),
          activeDownloadIds: state.activeDownloadIds.filter((id) => id !== itemId),
          items: newItems,
        };
      });

      const completedItem = get({ subscribe }).items.find((i) => i.id === itemId);
      if (completedItem) {
        const sizeMb = completedItem.filesize ? completedItem.filesize / (1024 * 1024) : 0;
        appStats.trackDownload(sizeMb, true);

        logs.debug(
          'queue',
          `Saving to history: title=${completedItem.title}, duration=${completedItem.duration}, size=${completedItem.filesize}, playlist=${completedItem.playlistTitle || 'none'}`
        );
        await history.add({
          url: completedItem.url,
          title: completedItem.title || 'Downloaded video',
          author: completedItem.author || 'Unknown',
          thumbnail: completedItem.thumbnail || '',
          extension: completedItem.extension,
          size: completedItem.filesize,
          duration: completedItem.duration || 0,
          filePath: completedItem.filePath || '',
          type: completedItem.type,
          playlistId: completedItem.playlistId,
          playlistTitle: completedItem.playlistTitle,
          playlistIndex: completedItem.playlistIndex,
        });

        emit('download-status-changed', {
          url: completedItem.url,
          status: 'completed',
          filePath: completedItem.filePath,
          title: completedItem.title,
        });

        sendDownloadNotification(
          'completed',
          translate('notifications.downloadComplete'),
          completedItem.title || 'Download finished'
        );
      }

      toast.success(translate('download.success'));

      setTimeout(() => {
        update((state) => ({
          ...state,
          items: state.items.filter((item) => item.id !== itemId),
        }));
      }, 3000);
    } catch (error) {
      if (cancelledIds.has(itemId)) {
        cancelledIds.delete(itemId);
        logs.debug('queue', `Download was cancelled, skipping error handling: ${url}`);
        return;
      }
      logs.error('queue', `Download failed for ${url}: ${error}`);

      appStats.trackDownload(0, false);

      const failedItem = get({ subscribe }).items.find((i) => i.id === itemId);
      if (failedItem) {
        logs.debug(
          'queue',
          `Failed item state: status=${failedItem.status}, progress=${failedItem.progress}, statusMessage=${failedItem.statusMessage}`
        );
      }

      update((state) => ({
        ...state,
        currentDownloadId:
          state.activeDownloadIds.length <= 1
            ? null
            : (state.activeDownloadIds.find((id) => id !== itemId) ?? null),
        activeDownloadIds: state.activeDownloadIds.filter((id) => id !== itemId),
        items: state.items.map((item) =>
          item.id === itemId
            ? { ...item, status: 'failed' as DownloadStatus, error: String(error) }
            : item
        ),
      }));

      emit('download-status-changed', {
        url: url,
        status: 'failed',
        error: String(error),
      });

      sendDownloadNotification(
        'failed',
        translate('notifications.downloadFailed'),
        failedItem?.title || String(error)
      );

      toast.error(`${translate('download.error')}: ${error}`);
    } finally {
      maxProgressMap.delete(url);
      processQueue();
    }
  }

  async function fetchVideoInfo(
    itemId: string,
    url: string,
    cookiesFromBrowser?: string,
    customCookies?: string
  ) {
    logs.debug('queue', `Fetching video info for: ${url}`);

    const MAX_RETRIES = 3;
    const RETRY_DELAY_MS = 1000;

    interface VideoInfo {
      title: string;
      uploader?: string;
      channel?: string;
      creator?: string;
      uploader_id?: string;
      thumbnail?: string;
      duration?: number;
      filesize?: number;
      ext?: string;
    }

    const currentSettings = getSettings();

    let lastError: unknown;

    for (let attempt = 1; attempt <= MAX_RETRIES; attempt++) {
      try {
        let info: VideoInfo;

        const playerClient = currentSettings.usePlayerClientForExtraction
          ? currentSettings.youtubePlayerClient
          : currentSettings.extractionPlayerClient || null;

        const proxyConfig = getProxyConfig();
        info = await getVideoInfoBackend({
          url,
          cookiesFromBrowser: cookiesFromBrowser ?? '',
          customCookies: customCookies ?? '',
          proxyConfig,
          youtubePlayerClient: playerClient,
        });
        logs.debug(
          'queue',
          `Video info (attempt ${attempt}): title=${info.title}, uploader=${info.uploader}`
        );

        const isTwitter = /(?:twitter\.com|x\.com)/i.test(url);
        const authorDisplay =
          isTwitter && info.uploader_id
            ? `@${info.uploader_id}`
            : info.uploader || info.channel || info.creator || '';

        let cleanTitle = (info.title || '').replace(/\.f(?:hls-?)?\d+$/i, '').trim();
        cleanTitle = cleanTitle.replace(/(\.f\d+)+$/i, '').trim();

        update((state) => ({
          ...state,
          items: state.items.map((item) =>
            item.id === itemId
              ? {
                  ...item,
                  title: cleanTitle.slice(0, 200) || item.title,
                  author: authorDisplay || item.author,
                  thumbnail: info.thumbnail || item.thumbnail,
                  duration: info.duration || item.duration,
                  filesize: info.filesize || item.filesize,
                  extension: info.ext || item.extension,
                }
              : item
          ),
        }));

        return;
      } catch (error) {
        lastError = error;
        logs.warn(
          'queue',
          `Video info fetch attempt ${attempt}/${MAX_RETRIES} failed for ${url}: ${error}`
        );

        if (attempt < MAX_RETRIES) {
          await new Promise((resolve) => setTimeout(resolve, RETRY_DELAY_MS * attempt));
        }
      }
    }

    logs.warn(
      'queue',
      `All ${MAX_RETRIES} attempts to fetch video info failed for ${url}: ${lastError}`
    );
  }

  return {
    subscribe,

    async init() {
      await setupListener();
      startCleanupInterval();

      const persistedItems = await loadQueue();
      if (persistedItems.length > 0) {
        const validItems = persistedItems.filter(
          (item) =>
            item.status === 'pending' || item.status === 'paused' || item.status === 'failed'
        );

        const resetItems = validItems.map((item) => ({
          ...item,
          status:
            item.status === 'downloading' ||
            item.status === 'processing' ||
            item.status === 'fetching-info'
              ? ('pending' as DownloadStatus)
              : item.status,
          progress: item.status === 'failed' ? item.progress : 0,
          speed: '',
          eta: '',
        }));

        if (resetItems.length > 0) {
          const currentState = get({ subscribe });
          const existingUrls = new Set(
            currentState.items
              .filter((i) => i.status !== 'completed' && i.status !== 'failed')
              .map((i) => i.url)
          );
          const uniqueResetItems = resetItems.filter((item) => !existingUrls.has(item.url));

          if (uniqueResetItems.length < resetItems.length) {
            logs.info(
              'queue',
              `Skipped ${resetItems.length - uniqueResetItems.length} duplicate items from storage`
            );
          }

          if (uniqueResetItems.length > 0) {
            const mergedItems = [...uniqueResetItems, ...currentState.items];
            update((state) => ({
              ...state,
              items: mergedItems,
            }));
            logs.info('queue', `Restored ${uniqueResetItems.length} queue items from storage`);
            saveQueue(mergedItems);
            processQueue();
          } else {
            logs.info('queue', 'No valid queue items to restore');
          }
        } else {
          logs.info('queue', 'No valid queue items to restore');
        }
      }
    },

    add(
      url: string,
      options?: Partial<DownloadOptions>,
      playlistInfo?: {
        playlistId: string;
        playlistTitle: string;
        playlistIndex?: number;
        usePlaylistFolder?: boolean;
      }
    ): string | null {
      if (!isAndroid()) {
        const depsState = get(deps);
        const ytdlpInstalled = depsState.ytdlp?.installed ?? false;
        const luxInstalled = depsState.lux?.installed ?? false;
        const ffmpegInstalled = depsState.ffmpeg?.installed ?? false;

        if ((!ytdlpInstalled && !luxInstalled) || !ffmpegInstalled) {
          logs.warn(
            'queue',
            `Missing dependencies: ytdlp=${ytdlpInstalled}, lux=${luxInstalled}, ffmpeg=${ffmpegInstalled}`
          );

          if (!ffmpegInstalled) {
            toast.error(translate('settings.deps.missingFfmpeg'));
          } else {
            toast.error('Missing backend: install yt-dlp or lux');
          }

          return null;
        }
      }

      const state = get({ subscribe });
      const existingItem = state.items.find(
        (item) => item.url === url && item.status !== 'completed' && item.status !== 'failed'
      );
      if (existingItem) {
        logs.debug('queue', `URL already in queue: ${url}`);
        return null;
      }

      const currentSettings = getSettings();
      let finalOptions: Partial<DownloadOptions> = { ...options };

      const isYouTubeMusic = /music\.youtube\.com/i.test(url);
      logs.info(
        'queue',
        `Add queue: isYouTubeMusic=${isYouTubeMusic}, setting=${currentSettings.youtubeMusicAudioOnly}, existingMode=${options?.downloadMode}`
      );

      if (isYouTubeMusic && currentSettings.youtubeMusicAudioOnly && !options?.downloadMode) {
        finalOptions.downloadMode = 'audio';
        logs.info('queue', `YouTube Music detected - set downloadMode to audio`);
      }

      logs.info('queue', `Final downloadMode: ${finalOptions.downloadMode}`);

      logs.info('queue', 'Using backend auto-selection');

      const id = crypto.randomUUID();
      const prefetched = finalOptions?.prefetchedInfo;

      const newItem: QueueItem = {
        id,
        url,
        status: 'pending',
        statusMessage: translate('downloads.status.queued'),
        title: prefetched?.title || url,
        author: prefetched?.author || '',
        thumbnail: prefetched?.thumbnail || '',
        duration: prefetched?.duration || 0,
        filesize: 0,
        extension: finalOptions?.downloadMode === 'audio' ? 'm4a' : 'mp4',
        filePath: '',
        progress: 0,
        speed: '',
        eta: '',
        addedAt: Date.now(),
        type: finalOptions?.downloadMode === 'audio' ? 'audio' : 'video',
        priority: 0,
        options: finalOptions,
        playlistId: playlistInfo?.playlistId,
        playlistTitle: playlistInfo?.playlistTitle,
        playlistIndex: playlistInfo?.playlistIndex,
        usePlaylistFolder: playlistInfo?.usePlaylistFolder,
        source: 'ytdlp',
      };

      let wasAdded = false;

      update((state) => {
        const alreadyExists = state.items.some(
          (item) => item.url === url && item.status !== 'completed' && item.status !== 'failed'
        );

        if (alreadyExists) {
          logs.info('queue', `Duplicate prevented (race condition): ${url}`);
          return state;
        }

        wasAdded = true;
        const newItems = [...state.items, newItem];
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
        };
      });

      if (!wasAdded) {
        return null;
      }

      processQueue();

      return id;
    },

    addFile(fileInfo: {
      url: string;
      filename: string;
      size?: number;
      mimeType?: string;
    }): string | null {
      const state = get({ subscribe });
      const existingItem = state.items.find(
        (item) =>
          item.url === fileInfo.url && item.status !== 'completed' && item.status !== 'failed'
      );
      if (existingItem) {
        logs.debug('queue', `URL already in queue: ${fileInfo.url}`);
        return null;
      }

      const id = crypto.randomUUID();

      const extension = fileInfo.filename.split('.').pop()?.toLowerCase() || 'bin';

      const newItem: QueueItem = {
        id,
        url: fileInfo.url,
        status: 'pending',
        statusMessage: translate('downloads.status.queued'),
        title: fileInfo.filename,
        author: new URL(fileInfo.url).hostname,
        thumbnail: '',
        duration: 0,
        filesize: fileInfo.size || 0,
        extension,
        filePath: '',
        progress: 0,
        speed: '',
        eta: '',
        addedAt: Date.now(),
        type: 'file',
        priority: 0,
        source: 'file',
        mimeType: fileInfo.mimeType,
        totalBytes: fileInfo.size,
        downloadedBytes: 0,
      };

      let wasAdded = false;

      update((state) => {
        const alreadyExists = state.items.some(
          (item) =>
            item.url === fileInfo.url && item.status !== 'completed' && item.status !== 'failed'
        );

        if (alreadyExists) {
          logs.info('queue', `Duplicate file prevented (race condition): ${fileInfo.url}`);
          return state;
        }

        wasAdded = true;
        const newItems = [...state.items, newItem];
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
        };
      });

      if (!wasAdded) {
        return null;
      }

      processQueue();

      logs.info(
        'queue',
        `Added file download: ${fileInfo.filename} (${fileInfo.size || 'unknown size'})`
      );

      return id;
    },

    addPlaylist(
      entries: Array<{
        url: string;
        title?: string;
        thumbnail?: string;
        author?: string;
        duration?: number;
        downloadMode?: 'auto' | 'audio' | 'mute';
        videoQuality?: string;
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
      }>,
      playlistInfo: {
        playlistId: string;
        playlistTitle: string;
        usePlaylistFolder?: boolean;
      },
      globalOptions?: Partial<DownloadOptions>,
      order: 'queue' | 'reverse' | 'shuffle' = 'queue'
    ): string[] {
      let orderedEntries = [...entries];
      switch (order) {
        case 'reverse':
          orderedEntries = orderedEntries.reverse();
          break;
        case 'shuffle':
          orderedEntries = orderedEntries.sort(() => Math.random() - 0.5);
          break;
      }

      const addedIds: string[] = [];

      orderedEntries.forEach((entry, index) => {
        const entryOptions: Partial<DownloadOptions> = {
          ...globalOptions,
          downloadMode: entry.downloadMode ?? globalOptions?.downloadMode,
          videoQuality: entry.videoQuality ?? globalOptions?.videoQuality,
          sponsorBlock: entry.sponsorBlock ?? globalOptions?.sponsorBlock,
          sponsorBlockSkipSponsors: entry.sponsorBlockSkipSponsors ?? globalOptions?.sponsorBlockSkipSponsors,
          sponsorBlockSkipIntros: entry.sponsorBlockSkipIntros ?? globalOptions?.sponsorBlockSkipIntros,
          sponsorBlockSkipSelfPromo: entry.sponsorBlockSkipSelfPromo ?? globalOptions?.sponsorBlockSkipSelfPromo,
          sponsorBlockSkipInteraction: entry.sponsorBlockSkipInteraction ?? globalOptions?.sponsorBlockSkipInteraction,
          chapters: entry.chapters ?? globalOptions?.chapters,
          embedSubtitles: entry.embedSubtitles ?? globalOptions?.embedSubtitles,
          subtitleLanguages: entry.subtitleLanguages ?? globalOptions?.subtitleLanguages,
          embedThumbnail: entry.embedThumbnail ?? globalOptions?.embedThumbnail,
          clearMetadata: entry.clearMetadata ?? globalOptions?.clearMetadata,
          prefetchedInfo: {
            title: entry.title,
            thumbnail: entry.thumbnail,
            author: entry.author,
            duration: entry.duration,
          },
        };

        const id = this.add(entry.url, entryOptions, {
          playlistId: playlistInfo.playlistId,
          playlistTitle: playlistInfo.playlistTitle,
          playlistIndex: index + 1,
          usePlaylistFolder: playlistInfo.usePlaylistFolder,
        });

        if (id) {
          addedIds.push(id);
        }
      });

      logs.info(
        'queue',
        `Added ${addedIds.length}/${entries.length} items from playlist "${playlistInfo.playlistTitle}"`
      );

      return addedIds;
    },

    async cancel(id: string) {
      const state = get({ subscribe });
      const item = state.items.find((i) => i.id === id);

      cancelledIds.add(id);

      if (item && (item.status === 'downloading' || item.status === 'processing')) {
        try {
          if (item.jobId) {
            jobToItemId.delete(item.jobId);
            jobWaiters.get(item.jobId)?.reject('cancelled');
            jobWaiters.delete(item.jobId);
            if (isAndroid()) {
              cancelAndroidJob(item.jobId);
            } else {
              await invoke('jobs_cancel', { jobId: item.jobId });
            }
          }
          logs.info('queue', `Download cancelled: ${item.url}`);
        } catch (err) {
          logs.warn('queue', `Failed to cancel download: ${err}`);
        }
      }

      // Emit cancelled event so notification popup can close
      if (item) {
        emit('download-status-changed', {
          url: item.url,
          status: 'cancelled',
        });
      }

      update((state) => {
        const newItems = state.items.filter((item) => item.id !== id);
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
          activeDownloadIds: state.activeDownloadIds.filter((activeId) => activeId !== id),
          currentDownloadId: state.currentDownloadId === id ? null : state.currentDownloadId,
        };
      });

      toast.info('Download cancelled');

      processQueue();
    },

    retry(id: string) {
      update((state) => {
        const newItems = state.items.map((item) =>
          item.id === id
            ? { ...item, status: 'pending' as DownloadStatus, error: undefined, progress: 0 }
            : item
        );
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
        };
      });
      processQueue();
    },

    clearFinished() {
      update((state) => {
        const newItems = state.items.filter(
          (item) => item.status !== 'completed' && item.status !== 'failed'
        );
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
        };
      });
    },

    clearAll() {
      update((state) => {
        saveQueue([]);
        return {
          ...state,
          items: [],
          activeDownloadIds: [],
          currentDownloadId: null,
        };
      });
    },

    pause() {
      update((state) => ({ ...state, isPaused: true }));
    },

    resume() {
      update((state) => ({ ...state, isPaused: false }));
      processQueue();
    },

    togglePause() {
      const state = get({ subscribe });
      if (state.isPaused) {
        update((s) => ({ ...s, isPaused: false }));
        processQueue();
      } else {
        update((s) => ({ ...s, isPaused: true }));
      }
    },

    pauseItem(id: string) {
      update((state) => {
        const newItems = state.items.map((item) =>
          item.id === id && item.status === 'pending'
            ? { ...item, status: 'paused' as DownloadStatus }
            : item
        );
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
        };
      });
    },

    resumeItem(id: string) {
      update((state) => {
        const newItems = state.items.map((item) =>
          item.id === id && item.status === 'paused'
            ? { ...item, status: 'pending' as DownloadStatus }
            : item
        );
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
        };
      });
      processQueue();
    },

    moveUp(id: string) {
      update((state) => {
        const newItems = state.items.map((item) =>
          item.id === id ? { ...item, priority: item.priority + 1 } : item
        );
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
        };
      });
    },

    moveDown(id: string) {
      update((state) => {
        const newItems = state.items.map((item) =>
          item.id === id ? { ...item, priority: Math.max(0, item.priority - 1) } : item
        );
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
        };
      });
    },

    moveToTop(id: string) {
      const state = get({ subscribe });
      const maxPriority = Math.max(...state.items.map((i) => i.priority), 0);
      update((s) => {
        const newItems = s.items.map((item) =>
          item.id === id ? { ...item, priority: maxPriority + 1 } : item
        );
        saveQueue(newItems);
        return {
          ...s,
          items: newItems,
        };
      });
    },

    cleanup() {
      if (cleanupInterval) {
        clearInterval(cleanupInterval);
        cleanupInterval = null;
      }
      if (unlisten) {
        unlisten();
        unlisten = null;
      }
      if (unlistenDownloadProgress) {
        unlistenDownloadProgress();
        unlistenDownloadProgress = null;
      }
      maxProgressMap.clear();
      videoInfoPromises.clear();
      cancelledIds.clear();
      jobToItemId.clear();
      jobWaiters.clear();
    },

    cancelPlaylist(playlistId: string) {
      const state = get({ subscribe });
      const playlistItems = state.items.filter((i) => i.playlistId === playlistId);

      playlistItems.forEach((item) => {
        cancelledIds.add(item.id);
        if (item.status === 'downloading' || item.status === 'processing') {
          if (item.jobId) {
            jobToItemId.delete(item.jobId);
            jobWaiters.get(item.jobId)?.reject('cancelled');
            jobWaiters.delete(item.jobId);
            if (isAndroid()) {
              try {
                cancelAndroidJob(item.jobId);
              } catch (e) {
                console.warn(e);
              }
            } else {
              invoke('jobs_cancel', { jobId: item.jobId }).catch(console.warn);
            }
          }
        }
      });

      const playlistItemIds = new Set(playlistItems.map((i) => i.id));
      update((state) => {
        const newItems = state.items.filter((item) => item.playlistId !== playlistId);
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
          activeDownloadIds: state.activeDownloadIds.filter((id) => !playlistItemIds.has(id)),
          currentDownloadId: playlistItems.some((i) => i.id === state.currentDownloadId)
            ? null
            : state.currentDownloadId,
        };
      });

      toast.info('Playlist downloads cancelled');
      processQueue();
    },

    pausePlaylist(playlistId: string) {
      update((state) => {
        const newItems = state.items.map((item) =>
          item.playlistId === playlistId && item.status === 'pending'
            ? { ...item, status: 'paused' as DownloadStatus }
            : item
        );
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
        };
      });
    },

    resumePlaylist(playlistId: string) {
      update((state) => {
        const newItems = state.items.map((item) =>
          item.playlistId === playlistId && item.status === 'paused'
            ? { ...item, status: 'pending' as DownloadStatus }
            : item
        );
        saveQueue(newItems);
        return {
          ...state,
          items: newItems,
        };
      });
      processQueue();
    },

    getPlaylistProgress(playlistId: string): { completed: number; total: number; failed: number } {
      const state = get({ subscribe });
      const items = state.items.filter((i) => i.playlistId === playlistId);
      return {
        completed: items.filter((i) => i.status === 'completed').length,
        failed: items.filter((i) => i.status === 'failed').length,
        total: items.length,
      };
    },
  };
}

export const queue = createQueueStore();

export const isQueuePaused = derived(queue, ($queue) => $queue.isPaused);

export const activeDownloadsCount = derived(
  queue,
  ($queue) =>
    $queue.items.filter((item) => item.status !== 'completed' && item.status !== 'failed').length
);

export const pendingDownloadsCount = derived(
  queue,
  ($queue) =>
    $queue.items.filter((item) => item.status === 'pending' || item.status === 'paused').length
);

export const activeDownloads = derived(queue, ($queue) =>
  $queue.items.filter((item) => item.status !== 'completed' && item.status !== 'failed')
);

export interface PlaylistGroup {
  playlistId: string;
  playlistTitle: string;
  items: QueueItem[];
  completed: number;
  failed: number;
  total: number;
  isExpanded: boolean;
}

export const groupedDownloads = derived(queue, ($queue) => {
  const activeItems = $queue.items.filter(
    (item) => item.status !== 'completed' && item.status !== 'failed'
  );

  const playlistMap = new Map<string, QueueItem[]>();
  const singles: QueueItem[] = [];

  activeItems.forEach((item) => {
    if (item.playlistId) {
      const existing = playlistMap.get(item.playlistId) || [];
      existing.push(item);
      playlistMap.set(item.playlistId, existing);
    } else {
      singles.push(item);
    }
  });

  const groups: PlaylistGroup[] = [];

  playlistMap.forEach((items, playlistId) => {
    items.sort((a, b) => (a.playlistIndex || 0) - (b.playlistIndex || 0));

    groups.push({
      playlistId,
      playlistTitle: items[0]?.playlistTitle || 'Playlist',
      items,
      completed: items.filter((i) => i.status === 'completed').length,
      failed: items.filter((i) => i.status === 'failed').length,
      total: items.length,
      isExpanded: true,
    });
  });

  return { groups, singles };
});
