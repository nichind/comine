<script lang="ts">
  interface Props {
    text: string;
    highlight: string;
    class?: string;
  }

  let { text, highlight, class: className = '' }: Props = $props();

  function escapeRegExp(string: string) {
    return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }

  let parts = $derived.by(() => {
    if (!highlight || !highlight.trim()) return [{ text, highlight: false }];

    const tokens = highlight
      .trim()
      .split(/\s+/)
      .filter((t) => t.length > 0);
    if (tokens.length === 0) return [{ text, highlight: false }];

    const pattern = new RegExp(`(${tokens.map(escapeRegExp).join('|')})`, 'gi');

    const result: { text: string; highlight: boolean }[] = [];
    let lastIndex = 0;

    const matches = [...text.matchAll(pattern)];

    for (const match of matches) {
      const matchIndex = match.index!;
      const matchText = match[0];

      if (matchIndex > lastIndex) {
        result.push({ text: text.substring(lastIndex, matchIndex), highlight: false });
      }

      result.push({ text: matchText, highlight: true });

      lastIndex = matchIndex + matchText.length;
    }

    if (lastIndex < text.length) {
      result.push({ text: text.substring(lastIndex), highlight: false });
    }

    return result;
  });
</script>

<span class="highlight-text {className}">
  {#each parts as part}
    {#if part.highlight}
      <mark>{part.text}</mark>
    {:else}
      <span>{part.text}</span>
    {/if}
  {/each}
</span>

<style>
  mark {
    background: rgba(255, 235, 59, 0.25);
    color: #ffd700;
    border-radius: 2px;
    padding: 0 1px;
    font-weight: inherit;
  }
</style>
