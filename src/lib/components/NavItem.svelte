<script lang="ts">
  import { spotlight } from '$lib/actions/spotlight';
  import { tooltip } from '$lib/actions/tooltip';
  import Icon, { type IconName } from './Icon.svelte';

  interface Props {
    href: string;
    icon: IconName;
    title?: string;
    active?: boolean;
    badge?: number;
    external?: boolean;
    register?: (node: HTMLElement) => void | { destroy?: () => void };
  }

  let {
    href,
    icon,
    title = '',
    active = false,
    badge,
    external = false,
    register,
  }: Props = $props();

  function registerAction(node: HTMLElement) {
    const result = register?.(node);
    if (result && typeof result === 'object') return result;
    return {};
  }
</script>

{#if external}
  <a
    {href}
    target="_blank"
    rel="noopener noreferrer"
    class="nav-item external"
    class:telegram={icon === 'telegram'}
    class:discord={icon === 'discord'}
    class:github={icon === 'github'}
    use:spotlight
    use:tooltip={title}
    use:registerAction
    data-tauri-drag-region="false"
  >
    <Icon name={icon} />
  </a>
{:else}
  <a
    {href}
    class="nav-item"
    class:active
    use:spotlight
    use:tooltip={title}
    use:registerAction
    data-tauri-drag-region="false"
  >
    <Icon name={icon} />
    {#if badge}
      <span class="badge">{badge}</span>
    {/if}
  </a>
{/if}

<style>
  .nav-item {
    width: 56px;
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.5);
    border-radius: 0 8px 8px 0;
    text-decoration: none;
    transition:
      color 0.15s ease,
      background 0.15s ease;
    position: relative;
    z-index: 1;
    border-left: 2px solid transparent;
  }

  .nav-item:hover {
    color: rgba(255, 255, 255, 0.8);
  }

  .nav-item.external.telegram:hover {
    color: #26a5e4;
    background: rgba(38, 165, 228, 0.15);
  }

  .nav-item.external.discord:hover {
    color: #5865f2;
    background: rgba(88, 101, 242, 0.15);
  }

  .nav-item.external.github:hover {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.12);
  }

  .nav-item.active {
    color: #ffffff;
    background: transparent;
    border-left-color: transparent;
  }

  .badge {
    position: absolute;
    bottom: 8px;
    right: 8px;
    background: var(--accent, #6366f1);
    color: white;
    font-size: 10px;
    font-weight: 700;
    min-width: 16px;
    height: 16px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 4px;
  }
</style>
