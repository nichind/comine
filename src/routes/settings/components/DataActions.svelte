<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings, updateSetting, resetSettings } from '$lib/stores/settings';
  import { history, type HistoryItem } from '$lib/stores/history';
  import { save, open } from '@tauri-apps/plugin-dialog';
  import { writeTextFile, readTextFile } from '@tauri-apps/plugin-fs';
  import { invoke } from '@tauri-apps/api/core';
  import { toast } from '$lib/components/Toast.svelte';
  import SettingItem from '$lib/components/SettingItem.svelte';
  import Icon from '$lib/components/Icon.svelte';
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
  let importMessage = $state<{ text: string; type: 'success' | 'error' } | null>(null);

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
        filters: [
          {
            name: 'JSON',
            extensions: ['json'],
          },
        ],
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
        filters: [
          {
            name: 'JSON',
            extensions: ['json'],
          },
        ],
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
          }
          
          if (newItems.length > 0) {
            toast.success($t('settings.importCount', { count: newItems.length }));
          } else {
            toast.info($t('settings.importNoNew'));
          }
        }

        if (data.settings) {
          // Merge settings safely
          const importedSettings = data.settings;
          const newSettings = { ...$settings, ...importedSettings };
          
          // Validate critical fields before applying
          if (
            newSettings.downloadPath &&
            typeof newSettings.downloadPath === 'string'
          ) {
            updateSetting('downloadPath', newSettings.downloadPath);
          }
        }

        importMessage = {
          text: $t('settings.importSuccess'),
          type: 'success',
        };
        setTimeout(() => (importMessage = null), 3000);
      }
    } catch (err) {
      console.error('Failed to import history:', err);
      importMessage = {
        text: $t('settings.importError'),
        type: 'error',
      };
      setTimeout(() => (importMessage = null), 3000);
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
</script>

<!-- Reset Settings -->
<SettingItem
  title={$t('settings.data.resetSettings')}
  description={$t('settings.data.resetSettingsDescription')}
  icon="undo"
  highlight={searchQuery}
>
  <button class="data-btn danger" onclick={() => (showResetModal = true)}>
    <Icon name="undo" size={16} />
    {$t('settings.data.resetSettings')}
  </button>
</SettingItem>

<!-- Clear History -->
<SettingItem
  title={$t('settings.data.clearHistory')}
  description={$t('settings.data.clearHistoryDescription')}
  icon="trash"
  highlight={searchQuery}
>
  <button class="data-btn danger" onclick={() => (showClearHistoryModal = true)}>
    <Icon name="trash" size={16} />
    {$t('settings.data.clearHistory')}
  </button>
</SettingItem>

<!-- Clear Cache -->
{#if onDesktop}
  <SettingItem
    title={$t('settings.data.clearCache')}
    description={$t('settings.data.clearCacheDescription')}
    icon="trash"
    highlight={searchQuery}
  >
    <button class="data-btn" onclick={handleClearCache} disabled={clearingCache}>
      {#if clearingCache}
        <span class="btn-spinner"></span>
      {:else}
        <Icon name="trash" size={16} />
      {/if}
      {$t('settings.data.clearCache')}
    </button>
  </SettingItem>
{/if}

<!-- Export History -->
<SettingItem
  title={$t('settings.data.exportHistory')}
  description={$t('settings.data.exportHistoryDescription')}
  icon="download"
  highlight={searchQuery}
>
  <button class="data-btn" onclick={handleExportHistory}>
    <Icon name="download" size={16} />
    {$t('settings.data.exportHistory')}
  </button>
</SettingItem>

<!-- Import History -->
<SettingItem
  title={$t('settings.data.importHistory')}
  description={$t('settings.data.importHistoryDescription')}
  icon="move_to_folder"
  highlight={searchQuery}
>
  <button class="data-btn" onclick={handleImportHistory}>
    <Icon name="move_to_folder" size={16} />
    {$t('settings.data.importHistory')}
  </button>
</SettingItem>

<!-- Import Message -->
{#if importMessage}
  <p
    class="import-message"
    class:success={importMessage.type === 'success'}
    class:error={importMessage.type === 'error'}
  >
    {importMessage.text}
  </p>
{/if}

<!-- Reset Settings Modal -->
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

<!-- Clear History Modal -->
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
  .data-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.05);
    color: white;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
  }

  .data-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
  }

  .data-btn.danger {
    background: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.2);
    color: #ef4444;
  }

  .data-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.2);
  }

  .data-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .import-message {
    padding: 10px;
    border-radius: 8px;
    font-size: 13px;
    background: rgba(255, 255, 255, 0.05);
    margin-top: 10px;
  }

  .import-message.success {
    background: rgba(34, 197, 94, 0.15);
    color: #22c55e;
  }

  .import-message.error {
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
  }

  .modal-btn {
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13px;
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
</style>