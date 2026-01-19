import type { HistoryItem } from '$lib/stores/history';

export const DOWNLOADS_CONTEXT_KEY = Symbol('downloads-context');

export interface DownloadsContext {
    openItem: (item: HistoryItem) => void;
    openAuthor: (item: HistoryItem) => void;
    playItem: (item: HistoryItem) => void;
    deleteItem: (id: string) => void;
    redownloadItem: (url: string) => void;
    openLink: (url: string) => void;
    openFileLocation: (path: string) => void;
}
