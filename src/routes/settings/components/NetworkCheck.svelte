<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings } from '$lib/stores/settings';
  import { isDesktop } from '$lib/utils/android';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from '$lib/components/Icon.svelte';
  import HighlightText from '$lib/components/HighlightText.svelte';

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
  <div class="network-card">
    <div class="header">
      <div class="header-content">
        <div class="icon-wrapper">
          <Icon name="globe" size={18} />
        </div>
        <div class="text-content">
          <div class="title">
            <HighlightText text={$t('settings.network.checkIp')} highlight={searchQuery} />
          </div>
          <div class="description">
            <HighlightText text={$t('settings.network.checkIpDescription')} highlight={searchQuery} />
          </div>
        </div>
      </div>
      <button
        class="check-btn"
        onclick={checkIp}
        disabled={checkingIp}
      >
        {#if checkingIp}
          <span class="btn-spinner"></span>
        {:else}
          <Icon name="globe" size={16} />
        {/if}
        <span class="btn-text">{$t('settings.network.checkIpBtn')}</span>
      </button>
    </div>

    {#if currentIp}
      <div class="result-row">
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
    {/if}
  </div>
{/if}

<style>
  .network-card {
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

  .result-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    background: rgba(0, 0, 0, 0.2);
    border-radius: var(--radius-md, 8px);
    flex-wrap: wrap;
  }

  .ip-address {
    font-family: monospace;
    font-size: var(--text-sm, 12px);
    color: rgba(255, 255, 255, 0.9);
  }

  .ip-badge {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-xs, 11px);
    font-weight: 500;
    padding: 3px 8px;
    border-radius: var(--radius-sm, 4px);
  }

  .ip-badge.proxy {
    background: rgba(74, 222, 128, 0.15);
    color: #4ade80;
  }

  .ip-badge.direct {
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
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
    .network-card {
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

    .result-row {
      padding: 12px 14px;
    }
  }
</style>
