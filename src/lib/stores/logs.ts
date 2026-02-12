import { writable, derived, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { browser } from '$app/environment';
import { isAndroid } from '$lib/utils/android';

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

export interface LogEntry {
  id: string;
  timestamp: Date;
  level: LogLevel;
  source: string;
  message: string;
}

interface LogFilters {
  levels: Set<LogLevel>;
  search: string;
}

interface LogsState {
  liveBuffer: LogEntry[];
  diskEntries: LogEntry[];
  diskLoaded: boolean;
  filters: LogFilters;
  sessionFile: string | null;
  diskCount: number;
  isAndroidPlatform: boolean;
}

const LIVE_BUFFER_SIZE = 100;
const ANDROID_BUFFER_SIZE = 500;
const MAX_DISK_ENTRIES = 500;

const initialState: LogsState = {
  liveBuffer: [],
  diskEntries: [],
  diskLoaded: false,
  filters: {
    levels: new Set(['trace', 'debug', 'info', 'warn', 'error']),
    search: '',
  },
  sessionFile: null,
  diskCount: 0,
  isAndroidPlatform: false,
};

let isInitializing = false;

function parseLogLine(line: string, index: number): LogEntry | null {
  const match = line.match(/^\[([^\]]+)\]\s*\[(\w+)\]\s*([^:]+):\s*(.*)$/);
  if (!match || match.length < 5) {
    return {
      id: `disk-${index}`,
      timestamp: new Date(),
      level: 'info',
      source: 'unknown',
      message: line,
    };
  }

  const timestampStr = match[1];
  const levelStr = match[2];
  const source = match[3];
  const message = match[4];
  const level = levelStr.toLowerCase() as LogLevel;

  return {
    id: `disk-${index}`,
    timestamp: new Date(timestampStr),
    level: ['trace', 'debug', 'info', 'warn', 'error'].includes(level) ? level : 'info',
    source: source.trim(),
    message: message.trim(),
  };
}

