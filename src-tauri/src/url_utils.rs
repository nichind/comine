use url::Url;

/// Check if hostname is a YouTube domain.
fn is_youtube_url(hostname: &str) -> bool {
    hostname.contains("youtube.com") || hostname.contains("youtu.be")
}

/// Check if a YouTube URL is a mix playlist.
fn is_youtube_mix(url: &Url) -> bool {
    let hostname = url.host_str().unwrap_or("").to_lowercase();
    if !is_youtube_url(&hostname) {
        return false;
    }
    let list = match url.query_pairs().find(|(k, _)| k == "list") {
        Some((_, v)) => v.to_string(),
        None => return false,
    };
    list.starts_with("RD")
        || list.starts_with("RDMM")
        || list.starts_with("RDAMVM")
        || list.starts_with("RDGMEM")
}

/// Check if a URL is likely a playlist.
pub fn is_likely_playlist(url_str: &str, ignore_mixes: bool) -> bool {
    let url = match Url::parse(url_str) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let hostname = url.host_str().unwrap_or("").to_lowercase();
    let pathname = url.path().to_lowercase();

    if is_youtube_url(&hostname) {
        let has_list = url.query_pairs().any(|(k, _)| k == "list");
        if !has_list {
            return false;
        }
        if ignore_mixes && is_youtube_mix(&url) {
            return false;
        }
        return true;
    }

    if hostname.contains("tiktok.com") {
        if regex::Regex::new(r"^/@[\w.-]+/?$")
            .map(|r| r.is_match(&pathname))
            .unwrap_or(false)
            && !pathname.contains("/video/")
        {
            return true;
        }
    }

    if hostname.contains("instagram.com") {
        if regex::Regex::new(r"^/[\w.-]+/?$")
            .map(|r| r.is_match(&pathname))
            .unwrap_or(false)
            && !regex::Regex::new(r"^/(p|reel|stories|tv)/")
                .map(|r| r.is_match(&pathname))
                .unwrap_or(false)
        {
            return true;
        }
        if pathname.contains("/highlights/") {
            return true;
        }
    }

    if hostname.contains("twitter.com") || hostname.contains("x.com") {
        if pathname.contains("/media") || pathname.contains("/likes") {
            return true;
        }
    }

    if hostname.contains("soundcloud.com") {
        if pathname.contains("/sets/")
            || pathname.contains("/likes")
            || pathname.contains("/reposts")
        {
            return true;
        }
        if regex::Regex::new(r"^/[\w-]+/?$")
            .map(|r| r.is_match(&pathname))
            .unwrap_or(false)
            && !pathname.contains("/tracks/")
        {
            return true;
        }
    }

    if hostname.contains("vimeo.com") {
        if pathname.contains("/album/")
            || pathname.contains("/showcase/")
            || pathname.contains("/channels/")
        {
            return true;
        }
    }

    if hostname.contains("twitch.tv") {
        if pathname.contains("/videos") || pathname.contains("/collections/") {
            return true;
        }
    }

    if hostname.contains("bandcamp.com") && pathname.contains("/album/") {
        return true;
    }

    if regex::Regex::new(r"(?i)/playlist\b")
        .map(|r| r.is_match(&pathname))
        .unwrap_or(false)
    {
        return true;
    }
    if regex::Regex::new(r"(?i)/album\b")
        .map(|r| r.is_match(&pathname))
        .unwrap_or(false)
    {
        return true;
    }
    if regex::Regex::new(r"(?i)/sets?\b")
        .map(|r| r.is_match(&pathname))
        .unwrap_or(false)
    {
        return true;
    }
    if regex::Regex::new(r"(?i)/collection\b")
        .map(|r| r.is_match(&pathname))
        .unwrap_or(false)
    {
        return true;
    }

    false
}

