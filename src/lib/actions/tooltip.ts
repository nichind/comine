let tooltipEl: HTMLDivElement | null = null;
let currentTarget: HTMLElement | null = null;
let showTimeout: ReturnType<typeof setTimeout> | null = null;
let hideTimeout: ReturnType<typeof setTimeout> | null = null;
let styleInjected = false;

function injectStyles() {
  if (styleInjected) return;
  const style = document.createElement('style');
  style.textContent = `
    .tooltip {
      position: fixed;
      z-index: 9999;
      padding: 8px 12px;
      background: rgba(24, 24, 26, 0.92);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      border: 1px solid rgba(255, 255, 255, 0.06);
      border-radius: 10px;
      color: rgba(255, 255, 255, 0.92);
      font-size: 12px;
      font-weight: 400;
      font-family: 'Jost', sans-serif;
      pointer-events: none;
      opacity: 0;
      transform: translateY(6px) scale(0.96);
      transition: opacity 0.2s ease-out, transform 0.2s ease-out;
      white-space: normal;
      word-wrap: break-word;
      max-width: min(280px, calc(100vw - 32px));
      text-align: center;
      box-shadow: 0 8px 32px rgba(0, 0, 0, 0.32), 0 2px 8px rgba(0, 0, 0, 0.16);
      will-change: opacity, transform;
    }
    .tooltip.visible {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  `;
  document.head.appendChild(style);
  styleInjected = true;
}

function createTooltip() {
  if (tooltipEl) return;
  injectStyles();
  tooltipEl = document.createElement('div');
  tooltipEl.className = 'tooltip';
  document.body.appendChild(tooltipEl);
}

function isElementInDOM(element: HTMLElement): boolean {
  return document.body.contains(element);
}

function showTooltip(target: HTMLElement, text: string) {
  if (!text) return;

  if (!isElementInDOM(target)) {
    hideTooltip();
    return;
  }

  createTooltip();
  if (!tooltipEl) return;

  currentTarget = target;
  tooltipEl.textContent = text;
  tooltipEl.classList.remove('visible');

  const rect = target.getBoundingClientRect();

  if (rect.width === 0 && rect.height === 0) {
    hideTooltip();
    return;
  }

  tooltipEl.style.top = '-9999px';
  tooltipEl.style.left = '-9999px';

  const tooltipRect = tooltipEl.getBoundingClientRect();

  let top = rect.bottom + 8;
  let left = rect.left + (rect.width - tooltipRect.width) / 2;

  if (top + tooltipRect.height > window.innerHeight - 8) {
    top = rect.top - tooltipRect.height - 8;
  }
  if (top < 8) {
    top = rect.bottom + 8;
  }

  if (left < 8) left = 8;
  if (left + tooltipRect.width > window.innerWidth - 8) {
    left = window.innerWidth - tooltipRect.width - 8;
  }

  tooltipEl.style.top = `${top}px`;
  tooltipEl.style.left = `${left}px`;

  requestAnimationFrame(() => {
    tooltipEl?.classList.add('visible');
  });
}

function hideTooltip() {
  if (!tooltipEl) return;
  tooltipEl.classList.remove('visible');
  currentTarget = null;
}

export function tooltip(node: HTMLElement, text?: string) {
  let tooltipText = text || node.getAttribute('title') || '';

  if (node.hasAttribute('title')) {
    node.removeAttribute('title');
  }

  function onEnter() {
    if (!isElementInDOM(node)) return;

    if (hideTimeout) {
      clearTimeout(hideTimeout);
      hideTimeout = null;
    }
    showTimeout = setTimeout(() => {
      if (isElementInDOM(node)) {
        showTooltip(node, tooltipText);
      }
    }, 200);
  }

  function onLeave() {
    if (showTimeout) {
      clearTimeout(showTimeout);
      showTimeout = null;
    }
    hideTimeout = setTimeout(hideTooltip, 100);
  }

  node.addEventListener('mouseenter', onEnter);
  node.addEventListener('mouseleave', onLeave);
  node.addEventListener('focus', onEnter);
  node.addEventListener('blur', onLeave);

  return {
    update(newText: string) {
      tooltipText = newText;
      if (currentTarget === node && tooltipEl) {
        tooltipEl.textContent = newText;
      }
    },
    destroy() {
      node.removeEventListener('mouseenter', onEnter);
      node.removeEventListener('mouseleave', onLeave);
      node.removeEventListener('focus', onEnter);
      node.removeEventListener('blur', onLeave);
      if (showTimeout) clearTimeout(showTimeout);
      if (hideTimeout) clearTimeout(hideTimeout);
      if (currentTarget === node) {
        hideTooltip();
      }
    },
  };
}
