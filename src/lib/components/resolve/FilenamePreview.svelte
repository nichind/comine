<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import { t } from '$lib/i18n';

  interface Props {
    template?: string;
    title?: string | null;
    author?: string | null;
    url?: string;
    loading?: boolean;
  }

  let {
    template = $bindable('%(title)s.%(ext)s'),
    title = null,
    author = null,
    url = '',
    loading = false,
  }: Props = $props();

  let filenamePreview = $derived.by(() => {
    const tpl = template || '%(title)s.%(ext)s';
    const ext = 'mp4';
    const cleanTitle = (title || 'video').replace(/[<>:"/\\|?*]/g, '_').trim();
    const cleanAuthor = (author || 'Unknown').replace(/[<>:"/\\|?*]/g, '_').trim();
    const id = url.match(/(?:v=|\/)([\w-]{11})(?:\?|&|$)/)?.[1] ?? 'unknown';

    return tpl
      .replace(/%\(ext\)s/g, ext)
      .replace(/%\(title\)s/g, cleanTitle)
      .replace(/%\(uploader\)s/g, cleanAuthor)
      .replace(/%\(channel\)s/g, cleanAuthor)
      .replace(/%\(id\)s/g, id)
      .replace(/%\([^)]+\)s/g, '_');
  });
</script>

{#if !loading}
  <div class="filename-row">
    <Icon name="pen_new" size={14} />
    <div class="filename-toggle">
      <span class="filename-preview-text" title={$t('download.filename_template')}>
        {filenamePreview}
      </span>
      <input
        class="filename-template-input"
        type="text"
        bind:value={template}
        placeholder={$t('download.filename_template')}
      />
    </div>
  </div>
{/if}

<style>
  .filename-row {
    display: flex;
    align-items: center;
    gap: 8px;
    color: rgba(255, 255, 255, 0.4);
    font-size: 12px;
    margin-top: 4px;
  }

  .filename-toggle {
    position: relative;
    flex: 1;
    min-width: 0;
  }

  .filename-preview-text {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    transition: opacity 0.15s;
    cursor: text;
  }

  .filename-toggle:hover .filename-preview-text,
  .filename-toggle:focus-within .filename-preview-text {
    opacity: 0;
    pointer-events: none;
  }

  .filename-template-input {
    position: absolute;
    inset: 0;
    width: 100%;
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.6);
    font-size: 12px;
    font-family: inherit;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .filename-toggle:hover .filename-template-input,
  .filename-toggle:focus-within .filename-template-input {
    opacity: 1;
  }

  .filename-template-input:focus {
    outline: none;
  }
</style>
