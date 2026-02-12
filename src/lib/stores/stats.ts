import { writable, get } from 'svelte/store';
import { browser } from '$app/environment';
import { settings } from './settings';
import { getPlatform, getAppVersion } from '$lib/composables/remoteSync';
import type { AppStats as BindingAppStats } from '$lib/bindings';

const STORAGE_KEY = 'comine_stats';
const INSTALLATION_ID_KEY = 'comine_installation_id';

export type AppStats = BindingAppStats & {
  lastSync: string | null;
};

interface HistoryBackfillInput {
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
    } catch {}
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

export const appStats = createStatsStore();
