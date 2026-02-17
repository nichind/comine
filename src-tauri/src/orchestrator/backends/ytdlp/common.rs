use crate::orchestrator::types::*;

pub use super::args::{normalize_url_for_ytdlp, PreparedCookieProxy, YtDlpArgsBuilder};
#[cfg(target_os = "android")]
pub use super::progress::parse_raw_progress;
pub use super::progress::ProgressTracker;

#[derive(Debug, Clone)]
pub enum YtdlpEvent {
    Filepath(String),
    Title(String),
    Thumbnail(String),
    Uploader(String),
    Channel(String),
    ChannelUrl(String),
    Duration(u64),
    Extractor(String),
    WebpageUrl(String),
    IsPlaylist(bool),
    PostProcessing,
}

pub fn parse_ytdlp_line_event(line: &str) -> Option<YtdlpEvent> {
    if line.starts_with("[Merger]")
        || line.starts_with("[ffmpeg]")
        || line.starts_with("[ExtractAudio]")
        || line.starts_with("[EmbedSubtitle]")
        || line.starts_with("[Metadata]")
        || line.starts_with("[FixupM3u8]")
        || line.starts_with("[FixupM4a]")
        || line.starts_with("[MoveFiles]")
        || line.contains("Merging formats")
        || line.contains("Post-process")
    {
        return Some(YtdlpEvent::PostProcessing);
    }

    if let Some(v) = parse_marker_value(line, ">>>FILEPATH:") {
        return Some(YtdlpEvent::Filepath(v));
    }
    if let Some(v) = parse_marker_value(line, ">>>TITLE:") {
        return Some(YtdlpEvent::Title(v));
    }
    if let Some(v) = parse_marker_value(line, ">>>THUMBNAIL:") {
        return Some(YtdlpEvent::Thumbnail(v));
    }
    if let Some(v) = parse_marker_value(line, ">>>UPLOADER:") {
        return Some(YtdlpEvent::Uploader(v));
    }
    if let Some(v) = parse_marker_value(line, ">>>CHANNEL:") {
        return Some(YtdlpEvent::Channel(v));
    }
    if let Some(v) = parse_marker_value(line, ">>>CHANNEL_URL:") {
        return Some(YtdlpEvent::ChannelUrl(v));
    }
    if let Some(v) = parse_duration_line(line) {
        return Some(YtdlpEvent::Duration(v));
    }
    if let Some(v) = parse_marker_value(line, ">>>EXTRACTOR:") {
        return Some(YtdlpEvent::Extractor(v));
    }
    if let Some(v) = parse_marker_value(line, ">>>WEBPAGE_URL:") {
        return Some(YtdlpEvent::WebpageUrl(v));
    }
    if let Some(v) = parse_is_playlist_line(line) {
        return Some(YtdlpEvent::IsPlaylist(v));
    }
    None
}

pub fn apply_metadata_event(
    event: &YtdlpEvent,
    metadata_tx: &tokio::sync::mpsc::UnboundedSender<crate::orchestrator::backends::MetadataEvent>,
) {
    use crate::orchestrator::backends::MetadataEvent;

    let patch = match event {
        YtdlpEvent::Title(v) => UrlInfoPatch {
            title: Some(v.clone()),
            ..Default::default()
        },
        YtdlpEvent::Thumbnail(v) => UrlInfoPatch {
            thumbnail: Some(v.clone()),
            ..Default::default()
        },
        YtdlpEvent::Uploader(v) => UrlInfoPatch {
            uploader: Some(v.clone()),
            ..Default::default()
        },
        YtdlpEvent::Channel(v) => UrlInfoPatch {
            channel: Some(v.clone()),
            ..Default::default()
        },
        YtdlpEvent::ChannelUrl(v) => UrlInfoPatch {
            channel_url: Some(v.clone()),
            ..Default::default()
        },
        YtdlpEvent::Duration(v) => UrlInfoPatch {
            duration: Some(*v),
            ..Default::default()
        },
        YtdlpEvent::Extractor(v) => UrlInfoPatch {
            extractor: Some(v.clone()),
            ..Default::default()
        },
        YtdlpEvent::WebpageUrl(v) => UrlInfoPatch {
            webpage_url: Some(v.clone()),
            ..Default::default()
        },
        YtdlpEvent::IsPlaylist(v) => UrlInfoPatch {
            is_playlist: Some(*v),
            ..Default::default()
        },
        YtdlpEvent::PostProcessing => {
            let _ = metadata_tx.send(MetadataEvent::PostProcessing);
            return;
        }
        _ => return,
    };
    let _ = metadata_tx.send(MetadataEvent::Patch(patch));
}

