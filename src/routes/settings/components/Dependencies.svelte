<script lang="ts">
  import { t } from '$lib/i18n';
  import { deps, type DependencyName } from '$lib/stores/deps';
  import { toast } from '$lib/components/Toast.svelte';
  import SettingItem from '$lib/components/SettingItem.svelte';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  async function installDepWithToast(
    dep: DependencyName,
    installer: () => Promise<unknown>,
    componentLabel: string
  ) {
    toast.info($t('deps.installing', { component: componentLabel }));
    try {
      await installer();
      toast.success($t('deps.installed', { component: componentLabel }));
    } catch (err) {
      console.error(`Failed to install dependency: ${dep}`, err);
      toast.error($t('deps.installFailed', { component: componentLabel }));
    }
  }
</script>

<!-- yt-dlp -->
<SettingItem title="yt-dlp" description={$t('settings.deps.ytdlpDescription')} highlight={searchQuery}>
  <div class="dep-item">
    <div class="dep-info">
      <span class="dep-badge required">{$t('settings.deps.required')}</span>
      {#if $deps.checking === 'ytdlp'}
        <span class="dep-status checking">{$t('settings.deps.checking')}</span>
      {:else if $deps.ytdlp?.installed}
        <span class="dep-status installed">v{$deps.ytdlp.version}</span>
      {:else}
        <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
      {/if}
    </div>
    <div class="dep-actions">
      {#if $deps.installingDeps.has('ytdlp')}
        <button class="dep-btn" disabled>
          <span class="btn-spinner"></span>
          {$t('settings.deps.installing')}
        </button>
      {:else if $deps.ytdlp?.installed}
        <button class="dep-btn danger" onclick={() => deps.uninstallYtdlp()}>
          {$t('settings.deps.uninstall')}
        </button>
        <button
          class="dep-btn"
          onclick={() => installDepWithToast('ytdlp', () => deps.installYtdlp(), 'yt-dlp')}
        >
          {$t('settings.deps.reinstall')}
        </button>
      {:else}
        <button
          class="dep-btn primary"
          onclick={() => installDepWithToast('ytdlp', () => deps.installYtdlp(), 'yt-dlp')}
        >
          {$t('settings.deps.install')}
        </button>
      {/if}
    </div>
  </div>
  {#if $deps.installingDeps.has('ytdlp') && $deps.installProgressMap.get('ytdlp')}
    <div class="dep-progress">
      <div
        class="dep-progress-bar"
        style="width: {$deps.installProgressMap.get('ytdlp')?.progress ?? 0}%"
      ></div>
    </div>
  {/if}
</SettingItem>

<!-- ffmpeg -->
<SettingItem title="ffmpeg" description={$t('settings.deps.ffmpegDescription')} highlight={searchQuery}>
  <div class="dep-item">
    <div class="dep-info">
      {#if $deps.checking === 'ffmpeg'}
        <span class="dep-status checking">{$t('settings.deps.checking')}</span>
      {:else if $deps.ffmpeg?.installed}
        <span class="dep-status installed">v{$deps.ffmpeg.version}</span>
      {:else}
        <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
      {/if}
    </div>
    <div class="dep-actions">
      {#if $deps.installingDeps.has('ffmpeg')}
        <button class="dep-btn" disabled>
          <span class="btn-spinner"></span>
          {$t('settings.deps.installing')}
        </button>
      {:else if $deps.ffmpeg?.installed}
        <button class="dep-btn danger" onclick={() => deps.uninstallFfmpeg()}>
          {$t('settings.deps.uninstall')}
        </button>
        <button
          class="dep-btn"
          onclick={() => installDepWithToast('ffmpeg', () => deps.installFfmpeg(), 'ffmpeg')}
        >
          {$t('settings.deps.reinstall')}
        </button>
      {:else}
        <button
          class="dep-btn primary"
          onclick={() => installDepWithToast('ffmpeg', () => deps.installFfmpeg(), 'ffmpeg')}
        >
          {$t('settings.deps.install')}
        </button>
      {/if}
    </div>
  </div>
  {#if $deps.installingDeps.has('ffmpeg') && $deps.installProgressMap.get('ffmpeg')}
    <div class="dep-progress">
      <div
        class="dep-progress-bar"
        style="width: {$deps.installProgressMap.get('ffmpeg')?.progress ?? 0}%"
      ></div>
    </div>
  {/if}
</SettingItem>

<!-- aria2 -->
<SettingItem title="aria2" description={$t('settings.deps.aria2Description')} highlight={searchQuery}>
  <div class="dep-item">
    <div class="dep-info">
      {#if $deps.checking === 'aria2'}
        <span class="dep-status checking">{$t('settings.deps.checking')}</span>
      {:else if $deps.aria2?.installed}
        <span class="dep-status installed">v{$deps.aria2.version}</span>
      {:else}
        <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
      {/if}
    </div>
    <div class="dep-actions">
      {#if $deps.installingDeps.has('aria2')}
        <button class="dep-btn" disabled>
          <span class="btn-spinner"></span>
          {$t('settings.deps.installing')}
        </button>
      {:else if $deps.aria2?.installed}
        <button class="dep-btn danger" onclick={() => deps.uninstallAria2()}>
          {$t('settings.deps.uninstall')}
        </button>
        <button
          class="dep-btn"
          onclick={() => installDepWithToast('aria2', () => deps.installAria2(), 'aria2')}
        >
          {$t('settings.deps.reinstall')}
        </button>
      {:else}
        <button
          class="dep-btn primary"
          onclick={() => installDepWithToast('aria2', () => deps.installAria2(), 'aria2')}
        >
          {$t('settings.deps.install')}
        </button>
      {/if}
    </div>
  </div>
  {#if $deps.installingDeps.has('aria2') && $deps.installProgressMap.get('aria2')}
    <div class="dep-progress">
      <div
        class="dep-progress-bar"
        style="width: {$deps.installProgressMap.get('aria2')?.progress ?? 0}%"
      ></div>
    </div>
  {/if}
</SettingItem>

<!-- deno -->
<SettingItem title="deno" description={$t('settings.deps.denoDescription')} highlight={searchQuery}>
  <div class="dep-item">
    <div class="dep-info">
      {#if $deps.checking === 'deno'}
        <span class="dep-status checking">{$t('settings.deps.checking')}</span>
      {:else if $deps.deno?.installed}
        <span class="dep-status installed">v{$deps.deno.version}</span>
      {:else}
        <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
      {/if}
    </div>
    <div class="dep-actions">
      {#if $deps.installingDeps.has('deno')}
        <button class="dep-btn" disabled>
          <span class="btn-spinner"></span>
          {$t('settings.deps.installing')}
        </button>
      {:else if $deps.deno?.installed}
        <button class="dep-btn danger" onclick={() => deps.uninstallDeno()}>
          {$t('settings.deps.uninstall')}
        </button>
        <button
          class="dep-btn"
          onclick={() => installDepWithToast('deno', () => deps.installDeno(), 'deno')}
        >
          {$t('settings.deps.reinstall')}
        </button>
      {:else}
        <button
          class="dep-btn primary"
          onclick={() => installDepWithToast('deno', () => deps.installDeno(), 'deno')}
        >
          {$t('settings.deps.install')}
        </button>
      {/if}
    </div>
  </div>
  {#if $deps.installingDeps.has('deno') && $deps.installProgressMap.get('deno')}
    <div class="dep-progress">
      <div
        class="dep-progress-bar"
        style="width: {$deps.installProgressMap.get('deno')?.progress ?? 0}%"
      ></div>
    </div>
  {/if}
</SettingItem>

<!-- quickjs -->
<SettingItem title="quickjs" description={$t('settings.deps.quickjsDescription')} highlight={searchQuery}>
  <div class="dep-item">
    <div class="dep-info">
      {#if $deps.checking === 'quickjs'}
        <span class="dep-status checking">{$t('settings.deps.checking')}</span>
      {:else if $deps.quickjs?.installed}
        <span class="dep-status installed">{$t('settings.deps.installed')}</span>
      {:else}
        <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
      {/if}
    </div>
    <div class="dep-actions">
      {#if $deps.installingDeps.has('quickjs')}
        <button class="dep-btn" disabled>
          <span class="btn-spinner"></span>
          {$t('settings.deps.installing')}
        </button>
      {:else if $deps.quickjs?.installed}
        <button class="dep-btn danger" onclick={() => deps.uninstallQuickjs()}>
          {$t('settings.deps.uninstall')}
        </button>
        <button
          class="dep-btn"
          onclick={() => installDepWithToast('quickjs', () => deps.installQuickjs(), 'quickjs')}
        >
          {$t('settings.deps.reinstall')}
        </button>
      {:else}
        <button
          class="dep-btn primary"
          onclick={() => installDepWithToast('quickjs', () => deps.installQuickjs(), 'quickjs')}
        >
          {$t('settings.deps.install')}
        </button>
      {/if}
    </div>
  </div>
  {#if $deps.installingDeps.has('quickjs') && $deps.installProgressMap.get('quickjs')}
    <div class="dep-progress">
      <div
        class="dep-progress-bar"
        style="width: {$deps.installProgressMap.get('quickjs')?.progress ?? 0}%"
      ></div>
    </div>
  {/if}
</SettingItem>

<!-- lux -->
<SettingItem title="lux" description={$t('settings.deps.luxDescription')} highlight={searchQuery}>
  <div class="dep-item">
    <div class="dep-info">
      <span class="dep-badge optional">{$t('settings.deps.optional')}</span>
      {#if $deps.checking === 'lux'}
        <span class="dep-status checking">{$t('settings.deps.checking')}</span>
      {:else if $deps.lux?.installed}
        <span class="dep-status installed">v{$deps.lux.version}</span>
      {:else}
        <span class="dep-status not-installed">{$t('settings.deps.notInstalled')}</span>
      {/if}
    </div>
    <div class="dep-actions">
      {#if $deps.installingDeps.has('lux')}
        <button class="dep-btn" disabled>
          <span class="btn-spinner"></span>
          {$t('settings.deps.installing')}
        </button>
      {:else if $deps.lux?.installed}
        <button class="dep-btn danger" onclick={() => deps.uninstallLux()}>
          {$t('settings.deps.uninstall')}
        </button>
        <button
          class="dep-btn"
          onclick={() => installDepWithToast('lux', () => deps.installLux(), 'lux')}
        >
          {$t('settings.deps.reinstall')}
        </button>
      {:else}
        <button
          class="dep-btn primary"
          onclick={() => installDepWithToast('lux', () => deps.installLux(), 'lux')}
        >
          {$t('settings.deps.install')}
        </button>
      {/if}
    </div>
  </div>
  {#if $deps.installingDeps.has('lux') && $deps.installProgressMap.get('lux')}
    <div class="dep-progress">
      <div
        class="dep-progress-bar"
        style="width: {$deps.installProgressMap.get('lux')?.progress ?? 0}%"
      ></div>
    </div>
  {/if}
</SettingItem>

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