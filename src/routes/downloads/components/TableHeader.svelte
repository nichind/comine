<script lang="ts">
  import Icon from '$lib/components/Icon.svelte';
  import Dropdown from '$lib/components/Dropdown.svelte';
  import type { SortType } from '$lib/stores/history';
  import type { SortDirection } from '$lib/stores/downloadsState.svelte';
  import { t } from '$lib/i18n';

  interface Props {
    sortType: SortType;
    sortDirection: SortDirection;
    onSortChange: (type: SortType, direction: SortDirection) => void;
  }

  let { sortType, sortDirection, onSortChange }: Props = $props();

  // Active popup state
  let openPopup = $state<SortType | null>(null);
  let anchorEl = $state<HTMLElement | null>(null);

  interface Column {
    key: string;
    labelKey: string;
    width: string;
    sortKey?: SortType;
    align?: 'left' | 'center' | 'right';
  }

  const columns: Column[] = [
    { key: 'thumb', labelKey: 'downloads.sort.date', width: '56px', sortKey: 'date' },
    { key: 'title', labelKey: 'downloads.table.title', width: '1fr', sortKey: 'name', align: 'left' },
    { key: 'actions', labelKey: '', width: '60px' },
    { key: 'format', labelKey: 'downloads.table.format', width: '50px', sortKey: 'format' },
    { key: 'size', labelKey: 'downloads.table.size', width: '70px', sortKey: 'size' },
    { key: 'duration', labelKey: 'downloads.table.duration', width: '60px', sortKey: 'duration' },
  ];

  function handleColumnClick(e: MouseEvent, column: Column) {
    if (!column.sortKey) return;
    
    // Toggle popup or open new one
    if (openPopup === column.sortKey) {
      openPopup = null;
      anchorEl = null;
    } else {
      anchorEl = e.currentTarget as HTMLElement;
      openPopup = column.sortKey;
    }
  }

  function selectSort(sortKey: SortType, direction: SortDirection) {
    onSortChange(sortKey, direction);
    openPopup = null;
    anchorEl = null;
  }

  function closePopup() {
    openPopup = null;
    anchorEl = null;
  }

  function handleKeydown(e: KeyboardEvent, column: Column) {
    if ((e.key === 'Enter' || e.key === ' ') && column.sortKey) {
      e.preventDefault();
      handleColumnClick(e as unknown as MouseEvent, column);
    }
  }
</script>

<div class="table-header" role="row">
  {#each columns as column}
    {#if column.sortKey}
      <button
        class="header-cell sortable"
        class:active={sortType === column.sortKey}
        class:open={openPopup === column.sortKey}
        style="width: {column.width}; min-width: {column.width}; justify-content: {column.align === 'left' ? 'flex-start' : 'center'};"
        role="columnheader"
        aria-sort={sortType === column.sortKey ? (sortDirection === 'asc' ? 'ascending' : 'descending') : 'none'}
        aria-haspopup="menu"
        aria-expanded={openPopup === column.sortKey}
        onclick={(e) => handleColumnClick(e, column)}
        onkeydown={(e) => handleKeydown(e, column)}
      >
        <span class="header-label">{column.labelKey ? $t(column.labelKey) : ''}</span>
      </button>
    {:else if column.labelKey}
      <div
        class="header-cell"
        style="width: {column.width}; min-width: {column.width};"
        role="columnheader"
      >
        <span class="header-label">{$t(column.labelKey)}</span>
      </div>
    {:else}
      <div
        class="header-cell spacer"
        style="width: {column.width}; min-width: {column.width};"
        role="columnheader"
      ></div>
    {/if}
  {/each}
</div>

<!-- Sort Popup -->
<Dropdown open={openPopup !== null} {anchorEl} onclose={closePopup}>
  <button 
    class="sort-option"
    class:selected={sortType === openPopup && sortDirection === 'asc'}
    role="menuitem"
    onclick={() => openPopup && selectSort(openPopup, 'asc')}
  >
    <Icon name="chevron_up" size={14} />
    <span>{$t('downloads.sort.ascending')}</span>
    {#if sortType === openPopup && sortDirection === 'asc'}
      <Icon name="check" size={14} />
    {/if}
  </button>
  <button 
    class="sort-option"
    class:selected={sortType === openPopup && sortDirection === 'desc'}
    role="menuitem"
    onclick={() => openPopup && selectSort(openPopup, 'desc')}
  >
    <Icon name="chevron_down" size={14} />
    <span>{$t('downloads.sort.descending')}</span>
    {#if sortType === openPopup && sortDirection === 'desc'}
      <Icon name="check" size={14} />
    {/if}
  </button>
</Dropdown>

<style>
  .table-header {
    display: grid;
    grid-template-columns: 56px 1fr 60px 50px 70px 60px;
    gap: 12px;
    align-items: center;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.08));
    height: 36px;
  }

  .header-cell {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-tertiary, rgba(255, 255, 255, 0.5));
    text-transform: uppercase;
    letter-spacing: 0.5px;
    white-space: nowrap;
    user-select: none;
  }

  .header-cell.sortable {
    cursor: pointer;
    background: transparent;
    border: none;
    padding: 4px 8px;
    border-radius: 4px;
    transition: all 0.15s ease;
  }

  .header-cell.sortable:hover {
    background: var(--bg-hover, rgba(255, 255, 255, 0.08));
    color: var(--text-secondary, rgba(255, 255, 255, 0.8));
  }

  .header-cell.sortable.active {
    color: var(--text-primary, rgba(255, 255, 255, 0.9));
  }

  .header-cell.sortable.open {
    background: var(--bg-hover, rgba(255, 255, 255, 0.08));
  }

  .header-cell.spacer {
    pointer-events: none;
  }

  .header-label {
    white-space: nowrap;
  }

  /* Sort option buttons (rendered inside Dropdown) */
  :global(.dropdown-menu) .sort-option {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text-secondary, rgba(255, 255, 255, 0.8));
    font-size: 13px;
    cursor: pointer;
    transition: all 0.12s;
    text-align: left;
  }

  :global(.dropdown-menu) .sort-option span {
    flex: 1;
  }

  :global(.dropdown-menu) .sort-option:hover {
    background: var(--bg-hover, rgba(255, 255, 255, 0.08));
  }

  :global(.dropdown-menu) .sort-option.selected {
    color: var(--accent);
    background: var(--accent-alpha, rgba(99, 102, 241, 0.1));
  }

  :global(.dropdown-menu) .sort-option.selected:hover {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.15));
  }

  @media (max-width: 700px) {
    .table-header {
      grid-template-columns: 48px 1fr auto;
    }

    .header-cell:nth-child(4),
    .header-cell:nth-child(5),
    .header-cell:nth-child(6) {
      display: none;
    }
  }
</style>
