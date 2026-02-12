<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    floating?: boolean;
    class?: string;
    children?: Snippet;
  }

  let { floating = false, class: className = '', children }: Props = $props();
</script>

<div class="page-toolbar {className}" class:floating>
  {@render children?.()}
</div>

<style>
  .page-toolbar {
    flex-shrink: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .page-toolbar.floating {
    position: absolute;
    bottom: 8px;
    left: 8px;
    right: 8px;
    z-index: 100;
    background: rgba(30, 30, 35, 0.85);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-lg, 12px);
    padding: 8px;
  }

  :global(.app.mobile) .page-toolbar.floating {
    bottom: 96px;
  }
</style>
