<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '$lib/i18n';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatSize } from '$lib/utils/format';

  interface TorrentFileEntry {
    index: number;
    path: string;
    size: number;
  }

  interface Props {
    magnetUrl: string;
    title: string;
    posterUrl?: string;
    onConfirm?: (selectedFiles: number[]) => void;
    onBack?: () => void;
  }

  let { magnetUrl, title, posterUrl, onConfirm, onBack }: Props = $props();

  const VIDEO_EXTENSIONS = new Set(['.mp4', '.mkv', '.avi', '.mov', '.webm', '.flv', '.m4v', '.ts', '.wmv']);

  let files = $state<TorrentFileEntry[]>([]);
  let selected = $state<Set<number>>(new Set());
  let loading = $state(true);
  let error = $state<string | null>(null);

  let totalSelectedSize = $derived(
    files
      .filter((f) => selected.has(f.index))
      .reduce((acc, f) => acc + f.size, 0)
  );

  let selectedCount = $derived(selected.size);

  function getExtension(path: string): string {
    const dot = path.lastIndexOf('.');
    return dot !== -1 ? path.slice(dot).toLowerCase() : '';
  }

  function getFileIcon(path: string): 'video' | 'music' | 'image' | 'file_text' {
    const ext = getExtension(path);
    if (VIDEO_EXTENSIONS.has(ext)) return 'video';
    if (['.mp3', '.flac', '.aac', '.ogg', '.wav', '.m4a'].includes(ext)) return 'music';
    if (['.jpg', '.jpeg', '.png', '.gif', '.webp', '.bmp'].includes(ext)) return 'image';
    return 'file_text';
  }

  function getFileName(path: string): string {
    return path.split('/').pop() ?? path;
  }

  function getFileDirname(path: string): string {
    const parts = path.split('/');
    if (parts.length <= 1) return '';
    return parts.slice(0, -1).join('/');
  }

  function toggleFile(index: number) {
    const next = new Set(selected);
    if (next.has(index)) {
      next.delete(index);
    } else {
      next.add(index);
    }
    selected = next;
  }

  function selectAll() {
    selected = new Set(files.map((f) => f.index));
  }

  function selectNone() {
    selected = new Set();
  }

  function selectVideosOnly() {
    selected = new Set(
      files.filter((f) => VIDEO_EXTENSIONS.has(getExtension(f.path))).map((f) => f.index)
    );
  }

  function handleConfirm() {
    onConfirm?.(Array.from(selected).sort((a, b) => a - b));
  }

  $effect(() => {
    async function fetchFiles() {
      loading = true;
      error = null;
      try {
        const result = await invoke<TorrentFileEntry[]>('torrent_list_files', { magnetUrl });
        files = result;
        // Auto-select all videos by default
        selected = new Set(
          result
            .filter((f) => VIDEO_EXTENSIONS.has(getExtension(f.path)))
            .map((f) => f.index)
        );
        if (selected.size === 0) {
          // If no videos, select all
          selected = new Set(result.map((f) => f.index));
        }
      } catch (e) {
        error = String(e);
      } finally {
        loading = false;
      }
    }

    void fetchFiles();
  });
</script>

