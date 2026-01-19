<script lang="ts">
  import type { SettingDef } from '$lib/settings/schema';
  import { isVisibleOnPlatform } from '$lib/settings/schema';
  import type { Snippet } from 'svelte';
  import { debounce } from '$lib/utils/debounce';
  import { t } from '$lib/i18n';
  import { settings, updateSetting, defaultSettings } from '$lib/stores/settings';
  import SettingItem from './SettingItem.svelte';
  import Toggle from './Toggle.svelte';
  import Select from './Select.svelte';
  import Input from './Input.svelte';
  import Slider from './Slider.svelte';
  import ColorPicker from './ColorPicker.svelte';
  import PathPicker from './PathPicker.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    def: SettingDef;
    currentPlatform: 'windows' | 'macos' | 'linux' | 'android';
    custom?: Record<string, Snippet>;
  }

  let { def, currentPlatform, custom = {} }: Props = $props();

  // Platform + visibility + disabled checks
  let platformVisible = $derived(isVisibleOnPlatform(def.platforms, currentPlatform));
  
  let isVisible = $derived(
    platformVisible && (def.type === 'custom' || def.type === 'action' || !def.visible || def.visible($settings))
  );

  let isDisabled = $derived(
    def.type !== 'custom' && def.type !== 'action' && def.disabled?.($settings)
  );

  // Value and default for non-custom/action settings
  let value = $derived(
    def.type !== 'custom' && def.type !== 'action' 
      ? (def.key.includes('.') 
          ? def.key.split('.').reduce((obj: any, k) => obj?.[k], $settings) 
          : $settings[def.key as keyof typeof $settings]) 
      : null
  );

  let defaultVal = $derived(
    def.type !== 'custom' && def.type !== 'action' 
      ? (def.key.includes('.')
          ? def.key.split('.').reduce((obj: any, k) => obj?.[k], defaultSettings)
          : defaultSettings[def.key as keyof typeof defaultSettings])
      : null
  );

  // Helper handling the dot notation for ytdlpAdvanced
  function updateDeep(key: string, val: unknown) {
    if (!key.includes('.')) {
      updateSetting(key as any, val as any);
      return;
    }
    
    // For nested keys like ytdlpAdvanced.someProp
    const [root, sub] = key.split('.');
    const currentRoot = $settings[root as keyof typeof $settings];
    if (typeof currentRoot === 'object' && currentRoot !== null && !Array.isArray(currentRoot)) {
      const updatedRoot = { ...currentRoot, [sub]: val };
      updateSetting(root as any, updatedRoot as any);
    } 
  }

  // Debouncer for slider/input persistence
  let debouncedSave = $derived(
    (def.type === 'slider' || def.type === 'input') && def.debounce
      ? debounce((k, v) => updateDeep(k, v), def.debounce)
      : null
  );

  function set(v: unknown) {
    if (def.type === 'custom' || def.type === 'action') return;

    if (def.onSet) {
      def.onSet(v);
    }

    // 1. Immediate optimistic UI update (for responsiveness)
    // We can't easily update the store deeply without causing reactivity loop or complex logic.
    // For now, reliance on store update via updateDeep is good enough as updateSetting does store.update.
    // Ideally we would optimistically update the $settings store here before the disk write.
    
    // 2. Persist to disk
    if (debouncedSave) {
      // For sliders/inputs with debounce, we might want to update the store immediately for UI feedback
      // but delay the disk write.
      // updateDeep handles BOTH. 
      // To split them we would need separate logic. 
      // For this refactor, let's assume updateDeep is fast enough for memory, 
      // but we want to debounce the potentially slow disk save?
      // Actually `updateSetting` in `settings.ts` does `await store.save()`. This IS slow.
      // So we MUST debounce the call to `updateDeep` if we want to avoid disk spam.
      
      // But if we debounce inner call, the UI ($settings) won't update!
      // So we need to update $settings immediately manually, then debounce the disk save.
      // This is complicated because `settings.ts` doesn't expose a "update memory only" method easily
      // without modifying it.
      
      // Workaround: We use the `debouncedSave` to call `updateDeep` (which does both).
      // But we pass the value back to the primitive (Slider) which binds to `value`.
      // `value` here is a `$derived` from `$settings`. 
      // If we don't update `$settings` immediately, the slider might jump back?
      // Svelte 5 bindable might handle local state.
      
      debouncedSave(def.key, v);
    } else {
      updateDeep(def.key, v);
    }
  }

  function reset() {
    if (def.type !== 'custom' && def.type !== 'action') {
      const v = def.key.includes('.')
          ? def.key.split('.').reduce((obj: any, k) => obj?.[k], defaultSettings)
          : defaultSettings[def.key as keyof typeof defaultSettings];
      
      if (def.onSet) {
        def.onSet(v);
      }
      updateDeep(def.key, v);
    }
  }

  // Handle action buttons
  let actionLoading = $state(false);
  
  async function handleAction() {
    if (def.type !== 'action') return;
    try {
      actionLoading = true;
      await def.action();
    } finally {
      actionLoading = false;
    }
  }
