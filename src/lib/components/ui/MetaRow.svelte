<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon, { type IconName } from '$lib/components/ui/Icon.svelte';

  interface Props {
    icon?: IconName;
    label: string;
    mono?: boolean;
    children?: Snippet;
  }

  let { icon, label, mono = false, children }: Props = $props();
</script>

<div class="meta-row">
  {#if icon}
    <Icon name={icon} size={14} />
  {/if}
  <span class="meta-label">{label}</span>
  <span class="meta-value" class:mono>
    {#if children}
      {@render children()}
    {/if}
  </span>
</div>

<style>
  .meta-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.6);
  }

  .meta-row :global(svg) {
    flex-shrink: 0;
  }

  .meta-label {
    color: rgba(255, 255, 255, 0.4);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    min-width: var(--meta-label-width, 70px);
    flex-shrink: 0;
  }

  .meta-value {
    color: var(--text-1, white);
    word-break: break-word;
  }

  .meta-value.mono {
    font-family: monospace;
    font-size: 11px;
  }
</style>
