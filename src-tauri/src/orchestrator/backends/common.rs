use crate::orchestrator::types::*;

impl UrlInfo {
    pub fn simple(url: &str, title: Option<String>, extractor: &str) -> Self {
        Self {
            url: url.to_string(),
            extractor: extractor.to_string(),
            title,
            ..Default::default()
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
            extractor: extractor.to_string(),
            content_type: ContentType::File,
            title,
            filesize,
            mime_type,
            ..Default::default()
        }
    }
}

pub fn extract_filename_from_url(url: &str) -> String {
    url.split('/')
        .next_back()
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
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "tiff" | "tif" => "image/tiff",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "opus" => "audio/opus",
        "wma" => "audio/x-ms-wma",
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "zip" => "application/zip",
        "rar" => "application/vnd.rar",
        "7z" => "application/x-7z-compressed",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "bz2" => "application/x-bzip2",
        "xz" => "application/x-xz",
        "zst" => "application/zstd",
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
        "exe" => "application/x-msdownload",
        "msi" => "application/x-msi",
        "dmg" => "application/x-apple-diskimage",
        "iso" => "application/x-iso9660-image",
        "img" => "application/x-raw-disk-image",
        "deb" => "application/vnd.debian.binary-package",
        "rpm" => "application/x-rpm",
        "apk" => "application/vnd.android.package-archive",
        "appimage" => "application/x-executable",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
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
            let name = &dn[3..];
            urlencoding::decode(name)
                .map(|s| s.into_owned().replace('+', " "))
                .unwrap_or_else(|_| name.replace('+', " "))
        })
}

pub fn resolve_effective_proxy(proxy: &Option<ProxyConfig>) -> Option<String> {
    if let Some(p) = proxy {
        if p.enabled {
            if let Some(ref url) = p.url {
                if !url.is_empty() {
                    return Some(url.clone());
                }
            }
            #[cfg(desktop)]
            {
                let system_proxy = crate::proxy::detect_system_proxy();
                if !system_proxy.url.is_empty() {
                    tracing::info!("Using detected system proxy: {}", system_proxy.url);
                    return Some(system_proxy.url);
                }
            }
        }
    }
    None
}

pub fn parse_size_str(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let mut num_end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() || c == '.' {
            num_end = i + 1;
        } else {
            break;
        }
    }

    if num_end == 0 {
        return None;
    }

    let num: f64 = s[..num_end].parse().ok()?;
    let unit = s[num_end..].trim().to_lowercase();

    let multiplier: u64 = match unit.as_str() {
        "b" => 1,
        "kb" | "k" => 1_000,
        "kib" => 1_024,
        "mb" | "m" => 1_000_000,
        "mib" => 1_048_576,
        "gb" | "g" => 1_000_000_000,
        "gib" => 1_073_741_824,
        "tb" => 1_000_000_000_000,
        "tib" => 1_099_511_627_776,
        _ => 1,
    };

    Some((num * multiplier as f64) as u64)
}

