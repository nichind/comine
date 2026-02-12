<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings, updateSetting, resetSetting } from '$lib/stores/settings';
  import { remoteDefaults, getEffectiveDefaultFrom } from '$lib/composables/remoteSync';
  import Icon from '$lib/components/ui/Icon.svelte';
  import HighlightText from '$lib/components/ui/HighlightText.svelte';
  import SettingsCard from '$lib/components/settings/SettingsCard.svelte';

  interface Props {
    searchQuery: string;
  }

  let { searchQuery }: Props = $props();

  const styles = [
    { value: 'solid', key: 'accentStyleSolid' },
    { value: 'gradient', key: 'accentStyleGradient' },
    { value: 'glow', key: 'accentStyleGlow' },
  ] as const;

  let isModified = $derived(
    $settings.accentStyle !== getEffectiveDefaultFrom($remoteDefaults, 'accentStyle')
  );

  function handleReset() {
    resetSetting('accentStyle');
  }
</script>

<SettingsCard icon="pen_new" class="accent-style-card">
  {#snippet title()}<HighlightText
      text={$t('settings.app.accentStyle')}
      highlight={searchQuery}
    />{/snippet}
  {#snippet description()}<HighlightText
      text={$t('settings.app.accentStyleDescription')}
      highlight={searchQuery}
    />{/snippet}
  {#snippet headerAction()}
    {#if isModified}
      <button class="reset-btn" onclick={handleReset} aria-label="Reset to default">
        <Icon name="undo" size={14} />
      </button>
    {/if}
  {/snippet}

  <div class="style-options" role="radiogroup" aria-label={$t('settings.app.accentStyle')}>
    {#each styles as style}
      <button
        type="button"
        class="style-option"
        class:active={$settings.accentStyle === style.value}
        onclick={() => updateSetting('accentStyle', style.value)}
        aria-pressed={$settings.accentStyle === style.value}
      >
        <span class="style-preview {style.value}"></span>
        <span class="style-label">{$t(`settings.app.${style.key}`)}</span>
      </button>
    {/each}
  </div>
</SettingsCard>

<style>
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

  .style-options {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }

  .style-option {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 12px 8px;
    background: rgba(255, 255, 255, 0.03);
    border: 2px solid transparent;
    border-radius: var(--radius-md, 8px);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .style-option:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .style-option.active {
    border-color: rgba(255, 255, 255, 0.4);
    background: rgba(255, 255, 255, 0.1);
  }

  .style-preview {
    width: 48px;
    height: 24px;
    border-radius: var(--radius-sm, 6px);
  }

  .style-preview.solid {
    background: var(--accent, #6366f1);
  }

  .style-preview.gradient {
    background: linear-gradient(
      135deg,
      var(--accent, #6366f1) 0%,
      var(--accent-secondary, #8b5cf6) 100%
    );
  }

  .style-preview.glow {
    background: var(--accent, #6366f1);
    box-shadow: 0 0 20px var(--accent-alpha, rgba(99, 102, 241, 0.5));
  }

  .style-label {
    font-size: var(--text-xs, 11px);
    color: rgba(255, 255, 255, 0.7);
    font-weight: 500;
  }

  @media (max-width: 640px) {
    .style-options {
      gap: 10px;
    }

    .style-option {
      padding: 14px 10px;
      gap: 10px;
    }

    .style-preview {
      width: 56px;
      height: 28px;
    }
  }
</style>
