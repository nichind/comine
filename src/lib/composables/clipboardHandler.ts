import { getCurrentWindow } from '@tauri-apps/api/window';
import { readText } from '@tauri-apps/plugin-clipboard-manager';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { goto } from '$app/navigation';
import { toast, dismissToast } from '$lib/components/ui/Toast.svelte';
import { translate } from '$lib/i18n';
import { getSettings } from '$lib/stores/settings';
import { isValidMediaUrl } from '$lib/utils/urlUtils';

const cleanupFns: (() => void)[] = [];
let resolvingToastId: number | null = null;

export async function downloadFromClipboard(): Promise<void> {
  const settings = getSettings();
  try {
    const text = await readText();
    if (text && isValidMediaUrl(text, settings.clipboardPatterns || [])) {
      goto(`/?url=${encodeURIComponent(text)}`);
      const appWindow = getCurrentWindow();
      await appWindow.show();
      await appWindow.setFocus();
    } else {
      toast.warning(translate('clipboard.noValidUrl'));
    }
  } catch {
    toast.error(translate('clipboard.error'));
  }
}

export async function setupClipboardWatcher(): Promise<void> {
  const settings = getSettings();
  if (settings.watchClipboard) {
    await invoke('start_clipboard_watcher');
  }

  const ulPaste = await listen<string>('clipboard-url-paste', (event) => {
    const url = event.payload;
    if (!url) return;
    goto(`/?url=${encodeURIComponent(url)}`);
  });
  cleanupFns.push(ulPaste);

  const ulResolving = await listen<string>('clipboard-url-resolving', (event) => {
    const url = event.payload;
    if (!url) return;
    const display = url.length > 50 ? url.slice(0, 50) + '...' : url;
    if (resolvingToastId !== null) {
      dismissToast(resolvingToastId);
    }
    resolvingToastId = toast.loading(
      translate('clipboard.fetchingInfo') || 'Fetching media info...',
      display
    );
  });
  cleanupFns.push(ulResolving);

  const ulResolved = await listen<string>('clipboard-url-resolved', () => {
    if (resolvingToastId !== null) {
      dismissToast(resolvingToastId);
      resolvingToastId = null;
    }
  });
  cleanupFns.push(ulResolved);

  const ulFile = await listen<{ url: string; filename: string | null }>(
    'clipboard-url-file',
    (event) => {
      const { filename } = event.payload;
      toast.info(
        `${translate('clipboard.fileDetected') || 'File URL detected'}: ${filename || 'file'}`
      );
    }
  );
  cleanupFns.push(ulFile);
}

export function cleanupClipboardListeners(): void {
  for (const fn of cleanupFns) fn();
  cleanupFns.length = 0;
  if (resolvingToastId !== null) {
    dismissToast(resolvingToastId);
    resolvingToastId = null;
  }
}
