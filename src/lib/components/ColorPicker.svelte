<script lang="ts">
  interface Props {
    value: string;
    disabled?: boolean;
    onchange?: (value: string) => void;
  }

  let { value = $bindable(), disabled = false, onchange }: Props = $props();

  function update(v: string) {
    value = v;
    onchange?.(v);
  }
</script>

<div class="color-picker-group">
  <input
    type="color"
    class="color-picker"
    {value}
    {disabled}
    oninput={(e) => update(e.currentTarget.value)}
  />
  <input
    type="text"
    class="color-text-input"
    {value}
    {disabled}
    oninput={(e) => update(e.currentTarget.value)}
    placeholder="#000000"
  />
</div>

<style>
  .color-picker-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .color-picker {
    width: 40px;
    height: 32px;
    padding: 2px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.05);
    cursor: pointer;
  }

  .color-picker::-webkit-color-swatch-wrapper {
    padding: 2px;
  }

  .color-picker::-webkit-color-swatch {
    border: none;
    border-radius: 4px;
  }

  .color-picker:disabled,
  .color-text-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .color-text-input {
    width: 90px;
    padding: 6px 10px;
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    color: white;
    outline: none;
    transition: all 0.2s;
  }

  .color-text-input:focus {
    border-color: rgba(99, 102, 241, 0.5);
  }
</style>
