let styleInjected = false;

function injectStyles() {
  if (styleInjected) return;

  const style = document.createElement('style');
  style.id = 'skeleton-loader-styles';
  style.textContent = `
    @keyframes skeletonShimmer {
      from { background-position: 200% 0; }
      to { background-position: -200% 0; }
    }

    .skeleton-loading {
      color: transparent !important;
      background: linear-gradient(
        90deg,
        rgba(255, 255, 255, 0.06) 0%,
        rgba(255, 255, 255, 0.15) 50%,
        rgba(255, 255, 255, 0.06) 100%
      ) !important;
      background-size: 200% 100% !important;
      pointer-events: none;
      user-select: none;
      animation: skeletonShimmer 1.5s ease-in-out infinite !important;
    }

    .skeleton-loading * {
      opacity: 0 !important;
      transition: opacity 0.2s ease-out !important;
    }

    .skeleton-loading img,
    .skeleton-loading svg,
    .skeleton-loading video {
      opacity: 0 !important;
    }

    /* Smooth transition when skeleton is removed */
    .skeleton-loaded {
      animation: skeletonFadeIn 0.3s ease-out forwards !important;
    }

    .skeleton-loaded * {
      animation: skeletonContentFadeIn 0.3s ease-out forwards !important;
    }

    @keyframes skeletonFadeIn {
      from {
        background: linear-gradient(
          90deg,
          rgba(255, 255, 255, 0.06) 0%,
          rgba(255, 255, 255, 0.15) 50%,
          rgba(255, 255, 255, 0.06) 100%
        );
      }
      to {
        background: transparent;
      }
    }

    @keyframes skeletonContentFadeIn {
      from { opacity: 0; }
      to { opacity: 1; }
    }

    /* Variant: pill shape (only applies if explicitly set) */
    .skeleton-loading.skeleton-pill {
      border-radius: 999px !important;
    }

    /* Variant: circle shape (only applies if explicitly set) */
    .skeleton-loading.skeleton-circle {
      border-radius: 50% !important;
    }

    /* Variant: no animation (static) */
    .skeleton-loading.skeleton-static {
      animation: none !important;
    }
  `;
  document.head.appendChild(style);
  styleInjected = true;
}

export interface SkeletonOptions {
  loading?: boolean;
  width?: string;
  height?: string;
  radius?: string;
  shape?: 'default' | 'pill' | 'circle';
  static?: boolean;
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

function applyStyles(node: HTMLElement, options: SkeletonOptions) {
  if (options.width) {
    node.style.width = options.width;
  }
  if (options.height) {
    node.style.height = options.height;
  }
  if (options.radius) {
    node.style.borderRadius = options.radius;
  }
}

function removeStyles(node: HTMLElement, options: SkeletonOptions) {
  if (options.width) {
    node.style.removeProperty('width');
  }
  if (options.height) {
    node.style.removeProperty('height');
  }
  if (options.radius) {
    node.style.removeProperty('border-radius');
  }
}

export function skeleton(node: HTMLElement, param?: SkeletonParam) {
  injectStyles();

  let currentOptions = normalizeOptions(param);
  let wasLoading = false;

  function updateState(options: SkeletonOptions) {
    if (options.loading) {
      node.classList.remove('skeleton-loaded');
      node.classList.add('skeleton-loading');

      node.classList.remove('skeleton-pill', 'skeleton-circle', 'skeleton-static');
      if (options.shape === 'pill') {
        node.classList.add('skeleton-pill');
      } else if (options.shape === 'circle') {
        node.classList.add('skeleton-circle');
      }
      if (options.static) {
        node.classList.add('skeleton-static');
      }

      applyStyles(node, options);
      wasLoading = true;
    } else {
      node.classList.remove(
        'skeleton-loading',
        'skeleton-pill',
        'skeleton-circle',
        'skeleton-static'
      );
      removeStyles(node, currentOptions);

      if (wasLoading) {
        node.classList.add('skeleton-loaded');
        setTimeout(() => {
          node.classList.remove('skeleton-loaded');
        }, 300);
        wasLoading = false;
      }
    }
  }

  updateState(currentOptions);

  return {
    update(newParam: SkeletonParam) {
      removeStyles(node, currentOptions);
      currentOptions = normalizeOptions(newParam);
      updateState(currentOptions);
    },
    destroy() {
      node.classList.remove(
        'skeleton-loading',
        'skeleton-loaded',
        'skeleton-pill',
        'skeleton-circle',
        'skeleton-static'
      );
      removeStyles(node, currentOptions);
    },
  };
}

export function skeletonPlaceholder(
  node: HTMLElement,
  options: Omit<SkeletonOptions, 'loading'> = {}
) {
  return skeleton(node, { ...options, loading: true });
}
