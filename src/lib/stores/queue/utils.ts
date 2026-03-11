import { get } from 'svelte/store';
import { translate } from '$lib/i18n';
import { formatSpeed, formatTime } from '$lib/utils/format';
import { settingsReady } from '$lib/stores/settings';
import type { Job, JobStatus } from '$lib/bindings';
import type { DownloadStatus, QueueItem } from './types';

export function isTerminalStatus(status: DownloadStatus): boolean {
  return status === 'completed' || status === 'failed';
}

export function isActiveStatus(status: DownloadStatus): boolean {
  return !isTerminalStatus(status);
}

export function waitForSettingsReady(): Promise<void> {
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

export function jobStatusToDownloadStatus(status: JobStatus): DownloadStatus {
  switch (status.type) {
    case 'queued':
    case 'resolving':
      return 'pending';
    case 'downloading':
      return 'downloading';
    case 'postProcessing':
      return 'processing';
    case 'paused':
      return 'paused';
    case 'completed':
      return 'completed';
    case 'failed':
    case 'cancelled':
      return 'failed';
    default:
      return 'pending';
  }
}

export function statusMessageFor(status: DownloadStatus): string {
  switch (status) {
    case 'pending':
      return translate('downloads.queue.waiting') || 'Waiting';
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

export function filenameExt(path: string): string {
  const normalized = path.replace(/\\/g, '/');
  const base = normalized.split('/').pop() || '';
  const idx = base.lastIndexOf('.');
  if (idx === -1) return '';
  return base.slice(idx + 1).toLowerCase();
}

export function jobToQueuePatch(job: Job): Partial<QueueItem> {
  const status = jobStatusToDownloadStatus(job.status);

  let filePath = '';
  if (job.status.type === 'completed') {
    filePath = job.status.data.output_path;
  }

  const extension = filePath ? filenameExt(filePath) : '';

  const speed = job.speed ? formatSpeed(job.speed) : '';
  const eta = job.eta ? formatTime(job.eta) : '';

  return {
    status,
    statusMessage: statusMessageFor(status),
    progress: Number.isFinite(job.progress) ? Math.max(0, Math.min(100, job.progress)) : 0,
    downloadedBytes: job.downloadedBytes,
    totalBytes: job.totalBytes ?? undefined,
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

export function jobToQueueItem(job: Job): QueueItem {
  const patch = jobToQueuePatch(job);
  return {
    id: job.id,
    jobId: job.id,
    url: job.request.url,
    status: patch.status ?? 'pending',
    statusMessage: patch.statusMessage ?? '',
    title: (job.title ?? job.request.url) || job.request.url,
    author: '',
    thumbnail: job.thumbnail ?? '',
    duration: 0,
    filesize: job.totalBytes ?? 0,
    extension: patch.extension ?? '',
    filePath: patch.filePath ?? '',
    progress: job.progress,
    speed: '',
    eta: '',
    error: job.lastError ?? undefined,
    addedAt: job.createdAt,
    type:
      job.contentType === 'Torrent'
        ? 'torrent'
        : job.request.quality.audioOnly
          ? 'audio'
          : 'video',
    options: undefined,
    source: 'ytdlp',
    downloadedBytes: job.downloadedBytes,
    totalBytes: job.totalBytes ?? undefined,
    backendName: job.backend,
  };
}
