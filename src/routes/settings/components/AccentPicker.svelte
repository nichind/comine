<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings, defaultSettings, updateSetting } from '$lib/stores/settings';
  import SettingItem from '$lib/components/SettingItem.svelte';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  function handleAccentColorChange(value: string) {
    if ($settings.useSystemAccent) {
      updateSetting('useSystemAccent', false);
    }
    updateSetting('accentColor', value);
  }
</script>

<SettingItem
  title={$t('settings.app.accentColor')}
  description={$t('settings.app.accentColorDescription')}
  icon="pen_new"
  value={$settings.accentColor}
  defaultValue={defaultSettings.accentColor}
  onReset={() => updateSetting('accentColor', defaultSettings.accentColor)}
  highlight={searchQuery}
>
  <div class="color-picker-group">
    <div class="color-presets" role="radiogroup" aria-label={$t('settings.app.accentColor')}>
      {#each ['#6366F1', '#8B5CF6', '#EC4899', '#EF4444', '#F97316', '#EAB308', '#22C55E', '#14B8A6', '#0EA5E9', '#3B82F6'] as color}
        <button
          type="button"
          class="color-swatch"
          class:active={$settings.accentColor.toUpperCase() === color.toUpperCase()}
          style="background: {color}"
          onclick={() => handleAccentColorChange(color)}
          aria-label={color}
          aria-pressed={$settings.accentColor.toUpperCase() === color.toUpperCase()}
        ></button>
      {/each}
      <button
        type="button"
        class="color-swatch rgb-swatch"
        class:active={$settings.accentColor === 'rgb'}
        onclick={() => handleAccentColorChange('rgb')}
        aria-label={$t('settings.app.rgbColor')}
        aria-pressed={$settings.accentColor === 'rgb'}
      ></button>
    </div>
    <input
      type="color"
      class="color-picker"
      value={$settings.accentColor === 'rgb' ? '#6366F1' : $settings.accentColor}
      disabled={$settings.accentColor === 'rgb'}
      oninput={(e) => handleAccentColorChange((e.target as HTMLInputElement).value)}
    />
    <input
      type="text"
      class="color-text-input"
      value={$settings.accentColor}
      oninput={(e) => handleAccentColorChange((e.target as HTMLInputElement).value)}
    />
  </div>
</SettingItem>

<style>
  .color-picker-group {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
  }

  .color-presets {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .color-swatch {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    transition: transform 0.1s;
    padding: 0;
  }

  .color-swatch:hover {
    transform: scale(1.1);
  }

  .color-swatch.active {
    border-color: white;
    transform: scale(1.1);
  }

  .rgb-swatch {
    background: linear-gradient(
      90deg,
      #ff0000 0%,
      #ff8000 14%,
      #ffff00 28%,
      #00ff00 42%,
      #00ffff 56%,
      #0000ff 70%,
      #8000ff 84%,
      #ff0080 100%
    );
    background-size: 200% 100%;
    animation: rgb-shift 3s linear infinite;
  }

  @keyframes rgb-shift {
    0% {
      background-position: 0% 50%;
    }
    100% {
      background-position: 200% 50%;
    }
  }

  .color-picker {
    width: 100%;
    height: 32px;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    background: transparent;
  }

  .color-text-input {
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    padding: 8px 12px;
    color: white;
    font-size: 13px;
    width: 100%;
  }

  .color-text-input:focus {
    outline: none;
    border-color: rgba(99, 102, 241, 0.5);
    background: rgba(255, 255, 255, 0.1);
  }
</style>