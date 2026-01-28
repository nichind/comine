<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings, updateSetting, resetSettings } from '$lib/stores/settings';
  import { history, type HistoryItem } from '$lib/stores/history';
  import { save, open } from '@tauri-apps/plugin-dialog';
  import { writeTextFile, readTextFile } from '@tauri-apps/plugin-fs';
  import { invoke } from '@tauri-apps/api/core';
  import { toast } from '$lib/components/Toast.svelte';
  import Icon, { type IconName } from '$lib/components/Icon.svelte';
  import HighlightText from '$lib/components/HighlightText.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import { onMount } from 'svelte';
  import { isDesktop } from '$lib/utils/android';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  let showResetModal = $state(false);
  let showClearHistoryModal = $state(false);
  let onDesktop = $state(true);
  let clearingCache = $state(false);

  onMount(() => {
    onDesktop = isDesktop();
  });

  function handleResetSettings() {
    resetSettings();
    showResetModal = false;
    toast.success($t('settings.resetSuccess'));
  }

  function handleClearHistory() {
    history.clear();
    showClearHistoryModal = false;
    toast.success($t('settings.historyCleared'));
  }

  async function handleExportHistory() {
    try {
      const historyJson = JSON.stringify(
        {
          history: $history.items,
          settings: $settings,
          version: 1,
          date: new Date().toISOString(),
        },
        null,
        2
      );

      const filePath = await save({
        filters: [{ name: 'JSON', extensions: ['json'] }],
        defaultPath: `comine-backup-${new Date().toISOString().split('T')[0]}.json`,
      });

      if (filePath) {
        await writeTextFile(filePath, historyJson);
        toast.success($t('settings.exportSuccess'));
      }
    } catch (err) {
      console.error('Failed to export history:', err);
      toast.error($t('settings.exportError'));
    }
  }

  async function handleImportHistory() {
    try {
      const filePath = await open({
        filters: [{ name: 'JSON', extensions: ['json'] }],
        multiple: false,
      });

      if (filePath) {
        const content = await readTextFile(filePath as string);
        const data = JSON.parse(content);

        if (data.history && Array.isArray(data.history)) {
          const currentItems = $history.items;
          const currentIds = new Set(currentItems.map((c) => c.id));

          const newItems = (data.history as HistoryItem[]).filter(
            (item) => item.id && !currentIds.has(item.id)
          );

          if (newItems.length > 0) {
            await history.restore(newItems);
            toast.success($t('settings.importCount', { count: newItems.length }));
          } else {
            toast.info($t('settings.importNoNew'));
          }
        }

        if (data.settings) {
          const importedSettings = data.settings;
          const newSettings = { ...$settings, ...importedSettings };

          if (newSettings.downloadPath && typeof newSettings.downloadPath === 'string') {
            updateSetting('downloadPath', newSettings.downloadPath);
          }
        }
      }
    } catch (err) {
      console.error('Failed to import history:', err);
      toast.error($t('settings.importError'));
    }
  }

  async function handleClearCache() {
    clearingCache = true;
    try {
      await invoke('clear_cache');
      toast.success($t('settings.cacheCleared'));
    } catch (err) {
      console.error('Failed to clear cache:', err);
      toast.error($t('settings.cacheError'));
    } finally {
      clearingCache = false;
    }
  }

  interface ActionItem {
    id: string;
    icon: IconName;
    titleKey: string;
    descKey: string;
    action: () => void;
    danger?: boolean;
    loading?: boolean;
    desktopOnly?: boolean;
  }

  let actions = $derived<ActionItem[]>([
    {
      id: 'reset',
      icon: 'undo',
      titleKey: 'settings.data.resetSettings',
      descKey: 'settings.data.resetSettingsDescription',
      action: () => (showResetModal = true),
      danger: true,
    },
    {
      id: 'clear-history',
      icon: 'trash',
      titleKey: 'settings.data.clearHistory',
      descKey: 'settings.data.clearHistoryDescription',
      action: () => (showClearHistoryModal = true),
      danger: true,
    },
    {
      id: 'clear-cache',
      icon: 'trash',
      titleKey: 'settings.data.clearCache',
      descKey: 'settings.data.clearCacheDescription',
      action: handleClearCache,
      loading: clearingCache,
      desktopOnly: true,
    },
    {
      id: 'export',
      icon: 'download',
      titleKey: 'settings.data.exportHistory',
      descKey: 'settings.data.exportHistoryDescription',
      action: handleExportHistory,
    },
    {
      id: 'import',
      icon: 'move_to_folder',
      titleKey: 'settings.data.importHistory',
      descKey: 'settings.data.importHistoryDescription',
      action: handleImportHistory,
    },
  ]);
