<script lang="ts">
  import { settings } from '$lib/stores/settings';
  import type {
    SurfaceStyle,
    ShadowIntensity,
    SurfaceSettings,
    BorderRadiusPreset,
    TextScalePreset,
  } from '$lib/stores/settings';

  const SURFACE_PRESETS: Record<SurfaceStyle, SurfaceSettings> = {
    glass: {
      opacity: 75,
      borderOpacity: 12,
      shadowIntensity: 'medium',
      accentTint: 0,
    },
    frosted: {
      opacity: 90,
      borderOpacity: 8,
      shadowIntensity: 'subtle',
      accentTint: 0,
    },
    elevated: {
      opacity: 100,
      borderOpacity: 5,
      shadowIntensity: 'strong',
      accentTint: 0,
    },
    accent: {
      opacity: 85,
      borderOpacity: 15,
      shadowIntensity: 'medium',
      accentTint: 15,
    },
    contrast: {
      opacity: 95,
      borderOpacity: 25,
      shadowIntensity: 'subtle',
      accentTint: 0,
    },
    custom: {
      opacity: 80,
      borderOpacity: 15,
      shadowIntensity: 'medium',
      accentTint: 5,
    },
  };

  const SHADOW_VALUES: Record<ShadowIntensity, string> = {
    none: 'none',
    subtle: '0 4px 12px rgba(0, 0, 0, 0.15), 0 2px 4px rgba(0, 0, 0, 0.1)',
    medium: '0 8px 24px rgba(0, 0, 0, 0.25), 0 4px 8px rgba(0, 0, 0, 0.15)',
    strong: '0 12px 40px rgba(0, 0, 0, 0.4), 0 6px 12px rgba(0, 0, 0, 0.25)',
  };

  const BLUR_VALUES: Record<SurfaceStyle, number> = {
    glass: 16,
    frosted: 24,
    elevated: 0,
    accent: 16,
    contrast: 12,
    custom: 16,
  };

  const BORDER_RADIUS_PRESETS: Record<Exclude<BorderRadiusPreset, 'custom'>, number> = {
    none: 0,
    subtle: 4,
    rounded: 10,
    pill: 20,
  };

  const TEXT_SCALE_PRESETS: Record<Exclude<TextScalePreset, 'custom'>, number> = {
    compact: 0.9,
    default: 1.0,
    large: 1.15,
  };

  let surfaceSettings = $derived.by(() => {
    const style = $settings.surfaceStyle || 'glass';
    if (style === 'custom') {
      return $settings.surfaceCustom || SURFACE_PRESETS.custom;
    }
    return SURFACE_PRESETS[style];
  });

  let currentStyle = $derived($settings.surfaceStyle || 'glass');

  let bgOpacity = $derived(surfaceSettings.opacity / 100);
  let borderOpacity = $derived(surfaceSettings.borderOpacity / 100);
  let shadow = $derived(SHADOW_VALUES[surfaceSettings.shadowIntensity]);
  let accentTint = $derived(surfaceSettings.accentTint / 100);
  let blur = $derived(BLUR_VALUES[currentStyle]);

  let borderRadius = $derived.by(() => {
    const preset = $settings.borderRadius || 'rounded';
    if (preset === 'custom') {
      return $settings.borderRadiusCustom ?? 10;
    }
    return BORDER_RADIUS_PRESETS[preset];
  });

  let textScale = $derived.by(() => {
    const preset = $settings.textScale || 'default';
    if (preset === 'custom') {
      return $settings.textScaleCustom ?? 1.0;
    }
    return TEXT_SCALE_PRESETS[preset];
  });

  function hexToRgb(hex: string): { r: number; g: number; b: number } {
    const num = parseInt(hex.replace('#', ''), 16);
    return {
      r: (num >> 16) & 0xff,
      g: (num >> 8) & 0xff,
      b: num & 0xff,
    };
  }

  let tintedBg = $derived.by(() => {
    if (accentTint === 0) return null;

    const accent = $settings.accentColor || '#6366F1';
    const rgb = hexToRgb(accent);

    const baseR = 18,
      baseG = 18,
      baseB = 18;
    const r = Math.round(baseR + (rgb.r - baseR) * accentTint * 0.3);
    const g = Math.round(baseG + (rgb.g - baseG) * accentTint * 0.3);
    const b = Math.round(baseB + (rgb.b - baseB) * accentTint * 0.3);

    return `${r}, ${g}, ${b}`;
  });

  $effect(() => {
    if (typeof document === 'undefined') return;

    const root = document.documentElement;

    root.style.setProperty('--surface-bg-opacity', bgOpacity.toString());
    root.style.setProperty('--surface-border-opacity', borderOpacity.toString());
    root.style.setProperty('--surface-shadow', shadow);
    root.style.setProperty('--surface-blur', `${blur}px`);
    root.style.setProperty('--surface-accent-tint', accentTint.toString());

    const baseBg = tintedBg || '18, 18, 18';
    root.style.setProperty('--surface-bg', `rgba(${baseBg}, ${bgOpacity})`);
    root.style.setProperty('--surface-bg-solid', `rgb(${baseBg})`);

    root.style.setProperty('--surface-border', `rgba(255, 255, 255, ${borderOpacity})`);

    const hoverOpacity = Math.min(bgOpacity + 0.05, 1);
    root.style.setProperty('--surface-bg-hover', `rgba(${baseBg}, ${hoverOpacity})`);

    const activeOpacity = Math.min(bgOpacity + 0.1, 1);
    root.style.setProperty('--surface-bg-active', `rgba(${baseBg}, ${activeOpacity})`);

    const mutedOpacity = Math.max(bgOpacity - 0.1, 0.3);
    root.style.setProperty('--surface-bg-muted', `rgba(${baseBg}, ${mutedOpacity})`);

    root.style.setProperty('--surface-text', 'rgba(255, 255, 255, 0.95)');
    root.style.setProperty('--surface-text-muted', 'rgba(255, 255, 255, 0.6)');

    root.style.setProperty('--radius', `${borderRadius}px`);
    root.style.setProperty('--radius-sm', `${Math.max(0, borderRadius - 4)}px`);
    root.style.setProperty('--radius-lg', `${borderRadius + 4}px`);
    root.style.setProperty('--radius-xl', `${borderRadius + 8}px`);

    root.style.setProperty('--text-scale', textScale.toString());
    root.style.setProperty('--text-xs', `${11 * textScale}px`);
    root.style.setProperty('--text-sm', `${12 * textScale}px`);
    root.style.setProperty('--text-base', `${13 * textScale}px`);
    root.style.setProperty('--text-md', `${14 * textScale}px`);
    root.style.setProperty('--text-lg', `${16 * textScale}px`);
    root.style.setProperty('--text-xl', `${18 * textScale}px`);
    root.style.setProperty('--text-2xl', `${22 * textScale}px`);

    root.classList.remove(
      'surface-glass',
      'surface-frosted',
      'surface-elevated',
      'surface-accent',
      'surface-contrast',
      'surface-custom'
    );
    root.classList.add(`surface-${currentStyle}`);
  });
</script>
