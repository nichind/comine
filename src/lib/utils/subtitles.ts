export interface SubtitleCue {
  index: number;
  startTime: number; // seconds
  endTime: number; // seconds
  text: string; // may contain HTML tags like <i>, <b>
}

// ---------------------------------------------------------------------------
// Internal helper
// ---------------------------------------------------------------------------

/**
 * Parse a subtitle timestamp to seconds.
 *
 * Accepts:
 *   SRT format  — `HH:MM:SS,mmm`
 *   VTT format  — `HH:MM:SS.mmm` or `MM:SS.mmm`
 */
function parseTimestamp(ts: string): number {
  // Normalise separator: both comma (SRT) and dot (VTT) are valid for ms
  const normalised = ts.trim().replace(',', '.');

  const parts = normalised.split(':');

  if (parts.length === 3) {
    // HH:MM:SS.mmm
    const hours = parseInt(parts[0], 10);
    const minutes = parseInt(parts[1], 10);
    const seconds = parseFloat(parts[2]);
    return hours * 3600 + minutes * 60 + seconds;
  }

  if (parts.length === 2) {
    // MM:SS.mmm  (VTT short form)
    const minutes = parseInt(parts[0], 10);
    const seconds = parseFloat(parts[1]);
    return minutes * 60 + seconds;
  }

  return 0;
}

// ---------------------------------------------------------------------------
// Sanitizer
// ---------------------------------------------------------------------------

const ALLOWED_TAGS = /^\/?(i|b|u|em|strong)$/i;

/**
 * Strip dangerous HTML from subtitle text, keeping only safe inline formatting tags.
 */
function sanitizeCueText(raw: string): string {
  return raw.replace(/<\/?([a-z][a-z0-9]*)\b[^>]*>/gi, (match, tag) =>
    ALLOWED_TAGS.test(tag) ? match.replace(/\s+on\w+\s*=\s*["'][^"']*["']/gi, '') : ''
  );
}

// ---------------------------------------------------------------------------
// SRT parser
// ---------------------------------------------------------------------------

/**
 * Parse an SRT subtitle file into an array of SubtitleCue objects.
 *
 * Expected block format:
 * ```
 * 1
 * 00:00:01,000 --> 00:00:04,000
 * Hello world
 *
 * 2
 * 00:00:05,500 --> 00:00:08,200
 * Line one
 * Line two
 * ```
 */
export function parseSRT(content: string): SubtitleCue[] {
  // Normalise line endings
  const normalised = content.replace(/\r\n/g, '\n').replace(/\r/g, '\n');

  // Split into blocks separated by one or more blank lines
  const blocks = normalised.trim().split(/\n{2,}/);

  const cues: SubtitleCue[] = [];

  for (const block of blocks) {
    const lines = block.trim().split('\n');
    if (lines.length < 3) continue;

    // Line 0: cue index (may be non-numeric in some files — fall back to sequential)
    const parsedIndex = parseInt(lines[0].trim(), 10);
    const index = isNaN(parsedIndex) ? cues.length + 1 : parsedIndex;

    // Line 1: timing  "00:00:01,000 --> 00:00:04,000"
    const timingMatch = lines[1].match(
      /^([\d:,]+)\s*-->\s*([\d:,]+)/,
    );
    if (!timingMatch) continue;

    const startTime = parseTimestamp(timingMatch[1]);
    const endTime = parseTimestamp(timingMatch[2]);

    // Remaining lines: cue text (join multi-line text with newline)
    const text = sanitizeCueText(lines.slice(2).join('\n').trim());

    cues.push({ index, startTime, endTime, text });
  }

  return cues;
}

// ---------------------------------------------------------------------------
// VTT parser
// ---------------------------------------------------------------------------

/**
 * Parse a WebVTT subtitle file into an array of SubtitleCue objects.
 *
 * Skips the mandatory `WEBVTT` header and any metadata/NOTE blocks before
 * the first cue. Cue settings (alignment, position, etc.) following `-->`
 * on the timing line are silently ignored.
 */
