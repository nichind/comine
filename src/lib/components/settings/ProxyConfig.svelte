<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings, updateSetting, resetSetting } from '$lib/stores/settings';
  import { remoteDefaults, getEffectiveDefaultFrom } from '$lib/composables/remoteSync';
  import { isDesktop } from '$lib/utils/android';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from '$lib/components/ui/Icon.svelte';
  import HighlightText from '$lib/components/ui/HighlightText.svelte';
  import SettingsCard from '$lib/components/settings/SettingsCard.svelte';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  let detectingSystemProxy = $state(false);
  let systemProxyStatus = $state<string | null>(null);

  let customProxyInput = $state(
    $settings.customProxyUrl ||
      (getEffectiveDefaultFrom($remoteDefaults, 'customProxyUrl') as string)
  );
  let proxyValidationError = $state<string | null>(null);

  $effect(() => {
    const externalValue = $settings.customProxyUrl || '';
    if (!proxyValidationError && customProxyInput !== externalValue) {
      customProxyInput = externalValue;
    }
  });

  function validateProxyUrl(url: string): boolean {
    if (!url.trim()) return true;
    const proxyRegex =
      /^(https?|socks5?):\/\/([a-zA-Z0-9.-]+|\[[a-fA-F0-9:]+\])(:\d{1,5})?(\/.*)?$/;
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

  function handleReset() {
    const effectiveDefault = getEffectiveDefaultFrom($remoteDefaults, 'customProxyUrl') as string;
    customProxyInput = effectiveDefault;
    proxyValidationError = null;
    resetSetting('customProxyUrl');
  }

  async function detectSystemProxy() {
    if (!isDesktop()) return;
    detectingSystemProxy = true;
    systemProxyStatus = null;
    try {
      const result = await invoke<{ url: string; source: string; description: string }>(
        'detect_system_proxy'
      );
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

  let isModified = $derived(
    $settings.customProxyUrl !== getEffectiveDefaultFrom($remoteDefaults, 'customProxyUrl')
  );
</script>

{#if $settings.proxyMode === 'system' && isDesktop()}
  <SettingsCard icon="globe" class="proxy-card">
    {#snippet title()}<HighlightText
        text={$t('settings.network.systemProxy')}
        highlight={searchQuery}
      />{/snippet}
    {#snippet description()}<HighlightText
        text={$t('settings.network.systemProxyDescription')}
        highlight={searchQuery}
      />{/snippet}
    {#snippet headerAction()}
      <button
        class="action-btn"
        onclick={detectSystemProxy}
        disabled={detectingSystemProxy}
        aria-label="Detect system proxy"
      >
        {#if detectingSystemProxy}
          <span class="btn-spinner"></span>
        {:else}
          <Icon name="refresh" size={16} />
        {/if}
      </button>
    {/snippet}

    {#if systemProxyStatus || detectingSystemProxy}
      <div class="status-row">
        {#if detectingSystemProxy}
          <span class="status detecting">
            <span class="btn-spinner"></span>
            {$t('settings.network.detectingProxy')}
          </span>
        {:else if systemProxyStatus?.includes('://')}
          <span class="status detected">
            <Icon name="check" size={14} />
            <span class="status-url">{systemProxyStatus}</span>
          </span>
        {:else}
          <span class="status none">
            <Icon name="warning" size={14} />
            {systemProxyStatus}
          </span>
        {/if}
      </div>
    {/if}
  </SettingsCard>
{/if}

{#if $settings.proxyMode === 'custom'}
  <SettingsCard icon="link" class="proxy-card">
    {#snippet title()}<HighlightText
        text={$t('settings.network.customProxyUrl')}
        highlight={searchQuery}
      />{/snippet}
    {#snippet description()}<HighlightText
        text={$t('settings.network.customProxyUrlDescription')}
        highlight={searchQuery}
      />{/snippet}
    {#snippet headerAction()}
      {#if isModified}
        <button class="reset-btn" onclick={handleReset} aria-label="Reset to default">
          <Icon name="undo" size={14} />
        </button>
      {/if}
    {/snippet}

    <div class="input-section">
      <input
        type="text"
        class="proxy-input"
        class:error={proxyValidationError}
        value={customProxyInput}
        oninput={(e) => handleCustomProxyInput((e.currentTarget as HTMLInputElement).value)}
        placeholder={$t('settings.network.customProxyUrlPlaceholder')}
      />
      {#if proxyValidationError}
        <div class="error-row">
          <span class="error-text">
            <Icon name="warning" size={12} />
            {proxyValidationError}
          </span>
          <span class="error-hint">{$t('settings.network.proxyValidFormats')}</span>
        </div>
      {/if}
    </div>
  </SettingsCard>
{/if}

<style>
  .reset-btn,
  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-sm, 6px);
    background: rgba(255, 255, 255, 0.08);
    border: none;
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    transition: all 0.2s;
    flex-shrink: 0;
  }

  .reset-btn {
    width: 28px;
    height: 28px;
    background: transparent;
    border: 1px solid transparent;
    color: rgba(255, 255, 255, 0.4);
  }

  .reset-btn:hover,
  .action-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
    color: rgba(255, 255, 255, 0.9);
  }

  .action-btn:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .status-row {
    padding: 10px 12px;
    background: rgba(0, 0, 0, 0.2);
    border-radius: var(--radius-md, 8px);
  }

  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--text-sm, 12px);
  }

  .status.detected {
    color: #4ade80;
  }

  .status.none {
    color: rgba(255, 255, 255, 0.5);
  }

  .status.detecting {
    color: rgba(255, 255, 255, 0.7);
  }

  .status-url {
    font-family: monospace;
    word-break: break-all;
  }

  .input-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .proxy-input {
    width: 100%;
    padding: 10px 12px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-sm, 6px);
    color: white;
    font-size: var(--text-sm, 12px);
    font-family: monospace;
  }

  .proxy-input:focus {
    outline: none;
    border-color: var(--accent, rgba(99, 102, 241, 0.5));
    background: rgba(255, 255, 255, 0.08);
  }

  .proxy-input.error {
    border-color: #ef4444;
    background: rgba(239, 68, 68, 0.08);
  }

  .error-row {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .error-text {
    display: flex;
    align-items: center;
    gap: 4px;
    color: #ef4444;
    font-size: var(--text-xs, 11px);
  }

  .error-hint {
    color: rgba(255, 255, 255, 0.4);
    font-size: var(--text-xs, 11px);
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
    .action-btn {
      width: 40px;
      height: 40px;
    }

    .proxy-input {
      padding: 12px 14px;
    }
  }
</style>
