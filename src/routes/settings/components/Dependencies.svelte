<script lang="ts">
  import { t } from '$lib/i18n';
  import { deps, type DependencyName } from '$lib/stores/deps';
  import { toast } from '$lib/components/Toast.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  interface DepConfig {
    name: DependencyName;
    label: string;
    descriptionKey: string;
    badge?: 'required' | 'optional';
    installer: () => Promise<boolean>;
    uninstaller: () => Promise<boolean>;
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

  async function uninstallDepWithToast(dep: DepConfig) {
    toast.info($t('deps.uninstalling', { component: dep.label }));
    const ok = await dep.uninstaller();
    if (ok) {
      toast.success($t('deps.uninstalled', { component: dep.label }));
    } else {
      console.error(`Failed to uninstall dependency: ${dep.name}`, $deps.error);
      toast.error(
        $deps.error && $deps.error.trim().length > 0
          ? $deps.error
          : $t('deps.uninstallFailed', { component: dep.label })
      );
    }
  }

  function getDepInfo(name: DependencyName) {
    return $deps[name];
  }

  function formatBytes(bytes: number | null | undefined): string {
    if (!bytes || bytes <= 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
    const value = bytes / Math.pow(1024, exp);
    const decimals = value >= 100 ? 0 : value >= 10 ? 1 : 2;
    return `${value.toFixed(decimals)} ${units[exp]}`;
  }
</script>

{#each DEPENDENCIES as dep (dep.name)}
  {@const info = getDepInfo(dep.name)}
  {@const isChecking = $deps.checking === dep.name}
  {@const isInstalling = $deps.installingDeps.has(dep.name)}

  <div class="dep-row">
    <div class="dep-main">
      <div class="dep-header">
        <span class="dep-name">{dep.label}</span>
        {#if dep.badge}
          <span class="dep-badge {dep.badge}">{$t(`settings.deps.${dep.badge}`)}</span>
        {/if}
        {#if isChecking}
          <span class="dep-version checking">
            <Icon name="spinner" size={12} />
          </span>
        {:else if info?.installed}
          <span class="dep-version installed">
            {info.version ?? ''}
            {#if info.diskSize}
              <span class="dep-size">({formatBytes(info.diskSize)})</span>
            {/if}
          </span>
        {:else}
          <span class="dep-version missing">{$t('settings.deps.notInstalled')}</span>
        {/if}
      </div>
      <div class="dep-desc">{$t(dep.descriptionKey)}</div>
    </div>

    <div class="dep-actions">
      {#if isInstalling}
        <button
          class="action-btn cancel"
          onclick={() => deps.cancelInstall(dep.name)}
          use:tooltip={$t('settings.deps.cancel') || 'Cancel'}
        >
          <Icon name="cross" size={18} />
        </button>
      {:else if info?.installed}
        <button
          class="action-btn reinstall"
          onclick={() => dep.installer()}
          use:tooltip={$t('settings.deps.reinstall')}
        >
          <Icon name="refresh" size={18} />
        </button>
        <button
          class="action-btn uninstall"
          onclick={() => uninstallDepWithToast(dep)}
          use:tooltip={$t('settings.deps.uninstall')}
        >
          <Icon name="trash" size={18} />
        </button>
      {:else}
        <button
          class="action-btn install"
          onclick={() => dep.installer()}
          use:tooltip={$t('settings.deps.install')}
        >
          <Icon name="download" size={18} />
        </button>
      {/if}
    </div>
  </div>
{/each}

{#if $deps.error}
  <p class="dep-error">{$deps.error}</p>
{/if}

<style>
  .dep-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 14px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: var(--radius-lg, 12px);
  }

  .dep-main {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }

  .dep-header {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .dep-name {
    font-size: 14px;
    font-weight: 550;
    color: rgba(255, 255, 255, 0.92);
  }

  .dep-badge {
    font-size: 9px;
    font-weight: 700;
    padding: 2px 5px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .dep-badge.required {
    background: rgba(239, 68, 68, 0.18);
    color: #f87171;
  }

  .dep-badge.optional {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.5);
  }

  .dep-version {
    font-size: 12px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.5);
  }

  .dep-version.installed {
    color: #4ade80;
  }

  .dep-version.missing {
    color: rgba(255, 255, 255, 0.35);
    font-style: italic;
  }

  .dep-version.checking {
    display: inline-flex;
    align-items: center;
    color: rgba(255, 255, 255, 0.5);
    animation: spin 0.8s linear infinite;
  }

  .dep-size {
    margin-left: 4px;
    color: rgba(255, 255, 255, 0.4);
  }

  .dep-desc {
    font-size: 12px;
    font-weight: 400;
    color: rgba(255, 255, 255, 0.45);
    line-height: 1.4;
  }

  .dep-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    border-radius: var(--radius, 10px);
    border: 1px solid transparent;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .action-btn.install {
    background: var(--accent, #6366f1);
    color: white;
  }

  .action-btn.install:hover {
    background: var(--accent-hover, #4f46e5);
    transform: scale(1.04);
  }

  .action-btn.reinstall {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .action-btn.reinstall:hover {
    background: rgba(255, 255, 255, 0.14);
    color: white;
  }

  .action-btn.uninstall {
    background: rgba(239, 68, 68, 0.12);
    border-color: rgba(239, 68, 68, 0.15);
    color: #f87171;
  }

  .action-btn.uninstall:hover {
    background: rgba(239, 68, 68, 0.22);
  }

  .action-btn.cancel {
    background: rgba(251, 191, 36, 0.15);
    border-color: rgba(251, 191, 36, 0.2);
    color: #fbbf24;
  }

  .action-btn.cancel:hover {
    background: rgba(251, 191, 36, 0.25);
  }

  .dep-error {
    margin-top: 10px;
    padding: 10px 12px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: var(--radius, 8px);
    color: #f87171;
    font-size: 13px;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
