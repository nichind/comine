use crate::orchestrator::backends::BackendError;
use crate::orchestrator::types::*;
use tracing::debug;

pub(crate) fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|v| v.as_str()).map(String::from)
}

pub(crate) fn str_field_or(v: &serde_json::Value, key: &str, fallback: &str) -> Option<String> {
    str_field(v, key).or_else(|| str_field(v, fallback))
}

/// Also parses string representations (e.g. ffprobe) and truncates floats to u64
/// (yt-dlp sometimes sends `120.0` for duration).
pub(crate) fn u64_field(v: &serde_json::Value, key: &str) -> Option<u64> {
    v.get(key).and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_f64().map(|f| f as u64))
            .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
    })
}

/// Also parses string representations.
pub(crate) fn u32_field(v: &serde_json::Value, key: &str) -> Option<u32> {
    v.get(key).and_then(|v| {
        v.as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .or_else(|| v.as_str().and_then(|s| s.parse::<u32>().ok()))
    })
}

/// Also parses string representations.
pub(crate) fn f64_field(v: &serde_json::Value, key: &str) -> Option<f64> {
    v.get(key).and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
    })
}

pub(crate) fn i32_field(v: &serde_json::Value, key: &str) -> Option<i32> {
    v.get(key).and_then(|v| v.as_i64()).map(|n| n as i32)
}

pub(crate) fn bool_field(v: &serde_json::Value, key: &str) -> Option<bool> {
    v.get(key).and_then(|v| v.as_bool())
}

pub fn parse_playlist_entry(v: &serde_json::Value, index: u32) -> Option<PlaylistEntry> {
    let id = str_field(v, "id").filter(|s| !s.is_empty())?;

    let url = str_field(v, "url")
        .or_else(|| str_field(v, "webpage_url"))
        .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={}", id));

    Some(PlaylistEntry {
        id,
        url,
        title: str_field(v, "title"),
        thumbnail: v
            .get("thumbnails")
            .and_then(|t| t.as_array())
            .and_then(|a| a.last())
            .and_then(|i| str_field(i, "url"))
            .or_else(|| str_field(v, "thumbnail")),
        duration: u64_field(v, "duration"),
        index,
        uploader: str_field_or(v, "uploader", "channel"),
        is_music: false,
    })
}

