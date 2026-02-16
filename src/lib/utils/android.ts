interface AndroidWindow extends Window {
  AndroidYtDlp?: object;
  __androidLog?: (level: string, source: string, message: string) => void;
  [key: string]: unknown;
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

interface ShareIntentEvent {
  url: string;
}

interface NavigateToEvent {
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

export function shareLogs(content: string): void {
  if (!isAndroid()) return;
  const bridge = (window as any).AndroidYtDlp;
  if (bridge?.shareLogs) {
    bridge.shareLogs(content);
  }
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
