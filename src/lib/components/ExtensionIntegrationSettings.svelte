<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { writeText } from '@tauri-apps/plugin-clipboard-manager';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import SettingsBlock from './SettingsBlock.svelte';
  import SettingItem from './SettingItem.svelte';
  import Toggle from './Toggle.svelte';
  import Button from './Button.svelte';
  import Input from './Input.svelte';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';
  import { t } from '$lib/i18n';
  import { toast } from '$lib/components/Toast.svelte';
  import { settings, updateSetting } from '$lib/stores/settings';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  let serverRunning = $state(false);
  let serverPort = $state(9549);
  let localEndpointInput = $state('');
  let isEditingLocalEndpoint = $state(false);
  let localEndpointSnapshot = $state('');
  let suppressLocalApplyOnce = $state(false);
  let showInstallModal = $state(false);

  let localEndpoint = $derived(normalizeLocalEndpoint(serverPort));

  let cookieReceipts = $derived($settings.extensionCookiesReceived || []);

  function formatWhen(ts: number): string {
    if (!ts) return '';
    try {
      return new Date(ts).toLocaleString();
    } catch {
      return '';
    }
  }

  $effect(() => {
    if (
      !isEditingLocalEndpoint &&
      $settings.extensionLocalPort &&
      $settings.extensionLocalPort !== serverPort
    ) {
      serverPort = $settings.extensionLocalPort;
      localEndpointInput = normalizeLocalEndpoint(serverPort);
      localEndpointSnapshot = localEndpointInput;
    }
  });

  function normalizeLocalEndpoint(port: number) {
    return `http://127.0.0.1:${port}`;
  }

  function extractPortFromInput(input: string): number | null {
    const trimmed = (input || '').trim();
    if (!trimmed) return null;

    const digitsOnly = trimmed.replace(/[^0-9]/g, '');
    if (digitsOnly.length >= 2 && digitsOnly.length <= 5 && /^\d+$/.test(trimmed)) {
      const p = Number(trimmed);
      return Number.isFinite(p) ? p : null;
    }

    const match = trimmed.match(/:(\d{2,5})(?:\b|\/|$)/);
    if (match?.[1]) {
      const p = Number(match[1]);
      return Number.isFinite(p) ? p : null;
    }
    return null;
  }

  async function applyLocalEndpointFromInput() {
    const prevPort = serverPort;
    const port = extractPortFromInput(localEndpointInput);
    if (!port || port < 1 || port > 65535) {
      localEndpointInput = normalizeLocalEndpoint(serverPort);
      toast.error('Invalid port');
      return;
    }

    if (port === serverPort) {
      localEndpointInput = normalizeLocalEndpoint(serverPort);
      return;
    }

    serverPort = port;
    localEndpointInput = normalizeLocalEndpoint(serverPort);

    void updateSetting('extensionLocalPort', serverPort);

    if (serverRunning) {
      try {
        await invoke('server_stop');
        await invoke('server_start', { port: serverPort });
        serverRunning = true;
      } catch {
        serverPort = prevPort;
        localEndpointInput = normalizeLocalEndpoint(serverPort);
        void updateSetting('extensionLocalPort', serverPort);
        serverRunning = false;
        toast.error('Failed to restart local server');
      }
    }
  }

  function matchesSearch(keywords: string[]) {
    const q = (searchQuery || '').trim().toLowerCase();
    if (!q) return true;
    return keywords.some((k) => k.toLowerCase().includes(q) || q.includes(k.toLowerCase()));
  }

  async function refresh() {
    try {
      serverRunning = await invoke<boolean>('server_is_running');
    } catch {}
  }

  async function toggleServer(next: boolean) {
    serverRunning = next;
    try {
      if (next) {
        await invoke('server_start', { port: serverPort });
      } else {
        await invoke('server_stop');
      }
      void updateSetting('extensionServerEnabled', next);
    } catch {
      serverRunning = !next;
      toast.error('Failed to change local server state');
    }
  }

  onMount(async () => {
    if ($settings.extensionLocalPort) {
      serverPort = $settings.extensionLocalPort;
      localEndpointInput = normalizeLocalEndpoint(serverPort);
    }
    localEndpointSnapshot = localEndpointInput;

    await refresh();

    if ($settings.extensionServerEnabled !== serverRunning) {
      void updateSetting('extensionServerEnabled', serverRunning);
    }
  });
</script>