pub fn playlist_url_info_from_json(
    url: &str,
    first: Option<&serde_json::Value>,
    entries: Vec<PlaylistEntry>,
    playlist_count: Option<u32>,
    pagination: Option<Pagination>,
) -> UrlInfo {
    UrlInfo {
        url: url.to_string(),
        id: first.and_then(|v| str_field(v, "playlist_id")),
        extractor: first
            .and_then(|v| str_field(v, "extractor"))
            .unwrap_or_else(|| "youtube".to_string()),
        content_type: ContentType::Playlist,
        title: first
            .and_then(|v| {
                v.get("playlist_title")
                    .or_else(|| v.get("channel"))
                    .and_then(|s| s.as_str())
            })
            .map(String::from),
        description: first.and_then(|v| str_field(v, "description")),
        thumbnail: first.and_then(|v| str_field(v, "thumbnail")),
        uploader: first
            .and_then(|v| {
                v.get("playlist_uploader")
                    .or_else(|| v.get("playlist_channel"))
                    .or_else(|| v.get("uploader"))
                    .or_else(|| v.get("channel"))
                    .and_then(|s| s.as_str())
            })
            .map(String::from),
        channel: first
            .and_then(|v| {
                v.get("playlist_channel")
                    .or_else(|| v.get("playlist_uploader"))
                    .or_else(|| v.get("channel"))
                    .and_then(|s| s.as_str())
            })
            .map(String::from),
        channel_url: first.and_then(|v| str_field_or(v, "channel_url", "uploader_url")),
        channel_id: first.and_then(|v| str_field_or(v, "channel_id", "uploader_id")),
        is_playlist: true,
        playlist_count,
        entries: Some(entries),
        pagination,
        playlist_id: first.and_then(|v| str_field(v, "playlist_id")),
        playlist_title: first.and_then(|v| str_field(v, "playlist_title")),
        ..Default::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PaginationContext {
    pub page_size: Option<u32>,
    pub cursor: Option<String>,
}

impl PaginationContext {
    /// Build from `ResolveSettings` if pagination was requested.
    pub fn from_settings(settings: &crate::orchestrator::types::ResolveSettings) -> Option<Self> {
        settings.page_size.map(|page_size| Self {
            page_size: Some(page_size),
            cursor: settings.cursor.clone(),
        })
    }
}

pub fn parse_ytdlp_output(
    output: &str,
    url: &str,
    pagination_ctx: Option<&PaginationContext>,
) -> Result<UrlInfo, BackendError> {
    let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();

    if lines.is_empty() {
        return Err(BackendError::ParseError(
            "No JSON output from yt-dlp".to_string(),
        ));
    }

    if lines.len() == 1 {
        return parse_ytdlp_single_json(lines[0], url);
    }

    let mut entries: Vec<PlaylistEntry> = Vec::new();
    let mut first_entry_info: Option<serde_json::Value> = None;

    for (idx, line) in lines.iter().enumerate() {
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => {
                if first_entry_info.is_none() {
                    first_entry_info = Some(v.clone());
                }

                if let Some(entry) = parse_playlist_entry(&v, idx as u32) {
                    entries.push(entry);
                }
            }
            Err(e) => {
                debug!("Failed to parse line {}: {}", idx, e);
                continue;
            }
        }
    }

    let first = first_entry_info.as_ref();

    let total_count_from_json = first
        .and_then(|v| v.get("playlist_count").and_then(|c| c.as_u64()))
        .map(|c| c as u32);

    let pagination =
        compute_pagination(pagination_ctx, entries.len() as u32, total_count_from_json);

    let playlist_count = total_count_from_json.or(Some(entries.len() as u32));

    Ok(playlist_url_info_from_json(
        url,
        first,
        entries,
        playlist_count,
        pagination,
    ))
}

/// Compute pagination info based on request context and returned entries
pub fn compute_pagination(
    ctx: Option<&PaginationContext>,
    returned_count: u32,
    total_count: Option<u32>,
) -> Option<Pagination> {
    let ctx = ctx?;
    let page_size = ctx.page_size?;

    let start: u32 = ctx
        .cursor
        .as_ref()
        .and_then(|c| c.parse().ok())
        .unwrap_or(1);

    let next_start = start.saturating_add(returned_count);

    let has_more = if let Some(total) = total_count {
        next_start <= total
    } else {
        returned_count >= page_size
    };

    Some(Pagination {
        has_more,
        next_cursor: if has_more {
            Some(next_start.to_string())
        } else {
            None
        },
        total_count,
    })
}

fn parse_ytdlp_single_json(json_str: &str, url: &str) -> Result<UrlInfo, BackendError> {
    let v: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| BackendError::ParseError(e.to_string()))?;

    let formats = v.get("formats").and_then(|f| f.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|f| {
                Some(VideoFormat {
                    format_id: f.get("format_id")?.as_str()?.to_string(),
                    ext: f.get("ext")?.as_str()?.to_string(),
                    resolution: str_field(f, "resolution"),
                    fps: u32_field(f, "fps"),
                    vcodec: str_field(f, "vcodec"),
                    acodec: str_field(f, "acodec"),
                    filesize: u64_field(f, "filesize"),
                    filesize_approx: u64_field(f, "filesize_approx"),
                    tbr: f64_field(f, "tbr"),
                    vbr: f64_field(f, "vbr"),
                    abr: f64_field(f, "abr"),
                    asr: u32_field(f, "asr"),
                    has_video: str_field(f, "vcodec").map(|s| s != "none").unwrap_or(false),
                    has_audio: str_field(f, "acodec").map(|s| s != "none").unwrap_or(false),
                    quality: i32_field(f, "quality"),
                    format_note: str_field(f, "format_note"),
                    rows: u32_field(f, "rows"),
                    columns: u32_field(f, "columns").or_else(|| u32_field(f, "cols")),
                    fragments: f
                        .get("fragments")
                        .and_then(|frag| frag.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|fr| {
                                    Some(Fragment {
                                        url: str_field(fr, "url").unwrap_or_default(),
                                        duration: f64_field(fr, "duration").unwrap_or(0.0),
                                    })
                                })
                                .collect()
                        }),
                    source_preference: i32_field(f, "source_preference"),
                    language: str_field(f, "language"),
                    dynamic_range: str_field(f, "dynamic_range"),
                    width: u32_field(f, "width"),
                    height: u32_field(f, "height"),
                    protocol: str_field(f, "protocol"),
                })
            })
            .collect()
    });

    let entries = v.get("entries").and_then(|e| e.as_array()).map(|arr| {
        arr.iter()
            .enumerate()
            .filter_map(|(idx, e)| parse_playlist_entry(e, idx as u32))
            .collect()
    });

    let chapters = v.get("chapters").and_then(|c| c.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|c| {
                Some(Chapter {
                    title: str_field(c, "title").unwrap_or_default(),
                    start_time: f64_field(c, "start_time").unwrap_or(0.0),
                    end_time: f64_field(c, "end_time").unwrap_or(0.0),
                })
            })
            .collect()
    });

    let content_type = if str_field(&v, "_type").as_deref() == Some("playlist") {
        ContentType::Playlist
    } else if bool_field(&v, "is_live").unwrap_or(false) {
        ContentType::Video
    } else if formats
        .as_ref()
        .map(|f: &Vec<VideoFormat>| f.iter().any(|fmt| fmt.has_video))
        .unwrap_or(false)
    {
        ContentType::Video
    } else if formats
        .as_ref()
        .map(|f: &Vec<VideoFormat>| f.iter().any(|fmt| fmt.has_audio))
        .unwrap_or(false)
    {
        ContentType::Audio
    } else {
        ContentType::Video
    };

    let related = parse_related(&v);

    Ok(UrlInfo {
        url: url.to_string(),
        webpage_url: str_field(&v, "webpage_url"),
        id: str_field(&v, "id"),
        extractor: str_field(&v, "extractor").unwrap_or_else(|| "unknown".to_string()),
        content_type,
        title: str_field(&v, "title"),
        description: str_field(&v, "description"),
        thumbnail: str_field(&v, "thumbnail"),
        thumbnails: parse_thumbnails(&v),
        duration: u64_field(&v, "duration"),
        upload_date: str_field(&v, "upload_date"),
        release_date: str_field(&v, "release_date"),
        modified_date: str_field(&v, "modified_date"),
        filesize: u64_field(&v, "filesize"),
        creator: parse_creator(&v),
        uploader: str_field(&v, "uploader"),
        channel: str_field(&v, "channel"),
        channel_url: str_field_or(&v, "channel_url", "uploader_url"),
        channel_id: str_field_or(&v, "channel_id", "uploader_id"),
        view_count: u64_field(&v, "view_count"),
        like_count: u64_field(&v, "like_count"),
        dislike_count: u64_field(&v, "dislike_count"),
        comment_count: u64_field(&v, "comment_count"),
        repost_count: u64_field(&v, "repost_count"),
        rating: f64_field(&v, "average_rating"),
        categories: v.get("categories").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|c| c.as_str().map(String::from))
                .collect()
        }),
        tags: v.get("tags").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(String::from))
                .collect()
        }),
        age_limit: u32_field(&v, "age_limit"),
        availability: str_field(&v, "availability"),
        language: str_field(&v, "language"),
        license: str_field(&v, "license"),
        location: str_field(&v, "location"),
        is_live: bool_field(&v, "is_live"),
        was_live: bool_field(&v, "was_live"),
        live_status: str_field(&v, "live_status"),
        concurrent_view_count: u64_field(&v, "concurrent_view_count"),
        formats,
        subtitles: parse_subtitles(&v),
        chapters,
        aspect_ratio: f64_field(&v, "aspect_ratio"),
        audio_channels: u32_field(&v, "audio_channels"),
        is_playlist: str_field(&v, "_type").as_deref() == Some("playlist"),
        playlist_count: u32_field(&v, "playlist_count").or_else(|| {
            entries
                .as_ref()
                .map(|e: &Vec<PlaylistEntry>| e.len() as u32)
        }),
        entries,
        playlist_id: str_field(&v, "playlist_id"),
        playlist_title: str_field(&v, "playlist_title"),
        playlist_index: u32_field(&v, "playlist_index"),
        related,
        music: parse_music_info(&v),
        series: parse_series_info(&v),
        ..Default::default()
    })
}

