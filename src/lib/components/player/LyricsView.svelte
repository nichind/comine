<script lang="ts">
  import { interpolateWordTimings, getActiveWordIndex } from '$lib/utils/wordTimings';
  import type { SubtitleCue } from '$lib/utils/subtitles';

  interface Props {
    cues: SubtitleCue[];
    currentTime: number;
    onseek?: (time: number) => void;
  }

  let { cues, currentTime, onseek }: Props = $props();

  let container: HTMLDivElement | undefined = $state(undefined);
  let userScrolling = $state(false);
  let scrollTimer: ReturnType<typeof setTimeout> | undefined;

  const words = $derived(interpolateWordTimings(cues));
  const activeIndex = $derived(getActiveWordIndex(words, currentTime));

  // Track the last index we scrolled to, to avoid redundant scrolls
  let lastScrolledIndex = -1;

  // Auto-scroll to keep active word cluster visible
  $effect(() => {
    const idx = activeIndex;
    if (idx < 0 || userScrolling || !container) return;

    // Only scroll when we advance to a new word
    if (idx === lastScrolledIndex) return;
    lastScrolledIndex = idx;

    const el = container.querySelector(`[data-wi="${idx}"]`) as HTMLElement | null;
    if (!el) return;

    const containerRect = container.getBoundingClientRect();
    const elRect = el.getBoundingClientRect();
    const targetScroll =
      container.scrollTop + (elRect.top - containerRect.top) - containerRect.height * 0.38;

    container.scrollTo({ top: targetScroll, behavior: 'smooth' });
  });

  function onScroll() {
    // Detect manual scroll — pause auto-scroll for 4 seconds
    userScrolling = true;
    clearTimeout(scrollTimer);
    scrollTimer = setTimeout(() => {
      userScrolling = false;
    }, 4000);
  }

  function onWordClick(index: number) {
    const word = words[index];
    if (word) onseek?.(word.startTime);
  }
</script>

<div
  class="lyrics-container"
  bind:this={container}
  onscroll={onScroll}
  role="region"
  aria-label="Lyrics"
>
  <div class="lyrics-spacer"></div>
  <div class="lyrics-content">
    {#each words as word, i (i)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <span
        class="lyric-word"
        class:past={activeIndex >= 0 && i < activeIndex}
        class:active={i === activeIndex}
        data-wi={i}
        onclick={() => onWordClick(i)}
      >{word.word}</span>{' '}
    {/each}
  </div>
  <div class="lyrics-spacer"></div>
</div>

<style>
  .lyrics-container {
    position: absolute;
    inset: 60px 0 100px 0;
    overflow-y: auto;
    overflow-x: hidden;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    /* Fade edges */
    mask-image: linear-gradient(
      to bottom,
      transparent 0%,
      black 12%,
      black 85%,
      transparent 100%
    );
    -webkit-mask-image: linear-gradient(
      to bottom,
      transparent 0%,
      black 12%,
      black 85%,
      transparent 100%
    );
    /* Hide scrollbar */
    scrollbar-width: none;
    -ms-overflow-style: none;
  }

  .lyrics-container::-webkit-scrollbar {
    display: none;
  }

  .lyrics-spacer {
    min-height: 40%;
    flex-shrink: 0;
    width: 100%;
    pointer-events: none;
  }

  .lyrics-content {
    width: 100%;
    max-width: 560px;
    padding: 0 32px;
    line-height: 2;
    text-align: left;
    box-sizing: border-box;
  }

  .lyric-word {
    display: inline;
    font-size: 22px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.35);
    transition: color 0.22s ease-out, text-shadow 0.22s ease-out;
    cursor: pointer;
    border-radius: 2px;
  }

  .lyric-word:hover {
    color: rgba(255, 255, 255, 0.55);
  }

  .lyric-word.past {
    color: rgba(255, 255, 255, 0.85);
  }

  .lyric-word.active {
    color: rgba(255, 255, 255, 1);
    text-shadow: 0 0 14px rgba(255, 255, 255, 0.3);
  }

  @media (max-width: 640px) {
    .lyrics-content {
      max-width: 100%;
      padding: 0 20px;
    }

    .lyric-word {
      font-size: 18px;
    }
  }
</style>
