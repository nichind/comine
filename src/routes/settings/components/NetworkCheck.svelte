<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings } from '$lib/stores/settings';
  import { isDesktop } from '$lib/utils/android';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from '$lib/components/Icon.svelte';
  import SettingItem from '$lib/components/SettingItem.svelte';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  let currentIp = $state<string | null>(null);
  let checkingIp = $state(false);
  let ipProxyUsed = $state(false);

  async function checkIp() {
    checkingIp = true;
    currentIp = null;
    try {
      const result = await invoke<{ ip: string; proxyUsed: boolean }>('check_ip', {
        proxyConfig: {
          mode: $settings.proxyMode,
          customUrl: $settings.customProxyUrl,
        },
      });
      currentIp = result.ip;
      ipProxyUsed = result.proxyUsed;
    } catch (e) {
      console.error(e);
    } finally {
      checkingIp = false;
    }
  }
</script>

{#if $settings.proxyMode !== 'none' && isDesktop()}
  <SettingItem
    title={$t('settings.network.checkIp')}
    description={$t('settings.network.checkIpDescription')}
    icon="globe"
    highlight={searchQuery}
  >
    <button class="check-btn" onclick={checkIp} disabled={checkingIp}>
      {#if checkingIp}
        <span class="btn-spinner"></span>
      {:else}
        <Icon name="globe" size={14} />
      {/if}
      {$t('settings.network.checkIpBtn')}
    </button>
  </SettingItem>

  {#if currentIp}
    <div class="setting-sub-row ip-result">
      <div class="ip-result-content">
        <span class="ip-address">{currentIp}</span>
        {#if ipProxyUsed}
          <span class="ip-badge proxy">
            <Icon name="check" size={12} />
            {$t('settings.network.proxyActive')}
          </span>
        {:else}
          <span class="ip-badge direct">
            <Icon name="warning" size={12} />
            {$t('settings.network.directConnection')}
          </span>
        {/if}
      </div>
    </div>
  {/if}
{/if}

<style>
  .check-btn {
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: white;
    padding: 6px 12px;
    border-radius: var(--radius-sm, 6px);
    cursor: pointer;
    font-size: var(--text-base, 13px);
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .check-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.15);
  }
  .check-btn:disabled {
    opacity: 0.5;
    cursor: wait;
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

  .setting-sub-row {
    margin-left: 56px;
    margin-top: 5px;
  }

  .ip-result-content {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: var(--text-base, 13px);
    background: rgba(0, 0, 0, 0.2);
    padding: 8px 12px;
    border-radius: var(--radius-sm, 6px);
    display: inline-flex;
  }

  .ip-address {
    font-family: monospace;
    opacity: 0.9;
  }

  .ip-badge {
    font-size: 11px;
    padding: 2px 6px;
    border-radius: var(--radius-sm, 4px);
    display: flex;
    align-items: center;
    gap: 4px;
    font-weight: 500;
  }
  .ip-badge.proxy {
    background: rgba(74, 222, 128, 0.1);
    color: #4ade80;
  }
  .ip-badge.direct {
    background: rgba(239, 68, 68, 0.1);
    color: #ef4444;
  }
</style>
