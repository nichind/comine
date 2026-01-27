<script lang="ts">
  import { onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { settings } from '$lib/stores/settings';
  import {
    extractDominantColor,
    getCachedColor,
    setColorInCache,
    rgbToRgba,
    type RGB,
  } from '$lib/utils/color';

  interface Props {
    thumbnailUrl: string | null | undefined;
    enabled?: boolean;
  }

  let { thumbnailUrl, enabled = true }: Props = $props();

  let glowColor = $state<RGB | null>(null);
  let destroyed = false;
  let currentUrl = $state<string | null>(null);
  let extractionInProgress = $state(false);

  let animationsDisabled = $derived($settings.disableAnimations);

  let glowStyle = $derived.by(() => {
    if (!glowColor || !enabled) return '';
    return `--glow-color: ${rgbToRgba(glowColor, 1)}; --glow-color-soft: ${rgbToRgba(glowColor, 0.12)}; --glow-color-medium: ${rgbToRgba(glowColor, 0.22)}; --glow-color-strong: ${rgbToRgba(glowColor, 0.35)};`;
  });

  let hasGlow = $derived(!!glowColor && enabled);

  function adjustBrightness(color: RGB, percent: number): RGB {
    const factor = 1 + percent / 100;
    return {
      r: Math.max(0, Math.min(255, Math.round(color.r * factor))),
      g: Math.max(0, Math.min(255, Math.round(color.g * factor))),
      b: Math.max(0, Math.min(255, Math.round(color.b * factor))),
    };
  }

  function applyThumbAccentVars(color: RGB) {
    const root = document.documentElement;
    const colorHex = `#${((1 << 24) | (color.r << 16) | (color.g << 8) | color.b).toString(16).slice(1)}`;
    const lightColor = adjustBrightness(color, 20);
    const darkColor = adjustBrightness(color, -15);

    root.style.setProperty('--thumb-accent', colorHex);
    root.style.setProperty('--thumb-accent-rgb', `${color.r}, ${color.g}, ${color.b}`);
    root.style.setProperty(
      '--thumb-accent-light',
      `#${((1 << 24) | (lightColor.r << 16) | (lightColor.g << 8) | lightColor.b).toString(16).slice(1)}`
    );
    root.style.setProperty(
      '--thumb-accent-dark',
      `#${((1 << 24) | (darkColor.r << 16) | (darkColor.g << 8) | darkColor.b).toString(16).slice(1)}`
    );
    root.style.setProperty('--thumb-accent-alpha', rgbToRgba(color, 0.2));
    root.style.setProperty('--thumb-accent-alpha-hover', rgbToRgba(color, 0.3));
    root.classList.add('has-thumb-accent');
  }

  function clearThumbAccentVars() {
    const root = document.documentElement;
    root.style.removeProperty('--thumb-accent');
    root.style.removeProperty('--thumb-accent-rgb');
    root.style.removeProperty('--thumb-accent-light');
    root.style.removeProperty('--thumb-accent-dark');
    root.style.removeProperty('--thumb-accent-alpha');
    root.style.removeProperty('--thumb-accent-alpha-hover');
    root.classList.remove('has-thumb-accent');
  }

  $effect(() => {
    if (glowColor && enabled) {
      applyThumbAccentVars(glowColor);
    } else {
      clearThumbAccentVars();
    }
  });

  async function extractColor(url: string) {
    if (!url || destroyed || extractionInProgress) return;
    if (currentUrl === url) return; // Already processed this URL (success or failure)

    extractionInProgress = true;

    try {
      const jsCached = getCachedColor(url);
      if (jsCached) {
        glowColor = jsCached;
        currentUrl = url;
        extractionInProgress = false;
        return;
      }

      try {
        const rustColor = await invoke<[number, number, number]>('extract_thumbnail_color', {
          url,
        });
        if (rustColor && !destroyed) {
          const color: RGB = { r: rustColor[0], g: rustColor[1], b: rustColor[2] };
          glowColor = color;
          currentUrl = url;
          setColorInCache(url, color);
          extractionInProgress = false;
          return;
        }
      } catch {}

      if (!destroyed) {
        const color = await extractDominantColor(url);
        if (color && !destroyed) {
          glowColor = color;
        }
        currentUrl = url;
      }
    } catch (e) {
      console.warn('[ThumbnailGlow] Color extraction failed:', e);
      currentUrl = url;
    } finally {
      if (!destroyed) {
        extractionInProgress = false;
      }
    }
  }

  $effect(() => {
    if (thumbnailUrl && enabled) {
      extractColor(thumbnailUrl);
    } else if (!thumbnailUrl) {
      glowColor = null;
      currentUrl = null;
    }
  });

  onDestroy(() => {
    destroyed = true;
    glowColor = null;
    clearThumbAccentVars();
  });
</script>

{#if hasGlow}
  <div class="thumbnail-glow-container" class:no-animation={animationsDisabled} style={glowStyle}>
    <div class="glow-orb glow-orb-primary"></div>
    <div class="glow-orb glow-orb-secondary"></div>
    <div class="glow-orb glow-orb-center"></div>
  </div>
{/if}

<style>
  .thumbnail-glow-container {
    position: fixed;
    inset: 0;
    z-index: -1;
    overflow: hidden;
    pointer-events: none;
    animation: glowFadeIn 0.6s ease-out;
  }

  .thumbnail-glow-container.no-animation {
    animation: none;
  }

  @keyframes glowFadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .glow-orb {
    position: absolute;
    border-radius: 50%;
    pointer-events: none;
    will-change: transform;
  }

  .glow-orb-primary {
    width: 60%;
    height: 60%;
    top: -15%;
    right: -15%;
    background: var(--glow-color-medium, transparent);
    filter: blur(100px);
    animation: glowPulse1 8s ease-in-out infinite;
  }

  .glow-orb-secondary {
    width: 45%;
    height: 45%;
    bottom: -10%;
    left: -10%;
    background: var(--glow-color-soft, transparent);
    filter: blur(80px);
    animation: glowPulse2 10s ease-in-out infinite;
  }

  .glow-orb-center {
    width: 30%;
    height: 30%;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    background: var(--glow-color-soft, transparent);
    filter: blur(100px);
    animation: glowPulse3 12s ease-in-out infinite;
  }

  .no-animation .glow-orb-primary,
  .no-animation .glow-orb-secondary,
  .no-animation .glow-orb-center {
    animation: none;
  }

  @keyframes glowPulse1 {
    0%,
    100% {
      opacity: 0.7;
      transform: scale(1) translate(0, 0);
    }
    50% {
      opacity: 0.85;
      transform: scale(1.08) translate(-15px, 15px);
    }
  }

  @keyframes glowPulse2 {
    0%,
    100% {
      opacity: 0.6;
      transform: scale(1) translate(0, 0);
    }
    50% {
      opacity: 0.75;
      transform: scale(1.1) translate(15px, -12px);
    }
  }

  @keyframes glowPulse3 {
    0%,
    100% {
      opacity: 0.4;
      transform: translate(-50%, -50%) scale(1);
    }
    50% {
      opacity: 0.55;
      transform: translate(-50%, -50%) scale(1.15);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .glow-orb-primary,
    .glow-orb-secondary,
    .glow-orb-center {
      animation: none;
    }

    .thumbnail-glow-container {
      animation: none;
    }
  }
</style>
