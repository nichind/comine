<script lang="ts">
  import { t } from '$lib/i18n';
  import { deps } from '$lib/stores/deps';
  import { toast } from '$lib/components/ui/Toast.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import HighlightText from '$lib/components/ui/HighlightText.svelte';
  import SettingsCard from '$lib/components/settings/SettingsCard.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  let updatingChannel = $state(false);

  function isMasterVersion(version: string | null | undefined): boolean {
    const v = version ?? '';
    return v.includes('master') || v.includes('nightly') || /\d{4}\.\d{2}\.\d{2}\.\d+/.test(v);
  }

  async function switchChannel(channel: 'stable' | 'master') {
    updatingChannel = true;
    toast.info($t('settings.deps.switchingChannel', { channel }));
    try {
      const newVersion = await deps.updateYtdlpChannel(channel);
      toast.success($t('settings.deps.channelSwitched', { channel, version: newVersion }));
    } catch (err) {
      toast.error($t('settings.deps.channelSwitchFailed', { error: String(err) }));
    } finally {
      updatingChannel = false;
    }
  }
</script>

<SettingsCard icon="package" class="deps-card">
  {#snippet title()}<HighlightText
      text={$t('settings.deps.title')}
      highlight={searchQuery}
    />{/snippet}
  {#snippet description()}<HighlightText
      text={$t('settings.deps.description')}
      highlight={searchQuery}
    />{/snippet}

  <div class="deps-list">
    {#if $deps.ytdlp}
      {@const info = $deps.ytdlp}
      {@const isChecking = $deps.checking === 'ytdlp'}

      <div class="dep-row">
        <div class="dep-main">
          <div class="dep-header">
            <span class="dep-name">yt-dlp</span>
            <span class="dep-badge required">{$t('settings.deps.required')}</span>
            {#if isChecking}
              <span class="dep-version checking">
                <Icon name="spinner" size={12} />
              </span>
            {:else if info?.installed}
              <span class="dep-version installed">
                {info.version ?? 'embedded'}
              </span>
            {:else}
              <span class="dep-version missing">{$t('settings.deps.notInstalled')}</span>
            {/if}
          </div>
          <div class="dep-desc">{$t('settings.deps.ytdlpDescription')}</div>
        </div>

        <div class="dep-actions">
          {#if info?.installed}
            <button
              class="action-btn master"
              class:active={isMasterVersion(info.version)}
              onclick={() => switchChannel(isMasterVersion(info.version) ? 'stable' : 'master')}
              disabled={updatingChannel}
              use:tooltip={isMasterVersion(info.version)
                ? $t('settings.deps.switchToStable')
                : $t('settings.deps.switchToMaster')}
            >
              {#if updatingChannel}
                <Icon name="spinner" size={18} />
              {:else}
                <Icon name="code_funky" size={18} />
              {/if}
            </button>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</SettingsCard>

<style>
  .deps-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .dep-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: var(--radius-md, 8px);
  }

  .dep-main {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    flex: 1;
  }

  .dep-header {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .dep-name {
    font-size: var(--text-sm, 12px);
    font-weight: 600;
    color: rgba(255, 255, 255, 0.92);
  }

  .dep-badge {
    font-size: 9px;
    font-weight: 700;
    padding: 2px 5px;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .dep-badge.required {
    background: rgba(239, 68, 68, 0.18);
    color: #f87171;
  }

  .dep-version {
    font-size: var(--text-xs, 11px);
    font-weight: 500;
    color: rgba(255, 255, 255, 0.5);
  }

  .dep-version.installed {
    color: #4ade80;
  }

  .dep-version.missing {
    color: rgba(255, 255, 255, 0.35);
    font-style: italic;
  }

  .dep-version.checking {
    display: inline-flex;
    align-items: center;
    color: rgba(255, 255, 255, 0.5);
    animation: spin 0.8s linear infinite;
  }

  .dep-desc {
    font-size: var(--text-xs, 11px);
    font-weight: 400;
    color: rgba(255, 255, 255, 0.45);
    line-height: 1.4;
  }

  .dep-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border-radius: var(--radius-sm, 6px);
    border: 1px solid transparent;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .action-btn.master {
    background: rgba(139, 92, 246, 0.12);
    border-color: rgba(139, 92, 246, 0.15);
    color: #a78bfa;
  }

  .action-btn.master:hover {
    background: rgba(139, 92, 246, 0.22);
  }

  .action-btn.master.active {
    background: rgba(139, 92, 246, 0.25);
    border-color: rgba(139, 92, 246, 0.35);
    color: #c4b5fd;
  }

  .action-btn.master:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 640px) {
    .dep-row {
      padding: 12px 14px;
      flex-direction: column;
      align-items: stretch;
      gap: 10px;
    }

    .dep-actions {
      justify-content: flex-end;
    }

    .action-btn {
      width: 40px;
      height: 40px;
    }
  }
</style>
