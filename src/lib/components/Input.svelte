<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    value?: string;
    placeholder?: string;
    type?: 'text' | 'password' | 'email' | 'search';
    disabled?: boolean;
    oninput?: (e: Event) => void;
    onfocus?: (e: FocusEvent) => void;
    onblur?: (e: FocusEvent) => void;
    onkeydown?: (e: KeyboardEvent) => void;
    iconLeft?: Snippet;
    iconRight?: Snippet;
  }

  let {
    value = $bindable(''),
    placeholder = '',
    type = 'text',
    disabled = false,
    oninput,
    onfocus,
    onblur,
    onkeydown,
    iconLeft,
    iconRight,
  }: Props = $props();
</script>

<div class="input-wrapper">
  {#if iconLeft}
    <span class="icon-left">
      {@render iconLeft()}
    </span>
  {/if}
  <input
    class="input"
    class:has-icon-left={iconLeft}
    class:has-icon-right={iconRight}
    bind:value
    {type}
    {placeholder}
    {disabled}
    {oninput}
    {onfocus}
    {onblur}
    {onkeydown}
  />
  {#if iconRight}
    <span class="icon-right">
      {@render iconRight()}
    </span>
  {/if}
</div>

<style>
  .input-wrapper {
    position: relative;
    display: flex;
    align-items: center;
    width: 100%;
  }

  .input {
    width: 100%;
    padding: 8px 14px;
    font-family: inherit;
    font-size: 14px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0);
    border-radius: var(--radius, 8px);
    color: white;
    outline: none;
    transition: all 0.2s;
  }

  .input.has-icon-left {
    padding-left: 40px;
  }

  .input.has-icon-right {
    padding-right: 40px;
  }

  .input::placeholder {
    color: rgba(255, 255, 255, 0.32);
  }

  .input:focus {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.1);
  }

  .input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .icon-left,
  .icon-right {
    position: absolute;
    display: flex;
    align-items: center;
    color: rgba(255, 255, 255, 0.5);
    pointer-events: none;
  }

  .icon-left {
    left: 12px;
  }

  .icon-right {
    right: 12px;
  }

  .icon-left :global(svg),
  .icon-right :global(svg) {
    width: 18px;
    height: 18px;
  }
</style>
