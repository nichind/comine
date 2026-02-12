<script lang="ts">
  import { t } from '$lib/i18n';

  interface Props {
    tags?: string[] | null;
    categories?: string[] | null;
    maxVisible?: number;
  }

  let { tags = null, categories = null, maxVisible = 20 }: Props = $props();

  let showAllTags = $state(false);
  let showAllCategories = $state(false);

  let visibleTags = $derived(
    tags && tags.length > 0 ? (showAllTags ? tags : tags.slice(0, maxVisible)) : []
  );

  let visibleCategories = $derived(
    categories && categories.length > 0
      ? showAllCategories
        ? categories
        : categories.slice(0, maxVisible)
      : []
  );

  let hasContent = $derived(visibleTags.length > 0 || visibleCategories.length > 0);
</script>

{#if hasContent}
  <div class="tags-section">
    {#if visibleCategories.length > 0}
      <div class="chip-group">
        <span class="chip-label">{$t('resolve.categories')}</span>
        <div class="chips">
          {#each visibleCategories as cat}
            <span class="chip category">{cat}</span>
          {/each}
          {#if categories && categories.length > maxVisible}
            <button class="chip more" onclick={() => (showAllCategories = !showAllCategories)}>
              {showAllCategories ? $t('common.showLess') : `+${categories.length - maxVisible}`}
            </button>
          {/if}
        </div>
      </div>
    {/if}

    {#if visibleTags.length > 0}
      <div class="chip-group">
        <span class="chip-label">{$t('resolve.tags')}</span>
        <div class="chips">
          {#each visibleTags as tag}
            <span class="chip tag">{tag}</span>
          {/each}
          {#if tags && tags.length > maxVisible}
            <button class="chip more" onclick={() => (showAllTags = !showAllTags)}>
              {showAllTags ? $t('common.showLess') : `+${tags.length - maxVisible}`}
            </button>
          {/if}
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .tags-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .chip-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .chip-label {
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: rgba(255, 255, 255, 0.4);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    padding: 3px 10px;
    border-radius: 99px;
    font-size: 12px;
    cursor: default;
    transition: background 0.15s;
  }

  .chip.category {
    background: rgba(var(--accent-rgb, 139, 92, 246), 0.15);
    color: var(--accent, #8b5cf6);
    border: 1px solid rgba(var(--accent-rgb, 139, 92, 246), 0.2);
  }

  .chip.tag {
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.7);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  .chip.more {
    background: rgba(255, 255, 255, 0.04);
    color: rgba(255, 255, 255, 0.5);
    border: 1px solid rgba(255, 255, 255, 0.06);
    cursor: pointer;
    font-weight: 500;
  }

  .chip.more:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.8);
  }
</style>
