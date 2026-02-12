import { getContext } from 'svelte';
import { DOWNLOADS_CONTEXT_KEY, type DownloadsContext } from '$lib/stores/downloadsContext';
import type { UnifiedDownloadItem, DownloadsState } from '$lib/stores/downloadsState.svelte';
import type { MenuItem } from '$lib/components/ui/ContextMenu.svelte';
import { translate } from '$lib/i18n';
import { getConversionFormats } from '$lib/utils/conversion';
import {
  buildContextMenuItems,
  buildConvertMenuItems,
  handleContextMenuAction,
} from '$lib/utils/contextMenuActions';

export function useDownloadItem(
  getItem: () => UnifiedDownloadItem,
  getDState: () => DownloadsState
) {
  const ctx = getContext<DownloadsContext>(DOWNLOADS_CONTEXT_KEY);

  let contextMenuOpen = $state(false);
  let contextMenuX = $state(0);
  let contextMenuY = $state(0);

  let convertMenuOpen = $state(false);
  let convertMenuX = $state(0);
  let convertMenuY = $state(0);

  let isActiveDownload = $derived(getItem()?.isActive ?? false);
  let displayProgress = $derived(Math.max(0, Math.min(100, Math.round(getItem()?.progress ?? 0))));
  let isPending = $derived(getItem()?.status === 'pending');
  let isPaused = $derived(getItem()?.status === 'paused');
  let isDownloading = $derived(
    getItem()?.status === 'downloading' ||
      getItem()?.status === 'processing' ||
      getItem()?.status === 'fetching-info' ||
      getItem()?.status === 'converting'
  );
  let isFailed = $derived(getItem()?.status === 'failed');

  let fileMissing = $derived(
    !getItem()?.isActive && getDState().isFileMissing(getItem()?.id ?? '')
  );
  let isHovered = $derived(getDState().hoveredItemId === getItem()?.id);
  let thumbnailSrc = $derived(getDState().getThumbnailSrc(getItem()?.thumbnail));
  let colorStyle = $derived(getDState().getItemColorStyle(getItem()?.thumbnail));
  let isThumbnailFailed = $derived(getDState().isThumbnailFailed(getItem()?.id ?? ''));
  let localThumbnail = $derived(getDState().getLocalThumbnail(getItem()?.id ?? ''));

  let conversionFormats = $derived(getConversionFormats(getItem()?.extension));
  let convertMenuItems = $derived.by((): MenuItem[] => {
    return buildConvertMenuItems(conversionFormats);
  });

  let contextMenuItems = $derived.by((): MenuItem[] => {
    const item = getItem();
    if (!item) return [];
    const dState = getDState();
    const items = buildContextMenuItems(item, fileMissing, translate);
    const isSelected = dState.isItemSelected(item.id);
    const selectItem: MenuItem = isSelected
      ? { id: 'deselect', label: translate('downloads.deselectItem'), icon: 'check' }
      : { id: 'select', label: translate('downloads.selectItem'), icon: 'check' };
    return [selectItem, { id: 'divider-select', label: '', divider: true }, ...items];
  });

  $effect(() => {
    const item = getItem();
    if (item && !item.isActive && item.filePath) {
      getDState().checkFileExists(item.id, item.filePath);
    }
  });

  function handleImageLoad() {
    if (!getItem()) return;
    getDState().extractItemColor(thumbnailSrc);
  }

  async function handleImageError() {
    const item = getItem();
    if (!item) return;
    getDState().markThumbnailFailed(item.id);
    if (item.filePath && !localThumbnail) {
      await getDState().generateLocalThumbnail(item.id, item.filePath);
    }
  }

  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    contextMenuX = e.clientX;
    contextMenuY = e.clientY;
    contextMenuOpen = true;
  }

  function handleTap(e: MouseEvent) {
    if (window.matchMedia('(hover: hover)').matches) return;
    e.preventDefault();
    e.stopPropagation();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    contextMenuX = rect.left + rect.width / 2;
    contextMenuY = rect.top + rect.height / 2;
    contextMenuOpen = true;
  }

  function handleContextMenuSelect(id: string) {
    const item = getItem();
    if (!item) return;
    if (id === 'select' || id === 'deselect') {
      getDState().toggleSelection(item.id, true, false);
      return;
    }
    handleContextMenuAction(id, item, ctx, () => {
      convertMenuX = contextMenuX;
      convertMenuY = contextMenuY;
      convertMenuOpen = true;
    });
  }

  function openContextMenuAt(x: number, y: number) {
    contextMenuX = x;
    contextMenuY = y;
    contextMenuOpen = true;
  }

  function openConvertMenuAt(x: number, y: number) {
    convertMenuX = x;
    convertMenuY = y;
    convertMenuOpen = true;
  }

  return {
    ctx,

    get contextMenuOpen() {
      return contextMenuOpen;
    },
    set contextMenuOpen(v: boolean) {
      contextMenuOpen = v;
    },
    get contextMenuX() {
      return contextMenuX;
    },
    get contextMenuY() {
      return contextMenuY;
    },

    get convertMenuOpen() {
      return convertMenuOpen;
    },
    set convertMenuOpen(v: boolean) {
      convertMenuOpen = v;
    },
    get convertMenuX() {
      return convertMenuX;
    },
    get convertMenuY() {
      return convertMenuY;
    },

    get isActiveDownload() {
      return isActiveDownload;
    },
    get displayProgress() {
      return displayProgress;
    },
    get isPending() {
      return isPending;
    },
    get isPaused() {
      return isPaused;
    },
    get isDownloading() {
      return isDownloading;
    },
    get isFailed() {
      return isFailed;
    },

    get fileMissing() {
      return fileMissing;
    },
    get isHovered() {
      return isHovered;
    },
    get thumbnailSrc() {
      return thumbnailSrc;
    },
    get colorStyle() {
      return colorStyle;
    },
    get isThumbnailFailed() {
      return isThumbnailFailed;
    },
    get localThumbnail() {
      return localThumbnail;
    },

    get conversionFormats() {
      return conversionFormats;
    },
    get convertMenuItems() {
      return convertMenuItems;
    },
    get contextMenuItems() {
      return contextMenuItems;
    },

    handleImageLoad,
    handleImageError,
    handleContextMenu,
    handleTap,
    handleContextMenuSelect,
    openContextMenuAt,
    openConvertMenuAt,
  };
}