fn parse_subtitles(v: &serde_json::Value) -> Option<Vec<SubtitleTrack>> {
    let mut tracks = Vec::new();

    let sources = [("subtitles", false), ("automatic_captions", true)];

    for (key, is_automatic) in sources {
        if let Some(subs) = v.get(key).and_then(|s| s.as_object()) {
            for (lang, sub_list) in subs {
                if let Some(first) = sub_list.as_array().and_then(|a| a.first()) {
                    tracks.push(SubtitleTrack {
                        lang: lang.clone(),
                        name: str_field(first, "name"),
                        ext: str_field(first, "ext"),
                        url: str_field(first, "url"),
                        is_automatic,
                    });
                }
            }
        }
    }

    if tracks.is_empty() {
        None
    } else {
        Some(tracks)
    }
}

fn parse_thumbnails(v: &serde_json::Value) -> Option<Vec<Thumbnail>> {
    v.get("thumbnails").and_then(|t| t.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|t| {
                let url = str_field(t, "url")?;
                Some(Thumbnail {
                    url,
                    width: u32_field(t, "width"),
                    height: u32_field(t, "height"),
                    id: str_field(t, "id"),
                })
            })
            .collect()
    })
}

fn parse_creator(v: &serde_json::Value) -> Option<CreatorInfo> {
    let name = str_field_or(v, "channel", "uploader");
    let id = str_field_or(v, "channel_id", "uploader_id");
    let url = str_field_or(v, "channel_url", "uploader_url");

    if name.is_none() && id.is_none() && url.is_none() {
        return None;
    }

    Some(CreatorInfo {
        name,
        id,
        url,
        thumbnail: v
            .get("channel_follower_count")
            .and_then(|_| str_field(v, "uploader_thumbnail")),
        subscriber_count: u64_field(v, "channel_follower_count"),
        is_verified: bool_field(v, "channel_is_verified"),
    })
}

