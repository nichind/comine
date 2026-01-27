<script lang="ts">
  import { skeleton, type SkeletonOptions } from '$lib/actions/skeleton';

  interface Props {
    width?: string;
    height?: string;
    radius?: string;
    circle?: boolean;
    pill?: boolean;
    size?: string;
    static?: boolean;
    class?: string;
    style?: string;
  }

  let {
    width,
    height,
    radius,
    circle = false,
    pill = false,
    size,
    static: isStatic = false,
    class: className = '',
    style = '',
  }: Props = $props();

  let options = $derived<SkeletonOptions>({
    loading: true,
    width: circle && size ? size : width,
    height: circle && size ? size : height,
    radius,
    shape: circle ? 'circle' : pill ? 'pill' : 'default',
    static: isStatic,
  });
</script>

<div class="skeleton-placeholder {className}" {style} use:skeleton={options}></div>

<style>
  .skeleton-placeholder {
    display: block;
  }
</style>
