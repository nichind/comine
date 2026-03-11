<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, untrack } from 'svelte';
  import { t } from '$lib/i18n';
  import { navigation } from '$lib/stores/navigation';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatSize } from '$lib/utils/format';

  export interface TorrentOption {
    quality: string | null;
    codec: string | null;
    seeders: number | null;
    sizeBytes: number | null;
    qualityScore: number | null;
    magnetUrl: string | null;
    torrentUrl: string | null;
  }

  export interface TorrentSearchResult {
    title: string;
    year: number | null;
    contentType: string | null;
    ratingImdb: string | null;
    posterUrl: string | null;
    torrents: TorrentOption[];
  }

  interface TorrentSearchResponse {
    total: number;
    page: number;
    results: TorrentSearchResult[];
  }

  type ContentType = 'all' | 'movie' | 'tv';
  type QualityFilter = '' | '4K' | '1080p' | '720p';
  type SortFilter = 'relevance' | 'seeders' | 'rating' | 'date';

  interface Props {
    initialQuery?: string;
    onBack?: () => void;
  }

  let { initialQuery = '', onBack }: Props = $props();

  // eslint-disable-next-line svelte/reactivity -- intentional: seed state from prop once
  let query = $state(untrack(() => initialQuery));
  let contentType = $state<ContentType>('all');
  let quality = $state<QualityFilter>('');
  let sort = $state<SortFilter>('relevance');
  let page = $state(1);

  let results = $state<TorrentSearchResult[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let loadingMore = $state(false);
  let error = $state<string | null>(null);
  let hasSearched = $state(false);

  let suggestions = $state<string[]>([]);
  let showSuggestions = $state(false);

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let autocompleteTimer: ReturnType<typeof setTimeout> | null = null;

  let hasMore = $derived(results.length < total);

  async function doSearch(resetPage = true) {
    if (!query.trim()) return;

    if (resetPage) {
      page = 1;
      results = [];
    }

    if (page === 1) {
      loading = true;
    } else {
      loadingMore = true;
    }
    error = null;
    hasSearched = true;
    showSuggestions = false;

    try {
      const response = await invoke<TorrentSearchResponse>('torrent_search', {
        filters: {
          query: query.trim(),
          contentType: contentType === 'all' ? null : contentType,
          quality: quality || null,
          sort,
          page,
        },
      });
      if (page === 1) {
        results = response.results;
      } else {
        results = [...results, ...response.results];
      }
      total = response.total;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
      loadingMore = false;
    }
  }

  async function loadMore() {
    page += 1;
    await doSearch(false);
  }

  function handleInput() {
    if (autocompleteTimer) clearTimeout(autocompleteTimer);

    if (query.trim().length < 2) {
      suggestions = [];
      showSuggestions = false;
      return;
    }

    autocompleteTimer = setTimeout(async () => {
      try {
        const s = await invoke<string[]>('torrent_autocomplete', { query: query.trim() });
        suggestions = s;
        showSuggestions = s.length > 0;
      } catch {
        suggestions = [];
        showSuggestions = false;
      }
    }, 300);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      showSuggestions = false;
      doSearch();
    } else if (e.key === 'Escape') {
      showSuggestions = false;
    }
  }

  function selectSuggestion(s: string) {
    query = s;
    showSuggestions = false;
    doSearch();
  }

  function applyFilter() {
    if (hasSearched) {
      if (debounceTimer) clearTimeout(debounceTimer);
      debounceTimer = setTimeout(() => doSearch(), 100);
    }
  }

  $effect(() => {
    // trigger filter re-search when contentType, quality, or sort change
    contentType;
    quality;
    sort;
    applyFilter();
  });

  onMount(() => {
    if (initialQuery) {
      doSearch();
    }
  });

  function openDetail(result: TorrentSearchResult) {
    navigation.openTorrentDetail(result);
  }

  function getBestQuality(torrents: TorrentOption[]): string | null {
    if (!torrents.length) return null;
    const sorted = [...torrents].sort((a, b) => (b.qualityScore ?? 0) - (a.qualityScore ?? 0));
    return sorted[0].quality ?? null;
  }

  function getBestSeeders(torrents: TorrentOption[]): number {
    if (!torrents.length) return 0;
    return Math.max(...torrents.map((t) => t.seeders ?? 0));
  }

  function posterFallback(e: Event) {
    const img = e.currentTarget as HTMLImageElement;
    img.style.display = 'none';
    const fallback = img.nextElementSibling as HTMLElement | null;
    if (fallback) fallback.style.display = 'flex';
  }
