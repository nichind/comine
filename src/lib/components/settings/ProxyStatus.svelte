<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings } from '$lib/stores/settings';
  import { isDesktop } from '$lib/utils/android';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from '$lib/components/ui/Icon.svelte';
  import HighlightText from '$lib/components/ui/HighlightText.svelte';
  import SettingsCard from '$lib/components/settings/SettingsCard.svelte';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  interface ProxyStatusResult {
    mode: string;
    url: string;
    status: 'active' | 'detected' | 'failed' | 'none';
    failureCount: number;
    source: string;
  }

  interface ProxyTestResult {
    success: boolean;
    message: string;
    latencyMs: number | null;
  }

  let proxyStatus = $state<ProxyStatusResult | null>(null);
  let loadingStatus = $state(false);
  let testing = $state(false);
  let resetting = $state(false);
  let testResult = $state<ProxyTestResult | null>(null);
  let testError = $state<string | null>(null);

  async function refreshStatus() {
    loadingStatus = true;
    testResult = null;
    testError = null;
    try {
      proxyStatus = await invoke<ProxyStatusResult>('get_proxy_status');
    } catch (err) {
      console.error('Failed to get proxy status:', err);
      proxyStatus = null;
    } finally {
      loadingStatus = false;
    }
  }

  async function testProxy() {
    testing = true;
    testResult = null;
    testError = null;
    try {
      const result = await invoke<ProxyTestResult>('test_proxy');
      testResult = result;
    } catch (err) {
      testError = String(err);
    } finally {
      testing = false;
    }
  }

  async function resetProxyHealth() {
    resetting = true;
    testResult = null;
    testError = null;
    try {
      await invoke('reset_proxy_health');
      await refreshStatus();
    } catch (err) {
      console.error('Failed to reset proxy health:', err);
    } finally {
      resetting = false;
    }
  }

  function getProxyLabel(status: ProxyStatusResult): string {
    if (status.status === 'failed') return $t('settings.proxy.directFallback');
    if (status.mode === 'custom') {
      return `${$t('settings.proxy.customProxy')}: ${status.url || ''}`;
    }
    if (status.mode === 'system' && status.url) {
      return `${$t('settings.proxy.systemProxy')}: ${status.url}`;
    }
    return $t('settings.proxy.noProxy');
  }

  function getBadgeState(status: ProxyStatusResult): 'connected' | 'detected' | 'failed' {
    if (status.status === 'failed') return 'failed';
    if (status.status === 'detected') return 'detected';
    return 'connected';
  }
</script>

{#if isDesktop() && $settings.proxyMode !== 'none'}
  <SettingsCard icon="globe" class="proxy-status-card">
    {#snippet title()}
      <HighlightText text={$t('settings.proxy.status')} highlight={searchQuery} />
    {/snippet}
    {#snippet description()}
      <HighlightText
        text={proxyStatus ? getProxyLabel(proxyStatus) : '—'}
        highlight={searchQuery}
      />
    {/snippet}
    {#snippet headerAction()}
      <div class="header-actions">
        {#if proxyStatus}
          {@const badge = getBadgeState(proxyStatus)}
          <span class="status-badge {badge}" aria-label="Proxy status: {badge}">
            <span class="dot"></span>
            {#if badge === 'connected'}
              {$t('settings.proxy.connected')}
            {:else if badge === 'detected'}
              {$t('settings.proxy.detected')}
            {:else}
              {$t('settings.proxy.failed')}
            {/if}
          </span>
        {/if}
        <button
          class="icon-btn"
          onclick={refreshStatus}
          disabled={loadingStatus}
          aria-label="Refresh proxy status"
        >
          {#if loadingStatus}
            <span class="btn-spinner"></span>
          {:else}
            <Icon name="refresh" size={15} />
          {/if}
        </button>
      </div>
    {/snippet}

    <div class="actions-row">
      <button
        class="action-btn"
        onclick={testProxy}
        disabled={testing || resetting}
        aria-label={$t('settings.proxy.test')}
      >
        {#if testing}
          <span class="btn-spinner"></span>
          {$t('settings.proxy.testing')}
        {:else}
          <Icon name="globe" size={14} />
          {$t('settings.proxy.test')}
        {/if}
      </button>

      {#if proxyStatus?.status === 'failed'}
        <button
          class="action-btn danger"
          onclick={resetProxyHealth}
          disabled={resetting || testing}
          aria-label={$t('settings.proxy.reset')}
        >
          {#if resetting}
            <span class="btn-spinner"></span>
          {:else}
            <Icon name="restart" size={14} />
          {/if}
          {$t('settings.proxy.reset')}
        </button>
      {/if}
    </div>

    {#if testResult || testError}
      <div class="test-result" class:success={testResult?.success} class:failure={!testResult?.success || testError}>
        {#if testError}
          <Icon name="warning" size={14} />
          <span>{$t('settings.proxy.testFailed')}: {testError}</span>
        {:else if testResult?.success}
          <Icon name="check" size={14} />
          <span>
            {$t('settings.proxy.testSuccess')}
            {#if testResult.latencyMs !== null}
              ({testResult.latencyMs}ms)
            {/if}
          </span>
        {:else}
          <Icon name="warning" size={14} />
          <span>{$t('settings.proxy.testFailed')}: {testResult?.message}</span>
        {/if}
      </div>
    {/if}
  </SettingsCard>
{/if}

<style>
  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .status-badge {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: var(--text-xs, 11px);
    font-weight: 500;
    padding: 3px 8px;
    border-radius: var(--radius-sm, 4px);
    white-space: nowrap;
  }

  .status-badge.connected {
    background: rgba(74, 222, 128, 0.15);
    color: #4ade80;
  }

  .status-badge.detected {
    background: rgba(251, 191, 36, 0.15);
    color: #fbbf24;
  }

  .status-badge.failed {
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    flex-shrink: 0;
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border-radius: var(--radius-sm, 6px);
    background: rgba(255, 255, 255, 0.06);
    border: none;
    color: rgba(255, 255, 255, 0.55);
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .icon-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
    color: rgba(255, 255, 255, 0.9);
  }

  .icon-btn:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .actions-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .action-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    border-radius: var(--radius-sm, 6px);
    background: rgba(255, 255, 255, 0.07);
    border: 1px solid rgba(255, 255, 255, 0.07);
    color: rgba(255, 255, 255, 0.8);
    font-size: var(--text-sm, 12px);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .action-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
    color: rgba(255, 255, 255, 1);
  }

  .action-btn:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .action-btn.danger {
    background: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.2);
    color: #ef4444;
  }

  .action-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.18);
    color: #f87171;
  }

  .test-result {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--radius-md, 8px);
    font-size: var(--text-sm, 12px);
    background: rgba(0, 0, 0, 0.2);
  }

  .test-result.success {
    color: #4ade80;
  }

  .test-result.failure {
    color: #ef4444;
  }

  .btn-spinner {
    width: 13px;
    height: 13px;
    border: 2px solid rgba(255, 255, 255, 0.25);
    border-top-color: currentColor;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 640px) {
    .action-btn {
      flex: 1;
      justify-content: center;
      padding: 10px 12px;
    }

    .actions-row {
      flex-direction: row;
    }
  }
</style>