<div class="file-picker">
  <div class="picker-header">
    {#if onBack}
      <button class="back-btn" onclick={onBack}>
        <Icon name="alt_arrow_rigth" size={16} class="rotate-180" />
        <span>{$t('common.back')}</span>
      </button>
    {/if}

    <div class="header-info">
      <Icon name="checklist" size={16} />
      <span class="header-title">{$t('torrentSearch.fileSelection.title')}</span>
    </div>

    <div class="header-spacer"></div>

    <span class="header-count">{title}</span>
  </div>

  <div class="picker-content">
    {#if loading}
      <div class="state-container">
        <div class="spinner-wrap">
          <Icon name="spinner" size={28} />
        </div>
        <p class="state-text">{$t('torrentSearch.fileSelection.loading')}</p>
      </div>
    {:else if error}
      <div class="state-container">
        <Icon name="warning" size={32} />
        <p class="state-text">{$t('torrentSearch.error')}</p>
        <p class="state-hint">{error}</p>
      </div>
    {:else if files.length === 0}
      <div class="state-container">
        <Icon name="ghost" size={36} />
        <p class="state-text">{$t('torrentSearch.fileSelection.noFiles')}</p>
      </div>
    {:else}
      <div class="quick-actions">
        <button class="action-btn" onclick={selectAll}>
          <Icon name="double_check" size={13} />
          {$t('torrentSearch.fileSelection.selectAll')}
        </button>
        <button class="action-btn" onclick={selectNone}>
          <Icon name="close" size={13} />
          {$t('torrentSearch.fileSelection.selectNone')}
        </button>
        <button class="action-btn" onclick={selectVideosOnly}>
          <Icon name="video" size={13} />
          {$t('torrentSearch.fileSelection.videosOnly')}
        </button>
      </div>

      <div class="file-list">
        {#each files as file}
          {@const fname = getFileName(file.path)}
          {@const fdir = getFileDirname(file.path)}
          {@const fileIcon = getFileIcon(file.path)}
          {@const isSelected = selected.has(file.index)}
          <button
            class="file-row"
            class:selected={isSelected}
            onclick={() => toggleFile(file.index)}
            role="checkbox"
            aria-checked={isSelected}
          >
            <span class="file-check" class:checked={isSelected}>
              {#if isSelected}
                <Icon name="check" size={12} />
              {/if}
            </span>
            <span class="file-icon">
              <Icon name={fileIcon} size={16} />
            </span>
            <span class="file-info">
              {#if fdir}
                <span class="file-dir">{fdir}/</span>
              {/if}
              <span class="file-name">{fname}</span>
            </span>
            <span class="file-size">{file.size > 0 ? formatSize(file.size) : '—'}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>

  {#if !loading && !error && files.length > 0}
    <div class="picker-footer">
      <span class="footer-summary">
        {$t('torrentSearch.fileSelection.totalSelected')
          .replace('{count}', String(selectedCount))
          .replace('{size}', totalSelectedSize > 0 ? formatSize(totalSelectedSize) : '0 B')}
      </span>
      <button
        class="confirm-btn"
        onclick={handleConfirm}
        disabled={selectedCount === 0}
      >
        <Icon name="download" size={16} />
        {$t('torrentSearch.fileSelection.confirm')}
      </button>
    </div>
  {/if}
</div>

<style>
  :global(.rotate-180) {
    transform: rotate(180deg);
  }

  .file-picker {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  /* Header */
  .picker-header {
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
  }

  .back-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .header-info {
    display: flex;
    align-items: center;
    gap: 6px;
    color: rgba(255, 255, 255, 0.7);
    flex-shrink: 0;
  }

  .header-title {
    font-size: 13px;
    font-weight: 600;
  }

  .header-spacer {
    flex: 1;
  }

  .header-count {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.4);
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex-shrink: 0;
  }

  /* Content */
  .picker-content {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 0;
  }

  /* State */
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
    font-size: 15px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.6);
  }

  .state-hint {
    margin: 0;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.35);
    max-width: 300px;
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

  /* Quick actions */
  .quick-actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .action-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.09);
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.65);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }

  .action-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  /* File list */
  .file-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .file-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--radius-sm, 6px);
    background: transparent;
    border: 1px solid transparent;
    cursor: pointer;
    text-align: left;
    transition: all 0.1s;
    width: 100%;
  }

  .file-row:hover {
    background: rgba(255, 255, 255, 0.04);
  }

  .file-row.selected {
    background: rgba(99, 102, 241, 0.08);
    border-color: rgba(99, 102, 241, 0.2);
  }

  .file-check {
    width: 16px;
    height: 16px;
    border-radius: 4px;
    border: 1.5px solid rgba(255, 255, 255, 0.2);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    transition: all 0.1s;
    color: white;
  }

  .file-check.checked {
    background: var(--accent, #6366f1);
    border-color: var(--accent, #6366f1);
  }

  .file-icon {
    color: rgba(255, 255, 255, 0.4);
    flex-shrink: 0;
    display: flex;
  }

  .file-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .file-dir {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-name {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.8);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-row.selected .file-name {
    color: white;
  }

  .file-size {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
    white-space: nowrap;
    flex-shrink: 0;
  }

  /* Footer */
  .picker-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 0;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
    flex-shrink: 0;
    flex-wrap: wrap;
  }

  .footer-summary {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .confirm-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    background: var(--accent, #6366f1);
    border: none;
    border-radius: var(--radius, 8px);
    color: white;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s;
    flex-shrink: 0;
  }

  .confirm-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .confirm-btn:not(:disabled):hover {
    opacity: 0.85;
  }
</style>