/// Check if a URL is likely a channel/profile page.
pub fn is_likely_channel(url_str: &str) -> bool {
    let url = match Url::parse(url_str) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let hostname = url.host_str().unwrap_or("").to_lowercase();
    let pathname = url.path().to_lowercase();

    if is_youtube_url(&hostname) {
        if url.query_pairs().any(|(k, _)| k == "v" || k == "list") {
            return false;
        }
        if regex::Regex::new(r"(?i)^/(channel|c|user)/[^/]+")
            .map(|r| r.is_match(&pathname))
            .unwrap_or(false)
        {
            return true;
        }
        if regex::Regex::new(r"(?i)^/@[^/]+")
            .map(|r| r.is_match(&pathname))
            .unwrap_or(false)
        {
            return true;
        }
        if regex::Regex::new(
            r"(?i)^/[^/]+/(videos|shorts|live|streams|playlists|community|channels|about)/?$",
        )
        .map(|r| r.is_match(&pathname))
        .unwrap_or(false)
        {
            return true;
        }
        return false;
    }

    if hostname.contains("space.bilibili.com") {
        return true;
    }

    if hostname.contains("tiktok.com")
        || hostname.contains("douyin.com")
        || hostname.contains("iesdouyin.com")
    {
        if regex::Regex::new(r"^/@[\w.-]+/?$")
            .map(|r| r.is_match(&pathname))
            .unwrap_or(false)
            && !pathname.contains("/video/")
        {
            return true;
        }
    }

    if hostname.contains("instagram.com") {
        if regex::Regex::new(r"^/[\w.-]+/?$")
            .map(|r| r.is_match(&pathname))
            .unwrap_or(false)
            && !regex::Regex::new(r"^/(p|reel|stories|tv)/")
                .map(|r| r.is_match(&pathname))
                .unwrap_or(false)
        {
            return true;
        }
    }

    if hostname.contains("twitter.com") || hostname.contains("x.com") {
        if regex::Regex::new(r"^/[\w.-]+/?$")
            .map(|r| r.is_match(&pathname))
            .unwrap_or(false)
            && !pathname.contains("/status/")
        {
            return true;
        }
    }

    if hostname.contains("twitch.tv") {
        if regex::Regex::new(r"^/[\w-]+/?$")
            .map(|r| r.is_match(&pathname))
            .unwrap_or(false)
            && !regex::Regex::new(r"^/(videos|directory|downloads|p|settings)\b")
                .map(|r| r.is_match(&pathname))
                .unwrap_or(false)
        {
            return true;
        }
    }

    if hostname.contains("soundcloud.com") {
        if regex::Regex::new(r"^/[\w-]+/?$")
            .map(|r| r.is_match(&pathname))
            .unwrap_or(false)
            && !pathname.contains("/sets/")
        {
            return true;
        }
    }

    if regex::Regex::new(r"(?i)/(channel|user|profile|creator)\b")
        .map(|r| r.is_match(&pathname))
        .unwrap_or(false)
    {
        return true;
    }
    if regex::Regex::new(r"(?i)^/@[^/]+")
        .map(|r| r.is_match(&pathname))
        .unwrap_or(false)
    {
        return true;
    }

    false
}

/// Validate URL against user-configured patterns and common video sites.
pub fn is_valid_media_url(text: &str, patterns: &[String]) -> bool {
    let url = match Url::parse(text) {
        Ok(u) => u,
        Err(_) => return false,
    };

    match url.scheme() {
        "http" | "https" => {}
        _ => return false,
    }

    let hostname = url.host_str().unwrap_or("").to_lowercase();

    if patterns.iter().any(|p| hostname.contains(p)) {
        return true;
    }

    const COMMON_VIDEO_SITES: &[&str] = &[
        "youtube.com",
        "youtu.be",
        "music.youtube.com",
        "bilibili.com",
        "b23.tv",
        "douyin.com",
        "iqiyi.com",
        "youku.com",
        "qq.com",
        "mgtv.com",
        "le.com",
        "weibo.com",
        "kuaishou.com",
        "xiaohongshu.com",
        "xhslink.com",
        "huya.com",
        "douyu.com",
        "acfun.cn",
        "twitter.com",
        "x.com",
        "tiktok.com",
        "instagram.com",
        "facebook.com",
        "twitch.tv",
        "vimeo.com",
        "dailymotion.com",
        "nicovideo.jp",
    ];

    COMMON_VIDEO_SITES
        .iter()
        .any(|site| hostname.contains(site))
}

const FILE_EXTENSIONS: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "tgz", "tbz2", "exe", "msi", "dmg", "pkg", "deb",
    "rpm", "appimage", "jar", "apk", "ipa", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
    "odt", "ods", "odp", "jpg", "jpeg", "png", "gif", "webp", "svg", "bmp", "ico", "tiff", "psd",
    "mp4", "mkv", "avi", "mov", "webm", "mp3", "flac", "wav", "ogg", "aac", "m4a", "m4v", "iso",
    "img", "bin", "torrent", "rom", "nsp", "xci",
];

