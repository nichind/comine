<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Chip from '$lib/components/ui/Chip.svelte';
  import type { GalleryInfo } from '$lib/bindings/GalleryInfo';

  interface Props {
    gallery: GalleryInfo;
  }

  let { gallery }: Props = $props();

  let hasContent = $derived(
    !!gallery.artist ||
      !!gallery.circle ||
      !!gallery.language ||
      gallery.pageCount != null ||
      (gallery.parody && gallery.parody.length > 0) ||
      (gallery.characters && gallery.characters.length > 0)
  );
</script>

{#if hasContent}
  <div class="gallery-metadata">
    {#if gallery.artist}
      <div class="meta-row">
        <Icon name="user" size={14} />
        <span class="meta-label">{$t('resolve.gallery.artist')}</span>
        <span class="meta-value">{gallery.artist}</span>
      </div>
    {/if}

    {#if gallery.circle}
      <div class="meta-row">
        <Icon name="user" size={14} />
        <span class="meta-label">{$t('resolve.gallery.circle')}</span>
        <span class="meta-value">{gallery.circle}</span>
      </div>
    {/if}

    {#if gallery.pageCount != null}
      <div class="meta-row">
        <Icon name="documents" size={14} />
        <span class="meta-label">{$t('resolve.gallery.pages')}</span>
        <span class="meta-value">{gallery.pageCount}</span>
      </div>
    {/if}

    {#if gallery.language}
      <div class="meta-row">
        <Icon name="globe" size={14} />
        <span class="meta-label">{$t('resolve.gallery.language')}</span>
        <span class="meta-value">
          {gallery.language}
          {#if gallery.translated}
            <span class="tag-translated">(translated)</span>
          {/if}
        </span>
      </div>
    {/if}

    {#if gallery.parody && gallery.parody.length > 0}
      <div class="meta-row">
        <Icon name="book" size={14} />
        <span class="meta-label">{$t('resolve.gallery.parody')}</span>
        <div class="chip-list">
          {#each gallery.parody as p}
            <span class="meta-chip">{p}</span>
          {/each}
        </div>
      </div>
    {/if}

    {#if gallery.characters && gallery.characters.length > 0}
      <div class="meta-row">
        <Icon name="user" size={14} />
        <span class="meta-label">{$t('resolve.gallery.characters')}</span>
        <div class="chip-list">
          {#each gallery.characters as c}
            <span class="meta-chip">{c}</span>
          {/each}
        </div>
      </div>
    {/if}

    {#if gallery.convention}
      <div class="meta-row">
        <Icon name="star" size={14} />
        <span class="meta-label">{$t('resolve.gallery.convention')}</span>
        <span class="meta-value">{gallery.convention}</span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .gallery-metadata {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: var(--radius, 8px);
    border: 1px solid rgba(255, 255, 255, 0.05);
  }

  .meta-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.6);
  }

  .meta-row :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .meta-label {
    color: rgba(255, 255, 255, 0.4);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    min-width: 70px;
    flex-shrink: 0;
  }

  .meta-value {
    color: var(--text-1, white);
    word-break: break-word;
  }

  .tag-translated {
    color: rgba(255, 255, 255, 0.4);
    font-size: 11px;
  }

  .chip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .meta-chip {
    padding: 2px 8px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 99px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.8);
  }
</style>
