<script lang="ts">
  import { t } from '$lib/i18n';
  import { logs, filteredLogs, logStats, type LogLevel, type LogEntry } from '$lib/stores/logs';
  import { onMount, onDestroy } from 'svelte';
  import { beforeNavigate } from '$app/navigation';
  import Icon from '$lib/components/Icon.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { save } from '@tauri-apps/plugin-dialog';
  import { writeTextFile } from '@tauri-apps/plugin-fs';
  import Modal from '$lib/components/Modal.svelte';
  import Button from '$lib/components/Button.svelte';
  import { saveScrollPosition, getScrollPosition } from '$lib/stores/scroll';
  import VirtualList from '$lib/components/VirtualList.svelte';
  import { formatSize } from '$lib/utils/format';

  const ROUTE_PATH = '/logs';
  const ESTIMATED_HEIGHT_DESKTOP = 28;
  const ESTIMATED_HEIGHT_MOBILE = 48;

  let searchQuery = $state('');
  let virtualList: VirtualList<LogEntry> | null = $state(null);
  let autoScroll = $state(true);
  let hasScrolledDown = $state(false);
  let isMobile = $state(false);
  let isDesktop = $state(false);
  let isLoading = $state(true);

  let estimatedHeight = $derived(isMobile ? ESTIMATED_HEIGHT_MOBILE : ESTIMATED_HEIGHT_DESKTOP);

  beforeNavigate(() => {
    const pos = virtualList?.getScrollTop() ?? 0;
    saveScrollPosition(ROUTE_PATH, pos);
  });

  let showCopyModal = $state(false);
  let copyContentLength = $state(0);

  let activeFilters = $state<Set<LogLevel>>(new Set(['trace', 'debug', 'info', 'warn', 'error']));

  let resizeHandler: (() => void) | null = null;

  onMount(async () => {
    isMobile = window.innerWidth < 768;
    resizeHandler = () => {
      isMobile = window.innerWidth < 768;
    };
    window.addEventListener('resize', resizeHandler);

    const ua = navigator.userAgent.toLowerCase();
    isDesktop = !ua.includes('android') && !ua.includes('iphone') && !ua.includes('ipad');

    isLoading = true;
    await logs.loadFromDisk();
    isLoading = false;

    logs.info('system', 'Log viewer opened');

    const savedPosition = getScrollPosition(ROUTE_PATH);
    if (savedPosition > 0 && virtualList) {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          if (virtualList) {
            virtualList.setScrollTop(savedPosition);
            autoScroll = false;
          }
        });
      });
    }
  });

  onDestroy(() => {
    if (resizeHandler) {
      window.removeEventListener('resize', resizeHandler);
      resizeHandler = null;
    }
  });

  function toggleFilter(level: LogLevel) {
    const newFilters = new Set(activeFilters);
    if (newFilters.has(level)) {
      newFilters.delete(level);
    } else {
      newFilters.add(level);
    }
    activeFilters = newFilters;
    logs.toggleLevel(level);
  }

  function handleSearchChange(e: Event) {
    const target = e.target as HTMLInputElement;
    searchQuery = target.value;
    logs.setSearch(searchQuery);
  }

  function clearLogs() {
    logs.clear();
  }

  async function promptCopyLogs() {
    const text = await logs.exportAsText();
    copyContentLength = text.length;
    showCopyModal = true;
  }

  async function confirmCopyLogs() {
    const text = await logs.exportAsText();
    await navigator.clipboard.writeText(text);
    logs.info('system', 'Logs copied to clipboard');
    showCopyModal = false;
  }

  async function openLogsFolder() {
    await logs.openLogsFolder();
    logs.info('system', 'Opened logs folder');
  }

  async function downloadLogs() {
    const text = await logs.exportAsText();
    const defaultName = `comine-logs-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.txt`;

    try {
      const filePath = await save({
        defaultPath: defaultName,
        filters: [
          {
            name: 'Text Files',
            extensions: ['txt', 'log'],
          },
        ],
      });

      if (filePath) {
        await writeTextFile(filePath, text);
        logs.info('system', `Logs saved to ${filePath}`);
      }
    } catch (e) {
      const blob = new Blob([text], { type: 'text/plain' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = defaultName;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      logs.warn('system', 'Used fallback download method');
    }
  }

  function formatTime(date: Date): string {
    return date.toLocaleTimeString('en-US', {
      hour12: false,
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  }

  function getLevelShort(level: LogLevel): string {
    switch (level) {
      case 'trace':
        return 'TRC';
      case 'debug':
        return 'DBG';
      case 'info':
        return 'INF';
      case 'warn':
        return 'WRN';
      case 'error':
        return 'ERR';
    }
  }

  function handleScroll() {
    if (!virtualList) return;
    const scrollTop = virtualList.getScrollTop();
    const isAtTop = scrollTop <= 10;
    hasScrolledDown = !isAtTop;
    if (isAtTop && !autoScroll) {
      autoScroll = true;
    } else if (!isAtTop && autoScroll) {
      autoScroll = false;
    }
  }

  $effect(() => {
    if (autoScroll && virtualList && $filteredLogs.length > 0) {
      virtualList.scrollToTop();
    }
  });
</script>

<div class="logs-page">
  <div class="log-container" class:scrolled={hasScrolledDown}>
    {#if isLoading}
      <div class="empty-state">
        <div class="empty-icon">
          <Icon name="documents" size={56} />
        </div>
        <p class="empty-title">Loading logs...</p>
        <span class="empty-hint">Reading session log file from disk</span>
      </div>
    {:else if $filteredLogs.length === 0}
      <div class="empty-state">
        <div class="empty-icon">
          <Icon name="documents" size={56} />
        </div>
        <p class="empty-title">No logs yet</p>
        <span class="empty-hint">Logs will appear here as the app runs</span>
      </div>
    {:else}
      <VirtualList
        bind:this={virtualList}
        items={$filteredLogs}
        estimatedItemHeight={estimatedHeight}
        measureItems={true}
        overscan={10}
        containerClass="log-list-virtual"
        onscroll={handleScroll}
        getKey={(entry) => entry.id}
        useFadeMask={!isMobile}
        useCustomScrollbar={true}
      >
        {#snippet children(entry, _index)}
          {#if isMobile}
            <div class="log-entry mobile {entry.level}">
              <div class="log-header">
                <span class="log-time">{formatTime(entry.timestamp)}</span>
                <span class="log-level-badge {entry.level}">{getLevelShort(entry.level)}</span>
                <span class="log-source">{entry.source}</span>
              </div>
              <div class="log-message">{entry.message}</div>
            </div>
          {:else}
            <div class="log-entry {entry.level}">
              <span class="log-time">{formatTime(entry.timestamp)}</span>
              <span class="log-level-badge {entry.level}">{getLevelShort(entry.level)}</span>
              <span class="log-source">{entry.source}</span>
              <span class="log-message">{entry.message}</span>
            </div>
          {/if}
        {/snippet}
      </VirtualList>
    {/if}
  </div>

  <div class="floating-toolbar">
    <div class="toolbar-content">
      <div class="search-box">
        <Icon name="search" size={14} />
        <input
          type="text"
          placeholder="Search logs..."
          value={searchQuery}
          oninput={handleSearchChange}
        />
        {#if searchQuery}
          <button
            class="clear-search"
            onclick={() => {
              searchQuery = '';
              logs.setSearch('');
            }}
          >
            <Icon name="close" size={12} />
          </button>
        {/if}
      </div>

      <div class="filter-chips">
        <button
          class="chip trace"
          class:active={activeFilters.has('trace')}
          onclick={() => toggleFilter('trace')}
          use:tooltip={'Toggle trace logs'}
        >
          TRC
          {#if $logStats.trace > 0}<span class="chip-count">{$logStats.trace}</span>{/if}
        </button>
        <button
          class="chip debug"
          class:active={activeFilters.has('debug')}
          onclick={() => toggleFilter('debug')}
          use:tooltip={'Toggle debug logs'}
        >
          DBG
          {#if $logStats.debug > 0}<span class="chip-count">{$logStats.debug}</span>{/if}
        </button>
        <button
          class="chip info"
          class:active={activeFilters.has('info')}
          onclick={() => toggleFilter('info')}
          use:tooltip={'Toggle info logs'}
        >
          INF
          {#if $logStats.info > 0}<span class="chip-count">{$logStats.info}</span>{/if}
        </button>
        <button
          class="chip warn"
          class:active={activeFilters.has('warn')}
          onclick={() => toggleFilter('warn')}
          use:tooltip={'Toggle warning logs'}
        >
          WRN
          {#if $logStats.warn > 0}<span class="chip-count">{$logStats.warn}</span>{/if}
        </button>
        <button
          class="chip error"
          class:active={activeFilters.has('error')}
          onclick={() => toggleFilter('error')}
          use:tooltip={'Toggle error logs'}
        >
          ERR
          {#if $logStats.error > 0}<span class="chip-count">{$logStats.error}</span>{/if}
        </button>
      </div>

      <div class="toolbar-actions">
        {#if isDesktop}
          <button class="action-btn" onclick={openLogsFolder} use:tooltip={$t('logs.openFolder')}>
            <Icon name="folder" size={16} />
          </button>
        {/if}
        <button class="action-btn" onclick={promptCopyLogs} use:tooltip={'Copy to clipboard'}>
          <Icon name="copy" size={16} />
        </button>
        <button class="action-btn" onclick={downloadLogs} use:tooltip={'Download logs'}>
          <Icon name="download" size={16} />
        </button>
        <button
          class="action-btn"
          onclick={() => (autoScroll = !autoScroll)}
          class:active={autoScroll}
          use:tooltip={'Auto-scroll'}
        >
          <Icon name="sort" size={16} />
        </button>
        <button class="action-btn danger" onclick={clearLogs} use:tooltip={'Clear all logs'}>
          <Icon name="trash" size={16} />
        </button>
      </div>
    </div>
  </div>

  <div class="log-counter">
    <span class="counter-text">{$filteredLogs.length}</span>
    <span class="counter-label">logs</span>
  </div>
</div>

<Modal bind:open={showCopyModal} title={$t('logs.copyConfirmTitle')}>
  <div class="copy-confirm-content">
    <div class="warning-icon">
      <Icon name="warning" size={32} />
    </div>
    <p class="copy-warning-text">{$t('logs.copyConfirmMessage')}</p>
    <div class="copy-size-info">
      <span class="size-label">{$t('logs.contentSize')}:</span>
      <span class="size-value">{formatSize(copyContentLength)}</span>
      <span class="size-chars">({copyContentLength.toLocaleString()} {$t('logs.characters')})</span>
    </div>
    <p class="copy-hint">{$t('logs.copyHint')}</p>
  </div>
  {#snippet actions()}
    <div class="modal-actions">
      <Button variant="ghost" onclick={() => (showCopyModal = false)}>
        {$t('common.cancel')}
      </Button>
      <Button variant="primary" onclick={confirmCopyLogs}>
        <Icon name="copy" size={16} />
        {$t('logs.copyAnyway')}
      </Button>
    </div>
  {/snippet}
</Modal>

<style>
  .logs-page {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding-left: 0px;
  }

  .log-container {
    flex: 1;
    overflow: hidden;
    font-family: 'JetBrains Mono', 'Fira Code', 'SF Mono', 'Consolas', monospace;
    font-size: var(--text-sm, 12px);
    padding-bottom: 30px;
  }

  :global(.log-list-virtual) {
    padding: 4px 8px;
  }

  :global(.app.mobile) .log-container {
    padding-bottom: 180px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 300px;
    color: rgba(255, 255, 255, 0.3);
    gap: 16px;
    padding: 40px;
  }

  .empty-icon {
    opacity: 0.4;
  }

  .empty-title {
    font-size: 18px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.5);
    font-family: inherit;
  }

  .empty-hint {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.35);
  }

  .log-entry {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 6px 10px;
    border-radius: var(--radius-sm, 4px);
    line-height: 1.5;
    transition: background 0.1s;
  }

  .log-entry:hover {
    background: rgba(255, 255, 255, 0.04);
  }

  .log-entry.mobile {
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    margin-bottom: 2px;
    border-left: 3px solid transparent;
  }

  .log-entry.mobile.info {
    border-left-color: rgba(34, 197, 94, 0.5);
  }
  .log-entry.mobile.debug {
    border-left-color: rgba(99, 102, 241, 0.5);
  }
  .log-entry.mobile.warn {
    border-left-color: rgba(251, 191, 36, 0.5);
  }
  .log-entry.mobile.error {
    border-left-color: rgba(239, 68, 68, 0.5);
  }
  .log-entry.mobile.trace {
    border-left-color: rgba(156, 163, 175, 0.5);
  }

  .log-header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--text-xs, 11px);
  }

  .log-time {
    color: rgba(255, 255, 255, 0.35);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }

  .log-level-badge {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 5px;
    border-radius: 3px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex-shrink: 0;
  }

  .log-level-badge.info {
    color: rgba(34, 197, 94, 1);
    background: rgba(34, 197, 94, 0.15);
  }
  .log-level-badge.debug {
    color: rgba(99, 102, 241, 1);
    background: rgba(99, 102, 241, 0.15);
  }
  .log-level-badge.warn {
    color: rgba(251, 191, 36, 1);
    background: rgba(251, 191, 36, 0.15);
  }
  .log-level-badge.error {
    color: rgba(239, 68, 68, 1);
    background: rgba(239, 68, 68, 0.15);
  }
  .log-level-badge.trace {
    color: rgba(156, 163, 175, 0.9);
    background: rgba(156, 163, 175, 0.15);
  }

  .log-source {
    color: rgba(255, 255, 255, 0.5);
    font-size: var(--text-xs, 11px);
    flex-shrink: 0;
  }

  .log-source::before {
    content: '›';
    margin-right: 4px;
    opacity: 0.5;
  }

  .log-message {
    color: rgba(255, 255, 255, 0.85);
    flex: 1;
    word-break: break-word;
    white-space: pre-wrap;
  }

  .log-entry.mobile .log-message {
    font-size: var(--text-sm, 12px);
    line-height: 1.6;
    padding-left: 2px;
  }

  .log-entry.error .log-message {
    color: rgba(239, 68, 68, 0.95);
  }
  .log-entry.warn .log-message {
    color: rgba(251, 191, 36, 0.95);
  }

  .floating-toolbar {
    position: absolute;
    bottom: 8px;
    left: 8px;
    right: 8px;
    z-index: 100;
    background: rgba(30, 30, 35, 0.85);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-lg, 12px);
    padding: 8px;
  }

  :global(.app.mobile) .floating-toolbar {
    bottom: 96px;
  }

  .toolbar-content {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .search-box {
    display: flex;
    align-items: center;
    gap: 6px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-sm, 6px);
    padding: 0 10px;
    height: 30px;
    min-width: 120px;
    color: rgba(255, 255, 255, 0.6);
    transition: all 0.15s;
  }

  .search-box:focus-within {
    border-color: rgba(255, 255, 255, 0.25);
    background: rgba(255, 255, 255, 0.12);
  }

  .search-box input {
    background: transparent;
    border: none;
    outline: none;
    color: rgba(255, 255, 255, 0.9);
    font-size: var(--text-sm, 12px);
    width: 100%;
    font-family: inherit;
  }

  .search-box input::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }

  .clear-search {
    background: transparent;
    border: none;
    width: 14px;
    height: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: rgba(255, 255, 255, 0.4);
  }

  .clear-search:hover {
    color: rgba(255, 255, 255, 0.7);
  }

  .filter-chips {
    display: flex;
    gap: 4px;
  }

  .chip {
    font-size: 10px;
    font-weight: 600;
    padding: 0 8px;
    height: 30px;
    border-radius: var(--radius-sm, 6px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    transition: all 0.15s;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .chip:hover {
    background: rgba(255, 255, 255, 0.15);
    border-color: rgba(255, 255, 255, 0.2);
    color: rgba(255, 255, 255, 0.8);
  }

  .chip.active.trace {
    background: rgba(156, 163, 175, 0.2);
    border-color: rgba(156, 163, 175, 0.5);
    color: rgb(180, 185, 195);
  }
  .chip.active.info {
    background: rgba(34, 197, 94, 0.2);
    border-color: rgba(34, 197, 94, 0.5);
    color: rgb(34, 197, 94);
  }
  .chip.active.debug {
    background: rgba(99, 102, 241, 0.2);
    border-color: rgba(99, 102, 241, 0.5);
    color: rgb(129, 132, 255);
  }
  .chip.active.warn {
    background: rgba(251, 191, 36, 0.2);
    border-color: rgba(251, 191, 36, 0.5);
    color: rgb(251, 191, 36);
  }
  .chip.active.error {
    background: rgba(239, 68, 68, 0.2);
    border-color: rgba(239, 68, 68, 0.5);
    color: rgb(255, 100, 100);
  }

  .chip-count {
    font-size: 9px;
    opacity: 0.8;
  }

  .toolbar-actions {
    display: flex;
    gap: 4px;
    margin-left: auto;
  }

  .action-btn {
    width: 30px;
    height: 30px;
    border-radius: var(--radius-sm, 6px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s;
  }

  .action-btn:hover {
    background: rgba(255, 255, 255, 0.15);
    color: #fff;
    border-color: rgba(255, 255, 255, 0.2);
  }

  .action-btn.active {
    background: rgba(99, 102, 241, 0.2);
    border-color: rgba(99, 102, 241, 0.5);
    color: rgb(129, 132, 255);
  }

  .action-btn.danger:hover {
    background: rgba(239, 68, 68, 0.2);
    border-color: rgba(239, 68, 68, 0.5);
    color: rgb(255, 100, 100);
  }

  .log-counter {
    position: absolute;
    top: 8px;
    right: 8px;
    background: rgba(30, 30, 35, 0.85);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 8px);
    padding: 6px 10px;
    display: flex;
    align-items: baseline;
    gap: 4px;
  }

  .counter-text {
    font-size: var(--text-sm, 12px);
    font-weight: 500;
    color: rgba(255, 255, 255, 0.9);
    font-variant-numeric: tabular-nums;
  }

  .counter-label {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.5);
  }

  .copy-confirm-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 8px 0;
    text-align: center;
  }

  .warning-icon {
    color: rgba(251, 191, 36, 1);
    background: rgba(251, 191, 36, 0.15);
    padding: 12px;
    border-radius: 50%;
  }

  .copy-warning-text {
    color: rgba(255, 255, 255, 0.85);
    font-size: var(--text-md, 14px);
    line-height: 1.5;
    margin: 0;
  }

  .copy-size-info {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 12px 16px;
    background: rgba(0, 0, 0, 0.3);
    border-radius: var(--radius, 8px);
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
  }

  .size-label {
    color: rgba(255, 255, 255, 0.6);
    font-size: var(--text-sm, 12px);
  }

  .size-value {
    color: rgba(251, 191, 36, 1);
    font-size: var(--text-lg, 16px);
    font-weight: 600;
  }

  .size-chars {
    color: rgba(255, 255, 255, 0.5);
    font-size: var(--text-xs, 11px);
  }

  .copy-hint {
    color: rgba(255, 255, 255, 0.5);
    font-size: var(--text-sm, 12px);
    margin: 0;
  }

  .modal-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  @media (max-width: 768px) {
    .toolbar-content {
      gap: 4px;
    }

    .search-box {
      min-width: 80px;
      flex: 1;
      order: 0;
    }

    .toolbar-actions {
      order: 1;
    }

    .filter-chips {
      order: 2;
      width: 100%;
    }
  }
</style>
