import type { ClipRange } from '$lib/bindings';

export type DownloadStatus =
  | 'pending'
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
  options?: QueueAddOptions;
  source: QueueItemSource;
  downloadedBytes?: number;
  totalBytes?: number;
  backendName?: string;

  playlistId?: string;
  playlistTitle?: string;
  playlistIndex?: number;
}

export interface QueueState {
  items: QueueItem[];
  isPaused: boolean;
}
