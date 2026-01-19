<script lang="ts">
  interface Props {
    checked?: boolean;
    disabled?: boolean;
    label?: string;
    onchange?: (checked: boolean) => void;
  }

  let { checked = $bindable(false), disabled = false, label = '', onchange }: Props = $props();

  function toggle() {
    if (disabled) return;
    const newValue = !checked;
    checked = newValue;
    onchange?.(newValue);
  }
</script>

<button
  type="button"
  class="toggle-wrapper"
  class:disabled
  class:checked
  onclick={toggle}
  data-setting-activator="true"
  {disabled}
>
  {#if label}
    <span class="label">{label}</span>
  {/if}
  <span class="toggle">
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
    position: relative;
    width: 36px;
    height: 20px;
    background: rgba(255, 255, 255, 0.2);
    border-radius: 10px;
    transition: all 0.2s;
  }

  .slider {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    background: white;
    border-radius: 50%;
    transition: all 0.2s;
  }

  .indicator {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 6px;
    height: 6px;
    background: rgba(0, 0, 0, 0.3);
    border-radius: 3px;
    transition: all 0.2s ease-out;
  }

  .toggle-wrapper.checked .indicator {
    width: 2px;
    height: 6px;
    border-radius: 1px;
    background: var(--accent, #6366f1);
  }

  .toggle-wrapper.checked .toggle {
    background: var(--accent, #6366f1);
  }

  /* Gradient/glow style animation */
  :global(.accent-gradient) .toggle-wrapper.checked .toggle,
  :global(.accent-glow) .toggle-wrapper.checked .toggle {
    background: var(--accent-gradient, var(--accent, #6366f1));
    background-size: 200% 200%;
    animation: toggle-shift 3s ease infinite;
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
  }

  .toggle-wrapper.checked .slider {
    transform: translateX(16px);
  }

  .toggle-wrapper:not(.disabled):hover .slider,
  .toggle-wrapper:not(.disabled):active .slider {
    box-shadow: 0 0 0 4px var(--accent-alpha, rgba(99, 102, 241, 0.25));
  }

  @media (max-width: 640px) {
    .toggle {
      width: 44px;
      height: 24px;
      border-radius: 12px;
    }

    .slider {
      width: 20px;
      height: 20px;
    }

    .toggle-wrapper.checked .slider {
      transform: translateX(20px);
    }

    .indicator {
      width: 7px;
      height: 7px;
    }

    .toggle-wrapper.checked .indicator {
      width: 2px;
      height: 7px;
    }
  }

  .label {
    color: rgba(255, 255, 255, 0.9);
    font-size: 14px;
  }
</style>