pub fn parse_marker_value(line: &str, marker: &str) -> Option<String> {
    let idx = line.find(marker)?;
    let value = line[idx + marker.len()..].trim();
    if value.is_empty() || value == "NA" {
        None
    } else {
        Some(value.to_string())
    }
}

pub fn parse_duration_line(line: &str) -> Option<u64> {
    let val = parse_marker_value(line, ">>>DURATION:")?;
    val.parse::<f64>().ok().map(|n| n as u64)
}

pub fn parse_is_playlist_line(line: &str) -> Option<bool> {
    let val = parse_marker_value(line, ">>>PLAYLIST_COUNT:")?;
    let n = val.parse::<u32>().ok()?;
    Some(n > 0)
}

pub fn parse_ytdlp_error(stderr: &str) -> BackendError {
    let lower = stderr.to_lowercase();

    fn truncate(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len])
        }
    }

    if lower.contains("video unavailable") || lower.contains("not available") {
        BackendError::NotFound(truncate(stderr, 200))
    } else if lower.contains("private video") || lower.contains("sign in") {
        BackendError::Unauthorized(truncate(stderr, 200))
    } else if lower.contains("429") || lower.contains("rate limit") {
        BackendError::RateLimited(truncate(stderr, 200))
    } else if lower.contains("403") {
        BackendError::Forbidden(truncate(stderr, 200))
    } else if lower.contains("unsupported url") || lower.contains("no video formats") {
        BackendError::UnsupportedUrl(truncate(stderr, 200))
    } else {
        BackendError::Other(truncate(stderr, 200))
    }
}

pub struct TempCookieFile {
    pub path: std::path::PathBuf,
}

impl TempCookieFile {
    pub fn new(contents: &str) -> Option<Self> {
        let filename = format!("comine_cookies_{}.txt", uuid::Uuid::new_v4());
        let path = std::env::temp_dir().join(filename);
        if std::fs::write(&path, contents).is_ok() {
            Some(Self { path })
        } else {
            None
        }
    }
}

impl Drop for TempCookieFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn looks_like_netscape_cookie_text(s: &str) -> bool {
    s.contains('\n') || s.trim_start().starts_with("# Netscape")
}

