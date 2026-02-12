import { writable, get } from 'svelte/store';
import { browser } from '$app/environment';
import { logs } from '$lib/stores/logs';
import { history } from '$lib/stores/history';
import {
  settings,
  applyRemoteDefaultsMidSession,
  defaultSettings,
  type AppSettings,
} from '$lib/stores/settings';
import { translate } from '$lib/i18n';
import type { AppStats as BindingAppStats } from '$lib/bindings';

async function fetchWithTimeout(url: string, opts?: RequestInit, ms = 8000): Promise<Response> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), ms);
  try {
    return await fetch(url, { ...opts, signal: controller.signal });
  } finally {
    clearTimeout(timeout);
  }
}

export function getPlatform(): string {
  if (!browser) return 'unknown';
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('android')) return 'android';
  if (ua.includes('win')) return 'windows';
  if (ua.includes('linux')) return 'linux';
  if (ua.includes('mac')) return 'macos';
  return 'unknown';
}

export function getAppVersion(): string {
  if (!browser) return '0.0.0';
  // @ts-ignore — injected at build time
  return typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '0.0.0';
}

type AppStats = BindingAppStats & { lastSync: string | null };

const STATS_KEY = 'comine_stats';
const INSTALLATION_ID_KEY = 'comine_installation_id';

function getInstallationId(): string {
  if (!browser) return '';
  let id = localStorage.getItem(INSTALLATION_ID_KEY);
  if (!id) {
    id = crypto.randomUUID();
    localStorage.setItem(INSTALLATION_ID_KEY, id);
  }
  return id;
}

function createStatsStore() {
  const defaultStats: AppStats = {
    totalDownloads: 0,
    totalSizeMb: 0,
    successfulDownloads: 0,
    failedDownloads: 0,
    firstLaunch: new Date().toISOString(),
    lastSync: null,
  };

  let initial = { stats: defaultStats };
  if (browser) {
    try {
      const stored = localStorage.getItem(STATS_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        initial = { stats: { ...defaultStats, ...parsed.stats } };
      }
    } catch {}
  }

  const { subscribe, update } = writable(initial);

  if (browser) {
    subscribe((state) => localStorage.setItem(STATS_KEY, JSON.stringify(state)));
  }

  return {
    subscribe,
    mergeFromHistory(input: { totalSuccessfulDownloads: number; totalSizeBytes: number }) {
      const historyDownloads = Math.max(0, Math.floor(input.totalSuccessfulDownloads || 0));
      const historySizeMb = Math.max(0, (input.totalSizeBytes || 0) / (1024 * 1024));
      update((state) => ({
        stats: {
          ...state.stats,
          successfulDownloads: Math.max(state.stats.successfulDownloads, historyDownloads),
          totalDownloads: Math.max(state.stats.totalDownloads, historyDownloads),
          totalSizeMb: Math.max(state.stats.totalSizeMb, historySizeMb),
        },
      }));
    },
    getPayload() {
      const state = get({ subscribe });
      const settingsState = get(settings);
      return {
        id: getInstallationId(),
        platform: getPlatform(),
        version: getAppVersion(),
        locale: settingsState.language || 'en',
        stats: {
          total_downloads: state.stats.totalDownloads,
          successful_downloads: state.stats.successfulDownloads,
          total_size_mb: Math.round(state.stats.totalSizeMb),
          first_launch: state.stats.firstLaunch,
        },
      };
    },
  };
}

const appStats = createStatsStore();

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

export interface Broadcast {
  id: number;
  message: string;
  type: 'info' | 'warning' | 'error' | 'success';
  title?: string;
  icon?: string;
  url?: string;
  button_text?: string;
  platforms?: string;
  min_version?: string;
  max_version?: string;
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
let broadcastFetching = false;

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

async function fetchBroadcasts(showNotification: ShowNotificationFn): Promise<void> {
  if (!browser || document.visibilityState !== 'visible' || broadcastFetching) return;
  broadcastFetching = true;

  const dismissedKey = 'comine_dismissed_broadcasts';
  const dismissedRaw = localStorage.getItem(dismissedKey);
  const dismissed: number[] = dismissedRaw ? JSON.parse(dismissedRaw) : [];

  try {
    const res = await fetchWithTimeout('https://stats.comine.app/broadcast');
    if (!res.ok) return;

    const broadcasts: Broadcast[] = await res.json();
    if (!Array.isArray(broadcasts) || broadcasts.length === 0) return;

    const platform = getPlatform();
    const version = getAppVersion();

    for (const bc of broadcasts) {
      if (dismissed.includes(bc.id) || activeBroadcastNotifIds.has(bc.id)) continue;

      if (bc.platforms) {
        const platforms = bc.platforms.split(',').map((p) => p.trim().toLowerCase());
        if (!platforms.includes('all') && !platforms.includes(platform)) continue;
      }

      if (bc.min_version && compareVersions(version, bc.min_version) < 0) continue;
      if (bc.max_version && compareVersions(version, bc.max_version) > 0) continue;

      const notifId = showNotification({
        title: bc.title || getBroadcastTitle(bc.type),
        body: bc.message,
        thumbnail: bc.icon,
        duration: 0,
        url: bc.url,
        actionLabel: bc.button_text || (bc.url ? translate('broadcast.learnMore') : undefined),
        onAction: bc.url ? () => window.open(bc.url, '_blank') : undefined,
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
  } finally {
    broadcastFetching = false;
  }
}

async function maybeBackfillStatsFromHistory(): Promise<void> {
  if (!browser) return;

  const version = getAppVersion();
  const migrationKey = 'comine_stats_history_backfill_v1';
  if (localStorage.getItem(migrationKey) === version) return;

  try {
    const items = await history.getItems();
    appStats.mergeFromHistory({
      totalSuccessfulDownloads: items.length,
      totalSizeBytes: items.reduce((sum, item) => sum + (item.size || 0), 0),
    });
    localStorage.setItem(migrationKey, version);
    logs.info('stats', `Backfilled stats from history (v${version}): ${items.length} downloads`);
  } catch (e) {
    logs.debug('stats', `History backfill skipped/failed: ${e}`);
  }
}

async function maybePostStats(isSendStatsEnabled: () => boolean): Promise<void> {
  if (!browser || !isSendStatsEnabled()) return;

  const lastSyncKey = 'comine_last_stats_sync';
  const lastSyncTime = localStorage.getItem(lastSyncKey);
  const now = Date.now();
  if (lastSyncTime && now - parseInt(lastSyncTime) < 3_600_000) return;

  const payload = appStats.getPayload();
  try {
    const res = await fetchWithTimeout('https://stats.comine.app/', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    logs.info('stats', `Stats sent! Response: ${res.status}`);
  } catch (e) {
    logs.warn('stats', `Failed to send stats: ${e}`);
  }
  localStorage.setItem(lastSyncKey, now.toString());
}

export interface RemoteSyncOptions {
  showNotification: ShowNotificationFn;
  isSendStatsEnabled: () => boolean;
}

const SYNC_INTERVAL_MS = 30 * 60 * 1000;

async function runSyncTick(opts: RemoteSyncOptions): Promise<void> {
  if (!browser || document.visibilityState !== 'visible') return;

  const results = await Promise.allSettled([
    fetchBroadcasts(opts.showNotification),
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

  await maybeBackfillStatsFromHistory();
  maybePostStats(opts.isSendStatsEnabled);

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