export function parseVTT(content: string): SubtitleCue[] {
  // Normalise line endings
  const normalised = content.replace(/\r\n/g, '\n').replace(/\r/g, '\n');

  // Split into blocks separated by one or more blank lines
  const blocks = normalised.trim().split(/\n{2,}/);

  const cues: SubtitleCue[] = [];
  let autoIndex = 1;

  for (const block of blocks) {
    const lines = block.trim().split('\n');
    if (lines.length === 0) continue;

    // Skip the WEBVTT header block and NOTE / REGION / STYLE blocks
    const firstLine = lines[0].trim();
    if (
      firstLine.startsWith('WEBVTT') ||
      firstLine.startsWith('NOTE') ||
      firstLine.startsWith('REGION') ||
      firstLine.startsWith('STYLE')
    ) {
      continue;
    }

    // A cue block may optionally start with a cue identifier (non-timing line)
    // before the timing line. Detect which line holds `-->`.
    let timingLineIndex = 0;
    let cueLabel: string | null = null;

    if (!lines[0].includes('-->')) {
      // First line is a cue identifier
      cueLabel = lines[0].trim();
      timingLineIndex = 1;
    }

    if (timingLineIndex >= lines.length) continue;

    const timingLine = lines[timingLineIndex];
    // VTT timing may have cue settings after the end timestamp, e.g.:
    //   00:00:01.000 --> 00:00:04.000 align:left
    const timingMatch = timingLine.match(
      /^([\d:.]+)\s*-->\s*([\d:.]+)/,
    );
    if (!timingMatch) continue;

    const startTime = parseTimestamp(timingMatch[1]);
    const endTime = parseTimestamp(timingMatch[2]);

    // Remaining lines after the timing line are cue text
    const text = sanitizeCueText(lines.slice(timingLineIndex + 1).join('\n').trim());

    const index = cueLabel !== null ? (parseInt(cueLabel, 10) || autoIndex) : autoIndex;
    autoIndex++;

    cues.push({ index, startTime, endTime, text });
  }

  return cues;
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/**
 * Parse subtitle file content based on the given format identifier.
 *
 * Supported formats: `'srt'`, `'vtt'`.
 * Returns an empty array for unsupported formats.
 *
 * When format is `'vtt'`, also detects SRT content (missing `WEBVTT` header)
 * and routes to the SRT parser automatically. This handles edge-tts output
 * which produces SRT-format content with a `.vtt` extension.
 */
export function parseSubtitleContent(content: string, format: string): SubtitleCue[] {
  const fmt = format.toLowerCase().trim();

  switch (fmt) {
    case 'srt':
      return parseSRT(content);
    case 'vtt': {
      // edge-tts (and some other tools) produce SRT-format content with a .vtt extension.
      // Detect by checking for the WEBVTT header — if missing, parse as SRT.
      const trimmed = content.trimStart();
      if (!trimmed.startsWith('WEBVTT')) {
        return parseSRT(content);
      }
      return parseVTT(content);
    }
    default:
      return [];
  }
}

// ---------------------------------------------------------------------------
// Delay
// ---------------------------------------------------------------------------

/**
 * Return a new array of cues with start/end times shifted by `delaySeconds`.
 * Times are clamped to >= 0.
 */
export function applyDelay(cues: SubtitleCue[], delaySeconds: number): SubtitleCue[] {
  return cues.map((cue) => ({
    ...cue,
    startTime: Math.max(0, cue.startTime + delaySeconds),
    endTime: Math.max(0, cue.endTime + delaySeconds),
  }));
}

// ---------------------------------------------------------------------------
// Active cue lookup
// ---------------------------------------------------------------------------

/**
 * Find the cue that is active at `currentTime` (startTime <= currentTime < endTime).
 *
 * Uses a linear scan. Returns `null` when no cue is active.
 *
 * Note: SRT/VTT files are almost always time-sorted, so a linear scan is
 * straightforward. If the cue list is known to be sorted and large, a binary
 * search variant can be substituted.
 */
export function getActiveCue(cues: SubtitleCue[], currentTime: number): SubtitleCue | null {
  for (const cue of cues) {
    if (currentTime >= cue.startTime && currentTime < cue.endTime) {
      return cue;
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Time formatting
// ---------------------------------------------------------------------------

/**
 * Format a duration in seconds for display in a media player.
 *
 * - `>= 3600 s`  → `H:MM:SS`
 * - `< 3600 s`   → `M:SS`
 * - Negative     → `-M:SS` (or `-H:MM:SS`)
 */
export function formatPlayerTime(seconds: number): string {
  const negative = seconds < 0;
  const abs = Math.abs(seconds);

  const h = Math.floor(abs / 3600);
  const m = Math.floor((abs % 3600) / 60);
  const s = Math.floor(abs % 60);

  const prefix = negative ? '-' : '';

  if (h > 0) {
    return `${prefix}${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  }

  return `${prefix}${m}:${s.toString().padStart(2, '0')}`;
}
