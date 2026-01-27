<script lang="ts" module>
  import { writable } from 'svelte/store';

  export type ToastType = 'success' | 'error' | 'warning' | 'info' | 'progress' | 'loading';

  export interface Toast {
    id: number;
    message: string;
    type: ToastType;
    duration: number;
    progress?: number;
    subMessage?: string;
    actionLabel?: string;
    action?: () => void;
  }

  let idCounter = 0;
  export const toasts = writable<Toast[]>([]);

  export function toast(message: string, type: ToastType = 'info', duration = 4000) {
    const id = ++idCounter;
    toasts.update((t) => [...t, { id, message, type, duration }]);

    if (duration > 0) {
      setTimeout(() => dismissToast(id), duration);
    }

    return id;
  }

  export function dismissToast(id: number) {
    toasts.update((t) => t.filter((toast) => toast.id !== id));
  }

  export function updateToast(
    id: number,
    updates: Partial<
      Pick<Toast, 'message' | 'progress' | 'type' | 'subMessage' | 'actionLabel' | 'action'>
    >
  ) {
    toasts.update((t) => t.map((toast) => (toast.id === id ? { ...toast, ...updates } : toast)));
  }

  toast.success = (msg: string, duration?: number) => toast(msg, 'success', duration);
  toast.error = (msg: string, duration?: number) => toast(msg, 'error', duration);
  toast.warning = (msg: string, duration?: number) => toast(msg, 'warning', duration);
  toast.info = (msg: string, duration?: number) => toast(msg, 'info', duration);
  toast.progress = (msg: string, progress: number = 0, subMessage?: string) => {
    const id = ++idCounter;
    toasts.update((t) => [
      ...t,
      { id, message: msg, type: 'progress', duration: 0, progress, subMessage },
    ]);
    return id;
  };
  toast.loading = (msg: string, subMessage?: string) => {
    const id = ++idCounter;
    toasts.update((t) => [...t, { id, message: msg, type: 'loading', duration: 0, subMessage }]);
    return id;
  };
</script>

<script lang="ts">
  import { flip } from 'svelte/animate';
  import { fly, fade } from 'svelte/transition';
  import Icon, { type IconName } from './Icon.svelte';
  import { settings, type ToastPosition } from '$lib/stores/settings';
  import { isAndroid } from '$lib/utils/android';

  const iconMap: Record<ToastType, IconName> = {
    success: 'check',
    error: 'cross_circle',
    warning: 'warning',
    info: 'info',
    progress: 'download',
    loading: 'spinner',
  };

  let position = $derived<ToastPosition>(
    $settings.toastPosition || (isAndroid() ? 'top-right' : 'bottom-right')
  );

  let flyY = $derived(position.startsWith('top') ? -20 : 20);
</script>

<div
  class="toast-container"
  class:top={position.startsWith('top')}
  class:bottom={position.startsWith('bottom')}
  class:left={position.includes('left')}
  class:right={position.includes('right')}
  class:center={position.includes('center')}
>
  {#each $toasts as t (t.id)}
    <div
      class="toast {t.type}"
      animate:flip={{ duration: 200 }}
      in:fly={{ y: flyY, duration: 200 }}
      out:fade={{ duration: 150 }}
      role="alert"
    >
      <div class="toast-body">
        <span class="toast-icon" class:spinning={t.type === 'loading'}>
          <Icon name={iconMap[t.type]} size={15} />
        </span>
        <div class="toast-text">
          <span class="message">{t.message}</span>
          {#if t.subMessage}
            <span class="sub-message">{t.subMessage}</span>
          {/if}
        </div>
        {#if t.actionLabel && t.action}
          <button class="action" onclick={t.action}>{t.actionLabel}</button>
        {/if}
        <button class="dismiss" onclick={() => dismissToast(t.id)} aria-label="Dismiss">
          <Icon name="cross" size={12} />
        </button>
      </div>
      {#if t.type === 'progress'}
        <div class="progress-track">
          <div class="progress-fill" style="width: {t.progress ?? 0}%"></div>
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .toast-container {
    position: fixed;
    display: flex;
    flex-direction: column;
    gap: 10px;
    z-index: 2000;
    pointer-events: none;
  }

  .toast-container.bottom {
    bottom: 16px;
  }
  .toast-container.top {
    top: 16px;
  }
  .toast-container.right {
    right: 16px;
  }
  .toast-container.left {
    left: 16px;
  }
  .toast-container.center {
    left: 50%;
    transform: translateX(-50%);
  }

  @media (max-width: 480px) {
    .toast-container.right {
      right: 12px;
      left: unset;
    }
    .toast-container.left {
      left: 12px;
      right: unset;
    }
    .toast-container.center {
      left: 12px;
      right: 12px;
      transform: none;
    }
    .toast-container.bottom {
      bottom: calc(80px + env(safe-area-inset-bottom, 0px));
    }
    .toast-container.top {
      top: calc(env(safe-area-inset-top, 0px) + 12px);
    }
    .toast {
      min-width: unset;
      max-width: unset;
    }
  }

  .toast {
    display: flex;
    flex-direction: column;
    background: var(--surface-bg, rgba(18, 18, 18, 0.92));
    backdrop-filter: blur(var(--surface-blur, 16px));
    -webkit-backdrop-filter: blur(var(--surface-blur, 16px));
    border: 1px solid var(--surface-border, rgba(255, 255, 255, 0.06));
    border-radius: var(--radius-lg, 12px);
    box-shadow: var(
      --surface-shadow,
      0 8px 32px rgba(0, 0, 0, 0.32),
      0 2px 8px rgba(0, 0, 0, 0.16)
    );
    pointer-events: auto;
    overflow: hidden;
    min-width: 280px;
    max-width: 380px;
  }

  .toast-body {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 14px 16px;
  }

  .toast-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .toast.success .toast-icon {
    color: #4ade80;
  }
  .toast.error .toast-icon {
    color: #f87171;
  }
  .toast.warning .toast-icon {
    color: #fbbf24;
  }
  .toast.info .toast-icon {
    color: var(--accent, #818cf8);
  }
  .toast.progress .toast-icon {
    color: var(--accent, #818cf8);
  }
  .toast.loading .toast-icon {
    color: var(--accent, #818cf8);
  }

  .spinning {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .toast-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .message {
    font-size: 13px;
    font-weight: 500;
    color: var(--surface-text, rgba(255, 255, 255, 0.92));
    line-height: 1.4;
  }

  .sub-message {
    font-size: 12px;
    font-weight: 400;
    color: var(--surface-text-muted, rgba(255, 255, 255, 0.5));
    line-height: 1.35;
    word-break: break-word;
  }

  .dismiss {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    margin: -2px -4px -2px 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: var(--surface-text-muted, rgba(255, 255, 255, 0.3));
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .action {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 22px;
    padding: 0 10px;
    margin: -2px 0;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-sm, 6px);
    color: var(--surface-text, rgba(255, 255, 255, 0.8));
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
    flex-shrink: 0;
  }

  .action:hover {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.14);
  }

  .dismiss:hover {
    background: var(--surface-bg-hover, rgba(255, 255, 255, 0.08));
    color: var(--surface-text, rgba(255, 255, 255, 0.7));
  }

  .progress-track {
    height: 3px;
    background: var(--surface-border, rgba(255, 255, 255, 0.06));
  }

  .progress-fill {
    height: 100%;
    background: var(--accent, #6366f1);
    transition: width 0.25s ease-out;
  }
</style>
