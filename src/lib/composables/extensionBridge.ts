import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { get } from 'svelte/store';
import { goto } from '$app/navigation';
import type { Window as TauriWindow } from '@tauri-apps/api/window';
import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener';

import { logs } from '$lib/stores/logs';
import { queue } from '$lib/stores/queue';
import { getSettings, updateSettings, getProxyConfig } from '$lib/stores/settings';
import { mediaCache } from '$lib/stores/mediaCache';
import { toast } from '$lib/components/ui/Toast.svelte';
import { translate } from '$lib/i18n';
import { cleanUrl } from '$lib/utils/urlUtils';
import { openFile } from '$lib/utils/platform';
import { isMobile } from '$lib/utils/android';

interface ExtensionBridgeOptions {
  getAppWindow: () => TauriWindow | null;
  onWindowShown: () => Promise<void>;
}

const extensionDownloads = new Map<
  string,
  { id: string; lastState: string; lastProgress: number }
>();

export async function setupExtensionBridge(opts: ExtensionBridgeOptions): Promise<() => void> {
  const unlisteners: UnlistenFn[] = [];

  unlisteners.push(
    await listen<{
      url: string;
      title?: string | null;
      thumbnail?: string | null;
      id: string;
      openApp?: boolean;
      deviceId?: string;
      fromRelay?: boolean;
      options?: {
        videoQuality?: string;
        downloadMode?: string;
        audioQuality?: string;
        remux?: boolean;
        convertToMp4?: boolean;
        embedThumbnail?: boolean;
        clearMetadata?: boolean;
      };
    }>('extension-download', async (event) => {
      const {
        url: rawUrl,
        title,
        thumbnail,
        id,
        openApp = true,
        deviceId,
        fromRelay,
        options: extOptions,
      } = event.payload;
      logs.info(
        'extension',
        `Download received: ${rawUrl} (id: ${id}, openApp: ${openApp}, relay: ${fromRelay})`
      );

      const url = cleanUrl(rawUrl);
      if (!url) {
        logs.warn('extension', 'Invalid URL from extension');
        return;
      }

      if (title || thumbnail) {
        mediaCache.setPreview(url, {
          title: title || undefined,
          thumbnail: thumbnail || undefined,
        });
      }

      if (openApp) {
        const appWindow = opts.getAppWindow();
        if (appWindow) {
          try {
            await appWindow.show();
            await appWindow.unminimize();
            await appWindow.requestUserAttention(1);
            await appWindow.setFocus();
            await opts.onWindowShown();
          } catch (e) {
            logs.warn('extension', `Failed to focus window: ${e}`);
          }
        }

        goto(`/?url=${encodeURIComponent(url)}`);
        toast.info(
          `🔗 ${translate('extension.received') || 'URL received from browser extension'}`
        );
      } else {
        const currentSettings = getSettings();
        extensionDownloads.set(url, { id, lastState: 'queued', lastProgress: 0 });

        const videoQuality =
          extOptions?.videoQuality ?? currentSettings.defaultVideoQuality ?? 'max';
        const isYtmUrl = /music\.youtube\.com/i.test(url);
        const shouldForceAudio =
          isYtmUrl &&
          currentSettings.youtubeMusicAudioOnly &&
          (!extOptions?.downloadMode || extOptions?.downloadMode === 'auto');
        const downloadMode = shouldForceAudio
          ? 'audio'
          : ((extOptions?.downloadMode as 'auto' | 'audio' | 'mute' | undefined) ?? undefined);
        const audioQuality =
          extOptions?.audioQuality ?? currentSettings.defaultAudioQuality ?? 'best';
        const convertToMp4 = extOptions?.convertToMp4 ?? currentSettings.convertToMp4 ?? false;
        const remux = extOptions?.remux ?? currentSettings.remux ?? true;
        const clearMetadata = extOptions?.clearMetadata ?? currentSettings.clearMetadata ?? false;
        const embedThumbnail = extOptions?.embedThumbnail ?? currentSettings.embedThumbnail ?? true;

        const queueId = queue.add(url, {
          ignoreMixes: currentSettings.ignoreMixes ?? true,
          videoQuality,
          downloadMode,
          audioQuality,
          convertToMp4,
          remux,
          clearMetadata,
          embedThumbnail,
          dontShowInHistory: currentSettings.dontShowInHistory ?? false,
          useAria2: currentSettings.useAria2 ?? true,
          cookiesFromBrowser: currentSettings.cookiesFromBrowser ?? '',
          customCookies: currentSettings.customCookies ?? '',
          prefetchedInfo:
            title || thumbnail
              ? { title: title || undefined, thumbnail: thumbnail || undefined }
              : undefined,
        });

        if (queueId) {
          toast.info(`${translate('extension.quickDownload') || 'Quick download started'}`);
          logs.info('extension', `Quick download queued: ${queueId}`);
        } else {
          toast.info(translate('queue.alreadyInQueue') || 'Already in queue');
          extensionDownloads.delete(url);
        }
      }
    })
  );

  unlisteners.push(
    await listen<{
      url: string;
      id: string;
      deviceId?: string;
      fromRelay?: boolean;
    }>('extension-cancel', async (event) => {
      const { url, id, fromRelay } = event.payload;
      logs.info('extension', `Cancel received: ${url} (id: ${id}, relay: ${fromRelay})`);

      const state = get(queue);
      const item = state.items.find((i) => i.url === url);
      if (item) {
        queue.cancel(item.id);
        extensionDownloads.delete(url);
      }
    })
  );

  unlisteners.push(
    await listen<string>('server-open', async (event) => {
      const filePath = event.payload;
      logs.info('extension', `Server open request: ${filePath}`);
      try {
        if (isMobile()) {
          await openFile(filePath);
        } else {
          await openPath(filePath);
        }
      } catch (e) {
        logs.error('extension', `Failed to open file: ${e}`);
        toast.error(translate('downloads.openError') || 'Failed to open file');
      }
    })
  );

  unlisteners.push(
    await listen<string>('server-reveal', async (event) => {
      const filePath = event.payload;
      logs.info('extension', `Server reveal request: ${filePath}`);
      try {
        await revealItemInDir(filePath);
      } catch (e) {
        logs.error('extension', `Failed to reveal file: ${e}`);
        toast.error(translate('downloads.revealError') || 'Failed to show in folder');
      }
    })
  );

  unlisteners.push(
    await listen<{
      domain: string;
      sourceUrl?: string | null;
      count: number;
      cookies: string;
    }>('extension-cookies', async (event) => {
      const { domain, sourceUrl, count, cookies } = event.payload;
      logs.info('extension', `Cookies received: ${count} cookies from ${domain}`);

      const currentCookies = getSettings().customCookies || '';
      let newCookies = cookies;

      if (currentCookies && currentCookies.includes('# Netscape HTTP Cookie File')) {
        const existingMap = new Map<string, string>();
        for (const line of currentCookies.split('\n')) {
          if (line.startsWith('#') || !line.trim()) continue;
          const parts = line.split('\t');
          if (parts.length >= 7) {
            existingMap.set(`${parts[0]}|${parts[5]}`, line);
          }
        }
        for (const line of cookies.split('\n')) {
          if (line.startsWith('#') || !line.trim()) continue;
          const parts = line.split('\t');
          if (parts.length >= 7) {
            existingMap.set(`${parts[0]}|${parts[5]}`, line);
          }
        }
        newCookies = '# Netscape HTTP Cookie File\n' + Array.from(existingMap.values()).join('\n');
      }

      const currentReceipt = getSettings().extensionCookiesReceived || [];
      const nextEntry = {
        domain,
        sourceUrl: sourceUrl ?? null,
        count,
        receivedAt: Date.now(),
      };
      const merged = [nextEntry, ...currentReceipt.filter((e) => e.domain !== domain)].slice(0, 12);

      await updateSettings({
        customCookies: newCookies,
        cookiesFromBrowser: '',
        extensionCookiesReceived: merged,
      });

      toast.success(
        translate('extension.cookiesReceived') || `${count} cookies received from extension`
      );
    })
  );

  const progressUnsub = queue.subscribe((state) => {
    for (const [url, tracking] of extensionDownloads) {
      const item = state.items.find((i) => i.url === url);
      if (!item) continue;

      let extState: string;
      switch (item.status) {
        case 'pending':
        case 'paused':
          extState = 'queued';
          break;
        case 'downloading':
        case 'processing':
          extState = 'downloading';
          break;
        case 'completed':
          extState = 'completed';
          break;
        case 'failed':
          extState = 'error';
          break;
        default:
          extState = 'queued';
      }

      const progressChanged = Math.abs(item.progress - tracking.lastProgress) >= 1;
      const stateChanged = extState !== tracking.lastState;

      if (stateChanged || progressChanged) {
        tracking.lastState = extState;
        tracking.lastProgress = item.progress;

        invoke('extension_update_status', {
          id: tracking.id,
          state: extState,
          progress: Math.round(item.progress) || null,
          speed: item.speed || null,
          eta: item.eta || null,
          error: item.error || null,
          filePath: extState === 'completed' ? item.filePath || null : null,
          duration: item.duration || null,
        }).catch((err) => {
          logs.debug('extension', `Failed to update status: ${err}`);
        });

        if (extState === 'completed' || extState === 'error') {
          extensionDownloads.delete(url);
        }
      }
    }
  });

  return () => {
    for (const unlisten of unlisteners) unlisten();
    progressUnsub();
  };
}
