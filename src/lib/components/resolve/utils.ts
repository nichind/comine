import { formatSize } from '$lib/utils/format';
import type { VideoFormat } from '$lib/bindings/VideoFormat';
import type { UrlInfo } from '$lib/bindings/UrlInfo';

import type { IconName } from '$lib/components/ui/Icon.svelte';

export function getCodec(codec: string | null): string {
  if (!codec || codec === 'none') return '';
  if (codec.startsWith('avc1')) return 'H.264';
  if (codec.startsWith('av01')) return 'AV1';
  if (codec.startsWith('vp9') || codec.startsWith('vp09')) return 'VP9';
  if (codec.startsWith('hev1') || codec.startsWith('hvc1')) return 'H.265';
  if (codec.startsWith('mp4a')) return 'AAC';
  if (codec.startsWith('opus')) return 'Opus';
  return codec.split('.')[0];
}

export function getFmtSize(f: VideoFormat): string {
  const size = f.filesize ?? f.filesizeApprox;
  if (size) return formatSize(Number(size));
  return '';
}

export function getFormatTags(f: VideoFormat): string[] {
  const tags: string[] = [];

  if (f.dynamicRange) {
    const dr = f.dynamicRange.toLowerCase();
    if (dr.includes('dolby') || dr.includes('dovi')) tags.push('Dolby Vision');
    else if (dr.includes('hdr10+') || dr.includes('hdr10plus')) tags.push('HDR10+');
    else if (dr.includes('hdr10')) tags.push('HDR10');
    else if (dr.includes('hlg')) tags.push('HLG');
    else if (dr !== 'sdr' && dr !== '') tags.push('HDR');
  }

  if (f.language && f.language !== 'und') {
    tags.push(f.language.toUpperCase());
  }

  const note = f.formatNote?.toLowerCase() ?? '';
  if (note.includes('ai-generated') || note.includes('ai generated')) tags.push('AI');
  if (note.includes('ai-translated') || note.includes('ai translated')) tags.push('AI Translated');
  if (note.includes('dubbed') || note.includes('dub')) {
    if (!tags.some((t) => t === 'AI Translated')) tags.push('Dubbed');
  }
  if (note.includes('descriptive') || note.includes('description')) tags.push('Descriptive');
  if (note.includes('original')) tags.push('Original');
  if (note.includes('premium')) tags.push('Premium');

  return tags;
}

/**
 * Returns true if the format is NOT AI-generated, AI-translated, dubbed, or descriptive.
 * Used to prefer original/native tracks when applying quality presets.
 *
 * Context-aware: uses "original" keyword in formatNote (YouTube's signal)
 * and falls back to explicit non-original markers.
 * Returns null if all formats are considered original (single-language content).
 */
export function getOriginalFormatIds(formats: VideoFormat[]): Set<string> | null {
  const hasExplicitOriginals = formats.some((f) =>
    f.formatNote?.toLowerCase().includes('original')
  );

  if (!hasExplicitOriginals) {
    const hasNonOriginalMarkers = formats.some((f) => {
      const note = f.formatNote?.toLowerCase() ?? '';
      return (
        note.includes('ai-generated') ||
        note.includes('ai generated') ||
        note.includes('ai-translated') ||
        note.includes('ai translated') ||
        note.includes('dubbed') ||
        note.includes('descriptive')
      );
    });
    if (!hasNonOriginalMarkers) return null;
  }

  const ids = new Set<string>();
  for (const f of formats) {
    const note = f.formatNote?.toLowerCase() ?? '';

    if (note.includes('original')) {
      ids.add(f.formatId);
      continue;
    }

    if (
      note.includes('ai-generated') ||
      note.includes('ai generated') ||
      note.includes('ai-translated') ||
      note.includes('ai translated') ||
      note.includes('dubbed') ||
      note.includes('dub') ||
      note.includes('descriptive') ||
      note.includes('description')
    ) {
      continue;
    }

    if (!f.language) {
      ids.add(f.formatId);
      continue;
    }

    if (hasExplicitOriginals) {
      continue;
    }

    ids.add(f.formatId);
  }

  return ids.size === formats.length ? null : ids;
}

