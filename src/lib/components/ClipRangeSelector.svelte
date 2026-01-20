<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from './Icon.svelte';
  import { formatTime, parseTimeString } from '$lib/utils/format';

  interface ClipRange {
    id: string;
    start: number; // seconds
    end: number; // seconds
  }

  interface Storyboard {
    url: string;
    width: number;
    height: number;
    cols: number;
    rows: number;
    fragment_count: number;
    fragment_duration: number;
  }

  interface Chapter {
    title: string;
    start_time: number;
    end_time: number;
  }

  interface SponsorBlockSegment {
    category: string;
    segment: [number, number]; // [start, end] in seconds
    UUID?: string;
    actionType?: string;
  }

  interface Props {
    duration: number; // total video duration in seconds
    ranges?: ClipRange[];
    onchange?: (ranges: ClipRange[]) => void;
    disabled?: boolean;
    storyboard?: Storyboard | null;
    chapters?: Chapter[] | null;
    sponsorSegments?: SponsorBlockSegment[] | null;
  }

  let { duration, ranges = $bindable([]), onchange, disabled = false, storyboard = null, chapters = null, sponsorSegments = null }: Props = $props();

  // Track if chapters are expanded
  let chaptersExpanded = $state(false);
  
  // Hovered chapter for tooltip
  let hoveredChapterId = $state<number | null>(null);
  
  // Hovered SponsorBlock segment for tooltip
  let hoveredSegmentId = $state<number | null>(null);

  function formatCategory(category: string): string {
    return String(category)
      .replace(/[_-]+/g, ' ')
      .replace(/\s+/g, ' ')
      .trim()
      .replace(/\b\w/g, (m) => m.toUpperCase());
  }

  function colorForCategory(category: string): string {
    let hash = 0;
    for (let i = 0; i < category.length; i++) {
      hash = (hash * 31 + category.charCodeAt(i)) | 0;
    }
    const hue = Math.abs(hash) % 360;
    return `hsl(${hue} 80% 55%)`;
  }

  function createId(): string {
    try {
      return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    } catch {
      return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    }
  }

  // Initialize with full range if empty
  $effect(() => {
    if (ranges.length === 0 && duration > 0) {
      ranges = [{ id: createId(), start: 0, end: duration }];
    }
  });

  let trackRef: HTMLDivElement | undefined = $state();
  // Dragging state: 'start'/'end' for handle resize, 'move' for moving entire range
  let dragging = $state<{ rangeId: string; handle: 'start' | 'end' | 'move'; startOffset?: number } | null>(null);
  let selectedRangeId = $state<string | null>(null);
  let hoveredRangeId = $state<string | null>(null);
  
  // Two-click range creation mode
  let pendingRangeStart = $state<number | null>(null);
  let hoverPosition = $state<number | null>(null);
  
  // Storyboard preview state
  let previewPosition = $state<{ x: number; time: number } | null>(null);

  // Time input state for double-click editing
  let editingTime = $state<{ rangeId: string; handle: 'start' | 'end' } | null>(null);
  let timeInputValue = $state('');
  let timeInputRef: HTMLInputElement | undefined = $state();

  // Derived: is full video selected (single range from 0 to duration)
  let isFullVideo = $derived(
    ranges.length === 1 && 
    ranges[0].start <= 0.5 && 
    ranges[0].end >= duration - 0.5
  );

  // Derived: total selected duration
  let totalSelected = $derived(
    ranges.reduce((sum, r) => sum + (r.end - r.start), 0)
  );
  
  // Calculate storyboard sprite position for a given time
  // YouTube storyboards: each fragment has multiple frames arranged in a grid
  // fragment_duration = total duration covered by ONE fragment image
  // Each cell = fragment_duration / (cols * rows)
  function getStoryboardPosition(time: number): { url: string; x: number; y: number; width: number; height: number; displayWidth: number; displayHeight: number } | null {
    if (!storyboard || !duration || duration <= 0) {
      return null;
    }
    
    const cellsPerFragment = storyboard.cols * storyboard.rows;
    // Each cell represents this much time
    const cellDuration = storyboard.fragment_duration / cellsPerFragment;
    
    // Which cell overall?
    const cellIndex = Math.min(Math.floor(time / cellDuration), storyboard.fragment_count * cellsPerFragment - 1);
    
    // Which fragment (sprite sheet)?
    const fragmentIndex = Math.floor(cellIndex / cellsPerFragment);
    const cellInFragment = cellIndex % cellsPerFragment;
    
    // Position within the sprite sheet
    const col = cellInFragment % storyboard.cols;
    const row = Math.floor(cellInFragment / storyboard.cols);
    
    // Cell dimensions - YouTube provides per-cell dimensions
    const cellWidth = storyboard.width;
    const cellHeight = storyboard.height;
    
    // Scale for display (target ~200px width, but allow larger for high-res storyboards)
    const targetDisplayWidth = 200;
    const scale = Math.max(1, targetDisplayWidth / cellWidth);
    
    // Build URL - replace $M placeholder with fragment index
    let url = storyboard.url;
    // Android blocks cleartext by default; protocol-relative URLs would inherit the app scheme.
    if (url.startsWith('//')) {
      url = `https:${url}`;
    } else if (url.startsWith('http://')) {
      url = url.replace(/^http:\/\//, 'https://');
    }
    if (url.includes('$M')) {
      url = url.replace(/\$M/g, String(fragmentIndex));
    }
    
    return {
      url,
      x: col * cellWidth,
      y: row * cellHeight,
      width: cellWidth,
      height: cellHeight,
      displayWidth: Math.round(cellWidth * scale),
      displayHeight: Math.round(cellHeight * scale),
    };
  }

  function startTimeEdit(rangeId: string, handle: 'start' | 'end') {
    const range = ranges.find(r => r.id === rangeId);
    if (!range) return;
    
    editingTime = { rangeId, handle };
    timeInputValue = formatTime(handle === 'start' ? range.start : range.end);
    
    // Focus input on next tick
    setTimeout(() => timeInputRef?.focus(), 0);
  }

  function applyTimeEdit() {
    if (!editingTime) return;
    
    const parsed = parseTimeString(timeInputValue);
    if (parsed === null || parsed < 0 || parsed > duration) {
      editingTime = null;
      return;
    }
    
    const { rangeId, handle } = editingTime;
    const newRanges = ranges.map(r => {
      if (r.id !== rangeId) return r;
      
      if (handle === 'start') {
        const newStart = Math.min(parsed, r.end - 1);
        return { ...r, start: Math.max(0, newStart) };
      } else {
        const newEnd = Math.max(parsed, r.start + 1);
        return { ...r, end: Math.min(duration, newEnd) };
      }
    });
    
    updateRanges(newRanges);
    editingTime = null;
  }

  function cancelTimeEdit() {
    editingTime = null;
    timeInputValue = '';
  }

  function handleTimeInputKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      applyTimeEdit();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      cancelTimeEdit();
    }
  }

  function getPositionFromEvent(e: MouseEvent | Touch): number {
    if (!trackRef) return 0;
    const rect = trackRef.getBoundingClientRect();
    const x = Math.max(0, Math.min(e.clientX - rect.left, rect.width));
    return (x / rect.width) * duration;
  }

  function getPositionFromTouchEvent(e: TouchEvent): number {
    if (e.touches.length === 0) return 0;
    return getPositionFromEvent(e.touches[0]);
  }

  function snapToGrid(time: number, shiftKey: boolean): number {
    if (shiftKey) return Math.round(time * 10) / 10; // 0.1s precision
    return Math.round(time); // Snap to 1s intervals
  }

  function sortAndMergeRanges(inputRanges: ClipRange[]): ClipRange[] {
    if (inputRanges.length <= 1) return inputRanges;
    
    // Sort by start time
    const sorted = [...inputRanges].sort((a, b) => a.start - b.start);
    const merged: ClipRange[] = [];
    
    for (const range of sorted) {
      const last = merged[merged.length - 1];
      // If overlapping or touching (within 0.5s), merge
      if (last && range.start <= last.end + 0.5) {
        last.end = Math.max(last.end, range.end);
      } else {
        merged.push({ ...range });
      }
    }
    
    return merged;
  }

  function updateRanges(newRanges: ClipRange[], skipMerge = false) {
    const result = skipMerge ? newRanges : sortAndMergeRanges(newRanges);
    ranges = result;
    onchange?.(result);
  }

  function handleTrackClick(e: MouseEvent) {
    if (disabled || dragging) return;
    
    const clickTime = snapToGrid(getPositionFromEvent(e), e.shiftKey);
    
    // Check if clicking inside an existing range - don't start creation mode
    const clickedRange = ranges.find(r => clickTime >= r.start && clickTime <= r.end);
    if (clickedRange) {
      selectedRangeId = clickedRange.id;
      pendingRangeStart = null;
      return;
    }
    
    // Two-click range creation mode
    if (pendingRangeStart === null) {
      // First click - set the start point
      pendingRangeStart = clickTime;
    } else {
      // Second click - create the range
      const start = Math.min(pendingRangeStart, clickTime);
      const end = Math.max(pendingRangeStart, clickTime);
      
      if (end - start >= 1) { // Minimum 1 second range
        const newRange: ClipRange = {
          id: createId(),
          start,
          end
        };
        updateRanges([...ranges, newRange]);
        selectedRangeId = newRange.id;
      }
      pendingRangeStart = null;
    }
  }

  function handleMouseMove(e: MouseEvent) {
    if (disabled) return;
    
    if (dragging) {
      const time = getPositionFromEvent(e);
      const snappedTime = snapToGrid(time, e.shiftKey);
      const currentDrag = dragging;
      const rangeIndex = ranges.findIndex(r => r.id === currentDrag.rangeId);
      if (rangeIndex === -1) return;
      
      const newRanges = [...ranges];
      const range = { ...newRanges[rangeIndex] };
      
      if (currentDrag.handle === 'move') {
        // Move entire range
        const rangeDuration = range.end - range.start;
        const offset = currentDrag.startOffset ?? 0;
        let newStart = snappedTime - offset;
        
        // Clamp to bounds
        if (newStart < 0) newStart = 0;
        if (newStart + rangeDuration > duration) newStart = duration - rangeDuration;
        
        range.start = newStart;
        range.end = newStart + rangeDuration;
      } else if (currentDrag.handle === 'start') {
        const newStart = Math.max(0, Math.min(snappedTime, duration));
        if (newStart > range.end) {
          range.start = range.end;
          range.end = newStart;
          dragging = { rangeId: currentDrag.rangeId, handle: 'end' };
        } else {
          range.start = newStart;
        }
      } else {
        const newEnd = Math.max(0, Math.min(snappedTime, duration));
        if (newEnd < range.start) {
          range.end = range.start;
          range.start = newEnd;
          dragging = { rangeId: currentDrag.rangeId, handle: 'start' };
        } else {
          range.end = newEnd;
        }
      }
      
      newRanges[rangeIndex] = range;
      // Skip merge during drag for smoother UX, merge on mouse up
      updateRanges(newRanges, true);
    } else {
      // Track hover position for preview
      const pos = getPositionFromEvent(e);
      const inRange = ranges.some(r => pos >= r.start && pos <= r.end);
      hoverPosition = inRange ? null : pos;
    }
    
    // Update storyboard preview position
    if (trackRef && storyboard) {
      const rect = trackRef.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const time = getPositionFromEvent(e);
      previewPosition = { x, time };
    }
  }

  function handleTouchMove(e: TouchEvent) {
    if (disabled || !dragging) return;
    // Prevent scrolling via CSS touch-action instead of preventDefault (touch listeners can be passive).
    
    const time = getPositionFromTouchEvent(e);
    const snappedTime = snapToGrid(time, false);
    const currentDrag = dragging;
    const rangeIndex = ranges.findIndex(r => r.id === currentDrag.rangeId);
    if (rangeIndex === -1) return;
    
    const newRanges = [...ranges];
    const range = { ...newRanges[rangeIndex] };
    
    if (currentDrag.handle === 'move') {
      const rangeDuration = range.end - range.start;
      const offset = currentDrag.startOffset ?? 0;
      let newStart = snappedTime - offset;
      
      if (newStart < 0) newStart = 0;
      if (newStart + rangeDuration > duration) newStart = duration - rangeDuration;
      
      range.start = newStart;
      range.end = newStart + rangeDuration;
    } else if (currentDrag.handle === 'start') {
      const newStart = Math.max(0, Math.min(snappedTime, duration));
      if (newStart > range.end) {
        range.start = range.end;
        range.end = newStart;
        dragging = { rangeId: currentDrag.rangeId, handle: 'end' };
      } else {
        range.start = newStart;
      }
    } else {
      const newEnd = Math.max(0, Math.min(snappedTime, duration));
      if (newEnd < range.start) {
        range.end = range.start;
        range.start = newEnd;
        dragging = { rangeId: currentDrag.rangeId, handle: 'start' };
      } else {
        range.end = newEnd;
      }
    }
    
    newRanges[rangeIndex] = range;
    updateRanges(newRanges, true);

    // Keep storyboard preview updated while dragging on touch devices.
    if (trackRef && storyboard) {
      const touch = e.touches[0];
      if (touch) {
        const rect = trackRef.getBoundingClientRect();
        const x = touch.clientX - rect.left;
        previewPosition = { x, time };
      }
    }
  }

  function updateTouchPreview(e: TouchEvent) {
    if (disabled || dragging || !trackRef) return;

    const touch = e.touches[0];
    if (!touch) return;

    const time = getPositionFromTouchEvent(e);

    // Keep hover marker behavior aligned with mouse.
    const inRange = ranges.some((r) => time >= r.start && time <= r.end);
    hoverPosition = inRange ? null : time;

    if (storyboard) {
      const rect = trackRef.getBoundingClientRect();
      const x = touch.clientX - rect.left;
      previewPosition = { x, time };
    }
  }

  function clearTouchPreview() {
    hoverPosition = null;
    previewPosition = null;
  }

  function handleTouchEnd() {
    if (dragging) {
      updateRanges(ranges);
    }
    dragging = null;

    // Clear any lingering preview state.
    hoverPosition = null;
    previewPosition = null;
  }

  function handleMouseUp() {
    if (dragging) {
      // Merge ranges after drag ends
      updateRanges(ranges);
    }
    dragging = null;
  }

  function handleMouseLeave() {
    hoverPosition = null;
    previewPosition = null;
  }

  function startDrag(e: MouseEvent, rangeId: string, handle: 'start' | 'end' | 'move') {
    if (disabled) return;
    e.stopPropagation();
    e.preventDefault();
    
    const range = ranges.find(r => r.id === rangeId);
    if (!range) return;
    
    const clickTime = getPositionFromEvent(e);
    const startOffset = clickTime - range.start;
    
    dragging = { rangeId, handle, startOffset };
    selectedRangeId = rangeId;
    pendingRangeStart = null;
  }

  function startTouchDrag(e: TouchEvent, rangeId: string, handle: 'start' | 'end' | 'move') {
    if (disabled) return;
    e.stopPropagation();
    
    const range = ranges.find(r => r.id === rangeId);
    if (!range) return;
    
    const touchTime = getPositionFromTouchEvent(e);
    const startOffset = touchTime - range.start;
    
    dragging = { rangeId, handle, startOffset };
    selectedRangeId = rangeId;
    pendingRangeStart = null;
  }

  function deleteRange(rangeId: string) {
    if (ranges.length <= 1) {
      resetToFull();
      return;
    }
    updateRanges(ranges.filter(r => r.id !== rangeId));
    if (selectedRangeId === rangeId) {
      selectedRangeId = null;
    }
  }

  function resetToFull() {
    ranges = [{ id: createId(), start: 0, end: duration }];
    onchange?.(ranges);
    selectedRangeId = null;
    pendingRangeStart = null;
  }

  function selectChapter(chapter: Chapter) {
    if (disabled) return;
    // Create a new range from the chapter boundaries
    const newRange: ClipRange = {
      id: createId(),
      start: chapter.start_time,
      end: chapter.end_time
    };
    updateRanges([...ranges.filter(r => !(r.start <= 0.5 && r.end >= duration - 0.5)), newRange]);
    selectedRangeId = newRange.id;
    pendingRangeStart = null;
  }

  function cancelPendingRange() {
    pendingRangeStart = null;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (pendingRangeStart !== null) {
        e.preventDefault();
        cancelPendingRange();
        return;
      }
      if (selectedRangeId !== null) {
        e.preventDefault();
        selectedRangeId = null;
        return;
      }
    }
    
    if (!selectedRangeId || disabled) return;
    
    const rangeIndex = ranges.findIndex(r => r.id === selectedRangeId);
    if (rangeIndex === -1) return;
    
    const delta = e.shiftKey ? 0.1 : 1;
    const newRanges = [...ranges];
    const range = { ...newRanges[rangeIndex] };
    
    if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      deleteRange(selectedRangeId);
      return;
    }
    
    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      range.start = Math.max(0, range.start - delta);
      range.end = Math.max(range.start + 1, range.end - delta);
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      range.end = Math.min(duration, range.end + delta);
      range.start = Math.min(range.end - 1, range.start + delta);
    }
    
    newRanges[rangeIndex] = range;
    updateRanges(newRanges);
  }

  function toPercent(time: number): number {
    return (time / duration) * 100;
  }
