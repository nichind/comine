import { get } from 'svelte/store';
import { settings } from '$lib/stores/settings';

/**
 * Format bytes to human readable string
 * Uses settings to determine binary (KiB/MiB) or decimal (kB/MB) units
 */
export function formatSize(bytes: number): string {
  const { sizeUnit } = get(settings);
  const base = sizeUnit === 'binary' ? 1024 : 1000;
  const units =
    sizeUnit === 'binary' ? ['B', 'KiB', 'MiB', 'GiB', 'TiB'] : ['B', 'kB', 'MB', 'GB', 'TB'];

  if (bytes === 0) return '0 B';

  const i = Math.floor(Math.log(bytes) / Math.log(base));
  const size = bytes / Math.pow(base, i);

  const decimals = i === 0 ? 0 : size < 10 ? 2 : 1;

  return `${size.toFixed(decimals)} ${units[i]}`;
}

/**
 * Format speed (bytes per second) to human readable string
 */
export function formatSpeed(bytesPerSecond: number): string {
  const { sizeUnit } = get(settings);
  const base = sizeUnit === 'binary' ? 1024 : 1000;
  const units =
    sizeUnit === 'binary' ? ['B/s', 'KiB/s', 'MiB/s', 'GiB/s'] : ['B/s', 'kB/s', 'MB/s', 'GB/s'];

  if (bytesPerSecond === 0) return '0 B/s';

  const i = Math.floor(Math.log(bytesPerSecond) / Math.log(base));
  const speed = bytesPerSecond / Math.pow(base, i);

  const decimals = speed < 10 ? 2 : 1;

  return `${speed.toFixed(decimals)} ${units[Math.min(i, units.length - 1)]}`;
}

/**
 * Format duration in seconds to human readable string
 * Returns '--:--' for null/undefined, 'Live' for 0 or negative (live streams)
 */
export function formatDuration(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined) return '--:--';
  if (seconds <= 0) return 'Live';

  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);

  if (h > 0) {
    return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  }
  return `${m}:${s.toString().padStart(2, '0')}`;
}

/**
 * Truncate a title to a maximum length, adding ellipsis if needed
 * Useful for social media captions (Twitter, TikTok, Instagram) that can be very long
 */
export function truncateTitle(title: string, maxLength: number = 150): string {
  if (!title) return '';
  if (title.length <= maxLength) return title;

  const truncated = title.slice(0, maxLength);
  const lastSpace = truncated.lastIndexOf(' ');

  if (lastSpace > maxLength * 0.7) {
    return truncated.slice(0, lastSpace) + '…';
  }

  return truncated + '…';
}

/**
 * Calculate ETA based on downloaded bytes, total bytes, and speed
 */
export function calculateETA(downloaded: number, total: number, speed: number): string {
  if (speed <= 0 || downloaded >= total) return '--:--';

  const remaining = total - downloaded;
  const seconds = remaining / speed;

  return formatDuration(seconds);
}

/**
 * Check if a YouTube URL is a mix/radio playlist (auto-generated)
 * Mix playlists start with RD, RDMM, RDAMVM, RDGMEM prefixes
 */
export function isYouTubeMix(urlStr: string): boolean {
  try {
    const urlObj = new URL(urlStr);
    const hostname = urlObj.hostname.toLowerCase();

    if (!hostname.includes('youtube.com') && !hostname.includes('youtu.be')) {
      return false;
    }

    const list = urlObj.searchParams.get('list');
    if (!list) return false;

    return (
      list.startsWith('RD') ||
      list.startsWith('RDMM') ||
      list.startsWith('RDAMVM') ||
      list.startsWith('RDGMEM')
    );
  } catch {
    return false;
  }
}

/**
 * Check if a URL is likely a playlist or multi-video collection
 * Works across all major platforms supported by yt-dlp
 * @param urlStr - The URL to check
 * @param options - Optional settings
 * @param options.ignoreMixes - If true, YouTube mixes won't be considered playlists
 */
