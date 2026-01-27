<script lang="ts">
  interface Props {
    value: number;
    min: number;
    max: number;
    step?: number;
    suffix?: string;
    disabled?: boolean;
    onchange?: (value: number) => void;
  }

  let {
    value = $bindable(),
    min,
    max,
    step = 1,
    suffix = '',
    disabled = false,
    onchange,
  }: Props = $props();

  function handleInput(e: Event) {
    const v = parseInt((e.target as HTMLInputElement).value);
    value = v;
  }

  function handleChange(e: Event) {
    const v = parseInt((e.target as HTMLInputElement).value);
    onchange?.(v);
  }
</script>

<div class="slider-with-value" class:disabled>
  <input
    type="range"
    class="blur-slider"
    {min}
    {max}
    {step}
    {value}
    {disabled}
    oninput={handleInput}
    onchange={handleChange}
  />
  <span class="slider-value">{value}{suffix}</span>
</div>

<style>
  .slider-with-value {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 180px;
    flex: 1;
  }

  .slider-with-value.disabled {
    opacity: 0.5;
    pointer-events: none;
  }

  .blur-slider {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 6px;
    background: rgba(255, 255, 255, 0.15);
    border-radius: 3px;
    outline: none;
    cursor: pointer;
  }

  .blur-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    background: var(--accent, #6366f1);
    border-radius: 50%;
    cursor: pointer;
    transition:
      background 0.15s,
      transform 0.15s;
  }

  .blur-slider::-webkit-slider-thumb:hover {
    background: #818cf8;
    transform: scale(1.1);
  }

  .blur-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    background: var(--accent, #6366f1);
    border: none;
    border-radius: 50%;
    cursor: pointer;
    transition:
      background 0.15s,
      transform 0.15s;
  }

  .blur-slider::-moz-range-thumb:hover {
    background: #818cf8;
    transform: scale(1.1);
  }

  .slider-value {
    font-size: 13px;
    font-family: 'JetBrains Mono', monospace;
    color: rgba(255, 255, 255, 0.7);
    min-width: 40px;
    text-align: right;
  }
</style>
