<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings, defaultSettings, updateSetting } from '$lib/stores/settings';
  import SettingItem from '$lib/components/SettingItem.svelte';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();
</script>

<SettingItem
  title={$t('settings.app.accentStyle')}
  description={$t('settings.app.accentStyleDescription')}
  icon="pen_new"
  value={$settings.accentStyle}
  defaultValue={defaultSettings.accentStyle}
  onReset={() => updateSetting('accentStyle', defaultSettings.accentStyle)}
  highlight={searchQuery}
>
  <div class="accent-style-options">
    <button
      type="button"
      class="accent-style-btn"
      class:active={$settings.accentStyle === 'solid'}
      onclick={() => updateSetting('accentStyle', 'solid')}
    >
      {$t('settings.app.accentStyleSolid')}
    </button>
    <button
      type="button"
      class="accent-style-btn"
      class:active={$settings.accentStyle === 'gradient'}
      onclick={() => updateSetting('accentStyle', 'gradient')}
    >
      {$t('settings.app.accentStyleGradient')}
    </button>
    <button
      type="button"
      class="accent-style-btn"
      class:active={$settings.accentStyle === 'glow'}
      onclick={() => updateSetting('accentStyle', 'glow')}
    >
      {$t('settings.app.accentStyleGlow')}
    </button>
  </div>
</SettingItem>

<style>
  .accent-style-options {
    display: flex;
    gap: 8px;
  }

  .accent-style-btn {
    padding: 8px 16px;
    font-size: 12px;
    font-weight: 500;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    transition: all 0.15s ease;
    font-family: inherit;
  }

  .accent-style-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .accent-style-btn.active {
    background: var(--accent, #6366f1);
    border-color: var(--accent, #6366f1);
    color: white;
  }
</style>
