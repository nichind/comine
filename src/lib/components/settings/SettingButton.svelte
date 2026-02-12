<script lang="ts">
  let {
    label,
    value,
    onclick,
  }: {
    label: string;
    value: string;
    onclick?: () => void;
  } = $props();
</script>

<button class="setting-button" {onclick}>
  <span class="label">{label}:</span>
  <span class="value">{value}</span>
</button>

<style>
  .setting-button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    font-size: 13px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 8px);
    color: rgba(255, 255, 255, 0.9);
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
  }

  .setting-button:hover {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.2);
  }

  .label {
    color: rgba(255, 255, 255, 0.5);
    font-weight: 400;
  }

  .value {
    font-weight: 600;
  }
</style>