export function makeVideoLabel(f: VideoFormat): string {
  const parts: string[] = [];

  if (f.resolution) {
    const resParts = f.resolution.split('x');
    if (resParts.length >= 2 && resParts[1]) {
      const height = parseInt(resParts[1]);
      if (!isNaN(height) && height > 0) {
        parts.push(height + 'p');
      } else {
        parts.push(f.resolution);
      }
    } else if (resParts.length === 1 && resParts[0]) {
      const clean = resParts[0].replace('p', '');
      const height = parseInt(clean);
      if (!isNaN(height) && height > 0) {
        parts.push(height + 'p');
      } else {
        parts.push(f.resolution);
      }
    } else {
      parts.push('?');
    }
  } else if (f.formatNote) {
    parts.push(f.formatNote);
  } else {
    parts.push('?');
  }

  if (f.fps && f.fps > 30) parts.push(`${Math.round(f.fps)}fps`);
  const codec = getCodec(f.vcodec);
  if (codec) parts.push(codec);

  const tags = getFormatTags(f);
  if (tags.length > 0) parts.push(tags.join(', '));

  const size = getFmtSize(f);
  if (size) parts.push(size);
  return parts.join(' · ');
}

export function makeAudioLabel(f: VideoFormat): string {
  const parts: string[] = [];
  if (f.abr) parts.push(`${Math.round(f.abr)} kbps`);
  const codec = getCodec(f.acodec);
  if (codec) parts.push(codec);

  const tags = getFormatTags(f);
  if (tags.length > 0) parts.push(tags.join(', '));

  const size = getFmtSize(f);
  if (size) parts.push(size);
  return parts.join(' · ') || 'audio';
}

// Single registry replaces 3 duplicated platform-detection functions
interface PlatformEntry {
  name: string;
  icon: IconName;
  extractorPatterns: string[]; // matched against info.extractor (lowercase)
  urlPatterns?: string[]; // matched against URL (lowercase)
}

const PLATFORM_REGISTRY: PlatformEntry[] = [
  {
    name: 'YouTube Music',
    icon: 'platform_youtube_music',
    extractorPatterns: ['youtube:music', 'youtube_music'],
    urlPatterns: ['music.youtube.com'],
  },
  {
    name: 'YouTube',
    icon: 'platform_youtube',
    extractorPatterns: ['youtube'],
    urlPatterns: ['youtube.com', 'youtu.be'],
  },
  {
    name: 'Bilibili',
    icon: 'platform_bilibili',
    extractorPatterns: ['bilibili'],
    urlPatterns: ['bilibili.com', 'b23.tv'],
  },
  {
    name: 'X',
    icon: 'platform_twitter',
    extractorPatterns: ['twitter', 'x.com', 'nitter'],
    urlPatterns: ['twitter.com', 'x.com'],
  },
  {
    name: 'TikTok',
    icon: 'platform_tiktok',
    extractorPatterns: ['tiktok'],
    urlPatterns: ['tiktok.com', 'douyin.com', 'iesdouyin.com'],
  },
  {
    name: 'Instagram',
    icon: 'platform_instagram',
    extractorPatterns: ['instagram'],
    urlPatterns: ['instagram.com'],
  },
  {
    name: 'Twitch',
    icon: 'platform_twitch',
    extractorPatterns: ['twitch'],
    urlPatterns: ['twitch.tv'],
  },
  {
    name: 'Vimeo',
    icon: 'platform_vimeo',
    extractorPatterns: ['vimeo'],
    urlPatterns: ['vimeo.com'],
  },
  {
    name: 'Facebook',
    icon: 'platform_facebook',
    extractorPatterns: ['facebook'],
    urlPatterns: ['facebook.com', 'fb.watch'],
  },
  { name: 'SoundCloud', icon: 'platform_generic', extractorPatterns: ['soundcloud'] },
  { name: 'Reddit', icon: 'platform_generic', extractorPatterns: ['reddit'] },
  { name: 'Niconico', icon: 'platform_generic', extractorPatterns: ['niconico', 'nicovideo'] },
  { name: 'Dailymotion', icon: 'platform_generic', extractorPatterns: ['dailymotion'] },
  { name: 'Rumble', icon: 'platform_generic', extractorPatterns: ['rumble'] },
  {
    name: 'Weibo',
    icon: 'platform_weibo',
    extractorPatterns: ['weibo'],
    urlPatterns: ['weibo.com'],
  },
  { name: 'Pixiv', icon: 'platform_generic', extractorPatterns: ['pixiv'] },
  { name: 'Danbooru', icon: 'platform_generic', extractorPatterns: ['danbooru'] },
  { name: 'Gelbooru', icon: 'platform_generic', extractorPatterns: ['gelbooru'] },
  { name: 'Imgur', icon: 'platform_generic', extractorPatterns: ['imgur'] },
  { name: 'DeviantArt', icon: 'platform_generic', extractorPatterns: ['deviantart'] },
  { name: 'ArtStation', icon: 'platform_generic', extractorPatterns: ['artstation'] },
  { name: 'Flickr', icon: 'platform_generic', extractorPatterns: ['flickr'] },
  { name: 'Tumblr', icon: 'platform_generic', extractorPatterns: ['tumblr'] },
  { name: 'Pinterest', icon: 'platform_generic', extractorPatterns: ['pinterest'] },
  { name: 'e621', icon: 'platform_generic', extractorPatterns: ['e621', 'e926'] },
  { name: 'Sankaku', icon: 'platform_generic', extractorPatterns: ['sankaku'] },
  { name: 'Konachan', icon: 'platform_generic', extractorPatterns: ['konachan'] },
  { name: 'Safebooru', icon: 'platform_generic', extractorPatterns: ['safebooru'] },
  { name: 'Rule34', icon: 'platform_generic', extractorPatterns: ['rule34'] },
  { name: 'yande.re', icon: 'platform_generic', extractorPatterns: ['yande.re', 'yandere'] },
  { name: 'MangaDex', icon: 'platform_generic', extractorPatterns: ['mangadex'] },
  { name: 'nhentai', icon: 'platform_generic', extractorPatterns: ['nhentai'] },
  { name: 'Hitomi', icon: 'platform_generic', extractorPatterns: ['hitomi'] },
  { name: 'Mastodon', icon: 'platform_generic', extractorPatterns: ['mastodon', 'fediverse'] },
  { name: 'Torrent', icon: 'package', extractorPatterns: ['torrent', 'magnet'] },
  { name: 'Nyaa', icon: 'platform_generic', extractorPatterns: ['nyaa'] },
  { name: 'Google Drive', icon: 'platform_generic', extractorPatterns: ['gdrive', 'google_drive'] },
  { name: 'MEGA', icon: 'platform_generic', extractorPatterns: ['mega'] },
  { name: 'Dropbox', icon: 'platform_generic', extractorPatterns: ['dropbox'] },
  { name: 'OneDrive', icon: 'platform_generic', extractorPatterns: ['onedrive'] },
  { name: 'MediaFire', icon: 'platform_generic', extractorPatterns: ['mediafire'] },
];

