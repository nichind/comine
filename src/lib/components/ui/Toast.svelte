<script lang="ts" module>
  import { writable, get } from 'svelte/store';

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
  const dismissTimers = new Map<number, ReturnType<typeof setTimeout>>();

  export const toasts = writable<Toast[]>([]);

  export function toast(
    message: string,
    type: ToastType = 'info',
    duration: number = 4000
  ): number {
    const id = ++idCounter;
    toasts.update((t) => [...t, { id, message, type, duration }]);

    if (duration > 0) {
      const timerId = setTimeout(() => dismissToast(id), duration);
      dismissTimers.set(id, timerId);
    }

    return id;
  }

  export function dismissToast(id: number): void {
    const timerId = dismissTimers.get(id);
    if (timerId !== undefined) {
      clearTimeout(timerId);
      dismissTimers.delete(id);
    }

    const currentToasts = get(toasts);
    if (!currentToasts.some((t) => t.id === id)) {
      return;
    }

    toasts.update((t) => t.filter((toast) => toast.id !== id));
  }

  export function updateToast(
    id: number,
    updates: Partial<
      Pick<Toast, 'message' | 'progress' | 'type' | 'subMessage' | 'actionLabel' | 'action'>
    >
  ): void {
    if (updates.progress !== undefined) {
      updates = { ...updates, progress: Math.max(0, Math.min(100, updates.progress)) };
    }
    toasts.update((t) => t.map((toast) => (toast.id === id ? { ...toast, ...updates } : toast)));
  }

  export function clearAllToasts(): void {
    dismissTimers.forEach((timerId) => clearTimeout(timerId));
    dismissTimers.clear();
    toasts.set([]);
  }

  toast.success = (msg: string, duration?: number): number => toast(msg, 'success', duration);
  toast.error = (msg: string, duration?: number): number => toast(msg, 'error', duration);
  toast.warning = (msg: string, duration?: number): number => toast(msg, 'warning', duration);
  toast.info = (msg: string, duration?: number): number => toast(msg, 'info', duration);
  toast.progress = (msg: string, progress: number = 0, subMessage?: string): number => {
    const id = ++idCounter;
    const clampedProgress = Math.max(0, Math.min(100, progress));
    toasts.update((t) => [
      ...t,
      { id, message: msg, type: 'progress', duration: 0, progress: clampedProgress, subMessage },
    ]);
    return id;
  };
  toast.loading = (msg: string, subMessage?: string): number => {
    const id = ++idCounter;
    toasts.update((t) => [...t, { id, message: msg, type: 'loading', duration: 0, subMessage }]);
    return id;
  };
</script>

<script lang="ts">
  import { flip } from 'svelte/animate';
  import { fly, fade } from 'svelte/transition';
  import { browser } from '$app/environment';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon, { type IconName } from '$lib/components/ui/Icon.svelte';
  import { settings, type ToastPosition } from '$lib/stores/settings';
  import { isMobile } from '$lib/utils/android';

  const iconMap: Record<ToastType, IconName> = {
    success: 'check',
    error: 'cross_circle',
    warning: 'warning',
    info: 'info',
    progress: 'download',
    loading: 'spinner',
  };

  let position = $derived<ToastPosition>(
    $settings.toastPosition || (browser && isMobile() ? 'top-right' : 'bottom-right')
  );

  let flyX = $derived(position.includes('left') ? -18 : position.includes('right') ? 18 : 0);
  let flyY = $derived(position.startsWith('top') ? -20 : 20);

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      const currentToasts = $toasts;
      if (currentToasts.length > 0) {
        const lastToast = currentToasts[currentToasts.length - 1];
        dismissToast(lastToast.id);
        event.preventDefault();
      }
    }
  }

  $effect(() => {
    if (!browser) return;

    window.addEventListener('keydown', handleKeydown);
    return () => {
      window.removeEventListener('keydown', handleKeydown);
    };
  });
</script>

<div
  class="toast-container"
  class:top={position.startsWith('top')}
  class:bottom={position.startsWith('bottom')}
  class:left={position.includes('left')}
  class:right={position.includes('right')}
  class:center={position.includes('center')}
  role="region"
  aria-label="Notifications"
  aria-live="polite"
  aria-atomic="false"
