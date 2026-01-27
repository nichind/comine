import { invoke } from '@tauri-apps/api/core';
import { queue, type QueueItem } from './queue';
import { history, type HistoryItem } from './history';
import { logs } from './logs';
import { get } from 'svelte/store';

const isDesktop =
  typeof window !== 'undefined' &&
  !navigator.userAgent.includes('Android') &&
  !navigator.userAgent.includes('Mobile');

let queueDebounce: ReturnType<typeof setTimeout> | null = null;
let historyDebounce: ReturnType<typeof setTimeout> | null = null;
const DEBOUNCE_MS = 300;

function serializeQueueItem(item: QueueItem) {
  return {
    id: item.id,
    url: item.url,
    status: item.status,
    statusMessage: item.statusMessage || '',
    title: item.title || '',
    author: item.author || '',
    thumbnail: item.thumbnail || '',
    duration: item.duration || 0,
    progress: item.progress || 0,
    speed: item.speed || '',
    eta: item.eta || '',
    error: item.error || null,
    filePath: item.filePath || '',
    addedAt: item.addedAt || Date.now(),
  };
}

function serializeHistoryItem(item: HistoryItem) {
  return {
    id: item.id,
    url: item.url,
    title: item.title || '',
    author: item.author || '',
    thumbnail: item.thumbnail || '',
    duration: item.duration || 0,
    filePath: item.filePath || '',
    completedAt: item.downloadedAt || Date.now(),
  };
}

async function pushQueueToServer(items: QueueItem[]) {
  if (!isDesktop) return;

  try {
    const serialized = items.map(serializeQueueItem);
    await invoke('push_queue_status', { items: serialized });
  } catch (e) {
    logs.debug('serverSync', `Failed to push queue: ${e}`);
  }
}

async function pushHistoryToServer(items: HistoryItem[]) {
  if (!isDesktop) return;

  try {
    const recent = items.slice(0, 50);
    const serialized = recent.map(serializeHistoryItem);
    await invoke('push_history_status', { items: serialized });
  } catch (e) {
    logs.debug('serverSync', `Failed to push history: ${e}`);
  }
}

export function setupServerSync() {
  if (!isDesktop) return;

  queue.subscribe((state) => {
    if (queueDebounce) {
      clearTimeout(queueDebounce);
    }
    queueDebounce = setTimeout(() => {
      pushQueueToServer(state.items);
    }, DEBOUNCE_MS);
  });

  history.subscribe((state) => {
    if (historyDebounce) {
      clearTimeout(historyDebounce);
    }
    historyDebounce = setTimeout(() => {
      pushHistoryToServer(state.items);
    }, DEBOUNCE_MS);
  });

  const queueState = get(queue);
  const historyState = get(history);
  pushQueueToServer(queueState.items);
  pushHistoryToServer(historyState.items);

  logs.debug('serverSync', 'Server sync initialized');
}

export async function forceSync() {
  if (!isDesktop) return;

  const queueState = get(queue);
  const historyState = get(history);
  await Promise.all([pushQueueToServer(queueState.items), pushHistoryToServer(historyState.items)]);
}
