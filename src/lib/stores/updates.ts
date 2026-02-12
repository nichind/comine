import { writable, get } from 'svelte/store';
import { settings, updateSetting } from './settings';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  show as showNotification,
  dismiss as dismissNotification,
} from '$lib/components/layout/NotificationPopup.svelte';
import { toast } from '$lib/components/ui/Toast.svelte';
import { t } from '$lib/i18n';
import { isAndroid } from '$lib/utils/android';

declare const __GIT_BRANCH__: string;
declare const __APP_VERSION__: string;

const GIT_BRANCH = typeof __GIT_BRANCH__ !== 'undefined' ? __GIT_BRANCH__ : 'unknown';
const APP_VERSION = typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '0.0.0';

interface UpdateCheckResult {
  available: boolean;
  version: string | null;
  body: string | null;
  date: string | null;
  download_url: string | null;
  is_prerelease: boolean;
}

interface UpdateInfo {
  version: string;
  notes: string;
  pubDate: string;
  downloadUrl: string;
  isPreRelease: boolean;
}

interface UpdateState {
  available: boolean;
  checking: boolean;
  downloading: boolean;
  installTriggered: boolean;
  progress: number;
  info: UpdateInfo | null;
  lastCheck: number;
  error: string | null;
}

const defaultState: UpdateState = {
  available: false,
  checking: false,
  downloading: false,
  installTriggered: false,
  progress: 0,
  info: null,
  lastCheck: 0,
  error: null,
};

export const updateState = writable<UpdateState>(defaultState);

let checkInterval: ReturnType<typeof setInterval> | null = null;
let activeUpdateNotificationId: string | null = null;

function shouldAllowPreReleases(): boolean {
  const s = get(settings);
  if (s.allowPreReleases) return true;
  return GIT_BRANCH !== 'main' && GIT_BRANCH !== 'master' && GIT_BRANCH !== 'unknown';
}

export async function checkForUpdates(manual = false): Promise<UpdateInfo | null> {
  const s = get(settings);
  if (!manual && !s.autoUpdate) return null;

  updateState.update((state) => ({
    ...state,
    checking: true,
    error: null,
    installTriggered: false,
  }));

  try {
    const allowPre = shouldAllowPreReleases();
    const result = await invoke<UpdateCheckResult>('check_for_update', {
      allowPrerelease: allowPre,
    });

    updateState.update((state) => ({
      ...state,
      checking: false,
      lastCheck: Date.now(),
    }));

    if (!result.available || !result.version) {
      updateState.update((state) => ({ ...state, available: false, info: null }));
      return null;
    }

    const info: UpdateInfo = {
      version: result.version,
      notes: result.body || '',
      pubDate: result.date || '',
      downloadUrl: result.download_url || '',
      isPreRelease: result.is_prerelease,
    };

    updateState.update((state) => ({ ...state, available: true, info }));

    if (!manual) {
      showUpdateNotificationIfNeeded(info);
    }

    return info;
  } catch (e) {
    const error = e instanceof Error ? e.message : String(e);
    console.error('[Updates] Check failed:', error);
    updateState.update((state) => ({ ...state, checking: false, error }));
    return null;
  }
}

function showUpdateNotificationIfNeeded(info: UpdateInfo): void {
  const s = get(settings);

  if (s.dismissedUpdateVersion === info.version) {
    return;
  }

  if (activeUpdateNotificationId) {
    dismissNotification(activeUpdateNotificationId);
  }

  const translate = get(t);
  const bodyKey = info.isPreRelease
    ? 'updates.notificationBodyPreRelease'
    : 'updates.notificationBody';

  activeUpdateNotificationId = showNotification({
    title: translate('updates.notificationTitle'),
    body: translate(bodyKey, { version: info.version }),
    duration: 0,
    actionLabel: translate('updates.installNow'),
    onAction: () => {
      activeUpdateNotificationId = null;
      downloadAndInstall();
    },
  });
}

export async function downloadAndInstall(): Promise<void> {
  const state = get(updateState);
  if (!state.available || !state.info) return;

  updateState.update((s) => ({ ...s, downloading: true, progress: 0, error: null }));

  const allowPre = shouldAllowPreReleases();
  const info = state.info;
  let unlisten: UnlistenFn | null = null;

  try {
    const apkUrl = isAndroid() ? info.downloadUrl : undefined;

    if (isAndroid() && !apkUrl) {
      throw new Error('No download URL available');
    }

    let contentLength = 0;
    let downloaded = 0;

    unlisten = await listen<{ event: string; contentLength?: number; chunkLength?: number }>(
      'update-download-progress',
      (event) => {
        const data = event.payload;
        if (data.event === 'started' && data.contentLength) {
          contentLength = data.contentLength;
          downloaded = 0;
          updateState.update((s) => ({ ...s, progress: 0 }));
        } else if (data.event === 'progress' && data.chunkLength) {
          downloaded += data.chunkLength;
          const progress = contentLength > 0 ? Math.round((downloaded / contentLength) * 100) : 0;
          updateState.update((s) => ({ ...s, progress }));
        } else if (data.event === 'finished') {
          updateState.update((s) => ({ ...s, progress: 100 }));
        }
      }
    );

    await invoke('download_and_install_update', {
      allowPrerelease: allowPre,
      apkUrl,
    });

    updateState.update((s) => ({
      ...s,
      downloading: false,
      progress: 100,
      installTriggered: isAndroid(),
    }));
  } catch (e) {
    const error = e instanceof Error ? e.message : String(e);
    console.error('[Updates] Install failed:', error);
    updateState.update((s) => ({ ...s, downloading: false, error }));
    toast.error(`Update failed: ${error}`);
  } finally {
    if (unlisten) {
      unlisten();
    }
  }
}

let initialCheckTimeout: ReturnType<typeof setTimeout> | null = null;

export function startUpdateChecker(): void {
  if (checkInterval) return;

  const check = () => {
    const s = get(settings);
    if (s.autoUpdate) {
      checkForUpdates();
    }
  };

  initialCheckTimeout = setTimeout(check, 5000);
  checkInterval = setInterval(check, 60 * 60 * 1000);
}

export function stopUpdateChecker(): void {
  if (initialCheckTimeout) {
    clearTimeout(initialCheckTimeout);
    initialCheckTimeout = null;
  }
  if (checkInterval) {
    clearInterval(checkInterval);
    checkInterval = null;
  }
}

export function getCurrentVersion(): string {
  return APP_VERSION;
}

export async function clearDismissedVersionIfUpdated(): Promise<void> {
  const s = get(settings);
  if (s.dismissedUpdateVersion && s.dismissedUpdateVersion !== APP_VERSION) {
    await updateSetting('dismissedUpdateVersion', '');
  }
}
