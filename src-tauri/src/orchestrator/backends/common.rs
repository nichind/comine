//! Shared helpers for download backends.

use crate::orchestrator::types::*;

impl UrlInfo {
    pub fn simple(url: &str, title: Option<String>, extractor: &str) -> Self {
        Self {
            url: url.to_string(),
            title,
            thumbnail: None,
            duration: None,
            filesize: None,
            extractor: extractor.to_string(),
            is_playlist: false,
            playlist_count: None,
            formats: None,
            mime_type: None,
            entries: None,
            uploader: None,
            channel: None,
            view_count: None,
            like_count: None,
            description: None,
            upload_date: None,
            channel_url: None,
            channel_id: None,
            storyboards: None,
            chapters: None,
        }
    }

    pub fn with_file_info(
        url: &str,
        title: Option<String>,
        extractor: &str,
        filesize: Option<u64>,
        mime_type: Option<String>,
    ) -> Self {
        Self {
            url: url.to_string(),
            title,
            thumbnail: None,
            duration: None,
            filesize,
            extractor: extractor.to_string(),
            is_playlist: false,
            playlist_count: None,
            formats: None,
            mime_type,
            entries: None,
            uploader: None,
            channel: None,
            view_count: None,
            like_count: None,
            description: None,
            upload_date: None,
            channel_url: None,
            channel_id: None,
            storyboards: None,
            chapters: None,
        }
    }
}

pub fn extract_filename_from_url(url: &str) -> String {
    url.split('/')
        .last()
        .and_then(|s| s.split('?').next())
        .filter(|s| !s.is_empty() && s.contains('.'))
        .map(|s| {
            urlencoding::decode(s)
                .map(|d| d.into_owned())
                .unwrap_or_else(|_| s.to_string())
        })
        .unwrap_or_else(|| "download".to_string())
}

pub fn extract_filename_from_response(url: &str, resp: &reqwest::Response) -> String {
    if let Some(cd) = resp.headers().get("content-disposition") {
        if let Ok(cd_str) = cd.to_str() {
            if let Some(filename) = parse_content_disposition(cd_str) {
                return filename;
            }
        }
    }

    extract_filename_from_url(url)
}

fn parse_content_disposition(header: &str) -> Option<String> {
    let idx = header.find("filename=")?;
    let rest = &header[idx + 9..];

    let filename = rest
        .trim_start_matches('"')
        .split('"')
        .next()
        .or_else(|| rest.split(';').next())
        .unwrap_or("download");

    if filename.is_empty() {
        None
    } else {
        Some(filename.to_string())
    }
}

pub fn http_status_to_error(status: u16, url: &str) -> BackendError {
    match status {
        404 => BackendError::NotFound(url.to_string()),
        403 => BackendError::Forbidden(url.to_string()),
        401 => BackendError::Unauthorized(url.to_string()),
        429 => BackendError::RateLimited(url.to_string()),
        s if s >= 500 => BackendError::ServerError(format!("HTTP {}", s)),
        _ => BackendError::NetworkError(format!("HTTP {}", status)),
    }
}

pub fn guess_mime_type(url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(url);
    let ext = path.rsplit('.').next()?.to_lowercase();

    let mime = match ext.as_str() {
        // Images
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "tiff" | "tif" => "image/tiff",
        // Audio
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "opus" => "audio/opus",
        "wma" => "audio/x-ms-wma",
        // Video
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        // Archives
        "zip" => "application/zip",
        "rar" => "application/vnd.rar",
        "7z" => "application/x-7z-compressed",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "bz2" => "application/x-bzip2",
        "xz" => "application/x-xz",
        "zst" => "application/zstd",
        // Documents
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "mobi" => "application/x-mobipocket-ebook",
        "djvu" => "image/vnd.djvu",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        // Executables/Installers
        "exe" => "application/x-msdownload",
        "msi" => "application/x-msi",
        "dmg" => "application/x-apple-diskimage",
        "iso" => "application/x-iso9660-image",
        "img" => "application/x-raw-disk-image",
        "deb" => "application/vnd.debian.binary-package",
        "rpm" => "application/x-rpm",
        "apk" => "application/vnd.android.package-archive",
        "appimage" => "application/x-executable",
        // Fonts
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        // Data
        "json" => "application/json",
        "xml" => "application/xml",
        "csv" => "text/csv",
        "bin" | "dat" => "application/octet-stream",
        _ => return None,
    };

    Some(mime.to_string())
}

