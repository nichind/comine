<script lang="ts">
  import { t } from '$lib/i18n';
  import { deps, type DependencyName } from '$lib/stores/deps';
  import { toast } from '$lib/components/Toast.svelte';
  import SettingItem from '$lib/components/SettingItem.svelte';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  /** Dependency configuration for rendering */
  interface DepConfig {
    name: DependencyName;
    label: string;
    descriptionKey: string;
    badge?: 'required' | 'optional';
    installer: () => Promise<unknown>;
    uninstaller: () => void;
  }

  const DEPENDENCIES: DepConfig[] = [
    {
      name: 'ytdlp',
      label: 'yt-dlp',
      descriptionKey: 'settings.deps.ytdlpDescription',
      badge: 'required',
      installer: () => deps.installYtdlp(),
      uninstaller: () => deps.uninstallYtdlp(),
    },
    {
      name: 'ffmpeg',
      label: 'ffmpeg',
      descriptionKey: 'settings.deps.ffmpegDescription',
      installer: () => deps.installFfmpeg(),
      uninstaller: () => deps.uninstallFfmpeg(),
    },
    {
      name: 'aria2',
      label: 'aria2',
      descriptionKey: 'settings.deps.aria2Description',
      installer: () => deps.installAria2(),
      uninstaller: () => deps.uninstallAria2(),
    },
    {
      name: 'deno',
      label: 'deno',
      descriptionKey: 'settings.deps.denoDescription',
      installer: () => deps.installDeno(),
      uninstaller: () => deps.uninstallDeno(),
    },
    {
      name: 'quickjs',
      label: 'quickjs',
      descriptionKey: 'settings.deps.quickjsDescription',
      installer: () => deps.installQuickjs(),
      uninstaller: () => deps.uninstallQuickjs(),
    },
    {
      name: 'lux',
      label: 'lux',
      descriptionKey: 'settings.deps.luxDescription',
      badge: 'optional',
      installer: () => deps.installLux(),
      uninstaller: () => deps.uninstallLux(),
    },
  ];

  async function installDepWithToast(dep: DepConfig) {
    toast.info($t('deps.installing', { component: dep.label }));
    try {
      await dep.installer();
      toast.success($t('deps.installed', { component: dep.label }));
    } catch (err) {
      console.error(`Failed to install dependency: ${dep.name}`, err);
      toast.error($t('deps.installFailed', { component: dep.label }));
    }
  }

  function getDepInfo(name: DependencyName) {
    return $deps[name];
  }
</script>

{#each DEPENDENCIES as dep (dep.name)}
  {@const info = getDepInfo(dep.name)}
  {@const isChecking = $deps.checking === dep.name}
  {@const isInstalling = $deps.installingDeps.has(dep.name)}
  {@const progress = $deps.installProgressMap.get(dep.name)}

  <SettingItem title={dep.label} description={$t(dep.descriptionKey)} highlight={searchQuery}>
    <div class="dep-item">
      <div class="dep-info">
        {#if dep.badge}
          <span class="dep-badge {dep.badge}">{$t(`settings.deps.${dep.badge}`)}</span>
        {/if}
        {#if isChecking}
          <span class="dep-status checking">{$t('settings.deps.checking')}</span>
        {:else if info?.installed}
          <span class="dep-status installed">
            {info.version ? `v${info.version}` : $t('settings.deps.installed')}
          </span>
        {:else}
          <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
        {/if}
      </div>
      <div class="dep-actions">
        {#if isInstalling}
          <button class="dep-btn" disabled>
            <span class="btn-spinner"></span>
            {$t('settings.deps.installing')}
          </button>
        {:else if info?.installed}
          <button class="dep-btn danger" onclick={() => dep.uninstaller()}>
            {$t('settings.deps.uninstall')}
          </button>
          <button class="dep-btn" onclick={() => installDepWithToast(dep)}>
            {$t('settings.deps.reinstall')}
          </button>
        {:else}
          <button class="dep-btn primary" onclick={() => installDepWithToast(dep)}>
            {$t('settings.deps.install')}
          </button>
        {/if}
      </div>
    </div>
    {#if isInstalling && progress}
      <div class="dep-progress">
        <div class="dep-progress-bar" style="width: {progress.progress ?? 0}%"></div>
      </div>
    {/if}
  </SettingItem>
{/each}

{#if $deps.error}
  <p class="dep-error">{$deps.error}</p>
{/if}

<style>
  .dep-item {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .dep-info {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .dep-badge {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 4px;
    text-transform: uppercase;
  }

  .dep-badge.required {
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
  }

  .dep-badge.optional {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.6);
  }

  .dep-status {
    font-size: 12px;
    font-weight: 500;
    padding: 2px 8px;
    border-radius: 6px;
  }

  .dep-status.checking {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.7);
  }

  .dep-status.installed {
    background: rgba(34, 197, 94, 0.15);
    color: #22c55e;
  }

  .dep-status.not-installed {
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.4);
  }

  .dep-actions {
    display: flex;
    gap: 8px;
  }

  .dep-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.05);
    color: white;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
  }

  .dep-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
  }

  .dep-btn.primary {
    background: #6366f1;
    border-color: #6366f1;
  }

  .dep-btn.primary:hover:not(:disabled) {
    background: #4f46e5;
  }

  .dep-btn.danger {
    background: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.2);
    color: #ef4444;
  }

  .dep-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.2);
  }

  .dep-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .dep-progress {
    width: 100%;
    height: 4px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 2px;
    overflow: hidden;
    margin-top: 8px;
  }

  .dep-progress-bar {
    height: 100%;
    background: #6366f1;
    transition: width 0.2s;
  }

  .btn-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  .dep-error {
    margin-top: 10px;
    padding: 10px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: 8px;
    color: #ef4444;
    font-size: 13px;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>