<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { IconName } from '$lib/components/ui/Icon.svelte';
  import type { Snippet } from 'svelte';

  let {
    selected = false,
    icon,
    onclick,
    children,
  }: {
    selected?: boolean;
    icon?: IconName;
    onclick?: () => void;
    children?: Snippet;
  } = $props();
</script>

<button class="chip" class:selected {onclick}>
  {#if icon}
    <Icon name={icon} size={16} />
  {/if}
  {@render children?.()}
</button>

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 36px;
    padding: 0 14px;
    font-size: var(--text-base, 13px);
    font-weight: 500;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 10px);
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
    box-sizing: border-box;
  }

  .chip:hover {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.15);
    color: rgba(255, 255, 255, 0.9);
  }

  .chip.selected {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.2));
    border-color: var(--accent-alpha-hover, rgba(99, 102, 241, 0.4));
    color: rgba(255, 255, 255, 0.95);
  }

  .chip.selected:hover {
    background: var(--accent-alpha-hover, rgba(99, 102, 241, 0.3));
  }
</style>
