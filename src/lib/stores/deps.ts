import { writable, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logs } from './logs';
import { getProxyConfig } from './settings';
import { toast, updateToast, dismissToast } from '$lib/components/Toast.svelte';

export interface DependencyStatus {
  installed: boolean;
  version: string | null;
  path: string | null;
  updateAvailable: string | null;
  diskSize?: number | null;
}

export interface InstallProgress {
  stage: string;
  progress: number;
  downloaded: number;
  total: number;
  speed: number;
  message: string;
}

export interface ReleaseInfo {
  tag: string;
  name: string;
  publishedAt: string;
}

export type DependencyName = 'ytdlp' | 'ffmpeg' | 'aria2' | 'deno' | 'quickjs' | 'lux';

export interface DepsState {
  ytdlp: DependencyStatus | null;
  ffmpeg: DependencyStatus | null;
  aria2: DependencyStatus | null;
  deno: DependencyStatus | null;
  quickjs: DependencyStatus | null;
  lux: DependencyStatus | null;
  checking: DependencyName | null;
  installingDeps: Set<DependencyName>;
  installProgressMap: Map<DependencyName, InstallProgress>;
  installing: DependencyName | null;
  installProgress: InstallProgress | null;
  error: string | null;
  releases: ReleaseInfo[];
  hasCheckedAll: boolean;
}

const initialState: DepsState = {
  ytdlp: null,
  ffmpeg: null,
  aria2: null,
  deno: null,
  quickjs: null,
  lux: null,
  checking: null,
  installingDeps: new Set(),
  installProgressMap: new Map(),
  installing: null,
  installProgress: null,
  error: null,
  releases: [],
  hasCheckedAll: false,
};

const DEP_CONFIG: Record<
  DependencyName,
  { checkCommand: string; installCommand: string; uninstallCommand: string; progressEvent: string }
> = {
  ytdlp: {
    checkCommand: 'check_ytdlp',
    installCommand: 'install_ytdlp',
    uninstallCommand: 'uninstall_ytdlp',
    progressEvent: 'ytdlp-install-progress',
  },
  ffmpeg: {
    checkCommand: 'check_ffmpeg',
    installCommand: 'install_ffmpeg',
    uninstallCommand: 'uninstall_ffmpeg',
    progressEvent: 'ffmpeg-install-progress',
  },
  aria2: {
    checkCommand: 'check_aria2',
    installCommand: 'install_aria2',
    uninstallCommand: 'uninstall_aria2',
    progressEvent: 'aria2-install-progress',
  },
  deno: {
    checkCommand: 'check_deno',
    installCommand: 'install_deno',
    uninstallCommand: 'uninstall_deno',
    progressEvent: 'deno-install-progress',
  },
  quickjs: {
    checkCommand: 'check_quickjs',
    installCommand: 'install_quickjs',
    uninstallCommand: 'uninstall_quickjs',
    progressEvent: 'quickjs-install-progress',
  },
  lux: {
    checkCommand: 'check_lux',
    installCommand: 'install_lux',
    uninstallCommand: 'uninstall_lux',
    progressEvent: 'lux-install-progress',
  },
};