pub fn maybe_write_cookie_tempfile(cookies: &str) -> (String, Option<TempCookieFile>) {
    if looks_like_netscape_cookie_text(cookies) {
        if let Some(tmp) = TempCookieFile::new(cookies) {
            return (tmp.path.to_string_lossy().to_string(), Some(tmp));
        }
    }
    (cookies.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_marker_value_basic() {
        assert_eq!(
            parse_marker_value(">>>TITLE:My Video Title", ">>>TITLE:"),
            Some("My Video Title".to_string())
        );
    }

    #[test]
    fn test_parse_marker_value_na() {
        assert_eq!(parse_marker_value(">>>TITLE:NA", ">>>TITLE:"), None);
    }

    #[test]
    fn test_parse_marker_value_empty() {
        assert_eq!(parse_marker_value(">>>TITLE:", ">>>TITLE:"), None);
    }

    #[test]
    fn test_parse_marker_value_missing() {
        assert_eq!(parse_marker_value("no marker here", ">>>TITLE:"), None);
    }

    #[test]
    fn test_parse_marker_value_embedded() {
        assert_eq!(
            parse_marker_value("[info] >>>FILEPATH:/tmp/video.mp4", ">>>FILEPATH:"),
            Some("/tmp/video.mp4".to_string())
        );
    }

    #[test]
    fn test_parse_duration_integer() {
        assert_eq!(parse_duration_line(">>>DURATION:120"), Some(120));
    }

    #[test]
    fn test_parse_duration_float() {
        assert_eq!(parse_duration_line(">>>DURATION:120.5"), Some(120));
    }

    #[test]
    fn test_parse_duration_na() {
        assert_eq!(parse_duration_line(">>>DURATION:NA"), None);
    }

    #[test]
    fn test_event_filepath() {
        let event = parse_ytdlp_line_event(">>>FILEPATH:/downloads/video.mp4");
        assert!(matches!(event, Some(YtdlpEvent::Filepath(p)) if p == "/downloads/video.mp4"));
    }

    #[test]
    fn test_event_title() {
        let event = parse_ytdlp_line_event(">>>TITLE:Cool Video");
        assert!(matches!(event, Some(YtdlpEvent::Title(t)) if t == "Cool Video"));
    }

    #[test]
    fn test_event_thumbnail() {
        let event = parse_ytdlp_line_event(">>>THUMBNAIL:https://i.ytimg.com/vi/abc/hq.jpg");
        assert!(
            matches!(event, Some(YtdlpEvent::Thumbnail(t)) if t == "https://i.ytimg.com/vi/abc/hq.jpg")
        );
    }

    #[test]
    fn test_event_post_processing_merger() {
        let event = parse_ytdlp_line_event("[Merger] Merging formats");
        assert!(matches!(event, Some(YtdlpEvent::PostProcessing)));
    }

    #[test]
    fn test_event_post_processing_ffmpeg() {
        let event = parse_ytdlp_line_event("[ffmpeg] Converting");
        assert!(matches!(event, Some(YtdlpEvent::PostProcessing)));
    }

    #[test]
    fn test_event_post_processing_extract_audio() {
        let event = parse_ytdlp_line_event("[ExtractAudio] Extracting audio");
        assert!(matches!(event, Some(YtdlpEvent::PostProcessing)));
    }

    #[test]
    fn test_event_merging_formats() {
        let event = parse_ytdlp_line_event("Merging formats into...");
        assert!(matches!(event, Some(YtdlpEvent::PostProcessing)));
    }

    #[test]
    fn test_event_unrecognized() {
        let event = parse_ytdlp_line_event("some random output");
        assert!(event.is_none());
    }

    #[test]
    fn test_event_playlist_count() {
        let event = parse_ytdlp_line_event(">>>PLAYLIST_COUNT:5");
        assert!(matches!(event, Some(YtdlpEvent::IsPlaylist(true))));
    }

    #[test]
    fn test_event_playlist_count_zero() {
        let event = parse_ytdlp_line_event(">>>PLAYLIST_COUNT:0");
        assert!(matches!(event, Some(YtdlpEvent::IsPlaylist(false))));
    }

    #[test]
    fn test_error_video_unavailable() {
        let err = parse_ytdlp_error("ERROR: Video unavailable");
        assert!(matches!(err, BackendError::NotFound(_)));
    }

    #[test]
    fn test_error_private_video() {
        let err = parse_ytdlp_error("ERROR: Private video. Sign in if you've been granted access");
        assert!(matches!(err, BackendError::Unauthorized(_)));
    }

    #[test]
    fn test_error_rate_limited() {
        let err = parse_ytdlp_error("HTTP Error 429: Too Many Requests");
        assert!(matches!(err, BackendError::RateLimited(_)));
    }

    #[test]
    fn test_error_forbidden() {
        let err = parse_ytdlp_error("HTTP Error 403: Forbidden");
        assert!(matches!(err, BackendError::Forbidden(_)));
    }

    #[test]
    fn test_error_unsupported_url() {
        let err = parse_ytdlp_error("ERROR: Unsupported URL: https://example.com");
        assert!(matches!(err, BackendError::UnsupportedUrl(_)));
    }

    #[test]
    fn test_error_generic() {
        let err = parse_ytdlp_error("ERROR: Something went wrong");
        assert!(matches!(err, BackendError::Other(_)));
    }

    #[test]
    fn test_error_truncation() {
        let long_error = "a".repeat(500);
        let err = parse_ytdlp_error(&long_error);
        match err {
            BackendError::Other(s) => assert!(s.len() < 500),
            _ => panic!("Expected Other variant"),
        }
    }

    #[test]
    fn test_netscape_cookie_header() {
        assert!(looks_like_netscape_cookie_text(
            "# Netscape HTTP Cookie File\n.example.com\tTRUE\t/\tTRUE\t0\tname\tvalue"
        ));
    }

    #[test]
    fn test_multiline_cookie() {
        assert!(looks_like_netscape_cookie_text(".example.com\tTRUE\t/\tTRUE\t0\tname\tvalue\n.other.com\tTRUE\t/\tTRUE\t0\tname2\tvalue2"));
    }

    #[test]
    fn test_cookie_file_path() {
        assert!(!looks_like_netscape_cookie_text("cookies.txt"));
    }
}