const CONTENT_TYPE_ICONS: Record<string, IconName> = {
  Gallery: 'gallery',
  Image: 'gallery',
  Audio: 'music',
  Torrent: 'package',
  File: 'file_download',
};

function findByExtractor(ext: string): PlatformEntry | undefined {
  return PLATFORM_REGISTRY.find((p) => p.extractorPatterns.some((pat) => ext.includes(pat)));
}

function findByUrl(url: string): PlatformEntry | undefined {
  const u = url.toLowerCase();
  return PLATFORM_REGISTRY.find((p) => p.urlPatterns?.some((pat) => u.includes(pat)));
}

export function getPlatformName(info: UrlInfo | null, url?: string): string {
  const ext = info?.extractor?.toLowerCase() ?? '';
  const match = ext ? findByExtractor(ext) : undefined;
  if (match) return match.name;

  if (!ext && info?.contentType) {
    const ct = info.contentType;
    if (ct === 'Gallery' || ct === 'Image' || ct === 'Torrent' || ct === 'File' || ct === 'Article')
      return ct;
  }

  if (!ext && url) {
    const urlMatch = findByUrl(url);
    if (urlMatch) return urlMatch.name;
    try {
      const hostname = new URL(url).hostname.replace('www.', '');
      const domain = hostname.split('.')[0];
      if (domain) return domain.charAt(0).toUpperCase() + domain.slice(1);
    } catch {}
  }

  return ext || 'Media';
}

export function getPlatformIcon(info: UrlInfo | null): IconName {
  const ext = info?.extractor?.toLowerCase() ?? '';
  const match = ext ? findByExtractor(ext) : undefined;
  if (match) return match.icon;

  const ct = info?.contentType;
  if (ct && ct in CONTENT_TYPE_ICONS) return CONTENT_TYPE_ICONS[ct];

  return 'platform_generic';
}

export function getPlatformIconFromUrl(urlStr: string): IconName {
  if (!urlStr.trim()) return 'platform_generic';
  return findByUrl(urlStr)?.icon ?? 'platform_generic';
}
