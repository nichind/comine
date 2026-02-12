<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatSize } from '$lib/utils/format';
  import MediaGrid, {
    type MediaItemData,
    type ViewMode,
  } from '$lib/components/media/MediaGrid.svelte';
  import CollectionToolbar from '$lib/components/resolve/CollectionToolbar.svelte';
  import type { FileEntry } from '$lib/bindings/FileEntry';

  interface Props {
    files: FileEntry[];
    fileCount?: number | null;
    totalSize?: number | null;
    selectedFileIds?: Set<string>;
    loading?: boolean;
    onToggleFile?: (index: number) => void;
    onSelectAll?: () => void;
    onDeselectAll?: () => void;
  }

  let {
    files,
    fileCount = null,
    totalSize = null,
    selectedFileIds = $bindable(new Set<string>()),
    loading = false,
    onToggleFile,
    onSelectAll,
    onDeselectAll,
  }: Props = $props();

  let viewMode = $state<ViewMode>('grid');
  let searchQuery = $state('');

  let displayCount = $derived(fileCount ?? files.length);
  let displaySize = $derived(() => {
    if (totalSize) return formatSize(Number(totalSize));
    let sum = 0;
    for (const f of files) {
      if (f.filesize) sum += Number(f.filesize);
    }
    return sum > 0 ? formatSize(sum) : null;
  });

  let filteredFiles = $derived.by(() => {
    if (!searchQuery.trim()) return files;
    const q = searchQuery.toLowerCase();
    return files.filter(
      (f: FileEntry) =>
        (f.filename?.toLowerCase().includes(q) ?? false) ||
        (f.mimeType?.toLowerCase().includes(q) ?? false)
    );
  });

  function mapFile(file: FileEntry, index: number): MediaItemData {
    const idx = file.index ?? index;
    const name = file.filename ?? `File ${idx + 1}`;
    const sizeStr = file.filesize ? formatSize(Number(file.filesize)) : null;
    const resStr = file.width && file.height ? `${file.width}×${file.height}` : null;
    const subtitle = [sizeStr, resStr].filter(Boolean).join(' · ');

    return {
      id: String(idx),
      title: name,
      thumbnail: file.thumbnail ?? null,
      duration: null,
      author: subtitle || null,
    };
  }

  let mappedFiles = $derived(
    filteredFiles.map((f: FileEntry, i: number) => mapFile(f, files.indexOf(f)))
  );

  function toggleFile(id: string) {
    const newSet = new Set(selectedFileIds);
    if (newSet.has(id)) newSet.delete(id);
    else newSet.add(id);
    selectedFileIds = newSet;
    onToggleFile?.(Number(id));
  }

  function selectAll() {
    const newSet = new Set(selectedFileIds);
    filteredFiles.forEach((f: FileEntry, i: number) =>
      newSet.add(String(f.index ?? files.indexOf(f)))
    );
    selectedFileIds = newSet;
    onSelectAll?.();
  }

  function deselectAll() {
    const newSet = new Set(selectedFileIds);
    filteredFiles.forEach((f: FileEntry, i: number) =>
      newSet.delete(String(f.index ?? files.indexOf(f)))
    );
    selectedFileIds = newSet;
    onDeselectAll?.();
  }

  const emptySettings = new Map<string, Record<string, never>>();
</script>

{#if files.length > 0}
  <div class="file-section">
    <CollectionToolbar
      bind:searchQuery
      bind:viewMode
      {loading}
      hideModeSelector
      hideOptionsButton
      onSelectAll={selectAll}
      onDeselectAll={deselectAll}
    >
      {#snippet leftExtra()}
        <div class="file-summary">
          <Icon name="folder" size={14} />
          <span>{selectedFileIds.size}/{displayCount}</span>
          {#if displaySize()}
            <span class="file-total-size">· {displaySize()}</span>
          {/if}
        </div>
      {/snippet}
    </CollectionToolbar>

    <MediaGrid
      items={mappedFiles}
      selectedIds={selectedFileIds}
      {viewMode}
      perItemSettings={emptySettings}
      {loading}
      totalItemCount={searchQuery ? filteredFiles.length : displayCount}
      ontoggle={toggleFile}
    />
  </div>
{/if}

<style>
  .file-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1;
    min-height: 0;
  }

  .file-summary {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    white-space: nowrap;
  }

  .file-total-size {
    opacity: 0.7;
  }
</style>
