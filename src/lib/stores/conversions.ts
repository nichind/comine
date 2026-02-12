import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { get } from 'svelte/store';
import { history } from './history';
import { queue } from './queue';
import { settings } from './settings';
import { logs } from './logs';
import { safeDurationSeconds } from '$lib/utils/duration';
import type { HistoryItem } from './history';
import type {
  ConvertRequest,
  ConvertResult,
  ConvertProgress,
  FfmpegConvertSettings,
} from '$lib/bindings';

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
    return null;
  }

  const id = crypto.randomUUID();

  const appSettings = get(settings);
  const ffmpegSettings: FfmpegConvertSettings = {
    hwAccel: appSettings.ffmpeg.hwAccel,
  };

  const durationSeconds = safeDurationSeconds(item.duration);

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
      duration: durationSeconds,
      url: item.url,
    },
  };

  logs.debug(
    'conversions',
    `Starting with duration: ${item.duration} type: ${typeof item.duration}`
  );

  queue.addConversion({
    id,
    title: item.title,
    author: item.author,
    thumbnail: item.thumbnail ?? '',
    duration: typeof item.duration === 'number' ? item.duration : 0,
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
      filesize: result.filesize,
    });

    const resultDurationSeconds = result.duration ?? null;

    await history.add({
      url: item.url || `file://${result.outputPath}`,
      title: item.title,
      author: item.author,
      thumbnail: item.thumbnail ?? '',
      extension: result.extension,
      size: result.filesize,
      duration: resultDurationSeconds ?? item.duration,
      convertedFormat: item.extension.toUpperCase(),
      filePath: result.outputPath,
      type: audioOnly ? 'audio' : item.type,
      downloadSource: 'ffmpeg',
    });

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

    setTimeout(() => {
      queue.removeConversion(id);
    }, 5000);

    return null;
  }
}
