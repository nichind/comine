<script lang="ts" module>
  export interface ToggleProps {
    checked?: boolean;
    disabled?: boolean;
    label?: string;
    'aria-label'?: string;
    onchange?: (checked: boolean) => void;
  }
</script>

<script lang="ts">
  let {
    checked = $bindable(false),
    disabled = false,
    label = '',
    'aria-label': ariaLabel,
    onchange,
  }: ToggleProps = $props();

  function handleClick() {
    if (disabled) return;
    const newValue = !checked;
    checked = newValue;
    onchange?.(newValue);
  }
</script>

<button
  type="button"
  role="switch"
  class="toggle-wrapper"
  class:disabled
  class:checked
  aria-checked={checked}
  aria-label={label || ariaLabel || undefined}
  onclick={handleClick}
  data-setting-activator="true"
  {disabled}
>
  {#if label}
    <span class="label" aria-hidden="true">{label}</span>
  {/if}
  <span class="toggle" aria-hidden="true">
    <span class="slider">
      <span class="indicator"></span>
    </span>
  </span>
</button>

<style>
  .toggle-wrapper {
    display: inline-flex;
    align-items: center;
    gap: 12px;
    cursor: pointer;
    user-select: none;
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: inherit;
  }

  .toggle-wrapper.disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .toggle {
    --toggle-width: 44px;
    --toggle-height: 24px;
    --thumb-size: 18px;
    --thumb-offset: 3px;

    position: relative;
    width: var(--toggle-width);
    height: var(--toggle-height);
    background: rgba(255, 255, 255, 0.15);
    border-radius: calc(var(--toggle-height) / 2);
    transition: background 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .slider {
    position: absolute;
    top: var(--thumb-offset);
    left: var(--thumb-offset);
    width: var(--thumb-size);
    height: var(--thumb-size);
    background: white;
    border-radius: 50%;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    transition:
      transform 0.25s cubic-bezier(0.34, 1.56, 0.64, 1),
      box-shadow 0.2s ease;
    will-change: transform;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .indicator {
    width: 6px;
    height: 6px;
    background: rgba(0, 0, 0, 0.25);
    border-radius: 50%;
    transition:
      width 0.2s cubic-bezier(0.4, 0, 0.2, 1),
      height 0.2s cubic-bezier(0.4, 0, 0.2, 1),
      border-radius 0.2s cubic-bezier(0.4, 0, 0.2, 1),
      background 0.2s ease;
  }

  .toggle-wrapper.checked .indicator {
    width: 2px;
    height: 8px;
    border-radius: 1px;
    background: var(--accent, #6366f1);
  }

  .toggle-wrapper.checked .toggle {
    background: var(--accent, #6366f1);
  }

  :global(.accent-gradient) .toggle-wrapper.checked .toggle,
  :global(.accent-glow) .toggle-wrapper.checked .toggle {
    background: var(--accent-gradient, var(--accent, #6366f1));
    background-size: 200% 200%;
    animation: toggle-shift 3s ease 1;
  }

  @keyframes toggle-shift {
    0%,
    100% {
      background-position: 0% 50%;
    }
    50% {
      background-position: 100% 50%;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.accent-gradient) .toggle-wrapper.checked .toggle,
    :global(.accent-glow) .toggle-wrapper.checked .toggle {
      animation: none;
    }

    .slider {
      transition:
        transform 0.15s ease,
        box-shadow 0.15s ease;
    }

    .indicator {
      transition: none;
    }
  }

  .toggle-wrapper.checked .slider {
    transform: translateX(calc(var(--toggle-width) - var(--thumb-size) - var(--thumb-offset) * 2));
  }

  .toggle-wrapper:not(.disabled):hover .slider {
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.2),
      0 0 0 4px var(--accent-alpha, rgba(99, 102, 241, 0.15));
  }

  .toggle-wrapper:not(.disabled):active .slider {
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.2),
      0 0 0 6px var(--accent-alpha, rgba(99, 102, 241, 0.2));
    transform: scale(0.95);
  }

  .toggle-wrapper.checked:not(.disabled):active .slider {
    transform: translateX(calc(var(--toggle-width) - var(--thumb-size) - var(--thumb-offset) * 2))
      scale(0.95);
  }

  .toggle-wrapper:focus-visible .slider {
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.2),
      0 0 0 2px var(--accent, #6366f1),
      0 0 0 4px var(--accent-alpha, rgba(99, 102, 241, 0.4));
  }

  .toggle-wrapper:focus {
    outline: none;
  }

  @media (max-width: 640px) {
    .toggle {
      --toggle-width: 52px;
      --toggle-height: 28px;
      --thumb-size: 22px;
    }

    .indicator {
      width: 7px;
      height: 7px;
    }

    .toggle-wrapper.checked .indicator {
      width: 2.5px;
      height: 9px;
    }
  }

  @media (pointer: coarse) {
    .toggle {
      --toggle-width: 52px;
      --toggle-height: 28px;
      --thumb-size: 22px;
    }
  }

  .label {
    color: rgba(255, 255, 255, 0.9);
    font-size: var(--text-md, 14px);
  }
</style>
