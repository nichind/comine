import { writable, derived, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { type UnlistenFn } from '@tauri-apps/api/event';
import { logs } from '../logs';
import { toast } from '$lib/components/ui/Toast.svelte';
import { translate } from '$lib/i18n';
import { safeDuration, safeDurationOrUndefined } from '$lib/utils/duration';
import type { Job, JobControl } from '$lib/bindings';

import type {
  DownloadStatus,
  QueueItem,
  QueueItemSource,
  QueueState,
  QueueAddOptions,
  PlaylistGroup,
  GroupedDownloads,
} from './types';
import { isActiveStatus, isTerminalStatus, waitForSettingsReady, jobToQueueItem } from './utils';
import { setupJobEventListener } from './eventHandler';

export type { QueueItem, QueueAddOptions, PlaylistGroup, GroupedDownloads } from './types';
export type { PrefetchedInfo } from './types';

function toOverrides(ui?: QueueAddOptions): Record<string, unknown> {
  if (!ui) return {};
  const o: Record<string, unknown> = {};
  if (ui.downloadMode) o.downloadMode = ui.downloadMode;
  if (ui.videoQuality) o.videoQuality = ui.videoQuality;
  if (ui.audioQuality) o.audioQuality = ui.audioQuality;
  if (ui.cookiesFromBrowser?.trim()) o.cookiesFromBrowser = ui.cookiesFromBrowser.trim();
  if (ui.customCookies?.trim()) o.customCookies = ui.customCookies.trim();
  if (ui.embedThumbnail !== undefined) o.embedThumbnail = ui.embedThumbnail;
  if (ui.clearMetadata !== undefined) o.clearMetadata = ui.clearMetadata;
  if (ui.embedSubtitles !== undefined) o.embedSubtitles = ui.embedSubtitles;
  if (ui.subtitleLanguages?.trim()) o.subtitleLanguages = ui.subtitleLanguages.trim();
  if (ui.sponsorBlock !== undefined) o.sponsorBlock = ui.sponsorBlock;
  if (ui.sponsorBlock) {
    const cats: string[] = [];
    if (ui.sponsorBlockSkipSponsors) cats.push('sponsor');
    if (ui.sponsorBlockSkipIntros) cats.push('intro');
    if (ui.sponsorBlockSkipSelfPromo) cats.push('selfpromo');
    if (ui.sponsorBlockSkipInteraction) cats.push('interaction');
    if (cats.length > 0) o.sponsorBlockCategories = cats;
  }
  if (ui.useAria2 !== undefined) o.useAria2 = ui.useAria2;
  if (ui.outputTemplate) o.outputTemplate = ui.outputTemplate;
  if (ui.clipRanges && ui.clipRanges.length > 0) o.clipRanges = ui.clipRanges;
  return o;
}

function createQueueStore() {
  const { subscribe, update } = writable<QueueState>({
    items: [],
    isPaused: false,
  });

  let unlisten: UnlistenFn | null = null;

  const MAX_TERMINAL_ITEMS = 200;

  /**
   * Auto-trim completed/failed items from the frontend queue to prevent
   * unbounded memory growth. Keeps the most recent MAX_TERMINAL_ITEMS.
   * Runs periodically (every 60s) and after queue load.
   */
  function autoTrimTerminalItems() {
    update((s) => {
      const active = s.items.filter((i) => !isTerminalStatus(i.status));
      const terminal = s.items.filter((i) => isTerminalStatus(i.status));
      if (terminal.length <= MAX_TERMINAL_ITEMS) return s;

      terminal.sort((a, b) => b.addedAt - a.addedAt);
      const trimmed = terminal.slice(0, MAX_TERMINAL_ITEMS);
      return { ...s, items: [...active, ...trimmed] };
    });
  }

  async function loadQueue() {
    try {
      const jobs = await invoke<Job[]>('get_jobs');
      update((state) => {
        const existingConversions = state.items.filter((i) => i.source === 'convert');
        const jobItems = jobs.map(jobToQueueItem);
        jobItems.sort((a, b) => b.addedAt - a.addedAt);
        return { ...state, items: [...existingConversions, ...jobItems] };
      });
      autoTrimTerminalItems();
    } catch (e) {
      logs.error('queue', `Failed to load queue: ${e}`);
    }
  }

  async function setupListener() {
    if (unlisten) return;
    unlisten = await setupJobEventListener(update);
  }

  function findItemById(state: QueueState, itemId: string): QueueItem | undefined {
    return state.items.find((i) => i.id === itemId);
  }

  function invokeControl(jobId: string, action: JobControl) {
    invoke('control_job', { jobId, action }).catch((e) => {
      logs.error('queue', `Failed to control job (${action}): ${e}`);
    });
  }

  const pauseAll = () => {
    update((s) => ({ ...s, isPaused: true }));
    invoke('pause_all_jobs').catch((e) => logs.error('queue', `Failed to pause all: ${e}`));
  };

  const resumeAll = () => {
    update((s) => ({ ...s, isPaused: false }));
    invoke('resume_all_jobs').catch((e) => logs.error('queue', `Failed to resume all: ${e}`));
  };

  const cancelAll = () => {
    invoke('cancel_all_jobs').catch((e) => logs.error('queue', `Failed to cancel all: ${e}`));
  };

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
      jobId: id,
      url,
      status: 'pending',
      statusMessage: translate('downloads.queue.waiting') || 'Waiting',
      title: prefetched?.title || url,
      author: prefetched?.author || '',
      thumbnail: prefetched?.thumbnail || '',
      duration: safeDuration(prefetched?.duration),
      filesize: 0,
      extension: '',
      filePath: '',
      progress: 0,
      speed: '',
      eta: '',
      addedAt: Date.now(),
      type,
      options,
      source: extras?.source ?? 'ytdlp',
      playlistId: extras?.playlistId,
      playlistTitle: extras?.playlistTitle,
      playlistIndex: extras?.playlistIndex,
    };

    update((s) => ({ ...s, items: [newItem, ...s.items] }));

    void (async () => {
      await waitForSettingsReady();

      const overrides = toOverrides(options);
      if (extras?.filename) {
        overrides.filename = extras.filename;
      }

      const request = {
        url,
        id,
        overrides,
        playlistId: extras?.playlistId ?? null,
        playlistTitle: extras?.playlistTitle ?? null,
        playlistIndex: extras?.playlistIndex ?? null,
      };

      try {
        await invoke<string>('enqueue_url', { request });
      } catch (err: unknown) {
        const errObj = err as { type?: string; message?: string } | undefined;
        const isDuplicate = errObj?.type === 'duplicateUrl';

        if (isDuplicate) {
          update((s) => ({
            ...s,
            items: s.items.filter((i) => i.id !== id),
          }));
          toast.info(translate('downloads.queue.alreadyQueued') || 'Already in queue');
          return;
        }

        const msg =
          err instanceof Error
            ? err.message
            : typeof err === 'object' && err !== null
              ? JSON.stringify(err)
              : String(err);
        logs.error('queue', `Failed to enqueue: ${msg}`);
        update((s) => ({
          ...s,
          items: s.items.map((i) =>
            i.id === id
              ? {
                  ...i,
                  status: 'failed' as DownloadStatus,
                  statusMessage: 'Failed to start',
                  error: msg,
                }
              : i
          ),
        }));
        toast.error(msg);
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
        const entryDuration = safeDurationOrUndefined(entry.duration);
        const merged: QueueAddOptions = {
          ...globalOptions,
          ...entry,
          prefetchedInfo: {
            title: entry.title,
            author: entry.author,
            thumbnail: entry.thumbnail,
            duration: entryDuration,
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

      if (item.source === 'convert') {
        try {
          await invoke('cancel_conversion', { jobId: item.id });
          update((s) => ({
            ...s,
            items: s.items.map((i) =>
              i.id === itemId && i.source === 'convert'
                ? {
                    ...i,
                    status: 'failed' as DownloadStatus,
                    statusMessage: 'Cancelled',
                    error: 'Cancelled by user',
                  }
                : i
            ),
          }));
          setTimeout(
            () =>
              update((s) => ({
                ...s,
                items: s.items.filter((i) => !(i.id === itemId && i.source === 'convert')),
              })),
            2000
          );
        } catch (err) {
          logs.error('queue', `Failed to cancel conversion: ${err}`);
        }
        return;
      }

      if (item.jobId) invokeControl(item.jobId, 'cancel');
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
      const state = get({ subscribe });
      const item = state.items.find((i) => i.id === itemId);
      if (!item) return;

      if (item.jobId) {
        invoke('move_job_to_top', { jobId: item.jobId }).catch((e) =>
          logs.error('queue', `Failed to move job to top: ${e}`)
        );
      }

      update((s) => {
        const idx = s.items.findIndex((i) => i.id === itemId);
        if (idx === -1) return s;
        const found = s.items[idx];
        return { ...s, items: [found, ...s.items.slice(0, idx), ...s.items.slice(idx + 1)] };
      });
    },

    clearFinished: () => {
      invoke('clear_terminal_jobs', { filter: 'completed' }).catch((e) =>
        logs.error('queue', `Failed to clear completed jobs: ${e}`)
      );
      update((s) => ({ ...s, items: s.items.filter((i) => i.status !== 'completed') }));
    },
    clearFailed: () => {
      invoke('clear_terminal_jobs', { filter: 'failed' }).catch((e) =>
        logs.error('queue', `Failed to clear failed jobs: ${e}`)
      );
      update((s) => ({ ...s, items: s.items.filter((i) => i.status !== 'failed') }));
    },
    clearAll: () => {
      invoke('clear_terminal_jobs', { filter: 'all' }).catch((e) =>
        logs.error('queue', `Failed to clear all jobs: ${e}`)
      );
      update((s) => ({ ...s, items: s.items.filter((i) => isActiveStatus(i.status)) }));
    },

    retryAllFailed: () => {
      invoke('retry_all_failed').catch((e) => logs.error('queue', `Failed to retry all: ${e}`));
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
      const argDuration = safeDuration(args.duration);
      const newItem: QueueItem = {
        id: args.id,
        url: args.url || `convert://${args.id}`,
        status: 'converting',
        statusMessage: `Converting to ${args.targetFormat.toUpperCase()}`,
        title: args.title,
        author: args.author || '',
        thumbnail: args.thumbnail || '',
        duration: argDuration,
        filesize: 0,
        extension: args.targetFormat,
        filePath: '',
        progress: 0,
        speed: '',
        eta: '',
        addedAt: Date.now(),
        type: args.audioOnly ? 'audio' : 'video',
        source: 'convert',
      };

      update((s) => ({ ...s, items: [newItem, ...s.items] }));
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
      update((s) => ({
        ...s,
        items: s.items.map((i) => (i.id === id && i.source === 'convert' ? { ...i, ...patch } : i)),
      }));
    },

    removeConversion: (id: string) => {
      update((s) => ({
        ...s,
        items: s.items.filter((i) => !(i.id === id && i.source === 'convert')),
      }));
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

export const failedDownloads = derived(queue, ($q) =>
  $q.items.filter((i) => i.status === 'failed')
);

export const failedDownloadsCount = derived(failedDownloads, ($items) => $items.length);

export const completedDownloads = derived(queue, ($q) =>
  $q.items.filter((i) => i.status === 'completed')
);

export const completedDownloadsCount = derived(completedDownloads, ($items) => $items.length);
