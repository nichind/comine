<script lang="ts">
  interface Props {
    count?: number;
    width?: string;
    height?: string;
    radius?: string;
    circle?: boolean;
    pill?: boolean;
    inline?: boolean;
    duration?: number;
    block?: boolean;
    class?: string;
    style?: string;
  }

  let {
    count = 1,
    width,
    height,
    radius,
    circle = false,
    pill = false,
    inline = false,
    duration = 1.5,
    block = false,
    class: className = '',
    style = '',
  }: Props = $props();

  let skeletonStyle = $derived.by(() => {
    const parts: string[] = [];
    if (width) parts.push(`width: ${width}`);
    if (height) parts.push(`height: ${height}`);
    if (radius) parts.push(`border-radius: ${radius}`);
    if (circle) parts.push('border-radius: 50%');
    if (pill) parts.push('border-radius: 999px');
    if (duration !== 1.5) parts.push(`--skeleton-duration: ${duration}s`);
    if (style) parts.push(style);
    return parts.join('; ');
  });

  let items = $derived(Array.from({ length: Math.ceil(count) }, (_, i) => i));
  let lastItemWidth = $derived.by(() => {
    const fraction = count % 1;
    return fraction > 0 ? `${fraction * 100}%` : null;
  });
</script>

<span class="skeleton-wrapper {className}" class:inline class:block>
  {#each items as i (i)}
    <span
      class="skeleton"
      style={i === items.length - 1 && lastItemWidth
        ? `${skeletonStyle}; width: ${lastItemWidth}`
        : skeletonStyle}
    ></span>
    {#if !inline && i < items.length - 1}<br />{/if}
  {/each}
</span>

<style>
  @keyframes skeleton-shimmer {
    from {
      background-position: 200% 0;
    }
    to {
      background-position: -200% 0;
    }
  }

  .skeleton-wrapper {
    display: inline;
    line-height: inherit;
  }

  .skeleton-wrapper.inline {
    display: inline-flex;
    gap: 4px;
  }

  .skeleton-wrapper.block {
    display: block;
    width: 100%;
    height: 100%;
  }

  .skeleton-wrapper.block .skeleton {
    display: block;
  }

  .skeleton {
    display: inline-block;
    width: 100%;
    height: 1em;
    background: linear-gradient(
      90deg,
      var(--skeleton-base, rgba(255, 255, 255, 0.06)) 0%,
      var(--skeleton-highlight, rgba(255, 255, 255, 0.12)) 50%,
      var(--skeleton-base, rgba(255, 255, 255, 0.06)) 100%
    );
    background-size: 200% 100%;
    animation: skeleton-shimmer var(--skeleton-duration, 1.5s) ease-in-out infinite;
    border-radius: var(--radius-sm, 4px);
    line-height: 1;
    vertical-align: middle;
  }
</style>
