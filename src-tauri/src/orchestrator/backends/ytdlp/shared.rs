use tokio::sync::mpsc;
use tracing::{info, warn};

use super::json::{
    compute_pagination, parse_playlist_entry, parse_ytdlp_output, playlist_url_info_from_json,
    PaginationContext,
};
use crate::orchestrator::backends::{
    is_known_video_site, BackendCapabilities, StreamingResolveHandle,
};
use crate::orchestrator::types::*;

pub fn ytdlp_capabilities() -> BackendCapabilities {
    BackendCapabilities {
        name: "ytdlp".into(),
        streaming_resolve: true,
        playlists: true,
        pause_resume: true,
        multi_connection: false,
        format_selection: true,
        subtitles: true,
        speed_limit: true,
        proxy: true,
        cookies: true,
        torrent_magnet: false,
        post_processing: true,
    }
}

pub fn ytdlp_priority(url: &str) -> Priority {
    if is_known_video_site(url) {
        Priority::High
    } else if url.starts_with("http://") || url.starts_with("https://") {
        Priority::Medium
    } else {
        Priority::None
    }
}

pub fn emit_playlist_metadata(v: &serde_json::Value, tx: &mpsc::UnboundedSender<ResolveEvent>) {
    let _ = tx.send(ResolveEvent::MetadataUpdate {
        title: v
            .get("playlist_title")
            .or_else(|| v.get("channel"))
            .and_then(|s| s.as_str())
            .map(String::from),
        description: v
            .get("description")
            .and_then(|s| s.as_str())
            .map(String::from),
        thumbnail: v
            .get("thumbnail")
            .and_then(|s| s.as_str())
            .map(String::from),
        uploader: v
            .get("playlist_uploader")
            .or_else(|| v.get("uploader"))
            .or_else(|| v.get("channel"))
            .and_then(|s| s.as_str())
            .map(String::from),
        channel: v
            .get("playlist_channel")
            .or_else(|| v.get("channel"))
            .and_then(|s| s.as_str())
            .map(String::from),
        channel_url: v
            .get("channel_url")
            .and_then(|s| s.as_str())
            .map(String::from),
        playlist_id: v
            .get("playlist_id")
            .and_then(|s| s.as_str())
            .map(String::from),
        playlist_title: v
            .get("playlist_title")
            .and_then(|s| s.as_str())
            .map(String::from),
        playlist_count: v
            .get("playlist_count")
            .and_then(|c| c.as_u64())
            .map(|c| c as u32),
        content_type: Some(ContentType::Playlist),
    });
}

/// Process a single NDJSON line during streaming resolve.
/// Returns `Some(true)` if we should stop (single video result was emitted),
/// `Some(false)` to continue, `None` if the line was unparseable.
pub fn process_ndjson_line(
    line: &str,
    url: &str,
    entry_index: &mut u32,
    is_multi_line: &mut bool,
    first_entry_json: &mut Option<serde_json::Value>,
    entries: &mut Vec<PlaylistEntry>,
    tx: &mpsc::UnboundedSender<ResolveEvent>,
) -> Option<bool> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;

    if first_entry_json.is_none() {
        *first_entry_json = Some(v.clone());
    }

    if !*is_multi_line && *entry_index == 0 && v.get("formats").is_some() {
        match parse_ytdlp_output(line, url, None) {
            Ok(info) => {
                let _ = tx.send(ResolveEvent::Complete {
                    info: Box::new(info),
                });
            }
            Err(e) => {
                let _ = tx.send(ResolveEvent::Error {
                    message: e.to_string(),
                });
            }
        }
        return Some(true);
    }

    *is_multi_line = true;

    if *entry_index == 0 {
        emit_playlist_metadata(&v, tx);
    }

    if let Some(entry) = parse_playlist_entry(&v, *entry_index) {
        let _ = tx.send(ResolveEvent::EntryDiscovered {
            index: *entry_index,
            entry: entry.clone(),
        });
        entries.push(entry);
        *entry_index += 1;
    }

    Some(false)
}

pub fn finalize_streaming_resolve(
    url: &str,
    settings: &ResolveSettings,
    first_entry_json: Option<&serde_json::Value>,
    entries: Vec<PlaylistEntry>,
    tx: &mpsc::UnboundedSender<ResolveEvent>,
) {
    let playlist_count = first_entry_json
        .and_then(|v| v.get("playlist_count").and_then(|c| c.as_u64()))
        .map(|c| c as u32)
        .or(Some(entries.len() as u32));

    let pagination_ctx = PaginationContext::from_settings(settings);

    let pagination = pagination_ctx
        .as_ref()
        .and_then(|ctx| compute_pagination(Some(ctx), entries.len() as u32, playlist_count));

    let info =
        playlist_url_info_from_json(url, first_entry_json, entries, playlist_count, pagination);
    let _ = tx.send(ResolveEvent::Complete {
        info: Box::new(info),
    });
}

