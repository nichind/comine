<script lang="ts">
  import { slide } from 'svelte/transition';
  import { t } from '$lib/i18n';
  import type { DownloadMode } from '$lib/stores/settings';
  import CollectionToolbar from '$lib/components/resolve/CollectionToolbar.svelte';
  import EmbedOptions from '$lib/components/resolve/EmbedOptions.svelte';
  import SponsorBlockOptions from '$lib/components/resolve/SponsorBlockOptions.svelte';
  import Chip from '$lib/components/ui/Chip.svelte';
  import MediaGrid, {
    type MediaItemData,
    type ViewMode,
    type MediaItemSettings,
  } from '$lib/components/media/MediaGrid.svelte';
  import type { PlaylistEntry } from '$lib/bindings/PlaylistEntry';

  interface Props {
    entries: PlaylistEntry[];
    selectedIds?: Set<string>;
    perItemSettings?: Map<string, Partial<MediaItemSettings>>;
    totalCount?: number;
    loading?: boolean;
    showSponsorBlock?: boolean;
    showEmbedOptions?: boolean;
    bulkMode?: DownloadMode | null;
    searchQuery?: string;
    viewMode?: ViewMode;
    usePlaylistFolder?: boolean;
    skipSponsors?: boolean;
    skipIntros?: boolean;
    skipSelfPromo?: boolean;
    skipInteraction?: boolean;
    embedChapters?: boolean;
    embedThumbnail?: boolean;
    embedMetadata?: boolean;
    embedSubs?: boolean;
    subLangs?: string;
    onUpdateSettings?: (id: string, settings: Partial<MediaItemSettings>) => void;
    onOpenItem?: (item: MediaItemData) => void;
    onScroll?: (scrollTop: number, scrollHeight: number, clientHeight: number) => void;
    getDefaultSettings?: (item: MediaItemData) => MediaItemSettings;
  }

  let {
    entries,
    selectedIds = $bindable(new Set<string>()),
    perItemSettings = new Map(),
    totalCount = 0,
    loading = false,
    showSponsorBlock = true,
    showEmbedOptions = true,
    bulkMode = $bindable<DownloadMode | null>(null),
    searchQuery = $bindable(''),
    viewMode = $bindable<ViewMode>('list'),
    usePlaylistFolder = $bindable(true),
    skipSponsors = $bindable(false),
    skipIntros = $bindable(false),
    skipSelfPromo = $bindable(false),
    skipInteraction = $bindable(false),
    embedChapters = $bindable(true),
    embedThumbnail = $bindable(true),
    embedMetadata = $bindable(true),
    embedSubs = $bindable(false),
    subLangs = $bindable('en,ru'),
    onUpdateSettings,
    onOpenItem,
    onScroll,
    getDefaultSettings,
  }: Props = $props();

  let showOptions = $state(false);

  let filteredEntries = $derived.by(() => {
    if (!searchQuery.trim()) return entries;
    const q = searchQuery.toLowerCase();
    return entries.filter(
      (e) =>
        (e.title?.toLowerCase().includes(q) ?? false) ||
        (e.uploader?.toLowerCase().includes(q) ?? false)
    );
  });

  function mapEntry(entry: PlaylistEntry): MediaItemData {
    return {
      id: entry.id,
      title: entry.title ?? 'Untitled',
      thumbnail: entry.thumbnail ?? null,
      duration: entry.duration ? Number(entry.duration) : null,
      author: entry.uploader ?? null,
    };
  }

  function toggleItem(id: string) {
    const newSet = new Set(selectedIds);
    if (newSet.has(id)) newSet.delete(id);
    else newSet.add(id);
    selectedIds = newSet;
  }

  function selectAll() {
    const newSet = new Set(selectedIds);
    filteredEntries.forEach((e) => newSet.add(e.id));
    selectedIds = newSet;
  }

  function deselectAll() {
    const newSet = new Set(selectedIds);
    filteredEntries.forEach((e) => newSet.delete(e.id));
    selectedIds = newSet;
  }
</script>

{#if entries.length > 0}
  <div class="collection-section">
    <CollectionToolbar
      bind:searchQuery
      bind:viewMode
      bind:bulkMode
      selectedCount={selectedIds.size}
      {totalCount}
      bind:showOptions
      {loading}
      onSelectAll={selectAll}
      onDeselectAll={deselectAll}
      onToggleOptions={() => (showOptions = !showOptions)}
    />

    {#if showOptions}
      <div class="collection-options" transition:slide={{ axis: 'y', duration: 200 }}>
        <div class="options-compact">
          <div class="settings-group">
            <span class="section-label">GENERAL</span>
            <div class="chips-row">
              <Chip
                icon="folder"
                selected={usePlaylistFolder}
                onclick={() => (usePlaylistFolder = !usePlaylistFolder)}
              >
                {$t('playlist.createFolder')}
              </Chip>
            </div>
          </div>

          {#if showSponsorBlock}
            <SponsorBlockOptions
              bind:skipSponsors
              bind:skipIntros
              bind:skipSelfPromo
              bind:skipInteraction
              variant="chip"
            />
          {/if}

          {#if showEmbedOptions}
            <EmbedOptions
              bind:embedChapters
              bind:embedThumbnail
              bind:embedMetadata
              bind:embedSubs
              bind:subLangs
              variant="chip"
            />
          {/if}
        </div>
      </div>
    {/if}

    <MediaGrid
      items={filteredEntries}
      mapItem={mapEntry}
      {selectedIds}
      {viewMode}
      {perItemSettings}
      {loading}
      {getDefaultSettings}
      totalItemCount={searchQuery ? filteredEntries.length : totalCount}
      ontoggle={toggleItem}
      onupdatesettings={onUpdateSettings}
      onopenitem={onOpenItem}
      onscroll={onScroll}
    />
  </div>
{/if}

<style>
  .collection-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1;
    min-height: 0;
  }

  .collection-options {
    background: rgba(0, 0, 0, 0.15);
    border-radius: var(--radius, 8px);
    padding: 12px;
    margin-bottom: 8px;
    border: 1px solid rgba(255, 255, 255, 0.05);
  }

  .options-compact {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .settings-group {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .section-label {
    font-size: 11px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .chips-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
</style>
