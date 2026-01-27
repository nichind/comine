<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    open = $bindable(false),
    title,
    description,
    options,
    value = $bindable(''),
    columns = 4,
    onselect,
  }: {
    open?: boolean;
    title: string;
    description?: string;
    options: { value: string; label: string }[];
    value?: string;
    columns?: number;
    onselect?: (value: string) => void;
  } = $props();

  function select(opt: string) {
    value = opt;
    open = false;
    onselect?.(opt);
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) {
      open = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      open = false;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal-backdrop" onclick={handleBackdropClick}>
    <div
      class="modal"
      style="--columns: {columns}"
      role="dialog"
      aria-modal="true"
      aria-labelledby="modal-title"
    >
      <div class="modal-header">
        <h3 id="modal-title">{title}</h3>
        <button class="close-btn" onclick={() => (open = false)} aria-label="Close">
          <Icon name="close" size={20} />
        </button>
      </div>
      <div class="modal-content">
        {#if description}
          <p class="modal-description">{description}</p>
        {/if}
        <div class="options-grid" role="listbox">
          {#each options as opt}
            <button
              class="option"
              class:selected={value === opt.value}
              onclick={() => select(opt.value)}
              role="option"
              aria-selected={value === opt.value}
            >
              {opt.label}
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    animation: fadeIn 0.15s ease;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .modal {
    background: rgb(30, 32, 44);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-lg, 12px);
    min-width: 320px;
    max-width: 90vw;
    animation: slideIn 0.2s ease;
  }

  @keyframes slideIn {
    from {
      opacity: 0;
      transform: scale(0.95) translateY(-10px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .modal-header h3 {
    font-size: 16px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.95);
  }

  .close-btn {
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    padding: 4px;
    display: flex;
    border-radius: var(--radius-sm, 6px);
    transition: all 0.15s;
  }

  .close-btn:hover {
    color: rgba(255, 255, 255, 0.9);
    background: rgba(255, 255, 255, 0.1);
  }

  .modal-content {
    padding: 16px 20px 20px;
  }

  .modal-description {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.5);
    margin: 0 0 16px 0;
    line-height: 1.5;
  }

  .options-grid {
    display: grid;
    grid-template-columns: repeat(var(--columns), 1fr);
    gap: 8px;
  }

  .option {
    padding: 12px 16px;
    font-size: 14px;
    font-weight: 500;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 8px);
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    transition: all 0.15s;
  }

  .option:hover {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.2);
  }

  .option.selected {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.25));
    border-color: var(--accent-alpha-hover, rgba(99, 102, 241, 0.5));
    color: white;
  }

  @media (max-width: 480px) {
    .options-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }
</style>
