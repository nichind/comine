<script lang="ts">
  import { deps, type DependencyName } from '$lib/stores/deps';
  import { t } from '$lib/i18n';
  import { isAndroid } from '$lib/utils/android';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import { toast, updateToast, dismissToast } from '$lib/components/ui/Toast.svelte';

  const bundleDeps: DependencyName[] = ['aria2', 'ytdlp', 'ffmpeg', 'quickjs'];

  let bundleToastId = $state<number | null>(null);
  let isInstallingBundle = $state(false);

  let missingDeps = $derived(bundleDeps.filter((dep) => !$deps[dep]?.installed));

  let installingCount = $derived(bundleDeps.filter((dep) => $deps.installingDeps.has(dep)).length);

  let overallProgress = $derived.by(() => {
    if (installingCount === 0) return 0;

    let totalProgress = 0;
    let count = 0;

    for (const dep of bundleDeps) {
      if ($deps.installingDeps.has(dep)) {
        const progress = $deps.installProgressMap.get(dep);
        totalProgress += progress?.progress ?? 0;
        count++;
      } else if ($deps[dep]?.installed) {
        totalProgress += 100;
        count++;
      }
    }

    return count > 0 ? Math.round(totalProgress / bundleDeps.length) : 0;
  });

  let currentInstallingDep = $derived.by(() => {
    for (const dep of bundleDeps) {
      if ($deps.installingDeps.has(dep)) {
        return dep;
      }
    }
    return null;
  });

  async function installBundle() {
    if (isInstallingBundle || missingDeps.length === 0) return;

    isInstallingBundle = true;

    bundleToastId = toast.progress($t('deps.installingBundle'), 0);

    let successCount = 0;
    let failCount = 0;

    for (const dep of missingDeps) {
      if ($deps[dep]?.installed) continue;

      let success = false;
      if (dep === 'ytdlp') {
        success = await deps.installYtdlp();
      } else {
        success = await deps.install(dep);
      }

      if (success) {
        successCount++;
      } else {
        failCount++;
      }
    }

    if (bundleToastId !== null) {
      dismissToast(bundleToastId);
      bundleToastId = null;
    }

    if (failCount === 0) {
      toast.success($t('deps.bundleComplete'));
    } else if (successCount > 0) {
      toast.warning($t('deps.bundlePartial', { success: successCount, failed: failCount }));
    } else {
      toast.error($t('deps.bundleFailed'));
    }

    isInstallingBundle = false;
  }

  $effect(() => {
    if (bundleToastId !== null && currentInstallingDep) {
      const progress = $deps.installProgressMap.get(currentInstallingDep);
      updateToast(bundleToastId, {
        progress: overallProgress,
        subMessage: currentInstallingDep + (progress?.message ? `: ${progress.message}` : ''),
      });
    }
  });

  let showBanner = $derived(
    !isAndroid() && (missingDeps.length > 0 || installingCount > 0 || $deps.error)
  );
</script>

{#if showBanner}
  <div class="setup-banner">
    {#if $deps.error}
      <div class="banner-content error">
        <Icon name="cross_circle" size={18} />
        <span class="error-text">{$deps.error}</span>
        <button class="dismiss-btn" onclick={() => deps.clearError()}>
          {$t('common.dismiss')}
        </button>
      </div>
    {:else if installingCount > 0}
      <div class="banner-content installing">
        <div class="banner-left">
          <div class="spinner"></div>
          <div class="banner-text">
            <span class="banner-title">{$t('deps.installingBundle')}</span>
            <span class="banner-sub">{currentInstallingDep} • {overallProgress}%</span>
          </div>
        </div>
        <div class="progress-bar">
          <div class="progress-fill" style="width: {overallProgress}%"></div>
        </div>
      </div>
    {:else}
      <div class="banner-content">
        <div class="banner-left">
          <Icon name="download" size={18} />
          <div class="banner-text">
            <span class="banner-title">{$t('deps.setupRequired')}</span>
            <span class="banner-sub">{missingDeps.length} {$t('deps.componentsNeeded')}</span>
          </div>
        </div>
        <Button variant="primary" size="sm" onclick={installBundle}>
          {$t('deps.installAll')}
        </Button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .setup-banner {
    margin-bottom: 12px;
  }

  .banner-content {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 16px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius, 10px);
    transition: all 0.2s;
  }

  .banner-content.installing {
    flex-direction: column;
    align-items: stretch;
    gap: 10px;
    border-color: rgba(99, 102, 241, 0.25);
    background: rgba(99, 102, 241, 0.05);
  }

  .banner-content.error {
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.2);
  }

  .banner-left {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .banner-content > :global(svg) {
    color: rgba(255, 255, 255, 0.5);
    flex-shrink: 0;
  }

  .banner-content.error > :global(svg) {
    color: rgba(239, 68, 68, 0.8);
  }

  .banner-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .banner-title {
    font-size: 13px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.9);
  }

  .banner-sub {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
  }

  .progress-bar {
    width: 100%;
    height: 4px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--accent, #6366f1);
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .spinner {
    width: 18px;
    height: 18px;
    border: 2px solid rgba(99, 102, 241, 0.3);
    border-top-color: var(--accent, #6366f1);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .error-text {
    flex: 1;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.7);
  }

  .dismiss-btn {
    padding: 6px 12px;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.25);
    border-radius: var(--radius-sm, 6px);
    color: rgba(239, 68, 68, 0.9);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }

  .dismiss-btn:hover {
    background: rgba(239, 68, 68, 0.25);
  }
</style>
