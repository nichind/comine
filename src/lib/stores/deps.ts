import { writable, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logs } from './logs';
import { settings, getProxyConfig } from './settings';
import { toast, updateToast, dismissToast } from '$lib/components/ui/Toast.svelte';
import { formatSize, formatSpeed } from '$lib/utils/format';
import { translate } from '$lib/i18n';
import type { DependencyStatus, InstallProgress } from '$lib/bindings';

export type DependencyName =
  | 'ytdlp'
  | 'ffmpeg'
  | 'aria2'
  | 'deno'
  | 'quickjs'
  | 'lux'
  | 'gallery_dl'
  | 'edge_tts'
  | 'whisper';

export interface DepsState {
  ytdlp: DependencyStatus | null;
  ffmpeg: DependencyStatus | null;
  aria2: DependencyStatus | null;
  deno: DependencyStatus | null;
  quickjs: DependencyStatus | null;
  lux: DependencyStatus | null;
  gallery_dl: DependencyStatus | null;
  edge_tts: DependencyStatus | null;
  whisper: DependencyStatus | null;
  checking: DependencyName | null;
  installingDeps: Set<DependencyName>;
  installProgressMap: Map<DependencyName, InstallProgress>;
  installing: DependencyName | null;
  installProgress: InstallProgress | null;
  error: string | null;
  hasCheckedAll: boolean;
}

const initialState: DepsState = {
  ytdlp: null,
  ffmpeg: null,
  aria2: null,
  deno: null,
  quickjs: null,
  lux: null,
  gallery_dl: null,
  edge_tts: null,
  whisper: null,
  checking: null,
  installingDeps: new Set(),
  installProgressMap: new Map(),
  installing: null,
  installProgress: null,
  error: null,
  hasCheckedAll: false,
};

export interface DepConfigEntry {
  label: string;
  required: boolean;
  checkCommand: string;
  installCommand: string;
  uninstallCommand: string;
  progressEvent: string;
}

export const DEP_CONFIG: Record<DependencyName, DepConfigEntry> = {
  ytdlp: {
    label: 'yt-dlp',
    required: true,
    checkCommand: 'check_ytdlp',
    installCommand: 'install_ytdlp',
    uninstallCommand: 'uninstall_ytdlp',
    progressEvent: 'ytdlp-install-progress',
  },
  ffmpeg: {
    label: 'FFmpeg',
    required: true,
    checkCommand: 'check_ffmpeg',
    installCommand: 'install_ffmpeg',
    uninstallCommand: 'uninstall_ffmpeg',
    progressEvent: 'ffmpeg-install-progress',
  },
  aria2: {
    label: 'aria2',
    required: true,
    checkCommand: 'check_aria2',
    installCommand: 'install_aria2',
    uninstallCommand: 'uninstall_aria2',
    progressEvent: 'aria2-install-progress',
  },
  deno: {
    label: 'deno',
    required: true,
    checkCommand: 'check_deno',
    installCommand: 'install_deno',
    uninstallCommand: 'uninstall_deno',
    progressEvent: 'deno-install-progress',
  },
  quickjs: {
    label: 'QuickJS',
    required: true,
    checkCommand: 'check_quickjs',
    installCommand: 'install_quickjs',
    uninstallCommand: 'uninstall_quickjs',
    progressEvent: 'quickjs-install-progress',
  },
  lux: {
    label: 'lux',
    required: false,
    checkCommand: 'check_lux',
    installCommand: 'install_lux',
    uninstallCommand: 'uninstall_lux',
    progressEvent: 'lux-install-progress',
  },
  gallery_dl: {
    label: 'gallery-dl',
    required: true,
    checkCommand: 'check_gallery_dl',
    installCommand: 'install_gallery_dl',
    uninstallCommand: 'uninstall_gallery_dl',
    progressEvent: 'gallery-dl-install-progress',
  },
  edge_tts: {
    label: 'edge-tts',
    required: false,
    checkCommand: 'check_edge_tts',
    installCommand: 'install_edge_tts',
    uninstallCommand: 'uninstall_edge_tts',
    progressEvent: 'edge-tts-install-progress',
  },
  whisper: {
    label: 'whisper',
    required: false,
    checkCommand: 'check_whisper',
    installCommand: 'install_whisper',
    uninstallCommand: 'uninstall_whisper',
    progressEvent: 'whisper-install-progress',
  },
};

