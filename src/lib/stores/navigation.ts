import { writable, derived } from 'svelte/store';
import { mediaCache, type MediaPreview } from './mediaCache';
import { getQuickThumbnail } from '$lib/utils/format';

export type ViewType = 'home' | 'video' | 'playlist' | 'channel';

export interface ViewState {
  type: ViewType;
  url?: string;
  cachedData?: MediaPreview;
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

    getCachedPreview(url: string): MediaPreview | null {
      return mediaCache.getBestPreview(url);
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

    goHome() {
      set({ stack: [{ type: 'home' }] });
    },

    /**
     * Open a video view
     * @param url - The video URL
     * @param previewData - Optional preview data to cache (title, thumbnail, etc.)
     */
    openVideo(url: string, previewData?: MediaPreview) {
      if (previewData) {
        mediaCache.setPreview(url, { ...previewData, isPlaylist: false });
      }

      update((state) => {
        let cachedData = mediaCache.getBestPreview(url) ?? undefined;
        if (!cachedData) {
          const quickThumb = getQuickThumbnail(url);
          if (quickThumb) {
            cachedData = { thumbnail: quickThumb, isPlaylist: false };
          }
        }

        let newStack = [...state.stack, { type: 'video' as ViewType, url, cachedData }];
        if (newStack.length > MAX_STACK_DEPTH) {
          newStack = [newStack[0], ...newStack.slice(-(MAX_STACK_DEPTH - 1))];
        }
        return { ...state, stack: newStack };
      });
    },

    /**
     * Open a playlist view
     * @param url - The playlist URL
     * @param previewData - Optional preview data to cache (title, thumbnail, etc.)
     */
    openPlaylist(url: string, previewData?: MediaPreview) {
      if (previewData) {
        mediaCache.setPreview(url, { ...previewData, isPlaylist: true });
      }

      update((state) => {
        let cachedData = mediaCache.getBestPreview(url) ?? undefined;
        if (!cachedData) {
          const quickThumb = getQuickThumbnail(url);
          if (quickThumb) {
            cachedData = { thumbnail: quickThumb, isPlaylist: true };
          }
        }

        let newStack = [...state.stack, { type: 'playlist' as ViewType, url, cachedData }];
        if (newStack.length > MAX_STACK_DEPTH) {
          newStack = [newStack[0], ...newStack.slice(-(MAX_STACK_DEPTH - 1))];
        }
        return { ...state, stack: newStack };
      });
    },

    /**
     * Open a channel view
     * @param url - The channel URL
     * @param previewData - Optional preview data to cache (name, thumbnail, etc.)
     */
    openChannel(url: string, previewData?: MediaPreview) {
      if (previewData) {
        mediaCache.setPreview(url, { ...previewData, isPlaylist: false });
      }

      update((state) => {
        let cachedData = mediaCache.getBestPreview(url) ?? undefined;
        if (!cachedData) {
          const quickThumb = getQuickThumbnail(url);
          if (quickThumb) {
            cachedData = { thumbnail: quickThumb, isPlaylist: false };
          }
        }

        let newStack = [...state.stack, { type: 'channel' as ViewType, url, cachedData }];
        if (newStack.length > MAX_STACK_DEPTH) {
          newStack = [newStack[0], ...newStack.slice(-(MAX_STACK_DEPTH - 1))];
        }
        return { ...state, stack: newStack };
      });
    },

    clearCachedData() {
      mediaCache.clearStale();
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

export const stackDepth = derived(navigation, ($nav) => {
  return $nav.stack.length;
});
