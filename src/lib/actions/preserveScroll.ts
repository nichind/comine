export interface PreserveScrollOptions {
  key: string;
  throttleMs?: number;
  observeTarget?: HTMLElement;
}

const DEFAULT_THROTTLE = 300;
const MAX_RESTORE_ATTEMPTS = 60;
const RESTORE_INTERVAL = 50;

function safeGet(key: string): number | null {
  try {
    const raw = sessionStorage.getItem(key);
    if (!raw) return null;
    const value = Number(raw);
    return Number.isFinite(value) ? value : null;
  } catch {
    return null;
  }
}

function safeSet(key: string, value: number): void {
  try {
    sessionStorage.setItem(key, String(value));
  } catch {}
}

export function preserveScroll(
  node: HTMLElement,
  options?: PreserveScrollOptions
): { update: (next?: PreserveScrollOptions) => void; destroy: () => void } {
  let key = options?.key ?? '';
  let throttleMs = options?.throttleMs ?? DEFAULT_THROTTLE;
  let observeTarget = options?.observeTarget;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let restoreTimer: ReturnType<typeof setInterval> | null = null;
  let hasRestored = false;
  let resizeObserver: ResizeObserver | null = null;
  let lastKnownScrollTop = 0;

  function saveNow(): void {
    if (!key) return;
    if (lastKnownScrollTop > 0) {
      safeSet(key, lastKnownScrollTop);
    }
  }

  function onScroll(): void {
    if (!key) return;
    lastKnownScrollTop = node.scrollTop;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(saveNow, throttleMs);
  }

  function tryRestore(): boolean {
    if (!key) return true;
    const saved = safeGet(key);
    if (saved === null || saved === 0) return true;

    const maxScroll = Math.max(0, node.scrollHeight - node.clientHeight);
    if (maxScroll < saved) return false;

    node.scrollTop = saved;
    hasRestored = true;
    return true;
  }

  function startRestoreLoop(): void {
    if (!key || hasRestored) return;
    if (restoreTimer !== null) clearInterval(restoreTimer);

    let attempts = 0;
    restoreTimer = setInterval(() => {
      attempts++;
      if (tryRestore() || attempts >= MAX_RESTORE_ATTEMPTS) {
        if (restoreTimer !== null) {
          clearInterval(restoreTimer);
          restoreTimer = null;
        }
      }
    }, RESTORE_INTERVAL);
  }

  function onResize(): void {
    if (!hasRestored) {
      tryRestore();
    }
  }

  node.addEventListener('scroll', onScroll, { passive: true });

  if (typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver(onResize);
    resizeObserver.observe(node);
  }

  startRestoreLoop();

  return {
    update(next?: PreserveScrollOptions) {
      const newKey = next?.key ?? '';
      if (newKey !== key) {
        hasRestored = false;
        key = newKey;
        startRestoreLoop();
      }
      throttleMs = next?.throttleMs ?? DEFAULT_THROTTLE;
      if (next?.observeTarget && next.observeTarget !== observeTarget) {
        observeTarget = next.observeTarget;
        resizeObserver?.observe(observeTarget);
      }
    },
    destroy() {
      if (saveTimer) clearTimeout(saveTimer);
      if (restoreTimer !== null) clearInterval(restoreTimer);
      saveNow();
      node.removeEventListener('scroll', onScroll);
      resizeObserver?.disconnect();
    },
  };
}
