<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { invoke } from '@tauri-apps/api/core';

  interface Props {
    value: string;
    pickType: 'file' | 'folder';
    disabled?: boolean;
    onchange?: (value: string) => void;
  }

  let { value = $bindable(), pickType, disabled = false, onchange }: Props = $props();

  async function pick() {
    if (disabled) return;

    try {
      let selected: string | null = null;

      if (pickType === 'folder') {
        selected = await invoke<string | null>('pick_folder');
      } else {
        const result = await open({
          directory: false,
          multiple: false,
        });
        if (result && typeof result === 'string') {
          selected = result;
        }
      }

      if (selected) {
        value = selected;
        onchange?.(selected);
      }
    } catch (err) {
      console.error('Failed to pick path:', err);
    }
  }
</script>

<button class="path-btn" onclick={pick} {disabled} title={value}>
  <Icon name="folder" size={16} />
  <span class="path-text">{value || 'Select...'}</span>
</button>

<style>
  .path-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.8);
    font-size: 13px;
    cursor: pointer;
    transition: all 0.15s;
    max-width: 280px;
  }

  .path-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.2);
    color: white;
  }

  .path-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .path-text {
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 220px;
  }
</style>
