<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings, defaultSettings, updateSetting } from '$lib/stores/settings';
  import Icon from '$lib/components/Icon.svelte';
  import HighlightText from '$lib/components/HighlightText.svelte';
  import { isAndroid } from '$lib/utils/android';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  const showSystemAccent = isAndroid();

  const presetColors = [
    '#6366F1',
    '#8B5CF6',
    '#EC4899',
    '#EF4444',
    '#F97316',
    '#EAB308',
    '#22C55E',
    '#14B8A6',
    '#0EA5E9',
    '#3B82F6',
  ];

  function handleAccentColorChange(value: string) {
    if ($settings.useSystemAccent) {
      updateSetting('useSystemAccent', false);
    }
    updateSetting('accentColor', value);
  }

  function selectSystemAccent() {
    updateSetting('useSystemAccent', true);
  }

  function handleReset() {
    updateSetting('accentColor', defaultSettings.accentColor);
    if (showSystemAccent) {
      updateSetting('useSystemAccent', true);
    }
  }

  let isModified = $derived(
    $settings.accentColor !== defaultSettings.accentColor ||
      (showSystemAccent && !$settings.useSystemAccent)
  );
</script>

<div class="accent-picker">
  <div class="header">
    <div class="header-content">
      <div class="icon-wrapper">
        <Icon name="pen_new" size={18} />
      </div>
      <div class="text-content">
        <div class="title">
          <HighlightText text={$t('settings.app.accentColor')} highlight={searchQuery} />
        </div>
        <div class="description">
          <HighlightText text={$t('settings.app.accentColorDescription')} highlight={searchQuery} />
        </div>
      </div>
    </div>
    {#if isModified}
      <button class="reset-btn" onclick={handleReset} aria-label="Reset to default">
        <Icon name="undo" size={14} />
      </button>
    {/if}
  </div>

  <div class="color-grid" role="radiogroup" aria-label={$t('settings.app.accentColor')}>
    {#if showSystemAccent}
      <button
        type="button"
        class="color-option system-option"
        class:active={$settings.useSystemAccent}
        onclick={selectSystemAccent}
        aria-label={$t('settings.app.useSystemAccent')}
        aria-pressed={$settings.useSystemAccent}
      >
        <span class="color-circle system-circle">
          <Icon name="widgets" size={16} />
        </span>
        <span class="color-label">{$t('settings.app.useSystemAccent')}</span>
      </button>
    {/if}

    {#each presetColors as color}
      <button
        type="button"
        class="color-option"
        class:active={!$settings.useSystemAccent &&
          $settings.accentColor.toUpperCase() === color.toUpperCase()}
        onclick={() => handleAccentColorChange(color)}
        aria-label={color}
        aria-pressed={!$settings.useSystemAccent &&
          $settings.accentColor.toUpperCase() === color.toUpperCase()}
      >
        <span class="color-circle" style="background: {color}"></span>
      </button>
    {/each}

    <button
      type="button"
      class="color-option"
      class:active={!$settings.useSystemAccent && $settings.accentColor === 'rgb'}
      onclick={() => handleAccentColorChange('rgb')}
      aria-label={$t('settings.app.rgbColor')}
      aria-pressed={!$settings.useSystemAccent && $settings.accentColor === 'rgb'}
    >
      <span class="color-circle rgb-circle"></span>
      <span class="color-label">RGB</span>
    </button>
  </div>

  {#if !$settings.useSystemAccent}
    <div class="custom-color-section">
      <div class="custom-row">
        <input
          type="color"
          class="color-picker-input"
          value={$settings.accentColor === 'rgb' ? '#6366F1' : $settings.accentColor}
          disabled={$settings.accentColor === 'rgb'}
          oninput={(e) => handleAccentColorChange((e.target as HTMLInputElement).value)}
        />
        <input
          type="text"
          class="color-text-input"
          value={$settings.accentColor}
          placeholder="#6366F1"
          oninput={(e) => handleAccentColorChange((e.target as HTMLInputElement).value)}
        />
      </div>
    </div>
  {/if}
</div>

<style>
  .accent-picker {
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

  .reset-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm, 5px);
    background: transparent;
    border: 1px solid transparent;
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    transition: all 0.2s;
    flex-shrink: 0;
  }

  .reset-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.9);
  }

  .color-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(44px, 1fr));
    gap: 8px;
    padding: 4px 0;
  }

  .color-option {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 8px 4px;
    background: rgba(255, 255, 255, 0.03);
    border: 2px solid transparent;
    border-radius: var(--radius-md, 8px);
    cursor: pointer;
    transition: all 0.15s ease;
    min-height: 48px;
  }

  .color-option:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .color-option.active {
    border-color: rgba(255, 255, 255, 0.4);
    background: rgba(255, 255, 255, 0.1);
  }

  .system-option {
    grid-column: 1 / -1;
    flex-direction: row;
    justify-content: flex-start;
    gap: 10px;
    padding: 10px 12px;
    min-height: 44px;
  }

  .color-circle {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    flex-shrink: 0;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  }

  .system-circle {
    background: linear-gradient(135deg, #4285f4, #34a853, #fbbc04, #ea4335);
    display: flex;
    align-items: center;
    justify-content: center;
    color: white;
  }

  .rgb-circle {
    background: conic-gradient(
      from 0deg,
      #ff0000,
      #ff8000,
      #ffff00,
      #00ff00,
      #00ffff,
      #0000ff,
      #8000ff,
      #ff0080,
      #ff0000
    );
    animation: rgb-rotate 4s linear infinite;
  }

  @keyframes rgb-rotate {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .color-label {
    font-size: var(--text-xs, 11px);
    color: rgba(255, 255, 255, 0.7);
    text-align: center;
    line-height: 1.2;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .system-option .color-label {
    font-size: var(--text-sm, 12px);
  }

  .custom-color-section {
    padding-top: 4px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .custom-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .color-picker-input {
    width: 44px;
    height: 36px;
    border: none;
    border-radius: var(--radius-sm, 6px);
    cursor: pointer;
    background: transparent;
    padding: 2px;
    flex-shrink: 0;
  }

  .color-picker-input::-webkit-color-swatch-wrapper {
    padding: 0;
  }

  .color-picker-input::-webkit-color-swatch {
    border: none;
    border-radius: var(--radius-sm, 6px);
  }

  .color-text-input {
    flex: 1;
    min-width: 0;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-sm, 6px);
    padding: 8px 12px;
    color: white;
    font-size: var(--text-sm, 12px);
    font-family: monospace;
  }

  .color-text-input:focus {
    outline: none;
    border-color: var(--accent, rgba(99, 102, 241, 0.5));
    background: rgba(255, 255, 255, 0.08);
  }

  @media (max-width: 640px) {
    .accent-picker {
      padding: 14px 16px;
      gap: 14px;
    }

    .color-grid {
      grid-template-columns: repeat(auto-fill, minmax(52px, 1fr));
      gap: 10px;
    }

    .color-option {
      padding: 10px 6px;
      min-height: 56px;
    }

    .color-circle {
      width: 32px;
      height: 32px;
    }

    .system-option {
      padding: 12px 14px;
    }

    .custom-row {
      gap: 10px;
    }

    .color-picker-input {
      width: 48px;
      height: 40px;
    }

    .color-text-input {
      padding: 10px 14px;
    }
  }
</style>