<SettingsBlock title={$t('settings.integration.title')} icon="extensions">
  {#if matchesSearch(['extension', 'browser', 'local', 'server'])}
    <SettingItem
      title={$t('settings.integration.localServer')}
      description={$t('settings.integration.localServerDescription', { port: serverPort })}
      icon="server"
      highlight={searchQuery}
    >
      <Toggle checked={serverRunning} onchange={(checked) => toggleServer(checked)} />
    </SettingItem>

    <div class="setting-sub-row">
      <div class="sub-row-left">
        <Icon name="link" size={14} />
        <span>{$t('settings.integration.localEndpoint')}</span>
      </div>
      <div class="sub-row-right">
        <Input
          value={isEditingLocalEndpoint ? localEndpointInput : localEndpoint}
          placeholder="http://127.0.0.1:9549"
          onfocus={() => {
            isEditingLocalEndpoint = true;
            localEndpointSnapshot = localEndpoint;
            localEndpointInput = localEndpoint;
          }}
          onblur={async () => {
            isEditingLocalEndpoint = false;
            if (suppressLocalApplyOnce) {
              suppressLocalApplyOnce = false;
              localEndpointInput = localEndpointSnapshot;
              return;
            }
            await applyLocalEndpointFromInput();
          }}
          oninput={(e: Event) => {
            localEndpointInput = (e.target as HTMLInputElement).value;
          }}
          onkeydown={async (e: KeyboardEvent) => {
            if (e.key === 'Enter') {
              (e.target as HTMLInputElement | null)?.blur?.();
              await applyLocalEndpointFromInput();
            }
            if (e.key === 'Escape') {
              suppressLocalApplyOnce = true;
              localEndpointInput = localEndpointSnapshot;
              (e.target as HTMLInputElement | null)?.blur?.();
            }
          }}
        />
        <Button
          variant="ghost"
          size="sm"
          onclick={async () => {
            try {
              await writeText(normalizeLocalEndpoint(serverPort));
              toast.success($t('common.copied'));
            } catch {
              try {
                await navigator.clipboard.writeText(normalizeLocalEndpoint(serverPort));
                toast.success($t('common.copied'));
              } catch {
                toast.error('Copy failed');
              }
            }
          }}
        >
          <Icon name="copy" size={14} />
        </Button>
      </div>
    </div>

    <div class="install-extension-row">
      <Button variant="secondary" size="sm" onclick={() => (showInstallModal = true)}>
        <Icon name="extensions" size={14} />
        {$t('settings.integration.installExtension')}
      </Button>
    </div>

    <div class="hint">
      {$t('settings.integration.hint')}
    </div>

    <SettingItem
      title="Cookies received"
      description={
        cookieReceipts.length
          ? `${cookieReceipts[0].count} cookie(s) from ${cookieReceipts[0].domain} · ${formatWhen(cookieReceipts[0].receivedAt)}`
          : 'No cookies received from extension yet'
      }
      icon="lock"
      highlight={searchQuery}
    >
      <Button
        variant="ghost"
        size="sm"
        disabled={!cookieReceipts.length}
        onclick={() => {
          const text = cookieReceipts
            .map((e) => `${e.domain} (${e.count})${e.sourceUrl ? `\n${e.sourceUrl}` : ''} · ${formatWhen(e.receivedAt)}`)
            .join('\n\n');
          if (text) {
            navigator.clipboard.writeText(text).then(
              () => toast.success($t('common.copied')),
              () => toast.error('Copy failed')
            );
          }
        }}
      >
        <Icon name="copy" size={14} />
      </Button>
    </SettingItem>
  {/if}
</SettingsBlock>

<Modal
  open={showInstallModal}
  title={$t('settings.integration.installExtension')}
  onclose={() => (showInstallModal = false)}
>
  <div class="install-modal-content">
    <p class="install-description">{$t('settings.integration.installDescription')}</p>

    <div class="install-options">
      <button
        class="install-option"
        onclick={async () => {
          try {
            await openUrl('https://github.com/nichind/comine-extension/releases');
          } catch {
            window.open('https://github.com/nichind/comine-extension/releases', '_blank');
          }
        }}
      >
        <Icon name="github" size={20} />
        <div class="install-option-text">
          <span class="install-option-title">GitHub Releases</span>
          <span class="install-option-desc">{$t('settings.integration.installFromGithub')}</span>
        </div>
        <Icon name="arrow_outward" size={14} />
      </button>
    </div>

    <div class="install-steps">
      <h4>{$t('settings.integration.installStepsTitle')}</h4>
      <ol>
        <li>{$t('settings.integration.installStep1')}</li>
        <li>{$t('settings.integration.installStep2')}</li>
        <li>{$t('settings.integration.installStep3')}</li>
      </ol>
    </div>
  </div>
</Modal>

<style>
  .setting-sub-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 0;
  }

  .sub-row-left {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
  }

  .sub-row-right {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 220px;
    justify-content: flex-end;
  }

  .sub-row-right :global(button) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .hint {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    margin-top: 8px;
  }

  .install-extension-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }

  .install-extension-row :global(button) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .install-modal-content {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .install-description {
    font-size: 14px;
    color: rgba(255, 255, 255, 0.7);
    margin: 0;
  }

  .install-options {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .install-option {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    cursor: pointer;
    transition: all 0.15s;
    text-align: left;
    color: white;
  }

  .install-option:hover {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.2);
  }

  .install-option-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .install-option-title {
    font-size: 14px;
    font-weight: 500;
  }

  .install-option-desc {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .install-steps {
    padding: 12px 16px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 8px;
  }

  .install-steps h4 {
    font-size: 13px;
    font-weight: 600;
    margin: 0 0 10px 0;
    color: rgba(255, 255, 255, 0.8);
  }

  .install-steps ol {
    margin: 0;
    padding-left: 20px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.6);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
</style>
