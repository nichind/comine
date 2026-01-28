import { invoke } from '@tauri-apps/api/core';

interface AndroidWindow extends Window {
  AndroidYtDlp?: {
    isReady(): boolean;
    getVersion(): string;
    updateChannel(channel: string, callbackName: string): void;
  };
  __YTDLP_READY__?: boolean;
  __androidLog?: (level: string, source: string, message: string) => void;
}

declare let window: AndroidWindow;

export function isAndroid(): boolean {
  return typeof window !== 'undefined' && 'AndroidYtDlp' in window;
}

export function isDesktop(): boolean {
  return typeof window !== 'undefined' && !('AndroidYtDlp' in window);
}

type LogHandler = (
  level: 'trace' | 'debug' | 'info' | 'warn' | 'error',
  source: string,
  message: string
) => void;

export function setupAndroidLogHandler(handler: LogHandler): void {
  window.__androidLog = (level: string, source: string, message: string) => {
    const validLevel = ['trace', 'debug', 'info', 'warn', 'error'].includes(level)
      ? (level as 'trace' | 'debug' | 'info' | 'warn' | 'error')
      : 'debug';
    handler(validLevel, source, message);
  };
}

export interface ShareIntentEvent {
  url: string;
}

export interface NavigateToEvent {
  path: string;
}

export function onShareIntent(callback: (url: string) => void): () => void {
  if (!isAndroid()) {
    return () => {};
  }

  const handler = (event: Event) => {
    const customEvent = event as CustomEvent<ShareIntentEvent>;
    const url = customEvent.detail?.url;
    if (url) {
      callback(url);
    }
  };

  window.addEventListener('share-intent', handler);
  return () => window.removeEventListener('share-intent', handler);
}

export function onNavigateTo(callback: (path: string) => void): () => void {
  if (!isAndroid()) {
    return () => {};
  }

  const handler = (event: Event) => {
    const customEvent = event as CustomEvent<NavigateToEvent>;
    const path = customEvent.detail?.path;
    if (path) {
      callback(path);
    }
  };

  window.addEventListener('navigate-to', handler);
  return () => window.removeEventListener('navigate-to', handler);
}

export async function openFileOnAndroid(filePath: string): Promise<boolean> {
  if (!isAndroid()) return false;
  try {
    return await invoke<boolean>('open_file', { path: filePath });
  } catch {
    return false;
  }
}

export async function openFolderOnAndroid(filePath: string): Promise<boolean> {
  if (!isAndroid()) return false;
  try {
    await invoke('open_folder', { path: filePath });
    return true;
  } catch {
    return false;
  }
}

export async function pickFileOnAndroid(mimeTypes: string): Promise<string | null> {
  if (!isAndroid()) return null;
  try {
    return await invoke<string | null>('pick_file', { mimeTypes });
  } catch {
    return null;
  }
}

export async function pickFolderOnAndroid(): Promise<string | null> {
  if (!isAndroid()) return null;
  try {
    return await invoke<string | null>('pick_folder');
  } catch {
    return null;
  }
}

export function callAndroidWithCallback<T>(
  method: (callbackName: string) => void
): Promise<T> {
  return new Promise((resolve, reject) => {
    const callbackName = `__android_cb_${Date.now()}_${Math.random().toString(36).slice(2)}`;
    
    (window as unknown as Record<string, unknown>)[callbackName] = (response: string) => {
      delete (window as unknown as Record<string, unknown>)[callbackName];
      try {
        const parsed = JSON.parse(response);
        if (parsed.error) {
          reject(new Error(parsed.error));
        } else {
          resolve(parsed as T);
        }
      } catch {
        resolve(response as unknown as T);
      }
    };
    
    try {
      method(callbackName);
    } catch (err) {
      delete (window as unknown as Record<string, unknown>)[callbackName];
      reject(err);
    }
  });
}

export function cleanupAndroidCallbacks(): void {}
