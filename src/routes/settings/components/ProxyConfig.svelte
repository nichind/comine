<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings, updateSetting, defaultSettings } from '$lib/stores/settings';
  import { isDesktop } from '$lib/utils/android';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from '$lib/components/Icon.svelte';
  import Input from '$lib/components/Input.svelte';
  import SettingItem from '$lib/components/SettingItem.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  let detectingSystemProxy = $state(false);
  let systemProxyStatus = $state<string | null>(null);
  
  let customProxyInput = $state($settings.customProxyUrl || defaultSettings.customProxyUrl);
  let proxyValidationError = $state<string | null>(null);

  // Sync local state when external settings change (e.g., reset, sync)
  $effect(() => {
    // Only sync if we're not actively editing (no validation error) and value differs
    const externalValue = $settings.customProxyUrl || '';
    if (!proxyValidationError && customProxyInput !== externalValue) {
      customProxyInput = externalValue;
    }
  }); 
  
  function validateProxyUrl(url: string): boolean {
    if (!url.trim()) return true;
    const proxyRegex = /^(https?|socks5?):\/\/([a-zA-Z0-9.-]+|\[[a-fA-F0-9:]+\])(:\d{1,5})?(\/.*)?$/;
    return proxyRegex.test(url.trim());
  }

  function handleCustomProxyInput(value: string) {
    customProxyInput = value;
    if (!value.trim()) {
      proxyValidationError = null;
      updateSetting('customProxyUrl', '');
      return;
    }
    if (validateProxyUrl(value)) {
       proxyValidationError = null;
       updateSetting('customProxyUrl', value.trim());
    } else {
       proxyValidationError = $t('settings.network.proxyInvalid');
    }
  }

  async function detectSystemProxy() {
    if (!isDesktop()) return;
    detectingSystemProxy = true;
    systemProxyStatus = null;
    try {
      const result = await invoke<{ url: string; source: string; description: string }>('detect_system_proxy');
      if (result?.url && result.url.length > 0) {
        systemProxyStatus = `${result.url} (${result.source})`;
      } else {
        systemProxyStatus = $t('settings.network.noSystemProxy');
      }
    } catch (err) {
      console.error('Failed to detect system proxy:', err);
      systemProxyStatus = $t('settings.network.noSystemProxy');
    } finally {
      detectingSystemProxy = false;
    }
  }
</script>

{#if $settings.proxyMode === 'system' && isDesktop()}
  <div class="setting-sub-row proxy-status">
    <div class="proxy-status-content">
      {#if detectingSystemProxy}
        <span class="proxy-detecting">
          <span class="btn-spinner"></span>
          {$t('settings.network.detectingProxy')}
        </span>
      {:else if systemProxyStatus}
        <span class="proxy-detected">
          <Icon name="check" size={14} />
          {systemProxyStatus}
        </span>
      {:else}
        <span class="proxy-none">
          <Icon name="warning" size={14} />
          {$t('settings.network.noSystemProxy')}
        </span>
      {/if}
    </div>
    <button
      class="dep-btn"
      onclick={detectSystemProxy}
      use:tooltip={$t('settings.network.recheckProxy')}
    >
      <Icon name="refresh" size={16} />
    </button>
  </div>
{/if}

{#if $settings.proxyMode === 'custom'}
  <SettingItem
    title={$t('settings.network.customProxyUrl')}
    description={$t('settings.network.customProxyUrlDescription')}
    icon="link"
    value={$settings.customProxyUrl}
    defaultValue={defaultSettings.customProxyUrl}
    onReset={() => {
      customProxyInput = defaultSettings.customProxyUrl;
      handleCustomProxyInput(defaultSettings.customProxyUrl);
    }}
    highlight={searchQuery}
  >
    <div class="proxy-input-group w-250">
      <div class="proxy-input-wrapper" class:error={proxyValidationError}>
        <Input
          value={customProxyInput}
          oninput={(e) => handleCustomProxyInput((e.currentTarget as HTMLInputElement).value)}
          placeholder={$t('settings.network.customProxyUrlPlaceholder')}
        />
      </div>
    </div>
  </SettingItem>

  {#if proxyValidationError}
    <div class="setting-sub-row proxy-error">
      <span class="error-text">
        <Icon name="warning" size={14} />
        {proxyValidationError}
      </span>
      <span class="error-hint">{$t('settings.network.proxyValidFormats')}</span>
    </div>
  {/if}
{/if}

<style>
  .setting-sub-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 8px;
    margin-top: 8px;
    margin-left: 36px; /* Indent to match setting content */
  }

  .proxy-status-content {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.7);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .proxy-detected { color: #4ade80; display: flex; align-items: center; gap: 6px; }
  .proxy-none { opacity: 0.6; display: flex; align-items: center; gap: 6px; }
  .proxy-detecting { opacity: 0.8; display: flex; align-items: center; gap: 8px; }

  .dep-btn {
    background: rgba(255, 255, 255, 0.1);
    border: none;
    color: white;
    width: 28px;
    height: 28px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  .dep-btn:hover { background: rgba(255, 255, 255, 0.2); }

  .btn-spinner {
    width: 12px;
    height: 12px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 1s linear infinite;
    display: block;
  }
  
  @keyframes spin { to { transform: rotate(360deg); } }

  .w-250 { width: 250px; }
  .proxy-input-wrapper.error :global(input) {
    border-color: #ef4444;
    background: rgba(239, 68, 68, 0.1);
  }

  .proxy-error {
    margin-top: 4px;
    background: transparent;
    padding-left: 0;
    justify-content: flex-start;
    gap: 12px;
  }
  .error-text { color: #ef4444; font-size: 12px; display: flex; align-items: center; gap: 4px; }
  .error-hint { opacity: 0.5; font-size: 12px; }
</style>