pub const DIRECT_FILE_EXTENSIONS: &[&str] = &[
    "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "zst", "iso", "img", "dmg", "exe", "msi", "deb",
    "rpm", "apk", "appimage", "bin", "dat", "jpg", "jpeg", "png", "gif", "webp", "bmp", "tiff",
    "svg", "ico", "avif", "mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma", "mp4", "mkv",
    "avi", "mov", "wmv", "flv", "webm", "pdf", "epub", "mobi", "djvu", "doc", "docx", "xls",
    "xlsx", "ppt", "pptx", "ttf", "otf", "woff", "woff2",
];

pub const VIDEO_HOSTING_DOMAINS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "vimeo.com",
    "dailymotion.com",
    "twitch.tv",
    "twitter.com",
    "x.com",
    "instagram.com",
    "facebook.com",
    "fb.watch",
    "tiktok.com",
    "reddit.com",
    "soundcloud.com",
    "bandcamp.com",
    "bilibili.com",
    "nicovideo.jp",
];

pub fn is_video_hosting_site(url: &str) -> bool {
    VIDEO_HOSTING_DOMAINS.iter().any(|d| url.contains(d))
}

pub fn has_file_extension(url: &str, extensions: &[&str]) -> bool {
    let lower = url.to_lowercase();
    let path = lower.split('?').next().unwrap_or(&lower);
    extensions
        .iter()
        .any(|ext| path.ends_with(&format!(".{}", ext)))
}

pub fn is_torrent_url(url: &str) -> bool {
    url.starts_with("magnet:") || url.ends_with(".torrent") || url.contains(".torrent?")
}

pub fn extract_magnet_name(magnet: &str) -> Option<String> {
    magnet
        .split('&')
        .find(|part| part.starts_with("dn="))
        .map(|dn| {
            let name = &dn[3..]; // Skip "dn="
            urlencoding::decode(name)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| name.replace('+', " "))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_filename_from_url() {
        assert_eq!(
            extract_filename_from_url("https://example.com/file.zip"),
            "file.zip"
        );
        assert_eq!(
            extract_filename_from_url("https://example.com/path/to/file.tar.gz"),
            "file.tar.gz"
        );
        assert_eq!(
            extract_filename_from_url("https://example.com/file.zip?token=abc"),
            "file.zip"
        );
        assert_eq!(
            extract_filename_from_url("https://example.com/My%20File.pdf"),
            "My File.pdf"
        );
        assert_eq!(
            extract_filename_from_url("https://example.com/"),
            "download"
        );
        assert_eq!(
            extract_filename_from_url("https://example.com/noextension"),
            "download"
        );
    }

    #[test]
    fn test_parse_content_disposition() {
        assert_eq!(
            parse_content_disposition(r#"attachment; filename="test.zip""#),
            Some("test.zip".to_string())
        );
        assert_eq!(
            parse_content_disposition("attachment; filename=test.zip"),
            Some("test.zip".to_string())
        );
        assert_eq!(parse_content_disposition("inline"), None);
    }

    #[test]
    fn test_guess_mime_type() {
        assert_eq!(guess_mime_type("file.mp4"), Some("video/mp4".to_string()));
        assert_eq!(guess_mime_type("file.MP4"), Some("video/mp4".to_string()));
        assert_eq!(
            guess_mime_type("https://example.com/file.zip?token=x"),
            Some("application/zip".to_string())
        );
        assert_eq!(guess_mime_type("file.unknown"), None);
    }

    #[test]
    fn test_extract_magnet_name() {
        assert_eq!(
            extract_magnet_name("magnet:?xt=urn:btih:abc&dn=Test+File&tr=udp://tracker"),
            Some("Test File".to_string())
        );
        assert_eq!(
            extract_magnet_name("magnet:?xt=urn:btih:abc&dn=Test%20File"),
            Some("Test File".to_string())
        );
        assert_eq!(extract_magnet_name("magnet:?xt=urn:btih:abc"), None);
    }

    #[test]
    fn test_is_torrent_url() {
        assert!(is_torrent_url("magnet:?xt=urn:btih:abc"));
        assert!(is_torrent_url("https://example.com/file.torrent"));
        assert!(is_torrent_url("https://example.com/file.torrent?token=x"));
        assert!(!is_torrent_url("https://example.com/file.zip"));
    }

    #[test]
    fn test_has_file_extension() {
        assert!(has_file_extension(
            "https://example.com/file.ZIP",
            &["zip", "rar"]
        ));
        assert!(has_file_extension(
            "https://example.com/file.zip?q=1",
            &["zip"]
        ));
        assert!(!has_file_extension(
            "https://example.com/file.pdf",
            &["zip", "rar"]
        ));
    }
}