function createLogsStore() {
  const { subscribe, set, update } = writable<LogsState>(initialState);

  let idCounter = 0;
  const onAndroid = browser && isAndroid();

  if (browser) {
    update((s) => ({ ...s, isAndroidPlatform: onAndroid }));
  }

  async function initLogging() {
    if (!browser || isInitializing || onAndroid) return;

    const state = get({ subscribe });
    if (state.sessionFile) return;

    isInitializing = true;
    try {
      const sessionFile = await invoke<string>('get_log_file_path');
      update((s) => ({ ...s, sessionFile }));
      console.debug('Log file initialized:', sessionFile);

      const deleted = await invoke<number>('cleanup_old_logs', { keepSessions: 10 });
      if (deleted > 0) {
        console.debug(`Cleaned up ${deleted} old log files`);
      }
    } catch (e) {
      console.warn('Failed to initialize file logging:', e);
    } finally {
      isInitializing = false;
    }
  }

  async function writeToFile(entry: LogEntry) {
    if (onAndroid) return;

    const state = get({ subscribe });
    if (!state.sessionFile) return;

    try {
      const time = entry.timestamp.toISOString();
      const line = `[${time}] [${entry.level.toUpperCase()}] ${entry.source}: ${entry.message}`;
      await invoke('append_log', { sessionFile: state.sessionFile, entry: line });
    } catch (e) {}
  }

  if (browser) {
    initLogging();
  }

  return {
    subscribe,

    log(level: LogLevel, source: string, message: string) {
      const entry: LogEntry = {
        id: `live-${++idCounter}`,
        timestamp: new Date(),
        level,
        source,
        message,
      };

      update((state) => {
        const maxSize = state.isAndroidPlatform ? ANDROID_BUFFER_SIZE : LIVE_BUFFER_SIZE;
        const liveBuffer = [entry, ...state.liveBuffer].slice(0, maxSize);
        return { ...state, liveBuffer };
      });

      writeToFile(entry);
    },

    debug(source: string, message: string) {
      this.log('debug', source, message);
    },
    info(source: string, message: string) {
      this.log('info', source, message);
    },
    warn(source: string, message: string) {
      this.log('warn', source, message);
    },
    error(source: string, message: string) {
      this.log('error', source, message);
    },

    async loadFromDisk(): Promise<void> {
      const state = get({ subscribe });

      if (state.isAndroidPlatform) {
        update((s) => ({ ...s, diskLoaded: true }));
        return;
      }

      if (!state.sessionFile) return;

      try {
        const lines = await invoke<string[]>('read_session_logs', {
          sessionFile: state.sessionFile,
        });

        const diskEntries = lines
          .slice(-MAX_DISK_ENTRIES)
          .map((line, index) => parseLogLine(line, index))
          .filter((entry): entry is LogEntry => entry !== null)
          .reverse();

        update((s) => ({
          ...s,
          diskEntries,
          diskLoaded: true,
          diskCount: lines.length,
        }));

        console.debug(`Loaded ${diskEntries.length} log entries from disk`);
      } catch (e) {
        console.error('Failed to load logs from disk:', e);
      }
    },

    clearMemory() {
      update((state) => ({
        ...state,
        liveBuffer: [],
        diskEntries: [],
        diskLoaded: false,
      }));
    },

    toggleLevel(level: LogLevel) {
      update((state) => {
        const levels = new Set(state.filters.levels);
        if (levels.has(level)) {
          levels.delete(level);
        } else {
          levels.add(level);
        }
        return { ...state, filters: { ...state.filters, levels } };
      });
    },

    setSearch(search: string) {
      update((state) => ({
        ...state,
        filters: { ...state.filters, search },
      }));
    },

    clear() {
      update((state) => ({ ...state, liveBuffer: [], diskEntries: [], diskLoaded: false }));
    },

    async exportAsText(): Promise<string> {
      const state = get({ subscribe });

      if (state.isAndroidPlatform || !state.sessionFile) {
        return state.liveBuffer
          .map((entry) => {
            const time = entry.timestamp.toISOString();
            return `[${time}] [${entry.level.toUpperCase()}] ${entry.source}: ${entry.message}`;
          })
          .reverse()
          .join('\n');
      }

      try {
        const lines = await invoke<string[]>('read_session_logs', {
          sessionFile: state.sessionFile,
        });
        return lines.join('\n');
      } catch (e) {
        console.error('Failed to read logs for export:', e);
        return state.liveBuffer
          .map((entry) => {
            const time = entry.timestamp.toISOString();
            return `[${time}] [${entry.level.toUpperCase()}] ${entry.source}: ${entry.message}`;
          })
          .reverse()
          .join('\n');
      }
    },

    async openLogsFolder(): Promise<void> {
      try {
        await invoke('open_logs_folder');
      } catch (e) {
        console.error('Failed to open logs folder:', e);
      }
    },
  };
}

export const logs = createLogsStore();

const mergedEntries = derived(logs, ($logs): LogEntry[] => {
  if (!$logs.diskLoaded) return $logs.liveBuffer;
  const liveMessages = new Set($logs.liveBuffer.map((e) => `${e.source}:${e.message}`));
  return [
    ...$logs.liveBuffer,
    ...$logs.diskEntries.filter((de) => !liveMessages.has(`${de.source}:${de.message}`)),
  ];
});

export const filteredLogs = derived([mergedEntries, logs], ([$entries, $logs]) => {
  return $entries.filter((entry) => {
    if (!$logs.filters.levels.has(entry.level)) return false;
    if ($logs.filters.search) {
      const search = $logs.filters.search.toLowerCase();
      return (
        entry.message.toLowerCase().includes(search) || entry.source.toLowerCase().includes(search)
      );
    }
    return true;
  });
});

export const logStats = derived(mergedEntries, ($entries) => {
  const counts = {
    trace: 0,
    debug: 0,
    info: 0,
    warn: 0,
    error: 0,
    total: $entries.length,
  };

  for (const entry of $entries) {
    counts[entry.level]++;
  }

  return counts;
});
