import type { UnifiedDownloadItem } from '$lib/stores/downloadsState.svelte';

export const DOWNLOADS_CONTEXT_KEY = Symbol('downloads-context');

export interface DownloadsContext {
  openItem: (item: UnifiedDownloadItem) => void;
  openAuthor: (item: UnifiedDownloadItem) => void;
  playItem: (item: UnifiedDownloadItem) => void;
  deleteItem: (id: string) => void;
  redownloadItem: (url: string) => void;
  openLink: (url: string) => void;
  openFileLocation: (path: string) => void;
  convertItem: (item: UnifiedDownloadItem, targetFormat: string, audioOnly: boolean) => void;
  showDetails: (item: UnifiedDownloadItem) => void;
}
