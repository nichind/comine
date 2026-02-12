<script lang="ts" module>
  export interface SliderProps {
    value: number;
    min: number;
    max: number;
    step?: number;
    suffix?: string;
    disabled?: boolean;
    label?: string;
    fillDirection?: 'left' | 'right';
    oninput?: (value: number) => void;
    onchange?: (value: number) => void;
  }
</script>

<script lang="ts">
  let {
    value = $bindable(),
    min,
    max,
    step = 1,
    suffix = '',
    disabled = false,
    label,
    fillDirection = 'left',
    oninput,
    onchange,
  }: SliderProps = $props();

  function clamp(val: number): number {
    return Math.min(max, Math.max(min, val));
  }

  function parseValue(input: string): number {
    const parsed = Number(input);
    if (Number.isNaN(parsed)) return min;
    return clamp(parsed);
  }

  function handleInput(e: Event) {
    const v = parseValue((e.target as HTMLInputElement).value);
    value = v;
    oninput?.(v);
  }

  function handleChange(e: Event) {
    const v = parseValue((e.target as HTMLInputElement).value);
    onchange?.(v);
  }

  const displayValue = $derived(value ?? min);
  const fillPercent = $derived(((displayValue - min) / (max - min)) * 100);
</script>

<div class="slider-container" class:disabled>
  <input
    type="range"
    class="slider"
    class:fill-right={fillDirection === 'right'}
    {min}
    {max}
    {step}
    value={displayValue}
    {disabled}
    aria-label={label}
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuenow={displayValue}
    style:--fill-percent="{fillPercent}%"
    oninput={handleInput}
    onchange={handleChange}
  />
  <span class="slider-value" aria-hidden="true">{displayValue}{suffix}</span>
</div>

<style>
  .slider-container {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 180px;
    flex: 1;
  }

  .slider-container.disabled {
    opacity: 0.5;
    pointer-events: none;
  }

  .slider {
    --track-height: 8px;
    --thumb-width: 4px;
    --thumb-height: 18px;
    --track-bg: rgba(255, 255, 255, 0.12);
    --fill-color: var(--accent, #6366f1);

    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    cursor: pointer;
    height: var(--thumb-height);
    touch-action: none;
    -webkit-tap-highlight-color: transparent;
  }

  .slider::-webkit-slider-runnable-track {
    height: var(--track-height);
    border-radius: calc(var(--track-height) / 2);
    background: linear-gradient(
      to right,
      var(--fill-color) 0%,
      var(--fill-color) var(--fill-percent),
      var(--track-bg) var(--fill-percent),
      var(--track-bg) 100%
    );
  }

  .slider.fill-right::-webkit-slider-runnable-track {
    background: linear-gradient(
      to right,
      var(--track-bg) 0%,
      var(--track-bg) var(--fill-percent),
      var(--fill-color) var(--fill-percent),
      var(--fill-color) 100%
    );
  }

  .slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: var(--thumb-width);
    height: var(--thumb-height);
    background: #fff;
    border: none;
    border-radius: 2px;
    margin-top: calc((var(--track-height) - var(--thumb-height)) / 2);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
    transition:
      box-shadow 0.15s,
      width 0.15s;
  }

  .slider::-webkit-slider-thumb:hover {
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
    width: 6px;
  }

  .slider:active::-webkit-slider-thumb {
    width: 6px;
  }

  .slider::-moz-range-track {
    height: var(--track-height);
    border-radius: calc(var(--track-height) / 2);
    background: var(--track-bg);
  }

  .slider::-moz-range-progress {
    height: var(--track-height);
    border-radius: calc(var(--track-height) / 2) 0 0 calc(var(--track-height) / 2);
    background: var(--fill-color);
  }

  .slider.fill-right::-moz-range-progress {
    background: var(--track-bg);
  }

  .slider.fill-right::-moz-range-track {
    background: var(--fill-color);
  }

  .slider::-moz-range-thumb {
    width: var(--thumb-width);
    height: var(--thumb-height);
    background: #fff;
    border: none;
    border-radius: 2px;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
    transition:
      box-shadow 0.15s,
      width 0.15s;
  }

  .slider::-moz-range-thumb:hover {
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
    width: 6px;
  }

  .slider:active::-moz-range-thumb {
    width: 6px;
  }

  .slider:focus-visible {
    outline: none;
  }

  .slider:focus-visible::-webkit-slider-thumb {
    box-shadow:
      0 0 0 3px rgba(99, 102, 241, 0.4),
      0 1px 4px rgba(0, 0, 0, 0.3);
  }

  .slider:focus-visible::-moz-range-thumb {
    box-shadow:
      0 0 0 3px rgba(99, 102, 241, 0.4),
      0 1px 4px rgba(0, 0, 0, 0.3);
  }

  .slider-value {
    font-size: var(--text-base, 13px);
    font-family: 'JetBrains Mono', monospace;
    color: rgba(255, 255, 255, 0.7);
    min-width: 40px;
    text-align: right;
    user-select: none;
  }

  @media (max-width: 640px) {
    .slider-container {
      min-width: 0;
      gap: 16px;
    }

    .slider {
      --track-height: 10px;
      --thumb-width: 6px;
      --thumb-height: 28px;
    }
  }

  @media (pointer: coarse) {
    .slider {
      --track-height: 12px;
      --thumb-width: 8px;
      --thumb-height: 32px;
    }

    .slider::-webkit-slider-thumb {
      width: 8px;
    }

    .slider::-moz-range-thumb {
      width: 8px;
    }
  }
</style>