</script>

<svelte:window 
  onmouseup={handleMouseUp}
  onmousemove={dragging ? handleMouseMove : undefined}
  ontouchend={handleTouchEnd}
  ontouchcancel={handleTouchEnd}
  ontouchmove={dragging ? handleTouchMove : undefined}
/>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div 
  class="clip-selector"
  class:disabled
  class:creating={pendingRangeStart !== null}
  tabindex="0"
  role="group"
  aria-label={$t('download.tracks.clipRange')}
  onkeydown={handleKeydown}
>
  <div class="selector-header">
    <span class="selector-label">{$t('download.tracks.clipRange')}</span>
    <div class="header-actions">
      {#if pendingRangeStart !== null}
        <button class="cancel-btn" onclick={cancelPendingRange}>
          <Icon name="close" size={12} />
        </button>
      {/if}
      {#if !isFullVideo}
        <button class="reset-btn" onclick={resetToFull} title={$t('download.tracks.resetToFull')}>
          <Icon name="restart" size={12} />
        </button>
      {/if}
    </div>
  </div>

  <div 
    class="timeline-track"
    bind:this={trackRef}
    onclick={handleTrackClick}
    onmousemove={!dragging ? handleMouseMove : undefined}
    ontouchstart={!dragging ? updateTouchPreview : undefined}
    ontouchmove={!dragging ? updateTouchPreview : undefined}
    ontouchend={clearTouchPreview}
    ontouchcancel={clearTouchPreview}
    onmouseleave={handleMouseLeave}
    role="presentation"
  >
    <!-- Background track -->
    <div class="track-bg"></div>
    
    <!-- Chapter markers (shown as subtle dividers on the timeline) -->
    {#if chapters && chapters.length > 0}
      {#each chapters as chapter, i}
        {#if chapter.start_time > 0}
          <div 
            class="chapter-marker"
            class:hovered={hoveredChapterId === i}
            style="left: {toPercent(chapter.start_time)}%"
            title={chapter.title}
          ></div>
        {/if}
      {/each}
      
      <!-- Chapter section highlight when hovering in list -->
      {#if hoveredChapterId !== null && chapters[hoveredChapterId]}
        {@const hoverChapter = chapters[hoveredChapterId]}
        <div 
          class="chapter-section-highlight"
          style="left: {toPercent(hoverChapter.start_time)}%; width: {toPercent(hoverChapter.end_time - hoverChapter.start_time)}%"
        ></div>
      {/if}
    {/if}
    
    <!-- SponsorBlock segments (shown as colored sections on the timeline) -->
    {#if sponsorSegments && sponsorSegments.length > 0}
      {#each sponsorSegments as segment, i}
        {@const startTime = segment.segment[0]}
        {@const endTime = segment.segment[1]}
        {@const color = colorForCategory(segment.category)}
        <div 
          class="sponsorblock-segment"
          class:hovered={hoveredSegmentId === i}
          style="left: {toPercent(startTime)}%; width: {toPercent(endTime - startTime)}%; --sb-color: {color}"
          title="{formatCategory(segment.category)} ({formatTime(startTime)} - {formatTime(endTime)})"
          onmouseenter={() => hoveredSegmentId = i}
          onmouseleave={() => hoveredSegmentId = null}
          role="img"
          aria-label="{formatCategory(segment.category)} segment from {formatTime(startTime)} to {formatTime(endTime)}"
        ></div>
      {/each}
    {/if}
    
    <!-- Range blocks -->
    {#each ranges as range (range.id)}
      {@const startPct = toPercent(range.start)}
      {@const widthPct = toPercent(range.end - range.start)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div 
        class="range-block"
        class:selected={selectedRangeId === range.id}
        class:hovered={hoveredRangeId === range.id}
        style="left: {startPct}%; width: {widthPct}%"
        onclick={(e) => { e.stopPropagation(); selectedRangeId = range.id; pendingRangeStart = null; }}
        onmousedown={(e) => { 
          // Only start move drag if not clicking on handles
          const target = e.target as HTMLElement;
          if (!target.closest('.handle')) {
            startDrag(e, range.id, 'move');
          }
        }}
        ontouchstart={(e) => {
          // Only start move drag if not touching handles
          const target = e.target as HTMLElement;
          if (!target.closest('.handle')) {
            startTouchDrag(e, range.id, 'move');
          }
        }}
        onmouseenter={() => { hoveredRangeId = range.id; }}
        onmouseleave={() => { hoveredRangeId = null; }}
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.stopPropagation(); selectedRangeId = range.id; } }}
        role="button"
        tabindex="0"
      >
        <div 
          class="handle start"
          onmousedown={(e) => startDrag(e, range.id, 'start')}
          ontouchstart={(e) => startTouchDrag(e, range.id, 'start')}
          ondblclick={(e) => { e.stopPropagation(); startTimeEdit(range.id, 'start'); }}
          role="presentation"
        >
          <div class="handle-grip"></div>
          {#if editingTime?.rangeId === range.id && editingTime?.handle === 'start'}
            <div class="time-input-popup">
              <input
                bind:this={timeInputRef}
                type="text"
                class="time-input"
                bind:value={timeInputValue}
                onkeydown={handleTimeInputKeydown}
                onblur={applyTimeEdit}
                placeholder="0:00"
              />
            </div>
          {/if}
        </div>
        <div 
          class="handle end"
          onmousedown={(e) => startDrag(e, range.id, 'end')}
          ontouchstart={(e) => startTouchDrag(e, range.id, 'end')}
          ondblclick={(e) => { e.stopPropagation(); startTimeEdit(range.id, 'end'); }}
          role="presentation"
        >
          <div class="handle-grip"></div>
          {#if editingTime?.rangeId === range.id && editingTime?.handle === 'end'}
            <div class="time-input-popup">
              <input
                bind:this={timeInputRef}
                type="text"
                class="time-input"
                bind:value={timeInputValue}
                onkeydown={handleTimeInputKeydown}
                onblur={applyTimeEdit}
                placeholder="0:00"
              />
            </div>
          {/if}
        </div>
        <!-- Show delete button on hover when there are multiple ranges -->
        {#if ranges.length > 1}
          <button 
            class="delete-btn"
            onclick={(e) => { e.stopPropagation(); deleteRange(range.id); }}
            onmousedown={(e) => e.stopPropagation()}
            title={$t('common.delete')}
          >
            <Icon name="close" size={10} />
          </button>
        {/if}
      </div>
    {/each}

    <!-- Pending range start marker -->
    {#if pendingRangeStart !== null}
      <div 
        class="pending-marker"
        style="left: {toPercent(pendingRangeStart)}%"
      ></div>
      <!-- Preview of range being created -->
      {#if hoverPosition !== null}
        {@const previewStart = Math.min(pendingRangeStart, hoverPosition)}
        {@const previewEnd = Math.max(pendingRangeStart, hoverPosition)}
        <div 
          class="pending-preview"
          style="left: {toPercent(previewStart)}%; width: {toPercent(previewEnd - previewStart)}%"
        ></div>
      {/if}
    {/if}

    <!-- Hover marker when not creating -->
    {#if hoverPosition !== null && pendingRangeStart === null}
      <div 
        class="hover-marker"
        style="left: {toPercent(hoverPosition)}%"
      ></div>
    {/if}

    <!-- Storyboard preview tooltip -->
    {#if previewPosition && storyboard}
      {@const spritePos = getStoryboardPosition(previewPosition.time)}
      {#if spritePos}
        {@const clampedX = Math.max(spritePos.displayWidth / 2 + 8, Math.min(previewPosition.x, (trackRef?.clientWidth ?? 400) - spritePos.displayWidth / 2 - 8))}
        <div 
          class="storyboard-preview"
          style="left: {clampedX}px"
        >
          <div 
            class="preview-image"
            style="
              background-image: url('{spritePos.url}');
              background-position: -{spritePos.x * (spritePos.displayWidth / spritePos.width)}px -{spritePos.y * (spritePos.displayHeight / spritePos.height)}px;
              background-size: {storyboard.cols * spritePos.displayWidth}px {storyboard.rows * spritePos.displayHeight}px;
              width: {spritePos.displayWidth}px;
              height: {spritePos.displayHeight}px;
            "
          ></div>
          <span class="preview-time">{formatTime(previewPosition.time)}</span>
        </div>
      {/if}
    {/if}
  </div>

  <div class="time-labels">
    <span class="time-start">0:00</span>
    <span class="time-selected">
      {#if pendingRangeStart !== null}
        {formatTime(pendingRangeStart)}
      {:else if isFullVideo}
        {$t('download.tracks.fullVideo')}
      {:else}
        {formatTime(totalSelected)} / {formatTime(duration)}
      {/if}
    </span>
    <span class="time-end">{formatTime(duration)}</span>
  </div>

  <!-- Chapters list (collapsible) -->
  {#if chapters && chapters.length > 0}
    <div class="chapters-section">
      <button 
        class="chapters-toggle"
        onclick={() => chaptersExpanded = !chaptersExpanded}
        type="button"
      >
        <Icon name="playlist" size={12} />
        <span>{chapters.length}</span>
        <Icon name={chaptersExpanded ? 'chevron_up' : 'chevron_down'} size={12} />
      </button>
      
      {#if chaptersExpanded}
        <div class="chapters-list">
          {#each chapters as chapter, i}
            <button
              class="chapter-item"
              class:hovered={hoveredChapterId === i}
              onclick={() => selectChapter(chapter)}
              onmouseenter={() => hoveredChapterId = i}
              onmouseleave={() => hoveredChapterId = null}
              type="button"
            >
              <span class="chapter-time">{formatTime(chapter.start_time)}</span>
              <span class="chapter-title">{chapter.title}</span>
              <span class="chapter-duration">{formatTime(chapter.end_time - chapter.start_time)}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <!-- SponsorBlock legend (shown when segments are present) -->
  {#if sponsorSegments && sponsorSegments.length > 0}
    {@const uniqueCategories = [...new Set(sponsorSegments.map(s => s.category))]}
    <div class="sponsorblock-legend">
      {#each uniqueCategories as category}
        <span class="legend-item" style="--sb-color: {colorForCategory(category)}">
          <span class="legend-dot"></span>
          {formatCategory(category)}
        </span>
      {/each}
    </div>
  {/if}
</div>

<style>
  .clip-selector {
    display: flex;
    flex-direction: column;
    gap: 6px;
    outline: none;
    overflow: visible;
  }

  .clip-selector.disabled {
    opacity: 0.5;
    pointer-events: none;
  }

  .selector-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .selector-label {
    font-size: 11px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.5);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .header-actions {
    display: flex;
    gap: 4px;
  }

  .reset-btn,
  .cancel-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 4px;
    background: rgba(255, 255, 255, 0.06);
    border: none;
    border-radius: 4px;
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    transition: all 0.15s;
  }

  .reset-btn:hover,
  .cancel-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: white;
  }

  .cancel-btn {
    background: rgba(239, 68, 68, 0.2);
    color: rgba(239, 68, 68, 0.8);
  }

  .cancel-btn:hover {
    background: rgba(239, 68, 68, 0.3);
    color: rgb(239, 68, 68);
  }

  .timeline-track {
    position: relative;
    height: 28px;
    cursor: pointer;
    overflow: visible;
    touch-action: none;
  }

  .creating .timeline-track {
    cursor: crosshair;
  }

  .track-bg {
    position: absolute;
    inset: 0;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 6px;
  }

  .range-block {
    position: absolute;
    top: 0;
    height: 100%;
    background: var(--accent-alpha, rgba(99, 102, 241, 0.3));
    border-radius: 6px;
    transition: background 0.15s;
    min-width: 8px;
    cursor: grab;
    touch-action: none;
  }

  .range-block:active {
    cursor: grabbing;
  }

  .range-block.selected {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.4));
  }

  .range-block:hover,
  .range-block.hovered {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.45));
  }

  .handle {
    position: absolute;
    top: -8px;
    bottom: -8px;
    width: 24px;
    height: auto;
    cursor: ew-resize;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 2;
    touch-action: none; /* Prevent browser handling of touch gestures */
  }

  .handle.start {
    left: -12px;
    border-radius: 6px 0 0 6px;
  }

  .handle.end {
    right: -12px;
    border-radius: 0 6px 6px 0;
  }

  .handle-grip {
    width: 4px;
    height: 18px;
    background: var(--accent, #6366f1);
    border-radius: 2px;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .range-block:hover .handle-grip,
  .range-block.selected .handle-grip {
    opacity: 1;
  }

  .handle:hover .handle-grip,
  .handle:active .handle-grip {
    background: white;
    opacity: 1;
  }

  /* Mobile touch targets - make handles much larger */
  @media (pointer: coarse) {
    .handle {
      width: 44px;
      top: -14px;
      bottom: -14px;
    }

    .handle.start {
      left: -22px;
    }

    .handle.end {
      right: -22px;
    }

    .handle-grip {
      width: 6px;
      height: 28px;
      opacity: 1; /* Always show on touch devices */
    }

    .timeline-track {
      height: 48px;
    }
  }

  /* Time input popup for double-click editing */
  .time-input-popup {
    position: absolute;
    top: -40px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 100;
  }

  .time-input {
    width: 70px;
    padding: 6px 8px;
    background: rgba(30, 30, 35, 0.98);
    border: 1px solid var(--accent, #3b82f6);
    border-radius: 6px;
    color: white;
    font-size: 12px;
    font-family: inherit;
    text-align: center;
    outline: none;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
  }

  .time-input::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }

  .time-input:focus {
    border-color: var(--accent, #3b82f6);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4), 0 0 0 2px rgba(59, 130, 246, 0.3);
  }

  .delete-btn {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    background: rgba(239, 68, 68, 0.8);
    border: none;
    border-radius: 50%;
    color: white;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s;
    z-index: 3;
  }

  .range-block:hover .delete-btn,
  .range-block.hovered .delete-btn {
    opacity: 1;
  }

  .delete-btn:hover {
    background: rgb(239, 68, 68);
    transform: translate(-50%, -50%) scale(1.1);
  }

  .pending-marker {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 3px;
    background: var(--accent, #6366f1);
    border-radius: 2px;
    pointer-events: none;
    transform: translateX(-50%);
    animation: pulse 1s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  .pending-preview {
    position: absolute;
    top: 0;
    height: 100%;
    background: var(--accent-alpha, rgba(99, 102, 241, 0.2));
    border-radius: 6px;
    pointer-events: none;
    border: 1px dashed var(--accent, #6366f1);
  }

  .hover-marker {
    position: absolute;
    top: 4px;
    bottom: 4px;
    width: 2px;
    background: rgba(255, 255, 255, 0.3);
    border-radius: 1px;
    pointer-events: none;
    transform: translateX(-50%);
  }

  .time-labels {
    display: flex;
    justify-content: space-between;
    font-size: 10px;
    color: rgba(255, 255, 255, 0.4);
    font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
  }

  .time-selected {
    color: rgba(255, 255, 255, 0.6);
  }

  .creating .time-selected {
    color: var(--accent, #6366f1);
  }

  /* Storyboard preview */
  .storyboard-preview {
    position: absolute;
    bottom: calc(100% + 12px);
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    pointer-events: none;
    z-index: 100;
  }

  .preview-image {
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.1);
    background-color: #000;
    background-repeat: no-repeat;
  }

  .preview-time {
    font-size: 11px;
    font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
    color: white;
    background: rgba(0, 0, 0, 0.85);
    padding: 3px 8px;
    border-radius: 4px;
    font-weight: 500;
  }

  /* Chapter markers on timeline */
  .chapter-marker {
    position: absolute;
    top: 2px;
    bottom: 2px;
    width: 1px;
    background: rgba(255, 255, 255, 0.25);
    pointer-events: none;
    z-index: 1;
    transition: background 0.15s ease, width 0.15s ease;
  }

  .chapter-marker.hovered {
    width: 2px;
    background: var(--accent, #3b82f6);
    box-shadow: 0 0 6px var(--accent, #3b82f6);
  }

  /* Chapter section highlight on timeline when hovering in list */
  .chapter-section-highlight {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--accent, #3b82f6);
    opacity: 0.15;
    border-radius: 2px;
    pointer-events: none;
    z-index: 0;
  }

  /* SponsorBlock segments on timeline */
  .sponsorblock-segment {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--sb-color, #00d400);
    opacity: 0.5;
    border-radius: 2px;
    pointer-events: auto;
    cursor: help;
    z-index: 0;
    transition: opacity 0.15s ease;
  }

  .sponsorblock-segment:hover,
  .sponsorblock-segment.hovered {
    opacity: 0.7;
    z-index: 0;
  }

  /* Chapters section */
  .chapters-section {
    margin-top: 4px;
  }

  .chapters-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    background: rgba(255, 255, 255, 0.04);
    border: none;
    border-radius: 4px;
    color: rgba(255, 255, 255, 0.5);
    font-size: 11px;
    cursor: pointer;
    transition: all 0.15s;
    width: 100%;
  }

  .chapters-toggle:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .chapters-toggle span {
    flex: 1;
    text-align: left;
  }

  .chapters-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: 4px;
    max-height: 200px;
    overflow-y: auto;
    padding-right: 4px;
  }

  .chapter-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    background: rgba(255, 255, 255, 0.03);
    border: none;
    border-radius: 4px;
    color: rgba(255, 255, 255, 0.7);
    font-size: 11px;
    cursor: pointer;
    transition: all 0.15s;
    text-align: left;
  }

  .chapter-item:hover,
  .chapter-item.hovered {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }

  .chapter-item:active {
    background: var(--accent-alpha, rgba(99, 102, 241, 0.2));
  }

  .chapter-time {
    font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
    color: rgba(255, 255, 255, 0.4);
    min-width: 40px;
    flex-shrink: 0;
  }

  .chapter-title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chapter-duration {
    font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
    color: rgba(255, 255, 255, 0.3);
    font-size: 10px;
  }

  .chapters-list::-webkit-scrollbar {
    width: 4px;
  }

  .chapters-list::-webkit-scrollbar-track {
    background: transparent;
  }

  .chapters-list::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.1);
    border-radius: 2px;
  }

  .chapters-list::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  /* SponsorBlock legend */
  .sponsorblock-legend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
    padding: 6px 8px;
    background: rgba(255, 255, 255, 0.03);
    border-radius: 6px;
    font-size: 10px;
    color: rgba(255, 255, 255, 0.5);
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .legend-dot {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--sb-color);
    opacity: 0.7;
  }
</style>
