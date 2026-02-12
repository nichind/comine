<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
  import ModeSelector from './ModeSelector.svelte';
  import type { DownloadMode } from '$lib/stores/settings';
  import type { ViewMode } from '$lib/components/media/MediaGrid.svelte';
  import type { Snippet } from 'svelte';

  interface Props {
    searchQuery?: string;
    viewMode?: ViewMode;
    bulkMode?: DownloadMode | null;
    selectedCount?: number;
    totalCount?: number;
    showOptions?: boolean;
    loading?: boolean;
    hideModeSelector?: boolean;
    hideOptionsButton?: boolean;
    onSelectAll?: () => void;
    onDeselectAll?: () => void;
    onToggleOptions?: () => void;
    leftExtra?: Snippet;
  }

  let {
    searchQuery = $bindable(''),
    viewMode = $bindable<ViewMode>('list'),
    bulkMode = $bindable<DownloadMode | null>(null),
    selectedCount = 0,
    totalCount = 0,
    showOptions = $bindable(false),
    loading = false,
    hideModeSelector = false,
    hideOptionsButton = false,
    onSelectAll,
    onDeselectAll,
    onToggleOptions,
    leftExtra,
  }: Props = $props();

  function toggleViewMode() {
    viewMode = viewMode === 'list' ? 'grid' : 'list';
  }
</script>

<div class="collection-toolbar">
  <div class="toolbar-left">
    <div class="search-box">
      <Icon name="search" size={14} />
      <input
        type="text"
        bind:value={searchQuery}
        placeholder={$t('playlist.search')}
        disabled={loading}
      />
      {#if searchQuery}
        <button class="clear-btn" onclick={() => (searchQuery = '')}>
          <Icon name="close" size={12} />
        </button>
      {/if}
    </div>

    {#if leftExtra}
      {@render leftExtra()}
    {:else if !hideModeSelector}
      <ModeSelector bind:mode={bulkMode} disabled={loading} />
    {/if}
  </div>

  <div class="toolbar-actions">
    <button class="text-btn" onclick={onSelectAll} disabled={loading}>
      {$t('playlist.selectAll')}
    </button>
    <button class="text-btn" onclick={onDeselectAll} disabled={loading}>
      {$t('playlist.deselectAll')}
    </button>
    {#if !hideOptionsButton}
      <button
        class="icon-btn-sm"
        class:active={showOptions}
        onclick={onToggleOptions}
        title="Options"
      >
        <Icon name="settings" size={14} />
      </button>
    {/if}
    <button
      class="icon-btn-sm"
      onclick={toggleViewMode}
      title={viewMode === 'list' ? 'Grid view' : 'List view'}
    >
      <Icon name={viewMode === 'list' ? 'widgets' : 'burger'} size={14} />
    </button>
  </div>
</div>

<style>
  .collection-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 4px;
    flex-wrap: wrap;
  }

  .toolbar-left {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: 1;
    min-width: 0;
  }

  .search-box {
    flex: 1;
    max-width: 300px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-sm, 6px);
  }

  .search-box :global(svg) {
    color: rgba(255, 255, 255, 0.4);
    flex-shrink: 0;
  }

  .search-box input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: white;
    font-size: 12px;
    min-width: 50px;
  }

  .search-box input::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }

  .search-box input:disabled {
    opacity: 0.5;
  }

  .clear-btn {
    background: none;
    border: none;
    padding: 0;
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    display: flex;
  }

  .clear-btn:hover {
    color: white;
  }

  .toolbar-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .text-btn {
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.6);
    font-size: 12px;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: var(--radius-sm, 4px);
    transition: all 0.15s;
  }

  .text-btn:hover:not(:disabled) {
    color: var(--accent);
    background: rgba(255, 255, 255, 0.05);
  }

  .text-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .icon-btn-sm {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    background: rgba(255, 255, 255, 0.06);
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    transition: all 0.15s;
  }

  .icon-btn-sm:hover,
  .icon-btn-sm.active {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }
</style>
