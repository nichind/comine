export function isYouTubeUrl(hostname: string): boolean {
  return hostname.includes('youtube.com') || hostname.includes('youtu.be');
}

export function isYouTubeMix(urlStr: string): boolean {
  try {
    const urlObj = new URL(urlStr);
    const hostname = urlObj.hostname.toLowerCase();

    if (!isYouTubeUrl(hostname)) {
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

export function isLikelyPlaylist(urlStr: string, options?: { ignoreMixes?: boolean }): boolean {
  try {
    const urlObj = new URL(urlStr);
    const hostname = urlObj.hostname.toLowerCase();
    const pathname = urlObj.pathname.toLowerCase();

    if (isYouTubeUrl(hostname)) {
      if (!urlObj.searchParams.has('list')) return false;
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

    if (isYouTubeUrl(hostname)) {
      if (urlObj.searchParams.has('v') || urlObj.searchParams.has('list')) {
        return false;
      }

      if (/^\/(channel|c|user)\/[^/]+/i.test(pathname)) return true;
      if (/^\/@[^/]+/i.test(pathname)) return true;
      if (
        /^\/[^/]+\/(videos|shorts|live|streams|playlists|community|channels|about)\/?$/i.test(
          pathname
        )
      )
        return true;

      return false;
    }

    if (hostname.includes('space.bilibili.com')) return true;

    if (
      hostname.includes('tiktok.com') ||
      hostname.includes('douyin.com') ||
      hostname.includes('iesdouyin.com')
    ) {
      if (pathname.match(/^\/@[\w.-]+\/?$/) && !pathname.includes('/video/')) return true;
    }

    if (hostname.includes('instagram.com')) {
      if (pathname.match(/^\/[\w.-]+\/?$/) && !pathname.match(/^\/(p|reel|stories|tv)\//))
        return true;
    }

    if (hostname.includes('twitter.com') || hostname.includes('x.com')) {
      if (pathname.match(/^\/[\w.-]+\/?$/) && !pathname.includes('/status/')) return true;
    }

    if (hostname.includes('twitch.tv')) {
      if (
        pathname.match(/^\/[\w-]+\/?$/) &&
        !pathname.match(/^\/(videos|directory|downloads|p|settings)\b/)
      )
        return true;
    }

    if (hostname.includes('soundcloud.com')) {
      if (pathname.match(/^\/[\w-]+\/?$/) && !pathname.includes('/sets/')) return true;
    }

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
      'youtube.com',
      'youtu.be',
      'music.youtube.com',
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

export function isHttpUrl(text: string): boolean {
  try {
    const urlObj = new URL(text);
    return ['http:', 'https:'].includes(urlObj.protocol);
  } catch {
    return false;
  }
}

export function cleanUrl(url: string, options?: { ignoreMixes?: boolean }): string {
  try {
    const parsed = new URL(url);
    const hostname = parsed.hostname.toLowerCase();

    if (isYouTubeUrl(hostname)) {
      parsed.searchParams.delete('si');
      parsed.searchParams.delete('feature');
      parsed.searchParams.delete('pp');

      if (options?.ignoreMixes && isYouTubeMix(url)) {
        parsed.searchParams.delete('list');
        parsed.searchParams.delete('index');
        parsed.searchParams.delete('start_radio');
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

const FILE_EXTENSIONS = new Set([
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
]);

export function isDirectFileUrl(text: string): { isFile: boolean; filename: string | null } {
  try {
    const url = new URL(text);
    if (!url.protocol.startsWith('http')) return { isFile: false, filename: null };

    const getFileExtension = (str: string): string | null => {
      const ext = str.split('.').pop()?.toLowerCase();
      return ext && FILE_EXTENSIONS.has(ext) ? ext : null;
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
