<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings, updateSetting, defaultSettings } from '$lib/stores/settings';
  import {
    updateState,
    checkForUpdates,
    downloadAndInstall,
    getCurrentVersion,
  } from '$lib/stores/updates';
  import Icon from '$lib/components/Icon.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import HighlightText from '$lib/components/HighlightText.svelte';

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

<div class="updates-card">
  <div class="header">
    <div class="header-content">
      <div class="icon-wrapper">
        <Icon name="download" size={18} />
      </div>
      <div class="text-content">
        <div class="title">
          <HighlightText text={$t('settings.app.updates')} highlight={searchQuery} />
        </div>
        <div class="description">
          <HighlightText
            text={$t('settings.app.currentVersion', { version: getCurrentVersion() })}
            highlight={searchQuery}
          />
        </div>
      </div>
    </div>
    <button class="check-btn" onclick={handleCheckForUpdates} disabled={checkingUpdate}>
      {#if checkingUpdate}
        <span class="btn-spinner"></span>
      {:else}
        <Icon name="download" size={16} />
      {/if}
      <span class="btn-text">{$t('settings.app.checkForUpdates')}</span>
    </button>
  </div>

  <div class="option-row">
    <div class="option-content">
      <Icon name="refresh" size={16} />
      <div class="option-text">
        <span class="option-title">{$t('settings.app.autoUpdate')}</span>
        <span class="option-desc">{$t('settings.app.autoUpdateDescription')}</span>
      </div>
    </div>
    <Toggle checked={$settings.autoUpdate} onchange={(v) => updateSetting('autoUpdate', v)} />
  </div>

  {#if $updateState.available && $updateState.info}
    <div class="update-available">
      <div class="update-header">
        <div class="update-info">
          <Icon name="download" size={18} />
          <span class="update-version">
            {$t('settings.app.updateAvailable', { version: $updateState.info.version })}
          </span>
          {#if $updateState.info.isPreRelease}
            <span class="update-badge pre">Pre-release</span>
          {/if}
        </div>
        <button
          class="install-btn"
          onclick={downloadAndInstall}
          disabled={$updateState.downloading || $updateState.installTriggered}
        >
          {#if $updateState.downloading}
            <span class="btn-spinner"></span>
            <span>{$t('settings.app.downloading')} {$updateState.progress}%</span>
          {:else if $updateState.installTriggered}
            <Icon name="check" size={14} />
            <span>{$t('settings.app.installTriggered')}</span>
          {:else}
            <span>{$t('settings.app.installUpdate')}</span>
          {/if}
        </button>
      </div>

      {#if $updateState.downloading}
        <div class="progress-bar">
          <div class="progress-fill" style="width: {$updateState.progress}%"></div>
        </div>
      {/if}

      {#if $updateState.info.notes}
        <div class="update-notes">
          <span class="notes-label">{$t('settings.app.whatsNew')}</span>
          <div class="notes-text">{$updateState.info.notes}</div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .updates-card {
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

  .check-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-sm, 6px);
    color: white;
    font-size: var(--text-sm, 12px);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .check-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
  }

  .check-btn:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .option-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: var(--radius-md, 8px);
  }

  .option-content {
    display: flex;
    align-items: center;
    gap: 10px;
    color: rgba(255, 255, 255, 0.5);
    min-width: 0;
  }

  .option-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .option-title {
    font-size: var(--text-sm, 12px);
    font-weight: 450;
    color: rgba(255, 255, 255, 0.85);
  }

  .option-desc {
    font-size: var(--text-xs, 11px);
    color: rgba(255, 255, 255, 0.4);
  }

  .update-available {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    background: rgba(var(--accent-rgb, 99, 102, 241), 0.1);
    border: 1px solid rgba(var(--accent-rgb, 99, 102, 241), 0.2);
    border-radius: var(--radius-md, 8px);
  }

  .update-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }

  .update-info {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--accent, #6366f1);
  }

  .update-version {
    font-size: var(--text-sm, 12px);
    font-weight: 600;
    color: rgba(255, 255, 255, 0.9);
  }

  .update-badge {
    font-size: 9px;
    padding: 2px 6px;
    border-radius: var(--radius-sm, 4px);
    text-transform: uppercase;
    font-weight: 700;
    letter-spacing: 0.5px;
  }

  .update-badge.pre {
    background: #eab308;
    color: black;
  }

  .install-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    background: var(--accent, #6366f1);
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: white;
    font-size: var(--text-sm, 12px);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .install-btn:hover:not(:disabled) {
    opacity: 0.9;
  }

  .install-btn:disabled {
    opacity: 0.7;
    cursor: default;
  }

  .progress-bar {
    height: 4px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--accent, #6366f1);
    transition: width 0.2s;
  }

  .update-notes {
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .notes-label {
    font-size: var(--text-xs, 11px);
    font-weight: 600;
    color: rgba(255, 255, 255, 0.6);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    display: block;
    margin-bottom: 6px;
  }

  .notes-text {
    font-size: var(--text-sm, 12px);
    color: rgba(255, 255, 255, 0.7);
    line-height: 1.5;
    white-space: pre-wrap;
    max-height: 150px;
    overflow-y: auto;
  }

  .btn-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 640px) {
    .updates-card {
      padding: 14px 16px;
      gap: 14px;
    }

    .header {
      flex-direction: column;
      gap: 14px;
    }

    .check-btn {
      width: 100%;
      justify-content: center;
      padding: 12px 16px;
    }

    .option-row {
      padding: 12px 14px;
    }

    .update-header {
      flex-direction: column;
      align-items: stretch;
    }

    .install-btn {
      width: 100%;
      justify-content: center;
      padding: 12px 16px;
    }
  }
</style>
