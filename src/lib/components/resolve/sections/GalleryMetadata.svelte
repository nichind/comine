<script lang="ts">
  import { t } from '$lib/i18n';
  import MetaRow from '$lib/components/ui/MetaRow.svelte';
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
      <MetaRow icon="user" label={$t('resolve.gallery.artist')}>
        {gallery.artist}
      </MetaRow>
    {/if}

    {#if gallery.circle}
      <MetaRow icon="user" label={$t('resolve.gallery.circle')}>
        {gallery.circle}
      </MetaRow>
    {/if}

    {#if gallery.pageCount != null}
      <MetaRow icon="documents" label={$t('resolve.gallery.pages')}>
        {gallery.pageCount}
      </MetaRow>
    {/if}

    {#if gallery.language}
      <MetaRow icon="globe" label={$t('resolve.gallery.language')}>
        {gallery.language}
        {#if gallery.translated}
          <span class="tag-translated">(translated)</span>
        {/if}
      </MetaRow>
    {/if}

    {#if gallery.parody && gallery.parody.length > 0}
      <MetaRow icon="book" label={$t('resolve.gallery.parody')}>
        <div class="chip-list">
          {#each gallery.parody as p}
            <span class="meta-chip">{p}</span>
          {/each}
        </div>
      </MetaRow>
    {/if}

    {#if gallery.characters && gallery.characters.length > 0}
      <MetaRow icon="user" label={$t('resolve.gallery.characters')}>
        <div class="chip-list">
          {#each gallery.characters as c}
            <span class="meta-chip">{c}</span>
          {/each}
        </div>
      </MetaRow>
    {/if}

    {#if gallery.convention}
      <MetaRow icon="star" label={$t('resolve.gallery.convention')}>
        {gallery.convention}
      </MetaRow>
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