</script>

<div class="data-card">
  <div class="header">
    <div class="header-content">
      <div class="icon-wrapper">
        <Icon name="folder" size={18} />
      </div>
      <div class="text-content">
        <div class="title">
          <HighlightText text={$t('settings.data.title')} highlight={searchQuery} />
        </div>
        <div class="description">
          <HighlightText text={$t('settings.data.description')} highlight={searchQuery} />
        </div>
      </div>
    </div>
  </div>

  <div class="actions-grid">
    {#each actions as item (item.id)}
      {#if !item.desktopOnly || onDesktop}
        <button
          class="action-item"
          class:danger={item.danger}
          onclick={item.action}
          disabled={item.loading}
        >
          <div class="action-icon">
            {#if item.loading}
              <span class="btn-spinner"></span>
            {:else}
              <Icon name={item.icon} size={20} />
            {/if}
          </div>
          <div class="action-text">
            <span class="action-title">{$t(item.titleKey)}</span>
            <span class="action-desc">{$t(item.descKey)}</span>
          </div>
        </button>
      {/if}
    {/each}
  </div>
</div>

<Modal bind:open={showResetModal} title={$t('settings.data.resetSettings')}>
  <p>{$t('settings.data.resetSettingsConfirm')}</p>

  {#snippet actions()}
    <button class="modal-btn" onclick={() => (showResetModal = false)}>
      {$t('common.cancel')}
    </button>
    <button class="modal-btn danger" onclick={handleResetSettings}>
      {$t('settings.data.resetSettings')}
    </button>
  {/snippet}
</Modal>

<Modal bind:open={showClearHistoryModal} title={$t('settings.data.clearHistory')}>
  <p>{$t('settings.data.clearHistoryConfirm')}</p>

  {#snippet actions()}
    <button class="modal-btn" onclick={() => (showClearHistoryModal = false)}>
      {$t('common.cancel')}
    </button>
    <button class="modal-btn danger" onclick={handleClearHistory}>
      {$t('settings.data.clearHistory')}
    </button>
  {/snippet}
</Modal>

<style>
  .data-card {
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

  .actions-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 8px;
  }

  .action-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 16px 12px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: var(--radius-md, 8px);
    cursor: pointer;
    transition: all 0.15s ease;
    text-align: center;
  }

  .action-item:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.1);
  }

  .action-item:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .action-item.danger {
    border-color: rgba(239, 68, 68, 0.15);
  }

  .action-item.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.25);
  }

  .action-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md, 8px);
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.7);
  }

  .action-item.danger .action-icon {
    background: rgba(239, 68, 68, 0.12);
    color: #f87171;
  }

  .action-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .action-title {
    font-size: var(--text-sm, 12px);
    font-weight: 500;
    color: rgba(255, 255, 255, 0.9);
  }

  .action-item.danger .action-title {
    color: #f87171;
  }

  .action-desc {
    font-size: var(--text-xs, 10px);
    color: rgba(255, 255, 255, 0.4);
    line-height: 1.3;
  }

  .modal-btn {
    padding: 8px 16px;
    border-radius: var(--radius, 8px);
    font-size: var(--text-base, 13px);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
    border: none;
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .modal-btn:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .modal-btn.danger {
    background: #ef4444;
    color: white;
  }

  .modal-btn.danger:hover {
    background: #dc2626;
  }

  .btn-spinner {
    width: 18px;
    height: 18px;
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
    .data-card {
      padding: 14px 16px;
      gap: 14px;
    }

    .actions-grid {
      grid-template-columns: repeat(2, 1fr);
      gap: 10px;
    }

    .action-item {
      padding: 14px 10px;
    }

    .action-icon {
      width: 44px;
      height: 44px;
    }
  }
</style>