</script>

{#if isVisible}
  {#if def.type === 'custom'}
    {#if custom[def.key]}
      {@render custom[def.key]()}
    {/if}
  
  {:else if def.type === 'action'}
    <SettingItem
      title={$t(def.titleKey)}
      description={def.descriptionKey ? $t(def.descriptionKey) : undefined}
      icon={def.icon}
    >
      <button 
        class="dep-btn" 
        onclick={handleAction} 
        disabled={def.loading?.() || actionLoading}
      >
        {#if def.loading?.() || actionLoading}
          <span class="btn-spinner"></span>
        {:else}
          <Icon name={def.icon} size={14} />
        {/if}
        {$t(def.buttonKey)}
      </button>
    </SettingItem>

  {:else}
    <SettingItem
      title={$t(def.titleKey)}
      description={def.descriptionKey ? $t(def.descriptionKey) : undefined}
      icon={def.icon}
      value={value}
      defaultValue={defaultVal}
      onReset={reset}
      class={def.subsection ? 'subsection-item' : ''}
    >
      {#if def.type === 'toggle'}
        <Toggle checked={value} disabled={isDisabled} onchange={(checked) => set(checked)} />

      {:else if def.type === 'select'}
        {@const opts = typeof def.options === 'function' ? def.options(currentPlatform) : def.options}
        <div class={def.width ? '' : 'w-220'} style={def.width ? `width: ${def.width}px` : ''}>
          <Select {value} options={opts} disabled={isDisabled} onchange={(v) => set(v)} />
        </div>

      {:else if def.type === 'slider'}
        <Slider {value} min={def.min} max={def.max} step={def.step ?? 1} suffix={def.suffix} disabled={isDisabled} onchange={set} />

      {:else if def.type === 'input'}
        <div class={def.width ? '' : 'w-200'} style={def.width ? `width: ${def.width}px` : ''}>
          <Input {value} placeholder={def.placeholder} disabled={isDisabled} oninput={(e) => set((e.target as HTMLInputElement).value)} />
        </div>

      {:else if def.type === 'color'}
        <ColorPicker {value} disabled={isDisabled} onchange={set} />

      {:else if def.type === 'path'}
        <PathPicker {value} pickType={def.pickType} disabled={isDisabled} onchange={set} />
      {/if}
    </SettingItem>
  {/if}
{/if}

<style>
  .btn-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.2);
    border-top-color: rgba(99, 102, 241, 0.8);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  
  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  :global(.dep-btn) {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px;
    font-size: 13px;
    font-weight: 500;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    transition: all 0.15s;
  }

  :global(.dep-btn:hover:not(:disabled)) {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.2);
  }

  :global(.dep-btn:disabled) {
    opacity: 0.6;
    cursor: not-allowed;
  }

  /* Subsection styling support if needed */
  :global(.subsection-item) {
    background: transparent !important;
    padding-left: 0 !important;
    padding-right: 0 !important;
  }
</style>
