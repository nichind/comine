import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { ResolveResult, ResolveEvent, ProxyConfig as BindingProxyConfig } from '$lib/bindings';
import type { ProxyConfig as AppProxyConfig } from '$lib/stores/settings';

export interface ResolveSettings {
  cookies_from_browser?: string | null;
  custom_cookies?: string | null;
  proxy?: BindingProxyConfig | null;
  youtube_player_client?: string | null;
  flat_playlist?: boolean;
  /** Number of entries to fetch per page (default: fetch all) */
  page_size?: number | null;
  /** Cursor for pagination (typically the 1-based start index) */
  cursor?: string | null;
}

export function convertProxyConfig(appConfig: AppProxyConfig): BindingProxyConfig {
  if (appConfig.mode === 'none') {
    return { enabled: false, url: null, username: null, password: null };
  }
  return {
    enabled: true,
    url: appConfig.mode === 'custom' ? appConfig.customUrl || null : null,
    username: null,
    password: null,
  };
}

export async function resolveUrl(url: string, settings?: ResolveSettings): Promise<ResolveResult> {
  return invoke<ResolveResult>('resolve_url', { url, settings: settings ?? null });
}

export type ResolveEventCallback = (event: ResolveEvent) => void;

export interface StreamingResolveSession {
  cancel: () => Promise<void>;
  unlisten: () => void;
}

export async function resolveUrlStream(
  url: string,
  settings: ResolveSettings | undefined,
  onEvent: ResolveEventCallback
): Promise<StreamingResolveSession> {
  const resolveId = crypto.randomUUID();
  const eventName = `resolve-event:${resolveId}`;

  // Set up the listener BEFORE starting the resolve
  const unlisten = await listen<ResolveEvent>(eventName, (event) => {
    onEvent(event.payload);
  });

  try {
    await invoke('resolve_url_stream', {
      resolveId,
      url,
      settings: settings ?? null,
    });
  } catch (e) {
    unlisten();
    throw e;
  }

  return {
    cancel: async () => {
      try {
        await invoke('cancel_resolve_stream', { resolveId });
      } catch {}
      unlisten();
    },
    unlisten,
  };
}
