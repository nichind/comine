let styleInjected = false;

function injectStyles() {
  if (styleInjected || typeof document === 'undefined') return;

  const style = document.createElement('style');
  style.id = 'skeleton-styles';
  style.textContent = `
    @keyframes skeleton-shimmer {
      0% { background-position: 200% 0; }
      100% { background-position: -200% 0; }
    }

    .skel-loading {
      position: relative;
      color: transparent !important;
      user-select: none;
      pointer-events: none;
      overflow: hidden;
      background: linear-gradient(
        90deg,
        var(--skel-base, rgba(255, 255, 255, 0.06)) 0%,
        var(--skel-base, rgba(255, 255, 255, 0.06)) 40%,
        var(--skel-highlight, rgba(255, 255, 255, 0.12)) 50%,
        var(--skel-base, rgba(255, 255, 255, 0.06)) 60%,
        var(--skel-base, rgba(255, 255, 255, 0.06)) 100%
      );
      background-size: 200% 100%;
      background-attachment: fixed;
      animation: skeleton-shimmer 2s ease-in-out infinite;
    }

    .skel-loading * {
      visibility: hidden !important;
    }

    .skel-loading img,
    .skel-loading svg,
    .skel-loading video,
    .skel-loading canvas {
      opacity: 0 !important;
    }

    .skel-reveal {
      animation: skel-fade-in 0.25s ease-out forwards;
    }

    .skel-reveal * {
      animation: skel-content-in 0.25s ease-out forwards;
    }

    @keyframes skel-fade-in {
      from { background-color: var(--skel-base, rgba(255, 255, 255, 0.06)); }
      to { background-color: transparent; }
    }

    @keyframes skel-content-in {
      from { opacity: 0; }
      to { opacity: 1; }
    }
  `;
  document.head.appendChild(style);
  styleInjected = true;
}

export interface SkeletonOptions {
  loading?: boolean;
  minWidth?: string;
  minHeight?: string;
  radius?: string;
  circle?: boolean;
  pill?: boolean;
  randomWidth?: boolean | [number, number];
}

type SkeletonParam = boolean | SkeletonOptions | undefined;

function normalizeOptions(param: SkeletonParam): SkeletonOptions {
  if (param === undefined || param === true) {
    return { loading: true };
  }
  if (param === false) {
    return { loading: false };
  }
  return param;
}

export function skeleton(node: HTMLElement, param?: SkeletonParam) {
  injectStyles();

  let opts = normalizeOptions(param);
  let wasLoading = false;
  let originalStyles: Partial<CSSStyleDeclaration> = {};

  let randomWidthPercent: string | undefined;

  function calculateRandomWidth() {
    const isTextTag = ['H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'P', 'LI'].includes(node.tagName);
    const shouldRandomize = opts.randomWidth !== false && (opts.randomWidth || isTextTag);

    if (shouldRandomize) {
      const [min, max] = Array.isArray(opts.randomWidth) ? opts.randomWidth : [60, 90];
      const percent = Math.floor(Math.random() * (max - min + 1) + min);
      randomWidthPercent = `${percent}%`;
    } else {
      randomWidthPercent = undefined;
    }
  }

  function saveOriginalStyles() {
    originalStyles = {
      minWidth: node.style.minWidth,
      minHeight: node.style.minHeight,
      borderRadius: node.style.borderRadius,
      width: node.style.width,
    };
  }

  function applyLoading() {
    node.classList.remove('skel-reveal');
    node.classList.add('skel-loading');

    if (opts.minWidth) node.style.minWidth = opts.minWidth;
    if (opts.minHeight) node.style.minHeight = opts.minHeight;
    if (opts.radius) node.style.borderRadius = opts.radius;
    if (opts.circle) node.style.borderRadius = '50%';
    if (opts.pill) node.style.borderRadius = '999px';

    if (randomWidthPercent) {
      node.style.width = randomWidthPercent;
    }

    wasLoading = true;
  }

  function removeLoading() {
    node.classList.remove('skel-loading');

    node.style.minWidth = originalStyles.minWidth || '';
    node.style.minHeight = originalStyles.minHeight || '';
    node.style.borderRadius = originalStyles.borderRadius || '';
    node.style.width = originalStyles.width || '';

    if (wasLoading) {
      node.classList.add('skel-reveal');

      const cleanup = () => {
        node.classList.remove('skel-reveal');
        node.removeEventListener('animationend', cleanup);
      };
      node.addEventListener('animationend', cleanup, { once: true });

      setTimeout(() => node.classList.remove('skel-reveal'), 300);
      wasLoading = false;
    }
  }

  function update() {
    if (opts.loading) {
      applyLoading();
    } else {
      removeLoading();
    }
  }

  calculateRandomWidth();
  saveOriginalStyles();
  update();

  return {
    update(newParam: SkeletonParam) {
      const oldLoading = opts.loading;
      opts = normalizeOptions(newParam);

      if (opts.loading !== oldLoading) {
        update();
      } else if (opts.loading) {
        applyLoading();
      }
    },
    destroy() {
      node.classList.remove('skel-loading', 'skel-reveal');
      node.style.minWidth = originalStyles.minWidth || '';
      node.style.minHeight = originalStyles.minHeight || '';
      node.style.borderRadius = originalStyles.borderRadius || '';
      node.style.width = originalStyles.width || '';
    },
  };
}
