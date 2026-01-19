<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings, updateSetting, defaultSettings } from '$lib/stores/settings';
  import { updateState, checkForUpdates, downloadAndInstall, getCurrentVersion } from '$lib/stores/updates';
  import SettingItem from '$lib/components/SettingItem.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import Icon from '$lib/components/Icon.svelte';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  let checkingUpdate = $state(false);

  async function handleCheckForUpdates() {
     checkingUpdate = true;
     try {
       await checkForUpdates(true);
     } finally {
       checkingUpdate = false;
     }
  }
</script>

<SettingItem
  title={$t('settings.app.updates')}
  description={$t('settings.app.currentVersion', { version: getCurrentVersion() })}
  icon="download"
  highlight={searchQuery}
>
  <button class="dep-btn" onclick={handleCheckForUpdates} disabled={checkingUpdate}>
    {#if checkingUpdate}
      <span class="btn-spinner"></span>
    {:else}
      <Icon name="download" size={14} />
    {/if}
    {$t('settings.app.checkForUpdates')}
  </button>
</SettingItem>

<SettingItem
  title={$t('settings.app.autoUpdate')}
  description={$t('settings.app.autoUpdateDescription')}
  icon="refresh"
  value={$settings.autoUpdate}
  defaultValue={defaultSettings.autoUpdate}
  onReset={() => updateSetting('autoUpdate', defaultSettings.autoUpdate)}
  highlight={searchQuery}
>
  <Toggle 
    checked={$settings.autoUpdate} 
    onchange={(v) => updateSetting('autoUpdate', v)} 
  />
</SettingItem>

{#if $updateState.available && $updateState.info}
  <div class="setting-sub-row update-available">
    <div class="update-info">
      <Icon name="download" size={16} />
      <span>{$t('settings.app.updateAvailable', { version: $updateState.info.version })}</span>
      {#if $updateState.info.isPreRelease}
        <span class="update-badge pre">Pre-release</span>
      {/if}
    </div>
    <button
      class="dep-btn primary"
      onclick={downloadAndInstall}
      disabled={$updateState.downloading || $updateState.installTriggered}
    >
      {#if $updateState.downloading}
        <span class="btn-spinner"></span>
        {$t('settings.app.downloading')}
        {$updateState.progress}%
      {:else if $updateState.installTriggered}
        <Icon name="check" size={14} />
        {$t('settings.app.installTriggered')}
      {:else}
        {$t('settings.app.installUpdate')}
      {/if}
    </button>
  </div>

  {#if $updateState.downloading}
    <div class="setting-sub-row update-progress">
      <div class="update-progress-bar">
        <div class="update-progress-fill" style="width: {$updateState.progress}%"></div>
      </div>
    </div>
  {/if}

  {#if $updateState.info.notes}
    <div class="setting-sub-row update-notes">
      <div class="update-notes-content">
        <span class="update-notes-label">{$t('settings.app.whatsNew')}</span>
        <div class="update-notes-text">
          {$updateState.info.notes}
        </div>
      </div>
    </div>
  {/if}
{/if}

<style>
  .dep-btn {
    background: rgba(255, 255, 255, 0.1);
    border: none;
    color: white;
    padding: 6px 12px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dep-btn:hover:not(:disabled) { background: rgba(255, 255, 255, 0.15); }
  .dep-btn:disabled { opacity: 0.5; cursor: default; }
  .dep-btn.primary { background: var(--accent-color, #6366f1); }
  .dep-btn.primary:hover:not(:disabled) { opacity: 0.9; }

  .btn-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .setting-sub-row {
    margin-left: 36px;
    margin-top: 8px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 8px;
    padding: 12px;
  }
  
  .update-available {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  
  .update-info {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 14px;
    font-weight: 500;
  }
  
  .update-badge {
    font-size: 10px;
    padding: 2px 6px;
    border-radius: 4px;
    text-transform: uppercase;
    font-weight: 700;
    letter-spacing: 0.5px;
  }
  .update-badge.pre { background: #eab308; color: black; }

  .update-progress { padding: 0 12px 12px 12px; margin-top: 0; }
  .update-progress-bar { height: 4px; background: rgba(255,255,255,0.1); border-radius: 2px; overflow: hidden; }
  .update-progress-fill { height: 100%; background: var(--accent-color, #6366f1); transition: width 0.2s; }
  
  .update-notes { margin-top: 4px; font-size: 13px; }
  .update-notes-label { font-weight: 600; display: block; margin-bottom: 4px; opacity: 0.8; }
  .update-notes-text { opacity: 0.7; line-height: 1.4; white-space: pre-wrap; max-height: 200px; overflow-y: auto; }
</style>
