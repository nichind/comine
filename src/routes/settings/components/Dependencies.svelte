<script lang="ts">
  import { t } from '$lib/i18n';
  import { deps, type DependencyName } from '$lib/stores/deps';
  import { toast } from '$lib/components/Toast.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import HighlightText from '$lib/components/HighlightText.svelte';
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

  let updatingYtdlpChannel = $state(false);

  function isMasterVersion(version: string | null | undefined): boolean {
    return (
      version.includes('master') ||
      version.includes('nightly') ||
      /\d{4}\.\d{2}\.\d{2}\.\d+/.test(version)
    );
  }

  async function switchYtdlpChannel(channel: 'stable' | 'master') {
    updatingYtdlpChannel = true;
    toast.info($t('settings.deps.switchingChannel', { channel }));
    try {
      const newVersion = await deps.updateYtdlpChannel(channel);
      toast.success($t('settings.deps.channelSwitched', { channel, version: newVersion }));
    } catch (err) {
      toast.error($t('settings.deps.channelSwitchFailed', { error: String(err) }));
    } finally {
      updatingYtdlpChannel = false;
    }
  }
</script>

<div class="deps-card">
  <div class="header">
    <div class="header-content">
      <div class="icon-wrapper">
        <Icon name="package" size={18} />
      </div>
      <div class="text-content">
        <div class="title">
          <HighlightText text={$t('settings.deps.title')} highlight={searchQuery} />
        </div>
        <div class="description">
          <HighlightText text={$t('settings.deps.description')} highlight={searchQuery} />
        </div>
      </div>
    </div>
  </div>

  <div class="deps-list">
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
            {#if dep.name === 'ytdlp'}
              <button
                class="action-btn master"
                class:active={isMasterVersion(info.version)}
                onclick={() =>
                  switchYtdlpChannel(isMasterVersion(info.version) ? 'stable' : 'master')}
                disabled={updatingYtdlpChannel}
                use:tooltip={isMasterVersion(info.version)
                  ? $t('settings.deps.switchToStable')
                  : $t('settings.deps.switchToMaster')}
              >
                {#if updatingYtdlpChannel}
                  <Icon name="spinner" size={18} />
                {:else}
                  <Icon name="code_funky" size={18} />
                {/if}
              </button>
            {/if}
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
  </div>

  {#if $deps.error}
    <div class="dep-error">{$deps.error}</div>
  {/if}
</div>

<style>
  .deps-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: var(--radius-lg, 12px);
  }

  .header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .header-content {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    flex: 1;
    min-width: 0;
  }

  .icon-wrapper {
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.5);
    flex-shrink: 0;
    width: 24px;
    padding-top: 2px;
  }

  .text-content {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .title {
    font-size: var(--text-md, 14px);
    font-weight: 450;
    color: rgba(255, 255, 255, 0.9);
    line-height: 1.3;
  }

  .description {
    font-size: var(--text-sm, 12px);
    font-weight: 350;
    color: rgba(255, 255, 255, 0.5);
    line-height: 1.4;
  }

  .deps-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .dep-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: var(--radius-md, 8px);
  }

  .dep-main {
    display: flex;
    flex-direction: column;
    gap: 3px;
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
    font-size: var(--text-sm, 12px);
    font-weight: 600;
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
    font-size: var(--text-xs, 11px);
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
    font-size: var(--text-xs, 11px);
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
    width: 34px;
    height: 34px;
    border-radius: var(--radius-sm, 6px);
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

  .action-btn.master {
    background: rgba(139, 92, 246, 0.12);
    border-color: rgba(139, 92, 246, 0.15);
    color: #a78bfa;
  }

  .action-btn.master:hover {
    background: rgba(139, 92, 246, 0.22);
  }

  .action-btn.master.active {
    background: rgba(139, 92, 246, 0.25);
    border-color: rgba(139, 92, 246, 0.35);
    color: #c4b5fd;
  }

  .action-btn.master:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .dep-error {
    padding: 10px 12px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: var(--radius-md, 8px);
    color: #f87171;
    font-size: var(--text-sm, 12px);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 640px) {
    .deps-card {
      padding: 14px 16px;
      gap: 14px;
    }

    .deps-list {
      gap: 10px;
    }

    .dep-row {
      padding: 12px 14px;
      flex-direction: column;
      align-items: stretch;
      gap: 10px;
    }

    .dep-actions {
      justify-content: flex-end;
    }

    .action-btn {
      width: 40px;
      height: 40px;
    }
  }
</style>