>
  {#each $toasts as t (t.id)}
    <div
      class="toast {t.type}"
      animate:flip={{ duration: 220 }}
      in:fly={{ x: flyX, y: flyY, duration: 220 }}
      out:fade={{ duration: 140 }}
      role="alert"
      aria-atomic="true"
    >
      <div class="toast-body">
        <span class="toast-icon" class:spinning={t.type === 'loading'} aria-hidden="true">
          <Icon name={iconMap[t.type]} size={15} />
        </span>
        <div class="toast-main">
          <div class="toast-header">
            <div class="toast-text">
              <span class="message">{t.message}</span>
              {#if t.subMessage}
                <span class="sub-message">{t.subMessage}</span>
              {/if}
            </div>

            <button
              class="dismiss"
              onclick={() => dismissToast(t.id)}
              aria-label="Dismiss notification"
            >
              <Icon name="cross" size={10} />
            </button>
          </div>

          {#if t.type === 'progress'}
            <div class="progress-block">
              <div
                class="progress-track"
                role="progressbar"
                aria-valuenow={t.progress ?? 0}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-label="{t.message} progress"
              >
                <div class="progress-fill" style="width: {t.progress ?? 0}%"></div>
              </div>
              <span class="progress-value">{Math.round(t.progress ?? 0)}%</span>
            </div>
          {/if}

          {#if t.actionLabel && t.action}
            <div class="toast-footer">
              <Button variant="ghost" size="sm" onclick={t.action}>{t.actionLabel}</Button>
            </div>
          {/if}
        </div>
      </div>
    </div>
  {/each}
</div>

<style>
  .toast-container {
    position: fixed;
    display: flex;
    flex-direction: column;
    gap: 12px;
    z-index: 2000;
    pointer-events: none;
  }

  .toast-container.bottom {
    bottom: 18px;
  }
  .toast-container.top {
    top: 18px;
  }
  .toast-container.right {
    right: 18px;
  }
  .toast-container.left {
    left: 18px;
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
  }

  .toast {
    --toast-accent: var(--accent, #7aa2ff);
    --toast-accent-soft: rgba(122, 162, 255, 0.14);
    --toast-accent-border: rgba(122, 162, 255, 0.26);
    --toast-accent-track: rgba(122, 162, 255, 0.2);

    position: relative;
    display: flex;
    flex-direction: column;
    width: min(380px, calc(100vw - 24px));
    background: rgba(14, 16, 21, 0.96);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 16px;
    box-shadow:
      0 18px 44px rgba(0, 0, 0, 0.34),
      0 4px 14px rgba(0, 0, 0, 0.18);
    pointer-events: auto;
    overflow: hidden;
  }

  .toast-body {
    display: flex;
    align-items: flex-start;
    gap: 11px;
    padding: 14px;
  }

  .toast-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: auto;
    height: auto;
    margin-top: 2px;
    color: var(--toast-accent);
    flex-shrink: 0;
    opacity: 0.95;
  }

  .toast.success {
    --toast-accent: #4fd18b;
    --toast-accent-soft: rgba(79, 209, 139, 0.14);
    --toast-accent-border: rgba(79, 209, 139, 0.26);
    --toast-accent-track: rgba(79, 209, 139, 0.2);
  }

  .toast.error {
    --toast-accent: #ff7a7a;
    --toast-accent-soft: rgba(255, 122, 122, 0.14);
    --toast-accent-border: rgba(255, 122, 122, 0.26);
    --toast-accent-track: rgba(255, 122, 122, 0.2);
  }

  .toast.warning {
    --toast-accent: #f3b34c;
    --toast-accent-soft: rgba(243, 179, 76, 0.14);
    --toast-accent-border: rgba(243, 179, 76, 0.26);
    --toast-accent-track: rgba(243, 179, 76, 0.2);
  }

  .toast.info,
  .toast.progress,
  .toast.loading {
    --toast-accent: var(--accent, #7aa2ff);
    --toast-accent-soft: rgba(122, 162, 255, 0.14);
    --toast-accent-border: rgba(122, 162, 255, 0.26);
    --toast-accent-track: rgba(122, 162, 255, 0.2);
  }

  .spinning {
    animation: spin 0.8s linear infinite;
    will-change: transform;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .toast-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .toast-header {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .toast-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .message {
    font-size: 13px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.94);
    line-height: 1.35;
    letter-spacing: 0.01em;
    word-break: break-word;
  }

  .sub-message {
    font-size: 12px;
    font-weight: 400;
    color: rgba(255, 255, 255, 0.58);
    line-height: 1.42;
    word-break: break-word;
  }

  .dismiss {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    margin: 0;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: 999px;
    color: rgba(255, 255, 255, 0.34);
    cursor: pointer;
    transition:
      background 0.15s ease,
      color 0.15s ease,
      opacity 0.15s ease;
    flex-shrink: 0;
    opacity: 0.9;
  }

  .dismiss:hover {
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.72);
    opacity: 1;
  }

  .dismiss:focus-visible {
    outline: 2px solid var(--accent, #6366f1);
    outline-offset: 1px;
  }

  .toast-footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .toast-footer :global(.btn) {
    align-self: flex-start;
    min-height: 28px;
    padding: 0 8px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
  }

  .toast-footer :global(.btn.ghost) {
    background: transparent;
    color: var(--toast-accent);
    box-shadow: none;
  }

  .toast-footer :global(.btn.ghost:hover:not(:disabled)) {
    background: rgba(255, 255, 255, 0.04);
    color: color-mix(in srgb, var(--toast-accent) 82%, white 18%);
    filter: none;
  }

  .toast-footer :global(.btn.ghost:focus-visible) {
    outline: 2px solid color-mix(in srgb, var(--toast-accent) 72%, white 28%);
    outline-offset: 2px;
  }

  .progress-block {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .progress-track {
    flex: 1;
    height: 6px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 999px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    border-radius: inherit;
    background: var(--toast-accent);
    transition: width 0.25s ease-out;
  }

  .progress-value {
    min-width: 36px;
    font-size: 11px;
    font-weight: 600;
    line-height: 1;
    text-align: right;
    color: rgba(255, 255, 255, 0.48);
    font-variant-numeric: tabular-nums;
  }
</style>
