import { writable, get } from 'svelte/store';
import { browser } from '$app/environment';
import { invoke } from '@tauri-apps/api/core';
import { logs } from '$lib/stores/logs';
import {
  applyRemoteDefaultsMidSession,
  defaultSettings,
  type AppSettings,
} from '$lib/stores/settings';
import { translate } from '$lib/i18n';
import type { Broadcast } from '$lib/bindings';

async function fetchWithTimeout(url: string, opts?: RequestInit, ms = 8000): Promise<Response> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), ms);
  try {
    return await fetch(url, { ...opts, signal: controller.signal });
  } finally {
    clearTimeout(timeout);
  }
}

export function compareVersions(a: string, b: string): number {
  const partsA = a.split('.').map((n) => parseInt(n) || 0);
  const partsB = b.split('.').map((n) => parseInt(n) || 0);
  for (let i = 0; i < Math.max(partsA.length, partsB.length); i++) {
    const numA = partsA[i] || 0;
    const numB = partsB[i] || 0;
    if (numA > numB) return 1;
    if (numA < numB) return -1;
  }
  return 0;
}

const REMOTE_DEFAULTS_URL =
  'https://raw.githubusercontent.com/nichind/comine/main/remote-defaults.json';
const REMOTE_DEFAULTS_CACHE_KEY = 'comine_remote_defaults';

interface RemoteDefaultsPayload {
  version: number;
  defaults: Record<string, unknown>;
}

function loadCachedDefaults(): Record<string, unknown> {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(REMOTE_DEFAULTS_CACHE_KEY);
    if (raw) return JSON.parse(raw) as Record<string, unknown>;
  } catch {}
  return {};
}

export const remoteDefaults = writable<Record<string, unknown>>(loadCachedDefaults());

let remoteDefaultsFetching = false;

async function fetchRemoteDefaults(): Promise<void> {
  if (!browser || remoteDefaultsFetching) return;
  remoteDefaultsFetching = true;

  try {
    const res = await fetchWithTimeout(REMOTE_DEFAULTS_URL, {
      cache: 'no-cache',
      headers: { Accept: 'application/json' },
    });

    if (!res.ok) return;

    const payload: RemoteDefaultsPayload = await res.json();
    if (!payload?.defaults || typeof payload.defaults !== 'object') return;

    remoteDefaults.set(payload.defaults);
    try {
      localStorage.setItem(REMOTE_DEFAULTS_CACHE_KEY, JSON.stringify(payload.defaults));
    } catch {}

    logs.debug(
      'remote-defaults',
      `Loaded ${Object.keys(payload.defaults).length} remote defaults (v${payload.version})`
    );
  } catch (e) {
    logs.debug('remote-defaults', `Fetch error: ${e}`);
  } finally {
    remoteDefaultsFetching = false;
  }
}

export function getEffectiveDefaultFrom(
  remoteSnapshot: Record<string, unknown>,
  key: string
): unknown {
  if (key in remoteSnapshot) return remoteSnapshot[key];
  if (key.includes('.')) {
    return key.split('.').reduce<Record<string, unknown> | undefined>(
      (obj, k) => {
        if (obj && typeof obj === 'object')
          return (obj as Record<string, unknown>)[k] as Record<string, unknown> | undefined;
        return undefined;
      },
      defaultSettings as unknown as Record<string, unknown>
    );
  }
  return defaultSettings[key as keyof AppSettings];
}

export function getEffectiveDefault(key: string): unknown {
  return getEffectiveDefaultFrom(get(remoteDefaults), key);
}