const installToastIds = new Map<DependencyName, number>();
const lastToastUpdateAt = new Map<DependencyName, number>();

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, exp);
  const decimals = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(decimals)} ${units[exp]}`;
}

function formatSpeed(bytesPerSec: number): string {
  if (!Number.isFinite(bytesPerSec) || bytesPerSec <= 0) return '0 B/s';
  return `${formatBytes(bytesPerSec)}/s`;
}

function createDepsStore() {
  const { subscribe, set, update } = writable<DepsState>(initialState);

  async function cancelInstall(dep: DependencyName): Promise<boolean> {
    try {
      logs.info('deps', `Cancelling ${dep} install...`);
      await invoke('cancel_dep_install', { dep });
      return true;
    } catch (err) {
      logs.error('deps', `Failed to cancel ${dep}: ${err}`);
      return false;
    }
  }

  async function checkDep(
    dep: DependencyName,
    checkUpdates: boolean = false
  ): Promise<DependencyStatus | null> {
    const config = DEP_CONFIG[dep];
    logs.debug('deps', `Checking ${dep} status...`);
    update((s) => ({ ...s, checking: dep, error: null }));

    try {
      const status = await invoke<DependencyStatus>(config.checkCommand, { checkUpdates });
      logs.info('deps', `${dep} check: installed=${status.installed}, version=${status.version}`);
      update((s) => ({ ...s, [dep]: status, checking: null }));
      return status;
    } catch (err) {
      logs.error('deps', `Failed to check ${dep}: ${err}`);
      update((s) => ({ ...s, checking: null, error: String(err) }));
      return null;
    }
  }

  async function installDep(dep: DependencyName, version?: string): Promise<boolean> {
    const config = DEP_CONFIG[dep];
    logs.info('deps', `Installing ${dep}${version ? ` version ${version}` : ''}...`);

    const toastId = toast.progress(`Installing ${dep}...`, 0, 'Starting…');
    installToastIds.set(dep, toastId);

    update((s) => {
      const newInstallingDeps = new Set(s.installingDeps);
      newInstallingDeps.add(dep);
      return {
        ...s,
        installingDeps: newInstallingDeps,
        installing: s.installing || dep,
        error: null,
      };
    });

    let lastLoggedProgress = -10;
    const unlisten = await listen<InstallProgress>(config.progressEvent, (event) => {
      const progress = event.payload.progress;
      if (progress >= lastLoggedProgress + 10 || event.payload.stage !== 'downloading') {
        logs.debug('deps', `${dep} install progress: ${event.payload.stage} - ${progress}%`);
        lastLoggedProgress = Math.floor(progress / 10) * 10;
      }
      update((s) => {
        const newProgressMap = new Map(s.installProgressMap);
        newProgressMap.set(dep, event.payload);
        return {
          ...s,
          installProgressMap: newProgressMap,
          installProgress: dep === 'ytdlp' ? event.payload : s.installProgress,
        };
      });

      const now = Date.now();
      const last = lastToastUpdateAt.get(dep) ?? 0;
      if (now - last < 200) return;
      lastToastUpdateAt.set(dep, now);

      const id = installToastIds.get(dep);
      if (!id) return;

      const downloaded = event.payload.downloaded ?? 0;
      const total = event.payload.total ?? 0;
      const speed = event.payload.speed ?? 0;

      const subParts: string[] = [];
      if (event.payload.stage) subParts.push(event.payload.stage);
      if (speed > 0) subParts.push(formatSpeed(speed));
      if (total > 0) subParts.push(`${formatBytes(downloaded)} / ${formatBytes(total)}`);
      else if (downloaded > 0) subParts.push(formatBytes(downloaded));

      updateToast(id, {
        message: event.payload.message || `Installing ${dep}...`,
        progress: event.payload.progress ?? 0,
        subMessage: subParts.join(' • '),
      });
    });

    try {
      const proxyConfig = getProxyConfig();
      const result = await invoke(config.installCommand, {
        ...(version ? { version } : {}),
        proxyConfig: proxyConfig,
      });
      logs.info('deps', `${dep} installed successfully: ${result}`);
      const status = await invoke<DependencyStatus>(config.checkCommand);

      const id = installToastIds.get(dep);
      if (id) {
        updateToast(id, {
          type: 'success',
          message: `Installed ${dep}`,
          progress: 100,
          subMessage: status.version ? `${status.version}` : undefined,
          actionLabel: undefined,
          action: undefined,
        });
        setTimeout(() => dismissToast(id), 2500);
        installToastIds.delete(dep);
      }

      update((s) => {
        const newInstallingDeps = new Set(s.installingDeps);
        newInstallingDeps.delete(dep);
        const newProgressMap = new Map(s.installProgressMap);
        newProgressMap.delete(dep);
        return {
          ...s,
          [dep]: status,
          installingDeps: newInstallingDeps,
          installProgressMap: newProgressMap,
          installing: newInstallingDeps.size > 0 ? Array.from(newInstallingDeps)[0] : null,
          installProgress: newInstallingDeps.size > 0 ? s.installProgress : null,
        };
      });
      return true;
    } catch (err) {
      logs.error('deps', `Failed to install ${dep}: ${err}`);

      const id = installToastIds.get(dep);
      if (id) {
        updateToast(id, {
          type: 'error',
          message: `Failed to install ${dep}`,
          subMessage: String(err),
          actionLabel: undefined,
          action: undefined,
        });
        setTimeout(() => dismissToast(id), 6000);
        installToastIds.delete(dep);
      }

      update((s) => {
        const newInstallingDeps = new Set(s.installingDeps);
        newInstallingDeps.delete(dep);
        const newProgressMap = new Map(s.installProgressMap);
        newProgressMap.delete(dep);
        return {
          ...s,
          installingDeps: newInstallingDeps,
          installProgressMap: newProgressMap,
          installing: newInstallingDeps.size > 0 ? Array.from(newInstallingDeps)[0] : null,
          error: String(err),
          installProgress: newInstallingDeps.size > 0 ? s.installProgress : null,
        };
      });
      return false;
    } finally {
      unlisten();
    }
  }

  async function uninstallDep(dep: DependencyName): Promise<boolean> {
    const config = DEP_CONFIG[dep];
    try {
      logs.info('deps', `Uninstalling ${dep}...`);
      await invoke(config.uninstallCommand);
      logs.info('deps', `${dep} uninstalled`);
      update((s) => ({
        ...s,
        [dep]: { installed: false, version: null, path: null, updateAvailable: null },
      }));
      return true;
    } catch (err) {
      logs.error('deps', `Failed to uninstall ${dep}: ${err}`);
      update((s) => ({ ...s, error: String(err) }));
      return false;
    }
  }

  return {
    subscribe,

    check: checkDep,
    install: installDep,
    uninstall: uninstallDep,

    cancelInstall,

    async checkYtdlp(checkUpdates: boolean = false) {
      return checkDep('ytdlp', checkUpdates);
    },

    async checkYtdlpUpdates() {
      logs.debug('deps', 'Checking for yt-dlp updates...');
      try {
        const status = await invoke<DependencyStatus>('check_ytdlp', { checkUpdates: true });
        if (status.updateAvailable) {
          logs.info('deps', `yt-dlp update available: ${status.updateAvailable}`);
          update((s) => ({
            ...s,
            ytdlp: s.ytdlp ? { ...s.ytdlp, updateAvailable: status.updateAvailable } : status,
          }));
        }
        return status.updateAvailable;
      } catch (err) {
        logs.error('deps', `Failed to check for updates: ${err}`);
        return null;
      }
    },

    async installYtdlp(version?: string) {
      return installDep('ytdlp', version);
    },

    async fetchReleases() {
      try {
        const proxyConfig = getProxyConfig();
        const releases = await invoke<ReleaseInfo[]>('get_ytdlp_releases', { proxyConfig });
        update((s) => ({ ...s, releases }));
        return releases;
      } catch (err) {
        logs.error('deps', `Failed to fetch releases: ${err}`);
        return [];
      }
    },

    async uninstallYtdlp() {
      return uninstallDep('ytdlp');
    },

    async checkFfmpeg() {
      return checkDep('ffmpeg');
    },

    async installFfmpeg() {
      return installDep('ffmpeg');
    },

    async uninstallFfmpeg() {
      return uninstallDep('ffmpeg');
    },

    async checkAria2() {
      return checkDep('aria2');
    },

    async installAria2() {
      return installDep('aria2');
    },

    async uninstallAria2() {
      return uninstallDep('aria2');
    },

    async checkDeno() {
      return checkDep('deno');
    },

    async installDeno() {
      return installDep('deno');
    },

    async uninstallDeno() {
      return uninstallDep('deno');
    },

    async checkQuickjs() {
      return checkDep('quickjs');
    },

    async installQuickjs() {
      return installDep('quickjs');
    },

    async uninstallQuickjs() {
      return uninstallDep('quickjs');
    },

    async checkLux() {
      return checkDep('lux');
    },

    async installLux() {
      return installDep('lux');
    },

    async uninstallLux() {
      return uninstallDep('lux');
    },

    async checkAll(force: boolean = false) {
      const state = get({ subscribe });
      if (state.hasCheckedAll && !force) {
        return;
      }

      logs.debug('deps', 'Checking all dependencies...');
      await Promise.all([
        checkDep('ytdlp'),
        checkDep('ffmpeg'),
        checkDep('aria2'),
        checkDep('deno'),
        checkDep('quickjs'),
        checkDep('lux'),
      ]);
      update((s) => ({ ...s, hasCheckedAll: true }));
      logs.debug('deps', 'All dependency checks complete');
    },

    clearError() {
      update((s) => ({ ...s, error: null }));
    },

    isReady(): boolean {
      const state = get({ subscribe });
      return state.ytdlp?.installed ?? false;
    },

    allInstalled(): boolean {
      const state = get({ subscribe });
      return (
        (state.ytdlp?.installed ?? false) &&
        (state.ffmpeg?.installed ?? false) &&
        (state.aria2?.installed ?? false)
      );
    },
  };
}

export const deps = createDepsStore();
