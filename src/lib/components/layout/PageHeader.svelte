<script lang="ts">
  import type { Snippet } from 'svelte';
  import Divider from '$lib/components/ui/Divider.svelte';

  interface Props {
    title: string;
    subtitle?: string;
    actions?: Snippet;
    noDivider?: boolean;
  }

  let { title, subtitle, actions, noDivider = false }: Props = $props();
</script>

<div class="page-header">
  <div class="page-header-content">
    <div class="page-header-text">
      <h1>{title}</h1>
      {#if subtitle}
        <p class="page-subtitle">{subtitle}</p>
      {/if}
    </div>
    {#if actions}
      <div class="page-header-actions">
        {@render actions()}
      </div>
    {/if}
  </div>
  {#if !noDivider}
    <Divider my={20} />
  {/if}
</div>

<style>
  .page-header {
    flex-shrink: 0;
  }

  .page-header-content {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .page-header-text {
    flex: 1;
    min-width: 0;
  }

  .page-header h1 {
    font-size: 28px;
    font-weight: 700;
    margin: 0 0 6px 0;
  }

  .page-subtitle {
    color: rgba(255, 255, 255, 0.6);
    font-size: var(--text-md, 14px);
    margin: 0;
  }

  .page-header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
</style>
