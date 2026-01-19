
import { invoke } from '@tauri-apps/api/core';
import {
	getPlaylistInfoOnAndroid,
	getVideoInfoOnAndroid,
	isAndroid,
	waitForAndroidYtDlp,
} from '$lib/utils/android';
import type { ProxyConfig } from '$lib/stores/settings';

export interface BackendVideoInfo {
	title: string;
	uploader?: string;
	channel?: string;
	creator?: string;
	uploader_id?: string;
	thumbnail?: string;
	duration?: number;
	filesize?: number;
	ext?: string;
}

export interface BackendVideoFormat {
	format_id: string;
	ext: string;
	resolution?: string | null;
	fps?: number | null;
	vcodec?: string | null;
	acodec?: string | null;
	filesize?: number | null;
	filesize_approx?: number | null;
	tbr?: number | null;
	vbr?: number | null;
	abr?: number | null;
	asr?: number | null;
	format_note?: string | null;
	has_video: boolean;
	has_audio: boolean;
	quality?: number | null;
}

export interface BackendVideoFormats {
	title: string;
	author?: string | null;
	thumbnail?: string | null;
	duration?: number | null;
	formats: BackendVideoFormat[];
	[key: string]: unknown;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null;
}

function asString(value: unknown, fallback = ''): string {
	return typeof value === 'string' ? value : fallback;
}

function asStringOrNull(value: unknown): string | null {
	return typeof value === 'string' ? value : null;
}

function asNumberOrNull(value: unknown): number | null {
	return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function asBoolean(value: unknown): boolean {
	return value === true;
}

function coerceBackendVideoFormat(value: unknown): BackendVideoFormat | null {
	if (!isRecord(value)) return null;

	const format_id = typeof value.format_id === 'string' ? value.format_id : null;
	const ext = typeof value.ext === 'string' ? value.ext : null;
	if (!format_id || !ext) return null;

	return {
		format_id,
		ext,
		resolution: typeof value.resolution === 'string' ? value.resolution : null,
		fps: asNumberOrNull(value.fps),
		vcodec: typeof value.vcodec === 'string' ? value.vcodec : null,
		acodec: typeof value.acodec === 'string' ? value.acodec : null,
		filesize: asNumberOrNull(value.filesize),
		filesize_approx: asNumberOrNull(value.filesize_approx),
		tbr: asNumberOrNull(value.tbr),
		vbr: asNumberOrNull(value.vbr),
		abr: asNumberOrNull(value.abr),
		asr: asNumberOrNull(value.asr),
		format_note: typeof value.format_note === 'string' ? value.format_note : null,
		has_video: asBoolean(value.has_video),
		has_audio: asBoolean(value.has_audio),
		quality: asNumberOrNull(value.quality),
	};
}

export async function getVideoInfoBackend(params: {
	url: string;
	cookiesFromBrowser?: string;
	customCookies?: string;
	proxyConfig: ProxyConfig;
	youtubePlayerClient?: string | null;
}): Promise<BackendVideoInfo> {
	const { url, cookiesFromBrowser, customCookies, proxyConfig, youtubePlayerClient } = params;

	if (isAndroid()) {
		await waitForAndroidYtDlp();
		const androidInfo = await getVideoInfoOnAndroid(url, youtubePlayerClient ?? null);
		if (!androidInfo) {
			throw new Error('Android backend returned no video info');
		}

		return {
			title: String(androidInfo.title || ''),
			uploader: String(androidInfo.uploader || ''),
			uploader_id: String(androidInfo.uploader_id || ''),
			channel: String(androidInfo.channel || ''),
			thumbnail: String(androidInfo.thumbnail || ''),
			duration: Number(androidInfo.duration || 0),
			ext: String(androidInfo.ext || ''),
		};
	}

	return invoke<BackendVideoInfo>('get_video_info', {
		url,
		cookiesFromBrowser: cookiesFromBrowser ?? '',
		customCookies: customCookies ?? '',
		proxyConfig,
		youtubePlayerClient: youtubePlayerClient ?? null,
	});
}
export async function getPlaylistInfoBackend<T = unknown>(params: {
	url: string;
	offset?: number;
	limit?: number;
	cookiesFromBrowser?: string;
	customCookies?: string;
	proxyConfig: ProxyConfig;
	youtubePlayerClient?: string | null;
}): Promise<T> {
	const { url, offset, limit, cookiesFromBrowser, customCookies, proxyConfig, youtubePlayerClient } = params;

	if (isAndroid()) {
		await waitForAndroidYtDlp();
		return (await getPlaylistInfoOnAndroid(url, youtubePlayerClient ?? null)) as unknown as T;
	}

	return invoke<T>('get_playlist_info', {
		url,
		offset: offset ?? 0,
		limit: limit ?? 50,
		cookiesFromBrowser: cookiesFromBrowser ?? '',
		customCookies: customCookies ?? '',
		proxyConfig,
		youtubePlayerClient: youtubePlayerClient ?? null,
	});
}

export async function getVideoFormatsBackend(params: {
	url: string;
	cookiesFromBrowser?: string;
	customCookies?: string;
	proxyConfig: ProxyConfig;
	youtubePlayerClient?: string | null;
}): Promise<BackendVideoFormats> {
	const { url, cookiesFromBrowser, customCookies, proxyConfig, youtubePlayerClient } = params;

	if (isAndroid()) {
		await waitForAndroidYtDlp();
		const androidInfo = await getVideoInfoOnAndroid(url, youtubePlayerClient ?? null);
		if (!androidInfo) throw new Error('Android backend returned no video info');

		// MainActivity returns a superset of fields (including formats)
		const title = asString(androidInfo.title, '');
		const rawFormats = androidInfo.formats;
		const formats = Array.isArray(rawFormats)
			? (rawFormats.map(coerceBackendVideoFormat).filter(Boolean) as BackendVideoFormat[])
			: [];
		if (!formats.length) throw new Error('Android backend returned no formats');

		return {
			title,
			author: asStringOrNull(androidInfo.author ?? androidInfo.uploader ?? null),
			thumbnail: asStringOrNull(androidInfo.thumbnail ?? null),
			duration: asNumberOrNull(androidInfo.duration),
			formats,
		};
	}

	return invoke<BackendVideoFormats>('get_video_formats', {
		url,
		cookiesFromBrowser: cookiesFromBrowser ?? '',
		customCookies: customCookies ?? '',
		proxyConfig,
		youtubePlayerClient: youtubePlayerClient ?? null,
	});
}

