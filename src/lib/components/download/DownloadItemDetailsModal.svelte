<script lang="ts">
  import { portal } from '$lib/actions/portal';
  import { skeleton } from '$lib/actions/skeleton';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { t } from '$lib/i18n';
  import { queue } from '$lib/stores/queue';
  import type { UnifiedDownloadItem } from '$lib/stores/downloadsState.svelte';
  import { formatSize, formatSpeed } from '$lib/utils/format';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { isMobile } from '$lib/utils/android';
  import { invoke } from '@tauri-apps/api/core';
  import { tick } from 'svelte';
  import type { MediaTechnicalInfo, MediaStreamInfo } from '$lib/bindings';

  interface Props {
    open: boolean;
    item: UnifiedDownloadItem | null;
    onClose: () => void;
  }

  let { open, item, onClose }: Props = $props();

  let dialogEl: HTMLDivElement | null = $state(null);

  let qItem = $derived.by(() => {
    if (!item) return undefined;
    return $queue.items.find((i) => i.id === item.id);
  });

  let title = $derived(item?.title ?? '');
  let url = $derived(item?.url ?? '');

  let statusLine = $derived.by(() => {
    if (!item) return '';
    if (item.status === 'failed') return item.error || $t('downloads.failed');

    const progress = Math.max(0, Math.min(100, Math.round(item.progress ?? 0)));

    if (!item.isActive) return $t('downloads.completed');

    const label = item.statusMessage || (item.status ? $t(`downloads.status.${item.status}`) : '');
    if (label && Number.isFinite(progress)) return `${label} ${progress}%`;
    if (label) return label;
    if (Number.isFinite(progress)) return `${progress}%`;
    return '';
  });

  let speedLine = $derived.by(() => {
    if (!item?.isActive) return '';
    const speedBps = qItem?.speedBps;
    if (typeof speedBps === 'number' && speedBps > 0) return formatSpeed(speedBps);
    if (item.speed) return item.speed;
    return '';
  });

  let bytesLine = $derived.by(() => {
    const dl = qItem?.downloadedBytes;
    const total = qItem?.totalBytes;
    if (typeof dl === 'number' && typeof total === 'number' && total > 0) {
      return `${formatSize(dl)} / ${formatSize(total)}`;
    }
    if (typeof dl === 'number' && dl > 0) return formatSize(dl);
    return '';
  });

  let postProcessFlags = $derived.by(() => {
    const flags: string[] = [];
    const opt = qItem?.options;
    if (!opt) return flags;

    if (opt.convertToMp4) flags.push($t('downloads.details.flags.convertToMp4'));
    if (opt.remux) flags.push($t('downloads.details.flags.remux'));
    if (opt.embedThumbnail) flags.push($t('downloads.status.embeddingThumbnail'));
    if (opt.embedSubtitles) flags.push($t('downloads.details.flags.embedSubtitles'));
    if (opt.clearMetadata) flags.push($t('downloads.details.flags.clearMetadata'));
    if (opt.chapters) flags.push($t('downloads.details.flags.chapters'));
    if (opt.sponsorBlock) flags.push($t('downloads.details.flags.sponsorBlock'));

    return flags;
  });

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {}
  }

  async function openFileLocation(filePath: string) {
    if (isMobile()) return;
    try {
      await revealItemInDir(filePath);
    } catch {}
  }

  let technicalLoading = $state(false);
  let technicalError = $state<string | null>(null);
  let technicalInfo = $state<MediaTechnicalInfo | null>(null);
  let lastTechPath = $state<string | null>(null);

  $effect(() => {
    if (open) return;
    technicalLoading = false;
    technicalError = null;
    technicalInfo = null;
    lastTechPath = null;
  });

  $effect(() => {
    if (!open) return;
    tick().then(() => dialogEl?.focus());

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return;

      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
        return;
      }
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  });

  $effect(() => {
    if (!open || !item) return;

    if (isMobile()) return;
    if (!item.filePath) return;

    if (lastTechPath === item.filePath && (technicalInfo || technicalError)) return;

    lastTechPath = item.filePath;
    technicalLoading = true;
    technicalError = null;
    technicalInfo = null;

    invoke<MediaTechnicalInfo>('get_media_technical_info', { filePath: item.filePath })
      .then((info) => {
        technicalInfo = info;
      })
      .catch((e) => {
        technicalError = String(e);
      })
      .finally(() => {
        technicalLoading = false;
      });
  });

  function isAudioStream(s: MediaStreamInfo) {
    return s.codecType === 'audio';
  }
  function isVideoStream(s: MediaStreamInfo) {
    return s.codecType === 'video';
  }

  function formatBitrate(bps: number | null | undefined) {
    if (!bps || !Number.isFinite(bps) || bps <= 0) return '';
    const kbps = bps / 1000;
    if (kbps < 1000) return `${Math.round(kbps)} kbps`;
    return `${(kbps / 1000).toFixed(2)} Mbps`;
  }

  function formatHz(hz: number | null | undefined) {
    if (!hz || !Number.isFinite(hz) || hz <= 0) return '';
    if (hz >= 1000) return `${(hz / 1000).toFixed(1)} kHz`;
    return `${Math.round(hz)} Hz`;
  }

  function parseFps(rate: string | null | undefined) {
    if (!rate) return '';
    const r = rate.trim();
    if (!r) return '';
    const parts = r.split('/');
    if (parts.length === 2) {
      const num = Number(parts[0]);
      const den = Number(parts[1]);
      if (!Number.isFinite(num) || !Number.isFinite(den) || den === 0) return '';
      const fps = num / den;
      if (!Number.isFinite(fps) || fps <= 0) return '';
      return fps >= 10 ? fps.toFixed(2) : fps.toFixed(3);
    }
    const n = Number(r);
    if (!Number.isFinite(n) || n <= 0) return '';
    return n >= 10 ? n.toFixed(2) : n.toFixed(3);
  }

  function getFocusableElements(root: HTMLElement): HTMLElement[] {
    const nodes = root.querySelectorAll<HTMLElement>(
      'button,[href],input,select,textarea,[tabindex]:not([tabindex="-1"])'
    );
    const res: HTMLElement[] = [];
    nodes.forEach((el) => {
      if ((el as HTMLButtonElement).disabled) return;
      if (el.getAttribute('aria-disabled') === 'true') return;
      if (el.offsetParent === null) return;
      res.push(el);
    });
    return res;
  }

  function handleDialogKeydown(e: KeyboardEvent) {
    if (e.key !== 'Tab') return;
    if (!dialogEl) return;

    const focusables = getFocusableElements(dialogEl);
    if (focusables.length === 0) {
      e.preventDefault();
      dialogEl.focus();
      return;
    }

    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement as HTMLElement | null;

    if (e.shiftKey) {
      if (!active || active === first || !dialogEl.contains(active)) {
        e.preventDefault();
        last.focus();
      }
    } else {
      if (!active || active === last || !dialogEl.contains(active)) {
        e.preventDefault();
        first.focus();
      }
    }
  }