#[cfg(desktop)]
pub async fn graceful_shutdown(child: &mut tokio::process::Child, label: &str) {
    use std::time::Duration;

    let pid = match child.id() {
        Some(id) => id,
        None => return,
    };

    #[cfg(unix)]
    {
        let sigint_result = unsafe { libc::kill(pid as i32, libc::SIGINT) };

        if sigint_result == 0 {
            tracing::info!(
                "[{}] Sent SIGINT to process {}, waiting for graceful exit...",
                label,
                pid
            );

            for _ in 0..30 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if child.try_wait().ok().flatten().is_some() {
                    tracing::info!("[{}] Process {} exited gracefully", label, pid);
                    return;
                }
            }

            tracing::warn!(
                "[{}] Process {} didn't exit after SIGINT, sending SIGTERM",
                label,
                pid
            );
            unsafe { libc::kill(pid as i32, libc::SIGTERM) };

            for _ in 0..10 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if child.try_wait().ok().flatten().is_some() {
                    tracing::info!("[{}] Process {} exited after SIGTERM", label, pid);
                    return;
                }
            }
        }

        tracing::warn!(
            "[{}] Force killing process {} after graceful shutdown timeout",
            label,
            pid
        );
        let _ = child.kill().await;
    }

    #[cfg(windows)]
    {
        let mut taskkill = crate::utils::new_command("taskkill");
        taskkill.args(["/PID", &pid.to_string(), "/T"]);

        let taskkill = taskkill.output().await;

        if taskkill.is_ok() {
            tracing::info!(
                "[{}] Sent taskkill to process {}, waiting for exit...",
                label,
                pid
            );

            for _ in 0..20 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if child.try_wait().ok().flatten().is_some() {
                    tracing::info!("[{}] Process {} exited gracefully", label, pid);
                    return;
                }
            }
        }

        tracing::warn!(
            "[{}] Force killing process {} after graceful shutdown timeout",
            label,
            pid
        );
        let _ = child.kill().await;
    }
}

#[cfg(desktop)]
pub fn apply_args_to_command(cmd: &mut tokio::process::Command, option_groups: Vec<Vec<String>>) {
    for group in option_groups {
        if group.len() == 1 {
            cmd.arg(&group[0]);
        } else {
            cmd.args(&group);
        }
    }
}

