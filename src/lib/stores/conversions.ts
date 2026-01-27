import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { get } from 'svelte/store';
import { toast } from '$lib/components/Toast.svelte';
import { history } from './history';
import { queue } from './queue';
import { settings } from './settings';
import type { HistoryItem } from './history';
import type {
  ConvertRequest,
  ConvertResult,
  ConvertProgress,
  FfmpegConvertSettings,
} from '$lib/bindings';

export { activeConversions, activeConversionsCount } from './queue';

let unlisten: UnlistenFn | null = null;

export function initConversions() {
  if (unlisten) return;

  listen<ConvertProgress>('convert-progress', (event) => {
    const progress = event.payload;

    queue.updateConversion(progress.jobId, {
      progress: progress.progress,
      speed: progress.speed ?? '',
    });
  }).then((unsub) => {
    unlisten = unsub;
  });
}

export function cleanupConversions() {
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
}

export async function cancelConversion(id: string): Promise<void> {
  try {
    await invoke('cancel_conversion', { jobId: id });

    queue.updateConversion(id, {
      status: 'failed',
      statusMessage: 'Cancelled',
      error: 'Conversion cancelled by user',
    });

    setTimeout(() => {
      queue.removeConversion(id);
    }, 2000);
  } catch (err) {
    console.error('Failed to cancel conversion:', err);
  }
}

export async function startConversion(
  item: Pick<
    HistoryItem,
    | 'url'
    | 'title'
    | 'author'
    | 'authorUrl'
    | 'thumbnail'
    | 'duration'
    | 'extension'
    | 'type'
    | 'filePath'
  >,
  targetFormat: string,
  audioOnly: boolean
): Promise<ConvertResult | null> {
  if (!item.filePath) {
    toast.error('File path not available');
    return null;
  }

  const id = crypto.randomUUID();

  const appSettings = get(settings);
  const ffmpegSettings: FfmpegConvertSettings = {
    hwAccel: appSettings.ffmpeg.hwAccel,
  };

  const durationToSerializableSeconds = (durationSeconds: number): number | null => {
    if (!Number.isFinite(durationSeconds) || durationSeconds <= 0) return null;
    return Math.floor(durationSeconds);
  };

  const toNumber = (value: bigint | number): number => {
    const n = Number(value);
    if (!Number.isFinite(n)) return 0;
    return n;
  };

  const durationSeconds = durationToSerializableSeconds(item.duration);

  const request: ConvertRequest = {
    jobId: id,
    sourcePath: item.filePath,
    targetFormat,
    outputDirectory: null,
    outputFilename: null,
    audioOnly,
    extraArgs: null,
    ffmpeg: ffmpegSettings,
    metadata: {
      title: item.title,
      author: item.author,
      thumbnail: item.thumbnail ?? '',
      duration: durationSeconds as unknown as bigint | null,
      url: item.url,
    },
  };

  queue.addConversion({
    id,
    title: item.title,
    author: item.author,
    thumbnail: item.thumbnail ?? '',
    duration: item.duration,
    url: item.url,
    targetFormat,
    audioOnly,
  });

  try {
    const result = await invoke<ConvertResult>('convert_local_file', { request });

    queue.updateConversion(id, {
      progress: 100,
      status: 'completed',
      statusMessage: 'Finished',
      filePath: result.outputPath,
      extension: result.extension,
      filesize: toNumber(result.filesize),
    });

    const resultDurationSeconds = result.duration != null ? toNumber(result.duration) : null;

    await history.add({
      url: item.url || `file://${result.outputPath}`,
      title: item.title,
      author: item.author,
      thumbnail: item.thumbnail ?? '',
      extension: result.extension,
      size: toNumber(result.filesize),
      duration: resultDurationSeconds ?? item.duration,
      convertedFormat: item.extension.toUpperCase(),
      filePath: result.outputPath,
      type: audioOnly ? 'audio' : item.type,
      downloadSource: 'ffmpeg',
    });

    toast.success(`Converted to ${targetFormat.toUpperCase()}`);

    setTimeout(() => {
      queue.removeConversion(id);
    }, 2000);

    return result;
  } catch (err) {
    console.error('Conversion failed:', err);

    queue.updateConversion(id, {
      status: 'failed',
      statusMessage: 'Failed',
      error: String(err),
    });

    toast.error(`Conversion failed: ${err}`);

    setTimeout(() => {
      queue.removeConversion(id);
    }, 5000);

    return null;
  }
}

export const conversions = {
  init: initConversions,
  cleanup: cleanupConversions,
  start: startConversion,
  cancel: cancelConversion,
};
