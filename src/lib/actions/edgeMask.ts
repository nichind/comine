export interface EdgeMaskOptions {
  size?: number;
  enabled?: boolean;
}

const DEFAULT_SIZE = 16;

const GRAD =
  'linear-gradient(to bottom, transparent 0px, black var(--em-t), black calc(100% - var(--em-b)), transparent 100%)';

export function edgeMask(
  node: HTMLElement,
  options?: EdgeMaskOptions
): { update: (next?: EdgeMaskOptions) => void; destroy: () => void } {
  let size = options?.size ?? DEFAULT_SIZE;
  let enabled = options?.enabled ?? true;

  let raf: number | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let mutationObserver: MutationObserver | null = null;
  let gradientApplied = false;
  let prevT = -1;
  let prevB = -1;
  let alive = true;

  function computeAndApply() {
    if (!alive || !enabled) return;

    const { scrollTop, scrollHeight, clientHeight } = node;
    const maxScroll = scrollHeight - clientHeight;

    if (maxScroll <= 1) {
      if (gradientApplied) {
        node.style.removeProperty('mask-image');
        node.style.removeProperty('-webkit-mask-image');
        gradientApplied = false;
      }
      if (prevT !== 0 || prevB !== 0) {
        node.style.setProperty('--em-t', '0px');
        node.style.setProperty('--em-b', '0px');
        prevT = 0;
        prevB = 0;
      }
      return;
    }

    if (!gradientApplied) {
      node.style.setProperty('mask-image', GRAD);
      node.style.setProperty('-webkit-mask-image', GRAD);
      gradientApplied = true;
    }

    const t = scrollTop > 0 ? Math.min(scrollTop, size) : 0;
    const b = scrollTop < maxScroll ? Math.min(maxScroll - scrollTop, size) : 0;

    if (t !== prevT) {
      node.style.setProperty('--em-t', t === size ? `${size}px` : `${t}px`);
      prevT = t;
    }
    if (b !== prevB) {
      node.style.setProperty('--em-b', b === size ? `${size}px` : `${b}px`);
      prevB = b;
    }
  }

  function scheduleUpdate() {
    if (!alive || raf !== null) return;
    raf = requestAnimationFrame(() => {
      raf = null;
      computeAndApply();
    });
  }

  function syncUpdate() {
    if (!alive) return;
    if (raf !== null) {
      cancelAnimationFrame(raf);
      raf = null;
    }
    computeAndApply();
  }

  function onScroll() {
    scheduleUpdate();
  }

  function startObservers() {
    if (typeof ResizeObserver !== 'undefined') {
      resizeObserver = new ResizeObserver(syncUpdate);
      resizeObserver.observe(node);
    }

    if (typeof MutationObserver !== 'undefined') {
      mutationObserver = new MutationObserver(scheduleUpdate);
      mutationObserver.observe(node, { childList: true, subtree: true });
    }
  }

  function stopObservers() {
    resizeObserver?.disconnect();
    resizeObserver = null;
    mutationObserver?.disconnect();
    mutationObserver = null;
  }

  function init() {
    if (!enabled) return;

    node.addEventListener('scroll', onScroll, { passive: true });
    startObservers();

    computeAndApply();

    queueMicrotask(() => {
      if (!alive) return;
      computeAndApply();
    });

    requestAnimationFrame(() => {
      if (!alive) return;
      computeAndApply();
    });
  }

  function clearStyles() {
    node.style.removeProperty('mask-image');
    node.style.removeProperty('-webkit-mask-image');
    node.style.removeProperty('--em-t');
    node.style.removeProperty('--em-b');
    gradientApplied = false;
    prevT = -1;
    prevB = -1;
  }

  function teardown() {
    alive = false;
    node.removeEventListener('scroll', onScroll);
    if (raf !== null) {
      cancelAnimationFrame(raf);
      raf = null;
    }
    stopObservers();
    clearStyles();
  }

  init();

  return {
    update(next?: EdgeMaskOptions) {
      const newSize = next?.size ?? DEFAULT_SIZE;
      const newEnabled = next?.enabled ?? true;
      const sizeChanged = newSize !== size;
      const enabledChanged = newEnabled !== enabled;

      if (!sizeChanged && !enabledChanged) return;

      size = newSize;
      enabled = newEnabled;

      if (!enabled) {
        node.removeEventListener('scroll', onScroll);
        stopObservers();
        if (raf !== null) {
          cancelAnimationFrame(raf);
          raf = null;
        }
        clearStyles();
      } else if (enabledChanged) {
        alive = true;
        node.addEventListener('scroll', onScroll, { passive: true });
        startObservers();
        computeAndApply();
        queueMicrotask(() => alive && computeAndApply());
        requestAnimationFrame(() => alive && computeAndApply());
      } else {
        prevT = -1;
        prevB = -1;
        computeAndApply();
      }
    },
    destroy() {
      teardown();
    },
  };
}