</script>

<div class="torrent-search">
  <div class="search-header">
    {#if onBack}
      <button class="back-btn" onclick={onBack}>
        <Icon name="alt_arrow_rigth" size={16} class="rotate-180" />
        <span>{$t('common.back')}</span>
      </button>
    {/if}

    <div class="search-input-wrap">
      <div class="search-icon">
        <Icon name="search" size={16} />
      </div>
      <input
        class="search-input"
        type="text"
        placeholder={$t('torrentSearch.searchPlaceholder')}
        bind:value={query}
        oninput={handleInput}
        onkeydown={handleKeydown}
        onfocus={() => { if (suggestions.length) showSuggestions = true; }}
        onblur={() => setTimeout(() => { showSuggestions = false; }, 150)}
        autocomplete="off"
        spellcheck="false"
      />
      {#if query}
        <button class="clear-btn" onclick={() => { query = ''; suggestions = []; showSuggestions = false; }}>
          <Icon name="close" size={14} />
        </button>
      {/if}
      <button class="search-btn" onclick={() => doSearch()} disabled={!query.trim()}>
        {$t('torrentSearch.title')}
      </button>

      {#if showSuggestions && suggestions.length}
        <ul class="suggestions" role="listbox">
          {#each suggestions as s}
            <li role="option" aria-selected="false">
              <button onclick={() => selectSuggestion(s)}>
                <Icon name="search" size={13} />
                <span>{s}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>

  <div class="filter-bar">
    <div class="filter-group">
      {#each ([['all', $t('torrentSearch.filters.all')], ['movie', $t('torrentSearch.filters.movies')], ['tv', $t('torrentSearch.filters.tvShows')]] as [ContentType, string][]) as [val, label]}
        <button
          class="filter-chip"
          class:active={contentType === val}
          onclick={() => { contentType = val; }}
        >
          {label}
        </button>
      {/each}
    </div>

    <div class="filter-group">
      <span class="filter-label">{$t('torrentSearch.filters.quality')}</span>
      {#each ([['', $t('torrentSearch.filters.anyQuality')], ['4K', '4K'], ['1080p', '1080p'], ['720p', '720p']] as [QualityFilter, string][]) as [val, label]}
        <button
          class="filter-chip"
          class:active={quality === val}
          onclick={() => { quality = val; }}
        >
          {label}
        </button>
      {/each}
    </div>

    <div class="filter-group">
      <span class="filter-label">{$t('torrentSearch.filters.sort')}</span>
      {#each ([['relevance', $t('torrentSearch.filters.relevance')], ['seeders', $t('torrentSearch.filters.seeders')], ['rating', $t('torrentSearch.filters.rating')], ['date', $t('torrentSearch.filters.date')]] as [SortFilter, string][]) as [val, label]}
        <button
          class="filter-chip"
          class:active={sort === val}
          onclick={() => { sort = val; }}
        >
          {label}
        </button>
      {/each}
    </div>
  </div>

  <div class="results-area">
    {#if loading}
      <div class="state-container">
        <div class="spinner-wrap">
          <Icon name="spinner" size={28} />
        </div>
        <p class="state-text">{$t('torrentSearch.loading')}</p>
      </div>
    {:else if error}
      <div class="state-container">
        <Icon name="warning" size={32} />
        <p class="state-text">{$t('torrentSearch.error')}</p>
        <p class="state-hint">{error}</p>
        <button class="retry-btn" onclick={() => doSearch()}>
          <Icon name="restart" size={14} />
          {$t('torrentSearch.retry')}
        </button>
      </div>
    {:else if hasSearched && !results.length}
      <div class="state-container">
        <Icon name="ghost" size={40} />
        <p class="state-text">{$t('torrentSearch.noResults')}</p>
        <p class="state-hint">{$t('torrentSearch.noResultsHint')}</p>
      </div>
    {:else if results.length}
      <div class="results-grid">
        {#each results as result}
          {@const bestQuality = getBestQuality(result.torrents)}
          {@const bestSeeders = getBestSeeders(result.torrents)}
          <button class="result-card" onclick={() => openDetail(result)}>
            <div class="poster-wrap">
              {#if result.posterUrl}
                <img
                  src={result.posterUrl}
                  alt={result.title}
                  class="poster-img"
                  decoding="async"
                  onerror={posterFallback}
                />
              {/if}
              <div class="poster-fallback" style={result.posterUrl ? 'display:none' : ''}>
                <Icon name="video" size={36} />
              </div>

              {#if bestQuality}
                <span class="quality-badge">{bestQuality}</span>
              {/if}

              {#if result.ratingImdb}
                <span class="rating-badge">
                  <Icon name="star" size={10} />
                  {result.ratingImdb}
                </span>
              {/if}
            </div>

            <div class="card-info">
              <p class="card-title" title={result.title}>{result.title}</p>
              <div class="card-meta">
                {#if result.year}
                  <span class="meta-year">{result.year}</span>
                {/if}
                {#if result.contentType}
                  <span class="meta-type">{result.contentType}</span>
                {/if}
              </div>
              {#if bestSeeders > 0}
                <div class="card-seeders">
                  <span class="seeders-dot"></span>
                  <span>{bestSeeders} {$t('torrentSearch.seeders')}</span>
                </div>
              {/if}
            </div>
          </button>
        {/each}
      </div>

      {#if hasMore}
        <div class="load-more-wrap">
          <button class="load-more-btn" onclick={loadMore} disabled={loadingMore}>
            {#if loadingMore}
              <Icon name="spinner" size={14} />
            {/if}
            {$t('torrentSearch.loadMore')}
          </button>
        </div>
      {/if}
    {:else if !hasSearched}
      <div class="state-container empty-start">
        <Icon name="search" size={40} />
        <p class="state-text">{$t('torrentSearch.title')}</p>
        <p class="state-hint">{$t('torrentSearch.searchPlaceholder')}</p>
      </div>
    {/if}
  </div>
</div>

<style>
  :global(.rotate-180) {
    transform: rotate(180deg);
  }

  .torrent-search {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  /* Header */
  .search-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    flex-shrink: 0;
  }

  .back-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.6);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    flex-shrink: 0;
    white-space: nowrap;
  }

  .back-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .search-input-wrap {
    position: relative;
    display: flex;
    align-items: center;
    flex: 1;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 8px);
    padding: 0 8px;
    gap: 6px;
    min-width: 0;
  }

  .search-input-wrap:focus-within {
    border-color: rgba(255, 255, 255, 0.2);
    background: rgba(255, 255, 255, 0.08);
  }

  .search-icon {
    color: rgba(255, 255, 255, 0.4);
    flex-shrink: 0;
    display: flex;
  }

  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: white;
    font-size: 14px;
    padding: 8px 0;
    min-width: 0;
  }

  .search-input::placeholder {
    color: rgba(255, 255, 255, 0.35);
  }

  .clear-btn {
    display: flex;
    align-items: center;
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    padding: 2px;
    border-radius: 4px;
    flex-shrink: 0;
    transition: color 0.15s;
  }

  .clear-btn:hover {
    color: rgba(255, 255, 255, 0.8);
  }

  .search-btn {
    flex-shrink: 0;
    padding: 5px 12px;
    background: var(--accent, #6366f1);
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: white;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s;
    white-space: nowrap;
  }

  .search-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .search-btn:not(:disabled):hover {
    opacity: 0.85;
  }

  /* Suggestions */
  .suggestions {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    right: 0;
    background: var(--surface, #1e1e2e);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: var(--radius, 8px);
    list-style: none;
    margin: 0;
    padding: 4px;
    z-index: 100;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    overflow: hidden;
  }

  .suggestions li button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.8);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .suggestions li button:hover {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  /* Filter bar */
  .filter-bar {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    padding: 10px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    flex-shrink: 0;
  }

  .filter-group {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }

  .filter-label {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
    font-weight: 500;
    margin-right: 2px;
    white-space: nowrap;
  }

  .filter-chip {
    padding: 4px 10px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 999px;
    color: rgba(255, 255, 255, 0.6);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
  }

  .filter-chip:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .filter-chip.active {
    background: var(--accent, #6366f1);
    border-color: var(--accent, #6366f1);
    color: white;
  }

  /* Results area */
  .results-area {
    flex: 1;
    overflow-y: auto;
    padding: 16px 0;
  }

  /* State containers */
  .state-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 60px 20px;
    color: rgba(255, 255, 255, 0.4);
    text-align: center;
  }

  .state-text {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.7);
  }

  .state-hint {
    margin: 0;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.4);
    max-width: 320px;
    line-height: 1.5;
  }

  .spinner-wrap {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .retry-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius, 8px);
    color: white;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    margin-top: 4px;
    transition: all 0.15s;
  }

  .retry-btn:hover {
    background: rgba(255, 255, 255, 0.14);
  }

  /* Results grid */
  .results-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
  }

  @media (max-width: 700px) {
    .results-grid {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  @media (max-width: 400px) {
    .results-grid {
      grid-template-columns: 1fr;
    }
  }

  .result-card {
    display: flex;
    flex-direction: column;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: var(--radius, 8px);
    overflow: hidden;
    cursor: pointer;
    text-align: left;
    transition: all 0.15s;
    padding: 0;
  }

  .result-card:hover {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.14);
    transform: translateY(-1px);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  }

  /* Poster */
  .poster-wrap {
    position: relative;
    aspect-ratio: 2 / 3;
    width: 100%;
    background: rgba(255, 255, 255, 0.04);
    overflow: hidden;
    flex-shrink: 0;
  }

  .poster-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .poster-fallback {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.2);
  }

  .quality-badge {
    position: absolute;
    top: 6px;
    left: 6px;
    padding: 2px 6px;
    background: var(--accent, #6366f1);
    border-radius: 4px;
    font-size: 10px;
    font-weight: 700;
    color: white;
    letter-spacing: 0.3px;
  }

  .rating-badge {
    position: absolute;
    top: 6px;
    right: 6px;
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 2px 6px;
    background: rgba(0, 0, 0, 0.7);
    border-radius: 4px;
    font-size: 10px;
    font-weight: 700;
    color: #fbbf24;
    backdrop-filter: blur(4px);
  }

  /* Card info */
  .card-info {
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }

  .card-title {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.9);
    line-height: 1.3;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .card-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .meta-year {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.45);
    font-weight: 500;
  }

  .meta-type {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.35);
    background: rgba(255, 255, 255, 0.06);
    padding: 1px 5px;
    border-radius: 4px;
    text-transform: capitalize;
  }

  .card-seeders {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
  }

  .seeders-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: #22c55e;
    flex-shrink: 0;
  }

  /* Load more */
  .load-more-wrap {
    display: flex;
    justify-content: center;
    padding-top: 20px;
  }

  .load-more-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 24px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 8px);
    color: rgba(255, 255, 255, 0.7);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }

  .load-more-btn:not(:disabled):hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .load-more-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .empty-start {
    opacity: 0.5;
  }
</style>
