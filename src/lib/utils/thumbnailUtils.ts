import { isYouTubeUrl } from './urlUtils';

export function getQuickThumbnail(urlStr: string): string | null {
  try {
    const urlObj = new URL(urlStr);
    const hostname = urlObj.hostname.toLowerCase();

    if (isYouTubeUrl(hostname)) {
      const ytMatch = urlStr.match(
        /(?:(?:music\.)?youtube\.com\/watch\?v=|youtu\.be\/)([a-zA-Z0-9_-]{11})/
      );
      if (ytMatch) {
        const videoId = ytMatch[1];
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
