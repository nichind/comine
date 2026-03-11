import type { SubtitleCue } from './subtitles';

export interface WordTiming {
  word: string;
  startTime: number;
  endTime: number;
  cueIndex: number;
}

/**
 * Interpolate per-word timings from phrase-level subtitle cues.
 * Words within each cue are spread evenly across the cue's duration.
 */
export function interpolateWordTimings(cues: SubtitleCue[]): WordTiming[] {
  const words: WordTiming[] = [];

  for (const cue of cues) {
    // Strip HTML tags and split into words
    const plain = cue.text.replace(/<[^>]+>/g, '');
    const tokens = plain.split(/\s+/).filter(Boolean);
    if (tokens.length === 0) continue;

    const cueDuration = cue.endTime - cue.startTime;
    const wordDuration = cueDuration / tokens.length;

    for (let i = 0; i < tokens.length; i++) {
      words.push({
        word: tokens[i],
        startTime: cue.startTime + i * wordDuration,
        endTime: cue.startTime + (i + 1) * wordDuration,
        cueIndex: cue.index,
      });
    }
  }

  return words;
}

/**
 * Binary search for the active word at a given time. O(log n).
 * Returns the index into the WordTiming[] array, or -1 if no word is active.
 */
export function getActiveWordIndex(words: WordTiming[], currentTime: number): number {
  let lo = 0;
  let hi = words.length - 1;

  while (lo <= hi) {
    const mid = (lo + hi) >>> 1;
    if (words[mid].endTime <= currentTime) {
      lo = mid + 1;
    } else if (words[mid].startTime > currentTime) {
      hi = mid - 1;
    } else {
      return mid;
    }
  }

  // If between cues (gap), return the last spoken word for continuity
  if (lo > 0 && lo < words.length && words[lo].startTime > currentTime) {
    return lo - 1;
  }

  return -1;
}
