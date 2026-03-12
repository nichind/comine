import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { translate } from '$lib/i18n';
import { formatSpeed, formatTime } from '$lib/utils/format';
import { safeDuration } from '$lib/utils/duration';
import { isIOS } from '$lib/utils/android';
import type { JobEvent } from '$lib/bindings';
import type { DownloadStatus, QueueItem, QueueState } from './types';
import {
  isTerminalStatus,
  jobStatusToDownloadStatus,
  statusMessageFor,
  jobToQueuePatch,
  jobToQueueItem,
  filenameExt,
} from './utils';
import { setColorInCache } from '$lib/utils/color';
import { pushSpeedPoint, clearSpeedPoints } from '$lib/stores/downloadSpeed';

const _isIOS = typeof window !== 'undefined' && isIOS();
let _liveActivityJobId: string | null = null;

type StateUpdater = (fn: (state: QueueState) => QueueState) => void;

const PROGRESS_THROTTLE_MS = 250;
const lastProgressUpdate = new Map<string, number>();

export async function setupJobEventListener(update: StateUpdater): Promise<UnlistenFn> {
  return listen<JobEvent>('job-event', (event) => {
    const payload = event.payload;

    if (payload.type === 'progress') {
      const jobId = payload.data.job_id;
      const now = Date.now();
      const last = lastProgressUpdate.get(jobId) ?? 0;
      if (now - last < PROGRESS_THROTTLE_MS) return;
      lastProgressUpdate.set(jobId, now);
    }

    update((state) => {
      const jobId =
        payload.type === 'added'
          ? payload.data.job.id
          : 'job_id' in payload.data
            ? payload.data.job_id
            : undefined;
      if (!jobId) return state;

      const index = state.items.findIndex((i) => i.jobId === jobId);

      const prevStatus = index !== -1 ? state.items[index].status : undefined;
      const wasFinished = prevStatus ? isTerminalStatus(prevStatus) : false;

      // For progress events, mutate the existing item in-place to avoid
      // copying the entire items array on every tick (multiple times/sec).
      // Other events create a shallow copy since they change array structure or
      // need to trigger derived stores that check item identity.
      const isProgressOnly = payload.type === 'progress';

      const newItems: QueueItem[] | null = isProgressOnly ? null : [...state.items];

      const applyPatchToItem = (patch: Partial<QueueItem>) => {
        if (index === -1) return;
        if (isProgressOnly) {
          const item = state.items[index];
          Object.assign(item, patch);
        } else {
          if (!newItems) return;
          newItems[index] = {
            ...newItems[index],
            ...patch,
            title: patch.title ?? newItems[index].title,
            thumbnail: patch.thumbnail ?? newItems[index].thumbnail,
          };
        }
      };

      switch (payload.type) {
        case 'added': {
          const addedItem = jobToQueueItem(payload.data.job);
          if (index !== -1) {
            // Include type from the full mapping — the placeholder created by
            // enqueueUrl defaults to 'video', but the backend knows the real
            // content type (e.g. 'torrent' for magnet links).
            applyPatchToItem({ ...jobToQueuePatch(payload.data.job), type: addedItem.type });
          } else {
            newItems!.unshift(addedItem);
          }
          break;
        }

        case 'started':
          applyPatchToItem({
            status: 'downloading',
            statusMessage: translate('downloads.status.starting') || 'Starting',
            backendName: payload.data.backend,
          });
          // iOS Dynamic Island: start Live Activity for the first active download
          if (_isIOS && !_liveActivityJobId) {
            _liveActivityJobId = jobId;
            const title =
              index !== -1 ? (newItems ?? state.items)[index].title || 'Downloading...' : 'Downloading...';
            invoke('ios_live_activity_start', { jobId, title }).catch(() => {});
          }
          break;

        case 'urlInfoPatched': {
          const patch = payload.data.patch;
          const itemPatch: Partial<QueueItem> = {};

          if (patch.title !== null) itemPatch.title = patch.title;
          if (patch.thumbnail !== null) itemPatch.thumbnail = patch.thumbnail;

          const author = patch.uploader ?? patch.channel;
          if (author !== null) itemPatch.author = author;
          if (patch.channelUrl !== null) itemPatch.authorUrl = patch.channelUrl;

          if (patch.duration !== null) {
            const dur = safeDuration(patch.duration, -1);
            if (dur >= 0) {
              itemPatch.duration = dur;
            }
          }

          applyPatchToItem(itemPatch);
          break;
        }

        case 'progress': {
          const progress = Math.max(0, Math.min(100, payload.data.progress));
          const speedBps = payload.data.speed ?? undefined;
          const speed =
            typeof speedBps === 'number' && Number.isFinite(speedBps) ? formatSpeed(speedBps) : '';
          const eta = payload.data.eta ? formatTime(payload.data.eta) : '';
          const currentStatus =
            index !== -1 ? (isProgressOnly ? state.items[index].status : prevStatus) : undefined;
          const isProcessing = currentStatus === 'processing' || progress >= 100;
          const status = isProcessing ? 'processing' : 'downloading';
          const statusMessage = isProcessing
            ? translate('downloads.status.processing')
            : translate('downloads.status.downloading');

          // Push speed data point (replaces 500ms polling interval)
          if (typeof speedBps === 'number' && Number.isFinite(speedBps)) {
            pushSpeedPoint(speedBps);
          }

          applyPatchToItem({
            status: status as DownloadStatus,
            statusMessage,
            progress,
            downloadedBytes: payload.data.downloaded_bytes,
            totalBytes: payload.data.total_bytes ?? undefined,
            speedBps,
            speed,
            eta,
          });

          if (index !== -1) {
            const item = isProgressOnly ? state.items[index] : newItems![index];
            emit('download-progress-parsed', {
              url: item.url,
              progress,
              speed,
              eta,
              status,
              statusMessage,
            });
          }
          // iOS Dynamic Island: update Live Activity
          if (_isIOS && _liveActivityJobId === jobId) {
            invoke('ios_live_activity_update', {
              jobId,
              progress: progress / 100,
              speedBps: speedBps ?? 0,
              downloadedBytes: payload.data.downloaded_bytes ?? 0,
              totalBytes: payload.data.total_bytes ?? 0,
              eta: payload.data.eta ?? 0,
            }).catch(() => {});
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
          if (index !== -1) {
            emit('download-status-changed', { url: newItems![index].url, status: 'cancelled' });
          }
          if (_isIOS && _liveActivityJobId === jobId) {
            invoke('ios_live_activity_stop').catch(() => {});
            _liveActivityJobId = null;
          }
          if (jobId) lastProgressUpdate.delete(jobId);
          break;

        case 'failed':
          applyPatchToItem({
            status: 'failed',
            statusMessage: payload.data.error || 'Failed',
            error: payload.data.error,
          });
          if (index !== -1) {
            emit('download-status-changed', {
              url: newItems![index].url,
              status: 'failed',
              error: payload.data.error,
            });
          }
          if (_isIOS && _liveActivityJobId === jobId) {
            invoke('ios_live_activity_stop').catch(() => {});
            _liveActivityJobId = null;
          }
          if (jobId) lastProgressUpdate.delete(jobId);
          break;

        case 'completed': {
          const ext = filenameExt(payload.data.output_path);
          const filesize = payload.data.filesize ?? undefined;
          applyPatchToItem({
            status: 'completed',
            progress: 100,
            statusMessage: 'Finished',
            filePath: payload.data.output_path,
            extension: ext,
            title: payload.data.title ?? undefined,
            thumbnail: payload.data.thumbnail ?? undefined,
            filesize,
            totalBytes: filesize,
          });

          if (index !== -1) {
            const item = newItems![index];
            emit('download-status-changed', {
              url: item.url,
              status: 'completed',
              filePath: payload.data.output_path,
              title: item.title,
            });
          }
          if (_isIOS && _liveActivityJobId === jobId) {
            invoke('ios_live_activity_finish', { jobId }).catch(() => {});
            _liveActivityJobId = null;
          }
          if (jobId) lastProgressUpdate.delete(jobId);
          break;
        }

        case 'filePathResolved': {
          applyPatchToItem({
            filePath: payload.data.output_path,
            extension: filenameExt(payload.data.output_path),
          });
          break;
        }

        case 'thumbnailColorExtracted': {
          if (index !== -1) {
            const items = newItems ?? state.items;
            const thumbnail = items[index].thumbnail;
            if (thumbnail) {
              const [r, g, b] = payload.data.color;
              setColorInCache(thumbnail, { r, g, b });
            }
          }
          break;
        }
      }

      // For progress events, reuse the same items array reference (item was
      // mutated in-place) but create a new state wrapper so subscribers fire.
      // This avoids O(n) array copy per tick while still triggering reactivity.
      if (isProgressOnly) return { ...state };
      return { ...state, items: newItems! };
    });
  });
}