</script>

{#if open && item}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="details-overlay" use:portal onclick={onClose}>
    <div
      class="details-modal"
      bind:this={dialogEl}
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleDialogKeydown}
      role="dialog"
      aria-modal="true"
      tabindex="-1"
    >
      <div class="details-header">
        <div class="details-title" {title}>{title}</div>
        <button class="details-close" onclick={onClose} aria-label={$t('common.close')}>
          <Icon name="close" size={16} />
        </button>
      </div>

      <div class="details-body">
        <div class="kv">
          <span class="k">{$t('downloads.details.status')}</span>
          <span class="v">{statusLine || '—'}</span>
        </div>

        {#if speedLine}
          <div class="kv">
            <span class="k">{$t('downloads.details.speed')}</span>
            <span class="v mono">{speedLine}</span>
          </div>
        {/if}

        {#if item.eta}
          <div class="kv">
            <span class="k">{$t('downloads.details.eta')}</span>
            <span class="v mono">{item.eta}</span>
          </div>
        {/if}

        {#if bytesLine}
          <div class="kv">
            <span class="k">{$t('downloads.details.transferred')}</span>
            <span class="v mono">{bytesLine}</span>
          </div>
        {/if}

        {#if item.filePath}
          <div class="kv">
            <span class="k">{$t('downloads.details.filePath')}</span>
            <span class="v mono wrap">{item.filePath}</span>
          </div>
        {/if}

        <div class="kv">
          <span class="k">{$t('downloads.details.url')}</span>
          <span class="v mono wrap">{url}</span>
        </div>

        {#if qItem?.jobId}
          <div class="kv">
            <span class="k">{$t('downloads.details.jobId')}</span>
            <span class="v mono">{qItem.jobId}</span>
          </div>
        {/if}

        {#if qItem?.backendName}
          <div class="kv">
            <span class="k">{$t('downloads.details.backend')}</span>
            <span class="v">{qItem.backendName}</span>
          </div>
        {/if}

        {#if postProcessFlags.length > 0}
          <div class="section-title">{$t('downloads.details.postProcessing')}</div>
          <div class="flags">
            {#each postProcessFlags as f (f)}
              <span class="flag">{f}</span>
            {/each}
          </div>
        {/if}

        {#if item.filePath && !isMobile()}
          <div class="section-title">{$t('downloads.details.media.title')}</div>

          {#if technicalLoading}
            <div class="media-skeleton" aria-hidden="true">
              <div
                class="s-row"
                use:skeleton={{ loading: true, minWidth: '70%', minHeight: '14px', radius: '6px' }}
              ></div>
              <div
                class="s-row"
                use:skeleton={{ loading: true, minWidth: '55%', minHeight: '14px', radius: '6px' }}
              ></div>
              <div
                class="s-row"
                use:skeleton={{ loading: true, minWidth: '85%', minHeight: '14px', radius: '6px' }}
              ></div>
              <div
                class="s-row"
                use:skeleton={{ loading: true, minWidth: '60%', minHeight: '14px', radius: '6px' }}
              ></div>
            </div>
          {:else if technicalError}
            <div class="hint error">{$t('downloads.details.media.failed')}</div>
          {:else if technicalInfo}
            {#if technicalInfo.format?.formatName}
              <div class="kv">
                <span class="k">{$t('downloads.details.media.container')}</span>
                <span class="v mono">
                  {technicalInfo.format.formatName}
                  {#if technicalInfo.format?.formatLongName}
                    <span class="muted"> ({technicalInfo.format.formatLongName})</span>
                  {/if}
                </span>
              </div>
            {/if}

            {#if technicalInfo.format?.bitRate}
              <div class="kv">
                <span class="k">{$t('downloads.details.media.overallBitrate')}</span>
                <span class="v mono">{formatBitrate(technicalInfo.format.bitRate)}</span>
              </div>
            {/if}

            {#each technicalInfo.streams.filter(isVideoStream) as s (s.index)}
              <div class="kv">
                <span class="k">{$t('downloads.details.media.video')}</span>
                <span class="v">
                  <span class="mono">
                    {s.codecName || '—'}
                    {#if s.profile}
                      <span class="muted">({s.profile})</span>{/if}
                    {#if s.codecLongName}
                      <span class="muted"> • {s.codecLongName}</span>
                    {/if}
                  </span>
                  {#if s.width && s.height}
                    <span class="muted"> • {s.width}×{s.height}</span>
                  {/if}
                  {#if parseFps(s.avgFrameRate)}
                    <span class="muted">
                      • {$t('downloads.details.media.fps')}: {parseFps(s.avgFrameRate)}</span
                    >
                  {:else if parseFps(s.rFrameRate)}
                    <span class="muted">
                      • {$t('downloads.details.media.fps')}: {parseFps(s.rFrameRate)}</span
                    >
                  {/if}
                  {#if s.bitRate}
                    <span class="muted"> • {formatBitrate(s.bitRate)}</span>
                  {/if}
                </span>
              </div>
            {/each}

            {#each technicalInfo.streams.filter(isAudioStream) as s (s.index)}
              <div class="kv">
                <span class="k">{$t('downloads.details.media.audio')}</span>
                <span class="v">
                  <span class="mono">
                    {s.codecName || '—'}
                    {#if s.profile}
                      <span class="muted">({s.profile})</span>{/if}
                    {#if s.codecLongName}
                      <span class="muted"> • {s.codecLongName}</span>
                    {/if}
                  </span>
                  {#if s.sampleRate}
                    <span class="muted"> • {formatHz(s.sampleRate)}</span>
                  {/if}
                  {#if s.channels}
                    <span class="muted">
                      • {$t('downloads.details.media.channels')}: {s.channels}</span
                    >
                  {/if}
                  {#if s.channelLayout}
                    <span class="muted"> • {s.channelLayout}</span>
                  {/if}
                  {#if s.bitRate}
                    <span class="muted"> • {formatBitrate(s.bitRate)}</span>
                  {/if}
                </span>
              </div>
            {/each}

            <div class="actions">
              <button class="action" onclick={() => copy(JSON.stringify(technicalInfo, null, 2))}>
                <Icon name="copy" size={14} />
                <span>{$t('downloads.details.media.copyJson')}</span>
              </button>
            </div>
          {/if}
        {/if}

        <div class="actions">
          <button class="action" onclick={() => copy(url)}>
            <Icon name="link" size={14} />
            <span>{$t('downloads.copyUrl')}</span>
          </button>
          {#if item.filePath}
            <button
              class="action"
              onclick={() => openFileLocation(item.filePath!)}
              disabled={isMobile()}
            >
              <Icon name="folder" size={14} />
              <span>{$t('downloads.openFolder')}</span>
            </button>
          {/if}
        </div>

        {#if item.error}
          <div class="section-title">{$t('downloads.details.error')}</div>
          <div class="error">{item.error}</div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .details-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 18px;
    box-sizing: border-box;
  }

  .details-modal {
    width: min(640px, 100%);
    max-height: min(78vh, 760px);
    overflow: hidden;
    border-radius: var(--radius, 12px);
    background: var(--surface-elevated-bg, rgba(20, 20, 22, 0.92));
    border: 1px solid var(--surface-border, rgba(255, 255, 255, 0.1));
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(var(--surface-blur, 12px));
  }

  .details-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 12px 12px 10px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .details-title {
    font-size: 13px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.92);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .details-close {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 0;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
  }

  .details-close:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .details-body {
    padding: 12px;
    overflow: auto;
    max-height: calc(78vh - 54px);
  }

  .kv {
    display: grid;
    grid-template-columns: 140px 1fr;
    gap: 10px;
    padding: 8px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }

  .k {
    color: rgba(255, 255, 255, 0.55);
    font-size: 12px;
  }

  .v {
    color: rgba(255, 255, 255, 0.9);
    font-size: 12px;
  }

  .mono {
    font-variant-numeric: tabular-nums;
  }

  .wrap {
    word-break: break-word;
  }

  .muted {
    color: rgba(255, 255, 255, 0.55);
  }

  .hint {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.55);
    padding: 8px 0;
  }

  .hint.error {
    color: rgba(255, 200, 200, 0.95);
  }

  .media-skeleton {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 8px 0;
  }

  .s-row {
    display: block;
  }

  .section-title {
    margin-top: 14px;
    margin-bottom: 8px;
    font-size: 12px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.72);
  }

  .flags {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .flag {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.75);
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.04);
    border-radius: 999px;
    padding: 4px 8px;
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 12px;
    flex-wrap: wrap;
  }

  .action {
    height: 32px;
    padding: 0 10px;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.85);
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  .action:hover {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .error {
    font-size: 12px;
    color: rgba(255, 200, 200, 0.95);
    border: 1px solid rgba(255, 90, 90, 0.22);
    background: rgba(255, 70, 70, 0.06);
    border-radius: 10px;
    padding: 10px;
    white-space: pre-wrap;
  }

  @media (max-width: 520px) {
    .kv {
      grid-template-columns: 1fr;
      gap: 6px;
    }
  }
</style>
