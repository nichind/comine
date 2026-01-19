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
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button
  type="button"
  class="checkbox-wrapper"
  class:disabled
  class:checked
  onclick={toggle}
  data-setting-activator="true"
  {disabled}
>
  <span class="checkbox">
    <svg
      class="checkmark"
      class:visible={checked}
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="3"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <polyline class="check-path" points="20 6 9 17 4 12"></polyline>
    </svg>
  </span>
  {#if label}
    <span class="label">{label}</span>
  {/if}
</button>

<style>
  .checkbox-wrapper {
    display: inline-flex;
    align-items: flex-start;
    gap: 8px;
    cursor: pointer;
    user-select: none;
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: inherit;
    text-align: left;
  }

  .checkbox-wrapper.disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .checkbox {
    width: 20px;
    height: 20px;
    min-width: 20px;
    min-height: 20px;
    border: 2px solid rgba(255, 255, 255, 0);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.05);
    transition: all 0.2s;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: white;
  }

  .checkbox-wrapper.checked .checkbox {
    background: var(--accent, #6366f1);
  }

  .checkmark {
    opacity: 0;
    transition: opacity 0.15s;
  }

  .checkmark.visible {
    opacity: 1;
  }

  .check-path {
    stroke-dasharray: 24;
    stroke-dashoffset: 24;
    transition: stroke-dashoffset 0.2s ease-out;
  }

  .checkmark.visible .check-path {
    stroke-dashoffset: 0;
  }

  @media (prefers-reduced-motion: reduce) {
    .check-path {
      transition: none;
      stroke-dasharray: none;
      stroke-dashoffset: 0;
    }
  }

  .label {
    color: rgba(255, 255, 255, 0.9);
    font-size: 14px;
    line-height: 20px;
  }
</style>
