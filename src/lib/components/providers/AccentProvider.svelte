<script lang="ts">
  import { settings } from '$lib/stores/settings';
  import { logs } from '$lib/stores/logs';
  import { isAndroid } from '$lib/utils/android';
  import { hslToHex, adjustBrightnessHex as adjustBrightness, hexToRgba } from '$lib/utils/color';
  import { onMount, onDestroy } from 'svelte';

  let systemAccentColor = $state<string | null>(null);
  let onMobile = $state(false);
  let rgbHue = $state(0);
  let rgbAnimationFrame: number | null = null;
  let lastRgbUpdate = 0;
  let fetchingSystemAccent = $state(false);
  let triedSystemAccentFetch = $state(false);
  let systemAccentRetryTimer: number | null = null;
  let systemAccentRetryAttempts = 0;

  let isRgbMode = $derived($settings.accentColor === 'rgb');

  function rgbLoop(timestamp: number) {
    if (!isRgbMode) return;
    if (document.visibilityState === 'hidden') {
      rgbAnimationFrame = null;
      return;
    }

    if (timestamp - lastRgbUpdate >= 100) {
      rgbHue = (rgbHue + 3) % 360;
      lastRgbUpdate = timestamp;
    }

    rgbAnimationFrame = requestAnimationFrame(rgbLoop);
  }

  function handleVisibilityChange() {
    if (document.visibilityState === 'visible' && isRgbMode && !rgbAnimationFrame) {
      lastRgbUpdate = 0;
      rgbAnimationFrame = requestAnimationFrame(rgbLoop);
    }
  }

  let effectiveAccent = $derived.by(() => {
    if ($settings.useSystemAccent && systemAccentColor) {
      return systemAccentColor;
    }
    if (isRgbMode) {
      return hslToHex(rgbHue / 360, 0.75, 0.5);
    }
    return $settings.accentColor || '#6366F1';
  });

  let accentStyle = $derived($settings.accentStyle || 'solid');

  let accentSecondary = $derived.by(() => {
    const hex = effectiveAccent.replace('#', '');
    const r = parseInt(hex.substring(0, 2), 16);
    const g = parseInt(hex.substring(2, 4), 16);
    const b = parseInt(hex.substring(4, 6), 16);

    const max = Math.max(r, g, b) / 255;
    const min = Math.min(r, g, b) / 255;
    let h = 0,
      s = 0;
    const l = (max + min) / 2;

    if (max !== min) {
      const d = max - min;
      s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
      switch (max) {
        case r / 255:
          h = ((g / 255 - b / 255) / d + (g < b ? 6 : 0)) / 6;
          break;
        case g / 255:
          h = ((b / 255 - r / 255) / d + 2) / 6;
          break;
        case b / 255:
          h = ((r / 255 - g / 255) / d + 4) / 6;
          break;
      }
    }

    const h2 = (h + 0.083) % 1;
    const s2 = Math.min(s * 1.1, 1);
    const l2 = Math.min(l * 1.15, 0.85);

    const hue2rgb = (p: number, q: number, t: number) => {
      if (t < 0) t += 1;
      if (t > 1) t -= 1;
      if (t < 1 / 6) return p + (q - p) * 6 * t;
      if (t < 1 / 2) return q;
      if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
      return p;
    };

    const q = l2 < 0.5 ? l2 * (1 + s2) : l2 + s2 - l2 * s2;
    const p = 2 * l2 - q;
    const r2 = Math.round(hue2rgb(p, q, h2 + 1 / 3) * 255);
    const g2 = Math.round(hue2rgb(p, q, h2) * 255);
    const b2 = Math.round(hue2rgb(p, q, h2 - 1 / 3) * 255);

    return `#${((1 << 24) | (r2 << 16) | (g2 << 8) | b2).toString(16).slice(1)}`;
  });

  let accentLight = $derived(adjustBrightness(effectiveAccent, 30));
  let accentDark = $derived(adjustBrightness(effectiveAccent, -20));
  let accentAlpha = $derived(hexToRgba(effectiveAccent, 0.2));
  let accentAlphaHover = $derived(hexToRgba(effectiveAccent, 0.3));

  let accentGradient = $derived(
    `linear-gradient(135deg, ${effectiveAccent} 0%, ${accentSecondary} 100%)`
  );
  let accentGradientHover = $derived(
    `linear-gradient(135deg, ${accentLight} 0%, ${adjustBrightness(accentSecondary, 15)} 100%)`
  );

  onMount(() => {
    onMobile = isAndroid();
    fetchSystemAccent();
    document.addEventListener('visibilitychange', handleVisibilityChange);
  });

  $effect(() => {
    if (!isAndroid()) return;
    if (!$settings.useSystemAccent) {
      triedSystemAccentFetch = false;
      systemAccentRetryAttempts = 0;
      if (systemAccentRetryTimer !== null) {
        clearTimeout(systemAccentRetryTimer);
        systemAccentRetryTimer = null;
      }
      return;
    }
    if (systemAccentColor) return;
    if (triedSystemAccentFetch) return;

    triedSystemAccentFetch = true;
    fetchSystemAccent();
  });

  $effect(() => {
    if (isRgbMode) {
      lastRgbUpdate = 0;
      rgbAnimationFrame = requestAnimationFrame(rgbLoop);
    } else {
      if (rgbAnimationFrame) {
        cancelAnimationFrame(rgbAnimationFrame);
        rgbAnimationFrame = null;
      }
    }
  });

  onDestroy(() => {
    if (rgbAnimationFrame) {
      cancelAnimationFrame(rgbAnimationFrame);
    }

    if (systemAccentRetryTimer !== null) {
      clearTimeout(systemAccentRetryTimer);
      systemAccentRetryTimer = null;
    }

    document.removeEventListener('visibilitychange', handleVisibilityChange);
  });

  async function fetchSystemAccent() {
    if (!isAndroid()) return;
    if (fetchingSystemAccent) return;

    fetchingSystemAccent = true;
    try {
      const androidColors = await getAndroidMaterialColors();
      if (androidColors?.primary) {
        systemAccentColor = androidColors.primary;
        logs.debug('AccentProvider', `Got Android Material color: ${systemAccentColor}`);
        systemAccentRetryAttempts = 0;
        if (systemAccentRetryTimer !== null) {
          clearTimeout(systemAccentRetryTimer);
          systemAccentRetryTimer = null;
        }
        return;
      }

      if ($settings.useSystemAccent && !systemAccentColor && systemAccentRetryAttempts < 10) {
        systemAccentRetryAttempts += 1;
        if (systemAccentRetryTimer !== null) {
          clearTimeout(systemAccentRetryTimer);
        }
        systemAccentRetryTimer = window.setTimeout(() => {
          systemAccentRetryTimer = null;
          fetchSystemAccent();
        }, 500);
      }
    } catch (e) {
      logs.warn('AccentProvider', `Could not get Android colors: ${e}`);
    } finally {
      fetchingSystemAccent = false;
    }
  }

  async function getAndroidMaterialColors(): Promise<{
    primary?: string;
    secondary?: string;
  } | null> {
    return new Promise((resolve) => {
      if (typeof window !== 'undefined' && window.AndroidColors) {
        try {
          const tryParse = (raw: string | null | undefined) => {
            if (!raw) return null;
            try {
              return JSON.parse(raw);
            } catch {
              return null;
            }
          };

          const parsedMaterial = tryParse(window.AndroidColors.getMaterialColors());
          if (parsedMaterial?.primary) {
            resolve(parsedMaterial);
            return;
          }

          const wallpaperFn = window.AndroidColors.getWallpaperColors;
          if (typeof wallpaperFn === 'function') {
            const parsedWallpaper = tryParse(wallpaperFn());
            if (parsedWallpaper?.primary) {
              resolve(parsedWallpaper);
              return;
            }
          }
        } catch (e) {
          console.error('Failed to parse Android colors:', e);
        }
      }
      resolve(null);
    });
  }

  $effect(() => {
    if (typeof document !== 'undefined') {
      document.documentElement.style.setProperty('--accent', effectiveAccent);
      document.documentElement.style.setProperty('--accent-secondary', accentSecondary);
      document.documentElement.style.setProperty('--accent-light', accentLight);
      document.documentElement.style.setProperty('--accent-dark', accentDark);
      document.documentElement.style.setProperty('--accent-alpha', accentAlpha);
      document.documentElement.style.setProperty('--accent-alpha-hover', accentAlphaHover);
      document.documentElement.style.setProperty('--accent-gradient', accentGradient);
      document.documentElement.style.setProperty('--accent-gradient-hover', accentGradientHover);
      document.documentElement.style.setProperty('--accent-style', accentStyle);

      if (accentStyle === 'gradient') {
        document.documentElement.style.setProperty('--accent-bg', accentGradient);
        document.documentElement.style.setProperty('--accent-bg-hover', accentGradientHover);
      } else {
        document.documentElement.style.setProperty('--accent-bg', effectiveAccent);
        document.documentElement.style.setProperty('--accent-bg-hover', accentLight);
      }

      document.documentElement.classList.remove('accent-solid', 'accent-gradient', 'accent-glow');
      document.documentElement.classList.add(`accent-${accentStyle}`);
    }
  });
</script>
