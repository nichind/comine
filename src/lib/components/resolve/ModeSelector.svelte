<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { DownloadMode } from '$lib/stores/settings';

  interface Props {
    mode?: DownloadMode | null;
    disabled?: boolean;
    onchange?: (mode: DownloadMode) => void;
  }

  let { mode = $bindable(null), disabled = false, onchange }: Props = $props();

  function selectMode(m: DownloadMode) {
    mode = m;
    onchange?.(m);
  }
</script>

<div class="mode-selector" class:disabled>
  <button
    class="mode-btn"
    class:active={mode === 'auto' || mode === null}
    onclick={() => selectMode('auto')}
    title="Auto (Video + Audio)"
    {disabled}
  >
    <Icon name="download" size={14} />
  </button>
  <button
    class="mode-btn"
    class:active={mode === 'audio'}
    onclick={() => selectMode('audio')}
    title="Audio Only"
    {disabled}
  >
    <Icon name="music" size={14} />
  </button>
  <button
    class="mode-btn"
    class:active={mode === 'mute'}
    onclick={() => selectMode('mute')}
    title="Video Only (No Audio)"
    {disabled}
  >
    <Icon name="video2" size={14} />
  </button>
</div>

<style>
  .mode-selector {
    display: flex;
    gap: 2px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: var(--radius-sm, 6px);
    padding: 2px;
  }

  .mode-selector.disabled {
    opacity: 0.5;
    pointer-events: none;
  }

  .mode-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 4px);
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    transition: all 0.15s;
  }

  .mode-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-1, white);
  }

  .mode-btn.active {
    background: var(--accent, #6366f1);
    color: white;
  }

  .mode-btn:disabled {
    cursor: not-allowed;
  }
</style>
