import { writable, get } from 'svelte/store';
import { browser } from '$app/environment';
import { settings } from './settings';

const STORAGE_KEY = 'comine_stats';
const INSTALLATION_ID_KEY = 'comine_installation_id';

export interface AppStats {
  totalDownloads: number;
  totalSizeMb: number;
  successfulDownloads: number;
  failedDownloads: number;
  firstLaunch: string;
  lastSync: string | null;
}

export interface HistoryBackfillInput {
  totalSuccessfulDownloads: number;
  totalSizeBytes: number;
}

interface StatsState {
  stats: AppStats;
}

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

  const defaultState: StatsState = {
    stats: defaultStats,
  };

  // Load from localStorage
  let initial = defaultState;
  if (browser) {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        initial = {
          ...defaultState,
          ...parsed,
          stats: { ...defaultStats, ...parsed.stats },
        };
      }
    } catch {
      // Ignore parse errors
    }
  }

  const { subscribe, set, update } = writable<StatsState>(initial);

  if (browser) {
    subscribe((state) => {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    });
  }

  return {
    subscribe,
    set,
    update,

    trackDownload(sizeMb: number, success: boolean) {
      update((state) => ({
        ...state,
        stats: {
          ...state.stats,
          totalDownloads: state.stats.totalDownloads + 1,
          totalSizeMb: state.stats.totalSizeMb + sizeMb,
          successfulDownloads: state.stats.successfulDownloads + (success ? 1 : 0),
          failedDownloads: state.stats.failedDownloads + (success ? 0 : 1),
        },
      }));
    },

    // Backfill stats from legacy history.
    mergeFromHistory(input: HistoryBackfillInput) {
      const historyDownloads = Math.max(0, Math.floor(input.totalSuccessfulDownloads || 0));
      const historySizeMb = Math.max(0, (input.totalSizeBytes || 0) / (1024 * 1024));

      update((state) => {
        const nextSuccessful = Math.max(state.stats.successfulDownloads, historyDownloads);
        const nextTotal = Math.max(state.stats.totalDownloads, historyDownloads);
        const nextTotalSizeMb = Math.max(state.stats.totalSizeMb, historySizeMb);

        return {
          ...state,
          stats: {
            ...state.stats,
            successfulDownloads: nextSuccessful,
            totalDownloads: nextTotal,
            totalSizeMb: nextTotalSizeMb,
          },
        };
      });
    },

    markSynced() {
      update((state) => ({
        ...state,
        stats: {
          ...state.stats,
          lastSync: new Date().toISOString(),
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

    getInstallationId,
  };
}

function getPlatform(): string {
  if (!browser) return 'unknown';
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('android')) return 'android';
  if (ua.includes('win')) return 'windows';
  if (ua.includes('linux')) return 'linux';
  if (ua.includes('mac')) return 'macos';
  return 'unknown';
}

function getAppVersion(): string {
  if (!browser) return '0.0.0';
  // @ts-ignore - injected by vite
  return typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '0.0.0';
}

export const appStats = createStatsStore();