pub async fn resolve_http_file(url: &str, extractor: &str) -> Result<UrlInfo, BackendError> {
    let client = crate::utils::http_client().map_err(BackendError::Other)?;

    let resp = client
        .head(url)
        .header("Accept", "*/*")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| BackendError::NetworkError(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(http_status_to_error(resp.status().as_u16(), url));
    }

    let content_length = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let filename = extract_filename_from_response(url, &resp);

    Ok(UrlInfo::with_file_info(
        url,
        Some(filename),
        extractor,
        content_length,
        content_type,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

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

    #[test]
    fn test_parse_size_str_bytes() {
        assert_eq!(parse_size_str("100B"), Some(100));
        assert_eq!(parse_size_str("0B"), Some(0));
        assert_eq!(parse_size_str("1B"), Some(1));
    }

    #[test]
    fn test_parse_size_str_kilobytes() {
        assert_eq!(parse_size_str("1KB"), Some(1_000));
        assert_eq!(parse_size_str("1K"), Some(1_000));
        assert_eq!(parse_size_str("1KiB"), Some(1_024));
        assert_eq!(parse_size_str("1.5KiB"), Some(1_536));
    }

    #[test]
    fn test_parse_size_str_megabytes() {
        assert_eq!(parse_size_str("1MB"), Some(1_000_000));
        assert_eq!(parse_size_str("1M"), Some(1_000_000));
        assert_eq!(parse_size_str("1MiB"), Some(1_048_576));
        assert_eq!(parse_size_str("1.5MiB"), Some(1_572_864));
        assert_eq!(parse_size_str("10.0MiB"), Some(10_485_760));
    }

    #[test]
    fn test_parse_size_str_gigabytes() {
        assert_eq!(parse_size_str("1GB"), Some(1_000_000_000));
        assert_eq!(parse_size_str("1G"), Some(1_000_000_000));
        assert_eq!(parse_size_str("1GiB"), Some(1_073_741_824));
        assert_eq!(parse_size_str("2.5GiB"), Some(2_684_354_560));
    }

    #[test]
    fn test_parse_size_str_terabytes() {
        assert_eq!(parse_size_str("1TB"), Some(1_000_000_000_000));
        assert_eq!(parse_size_str("1TiB"), Some(1_099_511_627_776));
    }

    #[test]
    fn test_parse_size_str_bare_number() {
        assert_eq!(parse_size_str("1024"), Some(1024));
        assert_eq!(parse_size_str("0"), Some(0));
    }

    #[test]
    fn test_parse_size_str_whitespace() {
        assert_eq!(parse_size_str("  1MiB  "), Some(1_048_576));
        assert_eq!(parse_size_str(" 100 KB "), Some(100_000));
    }

    #[test]
    fn test_parse_size_str_case_insensitive() {
        assert_eq!(parse_size_str("1mib"), Some(1_048_576));
        assert_eq!(parse_size_str("1MIB"), Some(1_048_576));
        assert_eq!(parse_size_str("1gb"), Some(1_000_000_000));
        assert_eq!(parse_size_str("1Gb"), Some(1_000_000_000));
    }

    #[test]
    fn test_parse_size_str_fractional() {
        assert_eq!(parse_size_str("0.5MiB"), Some(524_288));
        assert_eq!(parse_size_str("1.25GB"), Some(1_250_000_000));
        assert_eq!(parse_size_str("3.14MB"), Some(3_140_000));
    }

    #[test]
    fn test_parse_size_str_invalid() {
        assert_eq!(parse_size_str(""), None);
        assert_eq!(parse_size_str("   "), None);
        assert_eq!(parse_size_str("NA"), None);
        assert_eq!(parse_size_str("abc"), None);
        assert_eq!(parse_size_str("MiB"), None); // no number
    }

    #[test]
    fn test_parse_size_str_real_aria2_output() {
        assert_eq!(parse_size_str("5.0MiB"), Some(5_242_880));
        assert_eq!(parse_size_str("10.0MiB"), Some(10_485_760));
        assert_eq!(parse_size_str("1.2MiB"), Some(1_258_291));
        assert_eq!(parse_size_str("512KiB"), Some(524_288));
        assert_eq!(parse_size_str("256KiB"), Some(262_144));
    }

    #[test]
    fn test_parse_size_str_real_ytdlp_output() {
        assert_eq!(parse_size_str("1024"), Some(1024));
        assert_eq!(parse_size_str("1048576"), Some(1_048_576));
    }

    #[test]
    fn test_http_status_to_error_404() {
        let err = http_status_to_error(404, "https://example.com");
        assert!(matches!(err, BackendError::NotFound(_)));
    }

    #[test]
    fn test_http_status_to_error_403() {
        let err = http_status_to_error(403, "https://example.com");
        assert!(matches!(err, BackendError::Forbidden(_)));
    }

    #[test]
    fn test_http_status_to_error_401() {
        let err = http_status_to_error(401, "https://example.com");
        assert!(matches!(err, BackendError::Unauthorized(_)));
    }

    #[test]
    fn test_http_status_to_error_429() {
        let err = http_status_to_error(429, "https://example.com");
        assert!(matches!(err, BackendError::RateLimited(_)));
    }

    #[test]
    fn test_http_status_to_error_5xx() {
        let err = http_status_to_error(500, "https://example.com");
        assert!(matches!(err, BackendError::ServerError(_)));
        let err = http_status_to_error(503, "https://example.com");
        assert!(matches!(err, BackendError::ServerError(_)));
        let err = http_status_to_error(502, "https://example.com");
        assert!(matches!(err, BackendError::ServerError(_)));
    }

    #[test]
    fn test_http_status_to_error_other() {
        let err = http_status_to_error(418, "https://example.com");
        assert!(matches!(err, BackendError::NetworkError(_)));
        let err = http_status_to_error(301, "https://example.com");
        assert!(matches!(err, BackendError::NetworkError(_)));
    }

    #[test]
    fn test_is_video_hosting_site() {
        assert!(is_video_hosting_site(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(is_video_hosting_site("https://youtu.be/dQw4w9WgXcQ"));
        assert!(is_video_hosting_site("https://vimeo.com/123456"));
        assert!(is_video_hosting_site("https://www.twitch.tv/channel"));
        assert!(is_video_hosting_site(
            "https://www.tiktok.com/@user/video/123"
        ));
        assert!(!is_video_hosting_site("https://example.com/file.mp4"));
        assert!(!is_video_hosting_site("https://pixiv.net/artworks/123"));
    }
}
