import { writable, derived } from 'svelte/store';
import { mediaCache, type MediaPreview } from './mediaCache';
import { getQuickThumbnail } from '$lib/utils/thumbnailUtils';

type ViewType = 'home' | 'video' | 'playlist' | 'channel' | 'torrent-search' | 'torrent-detail';

export interface ViewState {
  type: ViewType;
  url?: string;
  cachedData?: MediaPreview;
  torrentQuery?: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  torrentResult?: any;
}

interface NavigationState {
  stack: ViewState[];
}

const MAX_STACK_DEPTH = 20;

function createNavigationStore() {
  const { subscribe, set, update } = writable<NavigationState>({
    stack: [{ type: 'home' }],
  });

  let currentState: NavigationState = { stack: [{ type: 'home' }] };

  subscribe((state) => {
    currentState = state;
  });

  return {
    subscribe,

    getStack(): ViewState[] {
      return currentState.stack;
    },

    push(view: ViewState) {
      if (view.url && view.cachedData) {
        mediaCache.setPreview(view.url, view.cachedData);
      }

      update((state) => {
        let newStack = [...state.stack, view];
        if (newStack.length > MAX_STACK_DEPTH) {
          newStack = [newStack[0], ...newStack.slice(-(MAX_STACK_DEPTH - 1))];
        }
        return { ...state, stack: newStack };
      });
    },

    pop(): boolean {
      let didPop = false;
      update((state) => {
        if (state.stack.length > 1) {
          didPop = true;
          return { ...state, stack: state.stack.slice(0, -1) };
        }
        return state;
      });
      return didPop;
    },

    replace(view: ViewState) {
      if (view.url && view.cachedData) {
        mediaCache.setPreview(view.url, view.cachedData);
      }

      update((state) => ({
        ...state,
        stack: [...state.stack.slice(0, -1), view],
      }));
    },

    reset() {
      set({ stack: [{ type: 'home' }] });
    },

    openVideo(url: string, previewData?: MediaPreview) {
      this._openView('video', url, false, previewData);
    },

    openPlaylist(url: string, previewData?: MediaPreview) {
      this._openView('playlist', url, true, previewData);
    },

    openChannel(url: string, previewData?: MediaPreview) {
      this._openView('channel', url, false, previewData);
    },

    openTorrentSearch(query?: string) {
      update((state) => {
        let newStack = [...state.stack, { type: 'torrent-search' as ViewType, torrentQuery: query }];
        if (newStack.length > MAX_STACK_DEPTH) {
          newStack = [newStack[0], ...newStack.slice(-(MAX_STACK_DEPTH - 1))];
        }
        return { ...state, stack: newStack };
      });
    },

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    openTorrentDetail(result: any) {
      update((state) => {
        let newStack = [
          ...state.stack,
          { type: 'torrent-detail' as ViewType, torrentResult: result },
        ];
        if (newStack.length > MAX_STACK_DEPTH) {
          newStack = [newStack[0], ...newStack.slice(-(MAX_STACK_DEPTH - 1))];
        }
        return { ...state, stack: newStack };
      });
    },

    _openView(type: ViewType, url: string, isPlaylist: boolean, previewData?: MediaPreview) {
      if (previewData) {
        mediaCache.setPreview(url, { ...previewData, isPlaylist });
      }

      update((state) => {
        let cachedData = mediaCache.getBestPreview(url) ?? undefined;
        if (!cachedData) {
          const quickThumb = getQuickThumbnail(url);
          if (quickThumb) {
            cachedData = { thumbnail: quickThumb, isPlaylist };
          }
        }

        let newStack = [...state.stack, { type, url, cachedData }];
        if (newStack.length > MAX_STACK_DEPTH) {
          newStack = [newStack[0], ...newStack.slice(-(MAX_STACK_DEPTH - 1))];
        }
        return { ...state, stack: newStack };
      });
    },
  };
}

export const navigation = createNavigationStore();

export const currentView = derived(navigation, ($nav) => {
  return $nav.stack[$nav.stack.length - 1] || { type: 'home' as ViewType };
});

export const previousView = derived(navigation, ($nav) => {
  if ($nav.stack.length < 2) return null;
  return $nav.stack[$nav.stack.length - 2];
});

export const canGoBack = derived(navigation, ($nav) => {
  return $nav.stack.length > 1;
});