pub async fn concat_section_files(
    app: &tauri::AppHandle,
    job_id: &str,
    captured_output_paths: Vec<String>,
    progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
) -> Option<String> {
    if captured_output_paths.len() <= 1 {
        return None;
    }

    info!(target: "ytdlp", "Multiple section files detected: {:?}", captured_output_paths);

    let ordered_paths = captured_output_paths;

    let first_path = std::path::Path::new(&ordered_paths[0]);
    let stem = first_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let ext = first_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("mkv");

    let clean_stem = strip_section_marker(stem);

    let merged_path = first_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(format!("{}.{}", clean_stem, ext));
    let merged_path_str = merged_path.to_string_lossy().to_string();

    match crate::orchestrator::convert::concat_files(
        app,
        job_id,
        ordered_paths,
        &merged_path_str,
        true,
        progress_tx,
    )
    .await
    {
        Ok(path) => {
            info!(target: "ytdlp", "Concatenation successful: {}", path);
            Some(path)
        }
        Err(e) => {
            warn!("Section concatenation failed: {}", e);
            None
        }
    }
}

/// E.g. "video_0.00-55.00" → "video", "video_123.45-678.90" → "video"
pub fn strip_section_marker(stem: &str) -> &str {
    if let Some(idx) = stem.rfind('_') {
        let suffix = &stem[idx + 1..];
        if suffix.contains('-')
            && suffix
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
        {
            return &stem[..idx];
        }
    }
    stem
}

pub fn make_streaming_handle<F, Fut>(resolve_fn: F) -> StreamingResolveHandle
where
    F: FnOnce(mpsc::UnboundedSender<ResolveEvent>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let _ = tx.send(ResolveEvent::Started {
        backend: "ytdlp".to_string(),
    });

    tokio::spawn(async move {
        resolve_fn(tx).await;
    });

    StreamingResolveHandle {
        events: Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_strip_section_marker_basic() {
        assert_eq!(strip_section_marker("video_0.00-55.00"), "video");
    }

    #[test]
    fn test_strip_section_marker_longer_timestamps() {
        assert_eq!(strip_section_marker("My Video_123.45-678.90"), "My Video");
    }

    #[test]
    fn test_strip_section_marker_no_marker() {
        assert_eq!(strip_section_marker("regular_filename"), "regular_filename");
    }

    #[test]
    fn test_strip_section_marker_no_underscore() {
        assert_eq!(strip_section_marker("nounderscores"), "nounderscores");
    }

    #[test]
    fn test_strip_section_marker_not_timestamp() {
        // Has underscore but the suffix isn't a timestamp range
        assert_eq!(strip_section_marker("video_part2"), "video_part2");
    }

    #[test]
    fn test_strip_section_marker_multiple_underscores() {
        assert_eq!(
            strip_section_marker("my_cool_video_10.00-20.00"),
            "my_cool_video"
        );
    }

    #[test]
    fn test_strip_section_marker_integer_timestamps() {
        assert_eq!(strip_section_marker("clip_0-30"), "clip");
    }

    #[test]
    fn test_ytdlp_priority_video_sites() {
        assert_eq!(
            ytdlp_priority("https://www.youtube.com/watch?v=abc"),
            Priority::High
        );
        assert_eq!(ytdlp_priority("https://vimeo.com/12345"), Priority::High);
        assert_eq!(
            ytdlp_priority("https://www.twitch.tv/videos/12345"),
            Priority::High
        );
    }

    #[test]
    fn test_ytdlp_priority_generic_http() {
        assert_eq!(ytdlp_priority("https://example.com/page"), Priority::Medium);
        assert_eq!(ytdlp_priority("http://random.site/video"), Priority::Medium);
    }

    #[test]
    fn test_ytdlp_priority_non_http() {
        assert_eq!(ytdlp_priority("magnet:?xt=urn:btih:abc"), Priority::None);
        assert_eq!(
            ytdlp_priority("ftp://files.example.com/file.zip"),
            Priority::None
        );
    }

    #[test]
    fn test_ytdlp_capabilities() {
        let caps = ytdlp_capabilities();
        assert_eq!(caps.name, "ytdlp");
        assert!(caps.streaming_resolve);
        assert!(caps.playlists);
        assert!(caps.format_selection);
        assert!(caps.subtitles);
        assert!(caps.post_processing);
        assert!(caps.cookies);
        assert!(caps.proxy);
        assert!(!caps.torrent_magnet);
        assert!(!caps.multi_connection);
    }
}