fn parse_related(v: &serde_json::Value) -> Option<Vec<RelatedEntry>> {
    let related_arr = v
        .get("related_videos")
        .or_else(|| v.get("suggested_videos"))
        .or_else(|| v.get("recommended"))
        .and_then(|r| r.as_array())?;

    let entries: Vec<RelatedEntry> = related_arr
        .iter()
        .filter_map(|r| {
            let url = str_field(r, "url").or_else(|| str_field(r, "webpage_url"))?;
            Some(RelatedEntry {
                url,
                title: str_field(r, "title"),
                thumbnail: str_field(r, "thumbnail").or_else(|| {
                    r.get("thumbnails")
                        .and_then(|t| t.as_array())
                        .and_then(|a| a.last())
                        .and_then(|i| str_field(i, "url"))
                }),
                duration: u64_field(r, "duration"),
                uploader: str_field_or(r, "uploader", "channel"),
                view_count: u64_field(r, "view_count"),
                extractor: str_field(r, "extractor"),
            })
        })
        .collect();

    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn parse_music_info(v: &serde_json::Value) -> Option<MusicInfo> {
    let track = str_field(v, "track");
    let album = str_field(v, "album");
    let artist = str_field(v, "artist");

    if track.is_none() && album.is_none() && artist.is_none() {
        return None;
    }

    Some(MusicInfo {
        track,
        track_number: u32_field(v, "track_number"),
        album,
        artist,
        album_artist: str_field(v, "album_artist"),
        composer: str_field(v, "composer"),
        genre: str_field(v, "genre"),
        release_year: u32_field(v, "release_year"),
        disc_number: u32_field(v, "disc_number"),
        disc_count: u32_field(v, "disc_count"),
    })
}

fn parse_series_info(v: &serde_json::Value) -> Option<SeriesInfo> {
    let series = str_field(v, "series");
    let season = str_field(v, "season");
    let episode = str_field(v, "episode");
    let season_number = u64_field(v, "season_number");
    let episode_number = u64_field(v, "episode_number");

    if series.is_none()
        && season.is_none()
        && episode.is_none()
        && season_number.is_none()
        && episode_number.is_none()
    {
        return None;
    }

    Some(SeriesInfo {
        series,
        series_id: str_field(v, "series_id"),
        season,
        season_number: season_number.map(|n| n as u32),
        season_id: str_field(v, "season_id"),
        episode,
        episode_number: episode_number.map(|n| n as u32),
        episode_id: str_field(v, "episode_id"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_str_field() {
        let v = json!({"title": "My Video", "empty": "", "num": 42});
        assert_eq!(str_field(&v, "title"), Some("My Video".to_string()));
        assert_eq!(str_field(&v, "empty"), Some("".to_string()));
        assert_eq!(str_field(&v, "missing"), None);
        assert_eq!(str_field(&v, "num"), None); // not a string
    }

    #[test]
    fn test_str_field_or() {
        let v = json!({"webpage_url": "https://example.com"});
        assert_eq!(
            str_field_or(&v, "url", "webpage_url"),
            Some("https://example.com".to_string())
        );
        let v2 = json!({"url": "https://direct.com", "webpage_url": "https://fallback.com"});
        assert_eq!(
            str_field_or(&v2, "url", "webpage_url"),
            Some("https://direct.com".to_string())
        );
    }

    #[test]
    fn test_u64_field_native() {
        let v = json!({"size": 1048576});
        assert_eq!(u64_field(&v, "size"), Some(1048576));
    }

    #[test]
    fn test_u64_field_from_string() {
        // ffprobe returns some numbers as strings
        let v = json!({"bit_rate": "128000"});
        assert_eq!(u64_field(&v, "bit_rate"), Some(128000));
    }

    #[test]
    fn test_u64_field_missing() {
        let v = json!({});
        assert_eq!(u64_field(&v, "size"), None);
    }

    #[test]
    fn test_u32_field_native() {
        let v = json!({"width": 1920});
        assert_eq!(u32_field(&v, "width"), Some(1920));
    }

    #[test]
    fn test_u32_field_from_string() {
        let v = json!({"sample_rate": "48000"});
        assert_eq!(u32_field(&v, "sample_rate"), Some(48000));
    }

    #[test]
    fn test_f64_field_native() {
        let v = json!({"duration": 120.5});
        assert_eq!(f64_field(&v, "duration"), Some(120.5));
    }

    #[test]
    fn test_f64_field_from_string() {
        let v = json!({"duration": "120.5"});
        assert_eq!(f64_field(&v, "duration"), Some(120.5));
    }

    #[test]
    fn test_bool_field() {
        let v = json!({"is_live": true});
        assert_eq!(bool_field(&v, "is_live"), Some(true));
        assert_eq!(bool_field(&v, "missing"), None);
    }

    #[test]
    fn test_i32_field() {
        let v = json!({"season_number": -1});
        assert_eq!(i32_field(&v, "season_number"), Some(-1));
    }

    #[test]
    fn test_parse_playlist_entry_basic() {
        let v = json!({
            "id": "abc123",
            "url": "https://example.com/video",
            "title": "Test Video",
            "duration": 120.0,
            "thumbnail": "https://example.com/thumb.jpg"
        });
        let entry = parse_playlist_entry(&v, 0).unwrap();
        assert_eq!(entry.id, "abc123");
        assert_eq!(entry.title.unwrap(), "Test Video");
        assert_eq!(entry.duration, Some(120));
    }

    #[test]
    fn test_parse_playlist_entry_no_id() {
        let v = json!({"url": "https://example.com/video"});
        assert!(parse_playlist_entry(&v, 0).is_none());
    }

    #[test]
    fn test_parse_playlist_entry_empty_id() {
        let v = json!({"id": "", "url": "https://example.com/video"});
        assert!(parse_playlist_entry(&v, 0).is_none());
    }
}
