import type { MenuItem } from '$lib/components/ui/ContextMenu.svelte';
import type { UnifiedDownloadItem } from '$lib/stores/downloadsState.svelte';
import type { DownloadsContext } from '$lib/stores/downloadsContext';
import { getConversionFormats, type FormatOption } from '$lib/utils/conversion';
import { openFile } from '$lib/utils/platform';
import { isMobile } from '$lib/utils/android';
import { queue } from '$lib/stores/queue';
import { history } from '$lib/stores/history';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type TranslateFn = (key: string, params?: any) => string;

export function buildActiveDownloadMenuItems(
  item: UnifiedDownloadItem,
  t: TranslateFn
): MenuItem[] {
  const items: MenuItem[] = [];
  const isPaused = item.status === 'paused';
  const isPending = item.status === 'pending';
  const isDownloading =
    item.status === 'downloading' ||
    item.status === 'processing' ||
    item.status === 'fetching-info' ||
    item.status === 'converting';

  items.push({ id: 'details', label: t('downloads.showDetails'), icon: 'info' });

  if (item.source !== 'convert') {
    if (isPaused) {
      items.push({ id: 'resume', label: t('downloads.queue.resumeItem'), icon: 'play' });
    } else if (isPending || isDownloading) {
      items.push({ id: 'pause', label: t('downloads.queue.pauseItem'), icon: 'pause' });
    }
  }

  items.push({ id: 'cancel', label: t('common.cancel'), icon: 'close', danger: true });

  return items;
}
export function buildHistoryItemMenuItems(
  item: UnifiedDownloadItem,
  fileMissing: boolean,
  conversionFormats: FormatOption[],
  t: TranslateFn
): MenuItem[] {
  const isMediaType = item.type === 'video' || item.type === 'audio';
  const hasChannelUrl = !!item.authorUrl;
  const isTorrentOnMobile = item.type === 'torrent' && isMobile();

  const items: MenuItem[] = [
    {
      id: 'play',
      label: isTorrentOnMobile ? 'Open in VLC' : t('downloads.play'),
      icon: isTorrentOnMobile ? 'arrow_outward' : 'play',
      disabled: fileMissing,
    },
    { id: 'details', label: t('downloads.showDetails'), icon: 'info' },
  ];

  if (isMediaType && !isTorrentOnMobile) {
    items.push({
      id: 'systemPlayer',
      label: t('downloads.playWithSystemPlayer'),
      icon: 'arrow_outward',
      disabled: fileMissing,
    });
  }

  if (hasChannelUrl) {
    items.push({ id: 'viewChannel', label: t('downloads.viewChannel'), icon: 'user' });
  }

  items.push({
    id: 'toggleFavourite',
    label: item.isFavourite ? t('downloads.removeFromFavourites') : t('downloads.addToFavourites'),
    icon: 'star',
  });

  items.push(
    { id: 'folder', label: t('downloads.openFolder'), icon: 'folder', disabled: fileMissing },
    {
      id: 'convert',
      label: t('downloads.convert'),
      icon: 'refresh',
      disabled: fileMissing || conversionFormats.length === 0,
    },
    { id: 'divider1', label: '', divider: true },
    { id: 'copyUrl', label: t('downloads.copyUrl'), icon: 'link' },
    { id: 'openLink', label: t('downloads.openLink'), icon: 'globe' },
    { id: 'redownload', label: t('downloads.redownload'), icon: 'download' },
    { id: 'divider2', label: '', divider: true },
    { id: 'delete', label: t('downloads.delete'), icon: 'trash', danger: true }
  );

  return items;
}
export function buildContextMenuItems(
  item: UnifiedDownloadItem,
  fileMissing: boolean,
  t: TranslateFn
): MenuItem[] {
  const conversionFormats = getConversionFormats(item.extension);

  if (item.isActive) {
    return buildActiveDownloadMenuItems(item, t);
  }

  return buildHistoryItemMenuItems(item, fileMissing, conversionFormats, t);
}
export function buildConvertMenuItems(formats: FormatOption[]): MenuItem[] {
  return formats.map((f) => ({
    id: `convert-${f.format}`,
    label: f.label,
    icon: f.audioOnly ? 'music' : 'video',
  }));
}
export function handleContextMenuAction(
  id: string,
  item: UnifiedDownloadItem,
  ctx: DownloadsContext,
  openConvertMenu: () => void
): boolean {
  const conversionFormats = getConversionFormats(item.extension);

  if (id.startsWith('convert-')) {
    const format = id.replace('convert-', '');
    const formatInfo = conversionFormats.find((f) => f.format === format);
    if (formatInfo && item.filePath) {
      ctx.convertItem(item, format, formatInfo.audioOnly);
    }
    return false;
  }

  switch (id) {
    case 'details':
      ctx.showDetails(item);
      break;
    case 'resume':
      queue.resumeItem(item.id);
      break;
    case 'pause':
      queue.pauseItem(item.id);
      break;
    case 'cancel':
      queue.cancel(item.id);
      break;
    case 'play':
      if (item.filePath) ctx.playItem(item);
      break;
    case 'systemPlayer':
      if (item.filePath) openFile(item.filePath);
      break;
    case 'viewChannel':
      ctx.openAuthor(item);
      break;
    case 'toggleFavourite':
      history.toggleFavourite(item.id);
      break;
    case 'folder':
      if (item.filePath) ctx.openFileLocation(item.filePath);
      break;
    case 'convert':
      openConvertMenu();
      return true;
    case 'copyUrl':
      navigator.clipboard.writeText(item.url);
      break;
    case 'openLink':
      ctx.openLink(item.url);
      break;
    case 'redownload':
      ctx.redownloadItem(item.url);
      break;
    case 'delete':
      ctx.deleteItem(item.id);
      break;
  }

  return false;
}