export function applyRemoteDefaultsToLoaded(
  loaded: Record<string, unknown>,
  userModifiedKeys: ReadonlySet<string>
): Set<string> {
  const remote = get(remoteDefaults);
  const applied = new Set<string>();

  for (const [key, value] of Object.entries(remote)) {
    if (userModifiedKeys.has(key)) continue;

    if (key.includes('.')) {
      const [parent, child] = key.split('.');
      const parentObj = loaded[parent];
      if (typeof parentObj === 'object' && parentObj !== null) {
        (parentObj as Record<string, unknown>)[child] = value;
        applied.add(key);
      }
    } else {
      loaded[key] = value;
      applied.add(key);
    }
  }

  return applied;
}

interface ShowNotificationFn {
  (opts: {
    title: string;
    body: string;
    thumbnail?: string;
    duration: number;
    url?: string;
    actionLabel?: string;
    onAction?: () => void;
    onDismiss?: () => void;
  }): string;
}

const activeBroadcastNotifIds = new Map<number, string>();

function getBroadcastTitle(type: string): string {
  switch (type) {
    case 'warning':
      return translate('broadcast.warning') || 'Warning';
    case 'error':
      return translate('broadcast.important') || 'Important';
    case 'success':
      return translate('broadcast.success') || 'Good news';
    default:
      return translate('broadcast.announcement') || 'Announcement';
  }
}

async function handleBroadcasts(showNotification: ShowNotificationFn): Promise<void> {
  if (!browser || document.visibilityState !== 'visible') return;

  const dismissedKey = 'comine_dismissed_broadcasts';
  const dismissedRaw = localStorage.getItem(dismissedKey);
  const dismissed: number[] = dismissedRaw ? JSON.parse(dismissedRaw) : [];

  try {
    const broadcasts = await invoke<Broadcast[]>('fetch_broadcasts');
    if (!Array.isArray(broadcasts) || broadcasts.length === 0) return;

    for (const bc of broadcasts) {
      if (dismissed.includes(bc.id) || activeBroadcastNotifIds.has(bc.id)) continue;

      const notifId = showNotification({
        title: bc.title || getBroadcastTitle(bc.broadcastType),
        body: bc.message,
        thumbnail: bc.icon ?? undefined,
        duration: 0,
        url: bc.url ?? undefined,
        actionLabel: bc.buttonText || (bc.url ? translate('broadcast.learnMore') : undefined),
        onAction: bc.url ? () => window.open(bc.url!, '_blank') : undefined,
        onDismiss: () => {
          const cur: number[] = JSON.parse(localStorage.getItem(dismissedKey) || '[]');
          if (!cur.includes(bc.id)) {
            cur.push(bc.id);
            localStorage.setItem(dismissedKey, JSON.stringify(cur));
          }
          activeBroadcastNotifIds.delete(bc.id);
        },
      });
      activeBroadcastNotifIds.set(bc.id, notifId);
    }
  } catch (e) {
    logs.debug('broadcast', `Failed to fetch broadcasts: ${e}`);
  }
}

export interface RemoteSyncOptions {
  showNotification: ShowNotificationFn;
}

const SYNC_INTERVAL_MS = 30 * 60 * 1000;

async function runSyncTick(opts: RemoteSyncOptions): Promise<void> {
  if (!browser || document.visibilityState !== 'visible') return;

  const results = await Promise.allSettled([
    handleBroadcasts(opts.showNotification),
    fetchRemoteDefaults(),
  ]);

  if (results[1].status === 'fulfilled') {
    try {
      await applyRemoteDefaultsMidSession();
    } catch (e) {
      logs.debug('remote-sync', `Failed to apply remote defaults mid-session: ${e}`);
    }
  }
}

export async function initRemoteSync(opts: RemoteSyncOptions): Promise<() => void> {
  if (!browser) return () => {};

  runSyncTick(opts);

  const timer = setInterval(() => runSyncTick(opts), SYNC_INTERVAL_MS);

  const onVisibility = () => {
    if (document.visibilityState === 'visible') runSyncTick(opts);
  };
  document.addEventListener('visibilitychange', onVisibility);

  return () => {
    clearInterval(timer);
    document.removeEventListener('visibilitychange', onVisibility);
  };
}
