<script lang="ts">
  interface Props {
    class?: string;
    my?: number; // margin-top and margin-bottom (same value)
    mt?: number; // margin-top
    mb?: number; // margin-bottom
  }

  let { class: className = '', my, mt, mb }: Props = $props();

  const marginTop = $derived(mt ?? my ?? 0);
  const marginBottom = $derived(mb ?? my ?? 0);

  const style = $derived(`margin-top: ${marginTop}px; margin-bottom: ${marginBottom}px;`);
</script>

<div class="divider {className}" {style}></div>

<style>
  .divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.07);
    width: 100%;
    box-sizing: border-box;
  }
</style>