const installToastIds = new Map<DependencyName, number>();
const lastToastUpdateAt = new Map<DependencyName, number>();

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
      if (total > 0) subParts.push(`${formatSize(downloaded)} / ${formatSize(total)}`);
      else if (downloaded > 0) subParts.push(formatSize(downloaded));

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

    async installYtdlp(version?: string) {
      return installDep('ytdlp', version);
    },

    async updateYtdlpChannel(channel: 'stable' | 'master') {
      logs.info('deps', `Updating yt-dlp to ${channel} channel...`);
      try {
        const newVersion = await invoke<string>('update_ytdlp_channel', { channel });
        logs.info('deps', `yt-dlp updated to ${newVersion}`);
        await checkDep('ytdlp');
        return newVersion;
      } catch (err) {
        logs.error('deps', `Failed to update yt-dlp channel: ${err}`);
        throw err;
      }
    },

    async forceUpdateYtdlp(): Promise<void> {
      const state = get({ subscribe });
      if (!state.ytdlp?.installed) {
        logs.debug('deps', 'Skipping yt-dlp self-update: not installed');
        return;
      }

      try {
        logs.info('deps', 'Running yt-dlp --update (self-update within current channel)...');
        const newVersion = await invoke<string>('self_update_ytdlp');
        logs.info('deps', `yt-dlp self-update complete: ${newVersion}`);
        await checkDep('ytdlp');
      } catch (err) {
        logs.warn('deps', `yt-dlp self-update failed: ${err}`);
      }
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
        checkDep('gallery_dl'),
        checkDep('edge_tts'),
        checkDep('whisper'),
      ]);
      update((s) => ({ ...s, hasCheckedAll: true }));
      logs.debug('deps', 'All dependency checks complete');
    },

    clearError() {
      update((s) => ({ ...s, error: null }));
    },

    async autoInstallBundle(): Promise<void> {
      await new Promise((r) => setTimeout(r, 1000));

      const state = get({ subscribe });
      const BUNDLE = (Object.entries(DEP_CONFIG) as [DependencyName, DepConfigEntry][])
        .filter(([, cfg]) => cfg.required)
        .map(([key, cfg]) => ({ name: cfg.label, key }));
      const missing = BUNDLE.filter((d) => !state[d.key]?.installed);
      if (missing.length === 0) return;

      const $t = translate;
      logs.info('deps', `Auto-installing ${missing.length} missing dependencies...`);

      const toastId = toast.progress(
        $t('deps.installing') || 'Installing components...',
        0,
        `0/${missing.length} ${$t('deps.components') || 'components'}`
      );

      let installed = 0;
      const total = missing.length;

      const aria2Idx = missing.findIndex((d) => d.key === 'aria2');
      if (aria2Idx !== -1) {
        const aria2 = missing.splice(aria2Idx, 1)[0];
        updateToast(toastId, {
          message: `${$t('deps.installing') || 'Installing'} ${aria2.name}...`,
          subMessage: `${installed}/${total} ${$t('deps.components') || 'components'}`,
        });
        if (await installDep('aria2')) installed++;
        updateToast(toastId, {
          progress: (installed / total) * 100,
          subMessage: `${installed}/${total} ${$t('deps.components') || 'components'}`,
        });
      }

      const results = await Promise.all(
        missing.map(async (dep) => {
          updateToast(toastId, {
            message: `${$t('deps.installing') || 'Installing'} ${dep.name}...`,
          });
          let success = false;
          if (dep.key === 'ytdlp') {
            success = await installDep('ytdlp');
            if (success) {
              try {
                await invoke<string>('update_ytdlp_channel', { channel: 'master' });
                await checkDep('ytdlp');
              } catch {}
            }
          } else {
            success = await installDep(dep.key);
          }
          if (success) {
            installed++;
            updateToast(toastId, {
              progress: (installed / total) * 100,
              subMessage: `${installed}/${total} ${$t('deps.components') || 'components'}`,
            });
          }
          return success;
        })
      );

      const allSuccess = results.every(Boolean) && (aria2Idx === -1 || installed > 0);
      if (allSuccess) {
        updateToast(toastId, {
          type: 'success',
          message: $t('deps.ready') || 'Components ready!',
          progress: 100,
        });
        setTimeout(() => dismissToast(toastId), 3000);
      } else {
        updateToast(toastId, {
          type: 'warning',
          message: $t('deps.someError') || 'Some components failed to install',
          subMessage: $t('deps.checkSettings') || 'Check Settings → Dependencies',
        });
        setTimeout(() => dismissToast(toastId), 5000);
      }
    },
  };
}

export const deps = createDepsStore();

let depUpdateUnlisten: (() => void) | null = null;

interface DepUpdatePayload {
  dep: DependencyName;
  version: string;
}

export async function listenForDepUpdates(): Promise<void> {
  if (depUpdateUnlisten) return;

  depUpdateUnlisten = await listen<DepUpdatePayload[]>('dep-updates-available', (event) => {
    const updates = event.payload;
    if (!updates || updates.length === 0) return;

    logs.info('deps', `Backend reports updates for: ${updates.map((u) => u.dep).join(', ')}`);

    for (const { dep } of updates) {
      deps.check(dep, true);
    }

    const s = get(settings);
    const $t = translate;

    if (s.autoUpdateDeps) {
      const toastId = toast.info(
        $t('deps.updatesAvailable') || `Updates available for ${updates.length} component(s)`,
        15000
      );
      updateToast(toastId, {
        actionLabel: $t('common.dismiss') || 'Dismiss',
        action: () => {
          dismissToast(toastId);
          dismissed = true;
        },
      });

      let dismissed = false;
      setTimeout(async () => {
        if (!dismissed) {
          dismissToast(toastId);
          for (const { dep } of updates) {
            await deps.install(dep);
          }
        }
      }, 15000);
    } else {
      if (updates.length === 1) {
        const { dep, version } = updates[0];
        const label = DEP_CONFIG[dep].label;
        const id = toast.info(
          `${$t('deps.updateAvailableFor') || 'Update available for'} ${label}: v${version}`,
          0
        );
        updateToast(id, {
          actionLabel: $t('deps.updateNow') || 'Update',
          action: () => {
            dismissToast(id);
            deps.install(dep);
          },
        });
      } else {
        const names = updates.map((u) => DEP_CONFIG[u.dep].label).join(', ');
        const id = toast.info(
          `${$t('deps.updatesAvailable') || 'Updates available for'}: ${names}`,
          0
        );
        updateToast(id, {
          actionLabel: $t('deps.updateAll') || 'Update All',
          action: async () => {
            dismissToast(id);
            for (const { dep } of updates) {
              await deps.install(dep);
            }
          },
        });
      }
    }
  });
}

export function stopDepUpdateListener(): void {
  if (depUpdateUnlisten) {
    depUpdateUnlisten();
    depUpdateUnlisten = null;
  }
}
