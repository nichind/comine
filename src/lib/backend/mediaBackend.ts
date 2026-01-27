import { invoke } from '@tauri-apps/api/core';
import type {
  ResolveResult,
  UrlInfo,
  VideoFormat,
  PlaylistEntry,
  ProxyConfig as BindingProxyConfig,
} from '$lib/bindings';
import type { ProxyConfig as AppProxyConfig } from '$lib/stores/settings';

export type { ResolveResult, UrlInfo, VideoFormat, PlaylistEntry };

const resolveCache = new Map<string, { result: ResolveResult; timestamp: number }>();
const CACHE_TTL = 5 * 60 * 1000;

function getCached(key: string): ResolveResult | null {
  const cached = resolveCache.get(key);
  if (!cached) return null;
  if (Date.now() - cached.timestamp > CACHE_TTL) {
    resolveCache.delete(key);
    return null;
  }
  return cached.result;
}

function setCache(key: string, result: ResolveResult) {
  if (resolveCache.size > 100) {
    const oldest = Array.from(resolveCache.entries()).sort(
      (a, b) => a[1].timestamp - b[1].timestamp
    )[0];
    if (oldest) resolveCache.delete(oldest[0]);
  }
  resolveCache.set(key, { result, timestamp: Date.now() });
}

export interface ResolveSettings {
  cookies_from_browser?: string | null;
  custom_cookies?: string | null;
  proxy?: BindingProxyConfig | null;
  youtube_player_client?: string | null;
  flat_playlist?: boolean;
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
  const cacheKey = settings ? `${url}::${JSON.stringify(settings)}` : url;
  const cached = getCached(cacheKey);
  if (cached) return cached;

  const result = await invoke<ResolveResult>('resolve_url', { url, settings: settings ?? null });
  setCache(cacheKey, result);
  return result;
}
