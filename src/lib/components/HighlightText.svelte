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
    
    // Split query by spaces to highlight individual tokens
    const tokens = highlight.trim().split(/\s+/).filter(t => t.length > 0);
    if (tokens.length === 0) return [{ text, highlight: false }];

    // Create a regex that matches any of the tokens, case insensitive
    const pattern = new RegExp(`(${tokens.map(escapeRegExp).join('|')})`, 'gi');
    
    const result: { text: string; highlight: boolean }[] = [];
    let lastIndex = 0;
    
    // String.prototype.matchAll / exec approach
    const matches = [...text.matchAll(pattern)];
    
    for (const match of matches) {
      const matchIndex = match.index!;
      const matchText = match[0];
      
      // Add text before match
      if (matchIndex > lastIndex) {
        result.push({ text: text.substring(lastIndex, matchIndex), highlight: false });
      }
      
      // Add match
      result.push({ text: matchText, highlight: true });
      
      lastIndex = matchIndex + matchText.length;
    }
    
    // Add remaining text
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
  .highlight-text {
    /* inherit from parent */
  }

  mark {
    background: rgba(255, 235, 59, 0.25);
    color: #ffd700;
    border-radius: 2px;
    padding: 0 1px;
    font-weight: inherit;
  }
</style>