/// Check if a URL points directly to a file based on extension in path/params.
pub fn is_direct_file_url(text: &str) -> (bool, Option<String>) {
    let url = match Url::parse(text) {
        Ok(u) => u,
        Err(_) => return (false, None),
    };

    if !matches!(url.scheme(), "http" | "https") {
        return (false, None);
    }

    let has_file_ext = |s: &str| -> bool {
        s.rsplit('.')
            .next()
            .map(|ext| FILE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
            .unwrap_or(false)
    };

    let path = url.path();
    let last_segment = path.split('/').filter(|s| !s.is_empty()).last();
    if let Some(segment) = last_segment {
        let decoded = urlencoding::decode(segment).unwrap_or_else(|_| segment.into());
        if has_file_ext(&decoded) {
            return (true, Some(decoded.into_owned()));
        }
    }

    if let Some(filename) = url
        .query_pairs()
        .find(|(k, _)| k == "filename")
        .map(|(_, v)| v)
    {
        if has_file_ext(&filename) {
            return (true, Some(filename.into_owned()));
        }
    }

    for param_name in &["response-content-disposition", "rscd"] {
        if let Some(val) = url
            .query_pairs()
            .find(|(k, _)| k == *param_name)
            .map(|(_, v)| v)
        {
            if let Some(caps) = regex::Regex::new(r#"(?i)filename[*]?=["']?([^"';\s]+)"#)
                .ok()
                .and_then(|re| re.captures(&val))
            {
                let filename = urlencoding::decode(caps.get(1).unwrap().as_str())
                    .unwrap_or_else(|_| caps.get(1).unwrap().as_str().into())
                    .into_owned();
                if has_file_ext(&filename) {
                    return (true, Some(filename));
                }
            }
        }
    }

    (false, None)
}

/// Remove tracking/share parameters from a URL.
pub fn clean_url(url_str: &str, ignore_mixes: bool) -> String {
    let mut parsed = match Url::parse(url_str) {
        Ok(u) => u,
        Err(_) => return url_str.to_string(),
    };
    let hostname = parsed.host_str().unwrap_or("").to_lowercase();

    let mut to_remove: Vec<String> = Vec::new();

    if is_youtube_url(&hostname) {
        to_remove.extend(["si", "feature", "pp"].iter().map(|s| s.to_string()));
        if ignore_mixes && is_youtube_mix(&parsed) {
            to_remove.extend(
                ["list", "index", "start_radio"]
                    .iter()
                    .map(|s| s.to_string()),
            );
        }
    }

    if hostname.contains("twitter.com") || hostname.contains("x.com") {
        to_remove.extend(
            ["s", "t", "ref_src", "ref_url", "src"]
                .iter()
                .map(|s| s.to_string()),
        );
    }

    if hostname.contains("instagram.com") {
        to_remove.extend(
            ["igshid", "igsh", "img_index"]
                .iter()
                .map(|s| s.to_string()),
        );
    }

    if hostname.contains("tiktok.com") {
        to_remove.extend(
            [
                "is_copy_url",
                "is_from_webapp",
                "sender_device",
                "sender_web_id",
                "_r",
                "_t",
                "checksum",
                "tt_from",
                "share_item_id",
                "share_app_id",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
    }

    if hostname.contains("soundcloud.com") {
        to_remove.extend(["si", "ref"].iter().map(|s| s.to_string()));
    }

    if hostname.contains("reddit.com") {
        to_remove.extend(["share_id", "utm_name"].iter().map(|s| s.to_string()));
    }

    // Generic tracking params
    to_remove.extend(
        [
            "utm_source",
            "utm_medium",
            "utm_campaign",
            "utm_term",
            "utm_content",
            "fbclid",
            "gclid",
            "dclid",
            "msclkid",
            "twclid",
            "_ga",
            "_gl",
            "mc_eid",
            "mc_cid",
            "oly_enc_id",
            "oly_anon_id",
            "__twitter_impression",
            "__cft__",
            "__tn__",
        ]
        .iter()
        .map(|s| s.to_string()),
    );

    let remaining: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| !to_remove.contains(&k.to_string()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    parsed.set_query(None);
    if !remaining.is_empty() {
        let mut pairs = parsed.query_pairs_mut();
        for (k, v) in &remaining {
            pairs.append_pair(k, v);
        }
    }

    parsed.to_string()
}

/// Extract a quick YouTube thumbnail URL from a video URL.
pub fn get_quick_thumbnail(url_str: &str) -> Option<String> {
    let re =
        regex::Regex::new(r"(?:(?:music\.)?youtube\.com/watch\?v=|youtu\.be/)([a-zA-Z0-9_-]{11})")
            .ok()?;
    let caps = re.captures(url_str)?;
    let video_id = caps.get(1)?.as_str();
    Some(format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", video_id))
}