export function isLikelyPlaylist(urlStr: string, options?: { ignoreMixes?: boolean }): boolean {
  try {
    const urlObj = new URL(urlStr);
    const hostname = urlObj.hostname.toLowerCase();
    const pathname = urlObj.pathname.toLowerCase();

    if (hostname.includes('youtube.com') || hostname.includes('youtu.be')) {
      if (!urlObj.searchParams.has('list')) return false;
      // If ignoreMixes is enabled, don't treat mixes as playlists
      if (options?.ignoreMixes && isYouTubeMix(urlStr)) return false;
      return true;
    }

    if (hostname.includes('tiktok.com')) {
      if (pathname.match(/^\/@[\w.-]+\/?$/) && !pathname.includes('/video/')) {
        return true;
      }
    }

    if (hostname.includes('instagram.com')) {
      if (pathname.match(/^\/[\w.-]+\/?$/) && !pathname.match(/^\/(p|reel|stories|tv)\//)) {
        return true;
      }
      if (pathname.includes('/highlights/')) return true;
    }

    if (hostname.includes('twitter.com') || hostname.includes('x.com')) {
      if (pathname.includes('/media')) return true;
      if (pathname.includes('/likes')) return true;
    }

    if (hostname.includes('soundcloud.com')) {
      if (pathname.includes('/sets/')) return true;
      if (pathname.includes('/likes')) return true;
      if (pathname.includes('/reposts')) return true;
      if (pathname.match(/^\/[\w-]+\/?$/) && !pathname.includes('/tracks/')) {
        return true;
      }
    }

    if (hostname.includes('vimeo.com')) {
      if (pathname.includes('/album/')) return true;
      if (pathname.includes('/showcase/')) return true;
      if (pathname.includes('/channels/')) return true;
    }

    if (hostname.includes('twitch.tv')) {
      if (pathname.includes('/videos')) return true;
      if (pathname.includes('/collections/')) return true;
    }

    if (hostname.includes('bandcamp.com')) {
      if (pathname.includes('/album/')) return true;
    }

    if (/\/playlist\b/i.test(pathname)) return true;
    if (/\/album\b/i.test(pathname)) return true;
    if (/\/sets?\b/i.test(pathname)) return true;
    if (/\/collection\b/i.test(pathname)) return true;

    return false;
  } catch {
    return false;
  }
}

export function isLikelyChannel(urlStr: string): boolean {
  try {
    const urlObj = new URL(urlStr);
    const hostname = urlObj.hostname.toLowerCase();
    const pathname = urlObj.pathname.toLowerCase();

    if (hostname.includes('youtube.com') || hostname.includes('youtu.be')) {
      if (urlObj.searchParams.has('v') || urlObj.searchParams.has('list')) {
        return false;
      }
      // /channel/UCxxxx, /c/ChannelName, /@handle, /user/username
      if (/^\/(channel|c|user)\/[^/]+/i.test(pathname)) return true;
      if (/^\/@[^/]+/i.test(pathname)) return true;
      // /ChannelName/videos, /ChannelName/shorts, /ChannelName/live, /ChannelName/streams
      if (
        /^\/[^/]+\/(videos|shorts|live|streams|playlists|community|channels|about)\/?$/i.test(
          pathname
        )
      )
        return true;

      return false;
    }

    // Bilibili user pages
    if (hostname.includes('space.bilibili.com')) return true;

    // TikTok/Douyin profiles
    if (hostname.includes('tiktok.com') || hostname.includes('douyin.com') || hostname.includes('iesdouyin.com')) {
      if (pathname.match(/^\/@[\w.-]+\/?$/) && !pathname.includes('/video/')) return true;
    }

    // Instagram profiles
    if (hostname.includes('instagram.com')) {
      if (pathname.match(/^\/[\w.-]+\/?$/) && !pathname.match(/^\/(p|reel|stories|tv)\//)) return true;
    }

    // Twitter/X profiles
    if (hostname.includes('twitter.com') || hostname.includes('x.com')) {
      if (pathname.match(/^\/[\w.-]+\/?$/) && !pathname.includes('/status/')) return true;
    }

    // Twitch channels (top-level username)
    if (hostname.includes('twitch.tv')) {
      if (pathname.match(/^\/[\w-]+\/?$/) && !pathname.match(/^\/(videos|directory|downloads|p|settings)\b/)) return true;
    }

    // SoundCloud user profile pages
    if (hostname.includes('soundcloud.com')) {
      if (pathname.match(/^\/[\w-]+\/?$/) && !pathname.includes('/sets/')) return true;
    }

    // Generic fallbacks (common patterns)
    if (/\/(channel|user|profile|creator)\b/i.test(pathname)) return true;
    if (/^\/@[^/]+/i.test(pathname)) return true;

    return false;
  } catch {
    return false;
  }
}

export function isValidMediaUrl(text: string, patterns: string[]): boolean {
  try {
    const urlObj = new URL(text);
    if (!['http:', 'https:'].includes(urlObj.protocol)) return false;

    const matchesPattern = patterns.some((pattern) => urlObj.hostname.includes(pattern));
    if (matchesPattern) return true;

    const hostname = urlObj.hostname.toLowerCase();
    const commonVideoSites = [
      // YouTube & Google
      'youtube.com',
      'youtu.be',
      'music.youtube.com',
      // Chinese platforms (lux)
      'bilibili.com',
      'b23.tv',
      'douyin.com',
      'iqiyi.com',
      'youku.com',
      'qq.com',
      'mgtv.com',
      'le.com',
      'weibo.com',
      'kuaishou.com',
      'xiaohongshu.com',
      'xhslink.com',
      'huya.com',
      'douyu.com',
      'acfun.cn',
      // Other common platforms (yt-dlp)
      'twitter.com',
      'x.com',
      'tiktok.com',
      'instagram.com',
      'facebook.com',
      'twitch.tv',
      'vimeo.com',
      'dailymotion.com',
      'nicovideo.jp',
    ];
    return commonVideoSites.some((site) => hostname.includes(site));
  } catch {
    return false;
  }
}

// More permissive than isValidMediaUrl: used for manual/deep-link flows where we want
// to allow trying any yt-dlp-supported site (or generic extractor) without maintaining a whitelist.
export function isHttpUrl(text: string): boolean {
  try {
    const urlObj = new URL(text);
    return ['http:', 'https:'].includes(urlObj.protocol);
  } catch {
    return false;
  }
}

export function getQuickThumbnail(urlStr: string): string | null {
  try {
    const urlObj = new URL(urlStr);
    const hostname = urlObj.hostname.toLowerCase();

    if (
      hostname.includes('youtube.com') ||
      hostname.includes('youtu.be') ||
      hostname.includes('music.youtube.com')
    ) {
      const ytMatch = urlStr.match(/(?:youtube\.com\/watch\?v=|youtu\.be\/)([a-zA-Z0-9_-]{11})/);
      if (ytMatch) {
        const videoId = ytMatch[1];
        return `https://i.ytimg.com/vi/${videoId}/mqdefault.jpg`;
      }
      const ytmMatch = urlStr.match(/music\.youtube\.com\/watch\?v=([a-zA-Z0-9_-]{11})/);
      if (ytmMatch) {
        const videoId = ytmMatch[1];
        return `https://i.ytimg.com/vi/${videoId}/mqdefault.jpg`;
      }
    }

    return null;
  } catch {
    return null;
  }
}

type YouTubeThumbSize = 'default' | 'mq' | 'hq' | 'sd' | 'maxres';

export function normalizeYouTubeThumbnailUrl(
  thumbnailUrl: string,
  size: YouTubeThumbSize = 'mq'
): string {
  try {
    const u = new URL(thumbnailUrl);
    const host = u.hostname.toLowerCase();
    if (!host.includes('ytimg.com')) return thumbnailUrl;

    // Common forms:
    // - https://i.ytimg.com/vi/<id>/mqdefault.jpg
    // - https://i.ytimg.com/vi/<id>/maxresdefault.jpg
    // - https://i.ytimg.com/vi/<id>/hqdefault.jpg
    const wanted =
      size === 'default'
        ? 'default.jpg'
        : size === 'mq'
          ? 'mqdefault.jpg'
          : size === 'hq'
            ? 'hqdefault.jpg'
            : size === 'sd'
              ? 'sddefault.jpg'
              : 'maxresdefault.jpg';

    u.pathname = u.pathname.replace(
      /(\/vi\/[^/]+\/)(?:default|mqdefault|hqdefault|sddefault|maxresdefault)\.jpg$/i,
      `$1${wanted}`
    );

    return u.toString();
  } catch {
    return thumbnailUrl;
  }
}

export function getDisplayThumbnailUrl(
  mediaUrl: string,
  thumbnailUrl: string | null | undefined,
  size: YouTubeThumbSize = 'mq'
): string | null {
  const thumb = thumbnailUrl ?? getQuickThumbnail(mediaUrl);
  if (!thumb) return null;

  // Force YouTube thumbs to known sizes.
  if (/ytimg\.com/i.test(thumb)) {
    return normalizeYouTubeThumbnailUrl(thumb, size);
  }

  return thumb;
}

export function getQuickYouTubeThumbnail(urlStr: string): string | null {
  return getQuickThumbnail(urlStr);
}

export function cleanUrl(url: string, options?: { ignoreMixes?: boolean }): string {
  try {
    const parsed = new URL(url);
    const hostname = parsed.hostname.toLowerCase();

    if (
      hostname.includes('youtube.com') ||
      hostname.includes('youtu.be') ||
      hostname.includes('music.youtube.com')
    ) {
      parsed.searchParams.delete('si');
      parsed.searchParams.delete('feature');
      parsed.searchParams.delete('pp');

      if (options?.ignoreMixes) {
        const list = parsed.searchParams.get('list');
        if (
          list &&
          (list.startsWith('RD') ||
            list.startsWith('RDMM') ||
            list.startsWith('RDAMVM') ||
            list.startsWith('RDGMEM'))
        ) {
          parsed.searchParams.delete('list');
          parsed.searchParams.delete('index');
          parsed.searchParams.delete('start_radio');
        }
      }
    }

    if (hostname.includes('twitter.com') || hostname.includes('x.com')) {
      parsed.searchParams.delete('s');
      parsed.searchParams.delete('t');
      parsed.searchParams.delete('ref_src');
      parsed.searchParams.delete('ref_url');
      parsed.searchParams.delete('src');
    }

    if (hostname.includes('instagram.com')) {
      parsed.searchParams.delete('igshid');
      parsed.searchParams.delete('igsh');
      parsed.searchParams.delete('img_index');
    }

    if (hostname.includes('tiktok.com')) {
      parsed.searchParams.delete('is_copy_url');
      parsed.searchParams.delete('is_from_webapp');
      parsed.searchParams.delete('sender_device');
      parsed.searchParams.delete('sender_web_id');
      parsed.searchParams.delete('_r');
      parsed.searchParams.delete('_t');
      parsed.searchParams.delete('checksum');
      parsed.searchParams.delete('tt_from');
      parsed.searchParams.delete('share_item_id');
      parsed.searchParams.delete('share_app_id');
    }

    if (hostname.includes('soundcloud.com')) {
      parsed.searchParams.delete('si');
      parsed.searchParams.delete('ref');
    }

    if (hostname.includes('reddit.com')) {
      parsed.searchParams.delete('share_id');
      parsed.searchParams.delete('utm_name');
    }

    const trackingParams = [
      'utm_source',
      'utm_medium',
      'utm_campaign',
      'utm_term',
      'utm_content',
      'fbclid',
      'gclid',
      'dclid',
      'msclkid',
      'twclid',
      '_ga',
      '_gl',
      'mc_eid',
      'mc_cid',
      'oly_enc_id',
      'oly_anon_id',
      '__twitter_impression',
      '__cft__',
      '__tn__',
    ];
    trackingParams.forEach((param) => parsed.searchParams.delete(param));

    return parsed.toString();
  } catch {
    return url;
  }
}

const FILE_EXTENSIONS = [
  'zip',
  'rar',
  '7z',
  'tar',
  'gz',
  'bz2',
  'xz',
  'tgz',
  'tbz2',
  'exe',
  'msi',
  'dmg',
  'pkg',
  'deb',
  'rpm',
  'appimage',
  'jar',
  'apk',
  'ipa',
  'pdf',
  'doc',
  'docx',
  'xls',
  'xlsx',
  'ppt',
  'pptx',
  'odt',
  'ods',
  'odp',
  'jpg',
  'jpeg',
  'png',
  'gif',
  'webp',
  'svg',
  'bmp',
  'ico',
  'tiff',
  'psd',
  'mp4',
  'mkv',
  'avi',
  'mov',
  'webm',
  'mp3',
  'flac',
  'wav',
  'ogg',
  'aac',
  'm4a',
  'm4v',
  'iso',
  'img',
  'bin',
  'torrent',
  'rom',
  'nsp',
  'xci',
];

export function isDirectFileUrl(text: string): { isFile: boolean; filename: string | null } {
  try {
    const url = new URL(text);
    if (!url.protocol.startsWith('http')) return { isFile: false, filename: null };

    const getFileExtension = (str: string): string | null => {
      const ext = str.split('.').pop()?.toLowerCase();
      return ext && FILE_EXTENSIONS.includes(ext) ? ext : null;
    };

    const pathParts = url.pathname.split('/').filter(Boolean);
    if (pathParts.length > 0) {
      const pathFilename = decodeURIComponent(pathParts[pathParts.length - 1]);
      if (getFileExtension(pathFilename)) {
        return { isFile: true, filename: pathFilename };
      }
    }

    const filenameParam = url.searchParams.get('filename');
    if (filenameParam && getFileExtension(filenameParam)) {
      return { isFile: true, filename: filenameParam };
    }

    const rcdParam = url.searchParams.get('response-content-disposition');
    if (rcdParam) {
      const filenameMatch = rcdParam.match(/filename[*]?=["']?([^"';\s]+)/i);
      if (filenameMatch) {
        const fn = decodeURIComponent(filenameMatch[1]);
        if (getFileExtension(fn)) {
          return { isFile: true, filename: fn };
        }
      }
    }

    const rscdParam = url.searchParams.get('rscd');
    if (rscdParam) {
      const filenameMatch = rscdParam.match(/filename[*]?=["']?([^"';\s]+)/i);
      if (filenameMatch) {
        const fn = decodeURIComponent(filenameMatch[1]);
        if (getFileExtension(fn)) {
          return { isFile: true, filename: fn };
        }
      }
    }

    return { isFile: false, filename: null };
  } catch {
    return { isFile: false, filename: null };
  }
}
