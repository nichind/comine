use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

pub mod constants {
    pub const DEFAULT_MAX_CONCURRENT: u32 = 2;
    pub const DEFAULT_MAX_RETRIES: u32 = 3;
    pub const DEFAULT_ARIA2_CONNECTIONS: u32 = 8;
    pub const DEFAULT_ARIA2_SPLITS: u32 = 8;
    pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    pub const PROGRESS_THROTTLE_MS: u64 = 250;
    pub const STDERR_TAIL_SIZE: usize = 80;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum Priority {
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Absolute = 4,
}

/// Content type hint - UI can use this to select primary layout,
/// but should ALWAYS render components based on what data exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum ContentType {
    #[default]
    Video,
    Audio,
    Image,
    Gallery,
    Torrent,
    File,
    Playlist,
    Article,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ResolveEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        backend: String,
    },

    #[serde(rename_all = "camelCase")]
    MetadataUpdate {
        title: Option<String>,
        description: Option<String>,
        thumbnail: Option<String>,
        uploader: Option<String>,
        channel: Option<String>,
        channel_url: Option<String>,
        playlist_id: Option<String>,
        playlist_title: Option<String>,
        playlist_count: Option<u32>,
        content_type: Option<ContentType>,
    },

    #[serde(rename_all = "camelCase")]
    EntryDiscovered {
        index: u32,
        entry: PlaylistEntry,
    },

    #[serde(rename_all = "camelCase")]
    EntriesBatch {
        entries: Vec<(u32, PlaylistEntry)>,
    },

    #[serde(rename_all = "camelCase")]
    Complete {
        info: Box<UrlInfo>,
    },

    #[serde(rename_all = "camelCase")]
    Error {
        message: String,
    },

    Cancelled,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct FileEntry {
    pub url: Option<String>,
    pub path: Option<String>,
    pub filename: Option<String>,
    pub filesize: Option<u64>,
    pub mime_type: Option<String>,
    pub thumbnail: Option<String>,
    pub index: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct RelatedEntry {
    pub url: String,
    pub title: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: Option<u64>,
    pub uploader: Option<String>,
    pub view_count: Option<u64>,
    pub extractor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct TorrentInfo {
    pub info_hash: Option<String>,
    pub magnet_uri: Option<String>,
    pub seeders: Option<u32>,
    pub leechers: Option<u32>,
    pub trackers: Option<Vec<String>>,
    pub created_by: Option<String>,
    pub creation_date: Option<String>,
    pub comment: Option<String>,
    pub piece_count: Option<u32>,
    pub piece_size: Option<u64>,
    pub is_private: Option<bool>,
    pub total_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct MusicInfo {
    pub track: Option<String>,
    pub track_number: Option<u32>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub composer: Option<String>,
    pub genre: Option<String>,
    pub release_year: Option<u32>,
    pub disc_number: Option<u32>,
    pub disc_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct SeriesInfo {
    pub series: Option<String>,
    pub series_id: Option<String>,
    pub season: Option<String>,
    pub season_number: Option<u32>,
    pub season_id: Option<String>,
    pub episode: Option<String>,
    pub episode_number: Option<u32>,
    pub episode_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct GalleryInfo {
    pub gallery_id: Option<String>,
    pub page_count: Option<u32>,
    pub artist: Option<String>,
    pub circle: Option<String>,
    pub parody: Option<Vec<String>>,
    pub characters: Option<Vec<String>>,
    pub language: Option<String>,
    pub translated: Option<bool>,
    pub convention: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct HttpInfo {
    pub original_url: Option<String>,
    pub final_url: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub accept_ranges: Option<bool>,
    pub content_disposition: Option<String>,
    pub server: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct CloudInfo {
    pub shared_by: Option<String>,
    pub share_date: Option<String>,
    pub expiry_date: Option<String>,
    pub password_protected: Option<bool>,
    pub download_limit: Option<u32>,
    pub download_count: Option<u32>,
    pub folder_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct CreatorInfo {
    pub name: Option<String>,
    pub id: Option<String>,
    pub url: Option<String>,
    pub thumbnail: Option<String>,
    pub subscriber_count: Option<u64>,
    pub is_verified: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolveSettings {
    pub cookies_from_browser: Option<String>,
    pub custom_cookies: Option<String>,
    pub proxy: Option<ProxyConfig>,
    pub youtube_player_client: Option<String>,
    #[serde(default)]
    pub flat_playlist: bool,
    pub page_size: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct ResolveResult {
    pub backend: String,
    pub info: UrlInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct UrlInfo {
    pub url: String,
    pub webpage_url: Option<String>,
    pub id: Option<String>,
    pub extractor: String,
    pub content_type: ContentType,

    pub title: Option<String>,
    pub description: Option<String>,
    pub thumbnail: Option<String>,
    pub thumbnails: Option<Vec<Thumbnail>>,

    pub duration: Option<u64>,
    pub upload_date: Option<String>,
    pub release_date: Option<String>,
    pub modified_date: Option<String>,

    pub filesize: Option<u64>,
    pub mime_type: Option<String>,

    pub creator: Option<CreatorInfo>,
    pub uploader: Option<String>,
    pub channel: Option<String>,
    pub channel_url: Option<String>,
    pub channel_id: Option<String>,

    pub view_count: Option<u64>,
    pub like_count: Option<u64>,
    pub dislike_count: Option<u64>,
    pub comment_count: Option<u64>,
    pub repost_count: Option<u64>,
    pub rating: Option<f64>,
    pub rating_count: Option<u64>,

    pub categories: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub age_limit: Option<u32>,
    pub availability: Option<String>,
    pub language: Option<String>,
    pub license: Option<String>,
    pub location: Option<String>,

    pub is_live: Option<bool>,
    pub was_live: Option<bool>,
    pub live_status: Option<String>,
    pub concurrent_view_count: Option<u64>,

    pub formats: Option<Vec<VideoFormat>>,
    pub subtitles: Option<Vec<SubtitleTrack>>,
    pub storyboards: Option<Vec<Storyboard>>,
    pub chapters: Option<Vec<Chapter>>,
    pub aspect_ratio: Option<f64>,
    pub audio_channels: Option<u32>,

    pub is_playlist: bool,
    pub playlist_count: Option<u32>,
    pub entries: Option<Vec<PlaylistEntry>>,
    pub pagination: Option<Pagination>,
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
    pub playlist_index: Option<u32>,

    pub files: Option<Vec<FileEntry>>,
    pub file_count: Option<u32>,

    pub related: Option<Vec<RelatedEntry>>,

    pub torrent: Option<TorrentInfo>,
    pub music: Option<MusicInfo>,
    pub series: Option<SeriesInfo>,
    pub gallery: Option<GalleryInfo>,
    pub http: Option<HttpInfo>,
    pub cloud: Option<CloudInfo>,

    pub reply_count: Option<u64>,
    pub quote_count: Option<u64>,
    pub is_pinned: Option<bool>,
    pub parent_url: Option<String>,

    #[cfg_attr(feature = "ts-export", ts(type = "Record<string, unknown> | null"))]
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct SubtitleTrack {
    pub lang: String,
    pub name: Option<String>,
    pub ext: Option<String>,
    pub url: Option<String>,
    pub is_automatic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct Thumbnail {
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct Pagination {
    pub has_more: bool,
    pub next_cursor: Option<String>,
    pub total_count: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct UrlInfoPatch {
    pub title: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: Option<u64>,
    pub uploader: Option<String>,
    pub channel: Option<String>,
    pub channel_url: Option<String>,
    pub extractor: Option<String>,
    pub webpage_url: Option<String>,
    pub is_playlist: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct Storyboard {
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub cols: u32,
    pub rows: u32,
    pub fragment_count: u32,
    pub fragment_duration: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct Chapter {
    pub title: String,
    pub start_time: f64,
    pub end_time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct VideoFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: Option<String>,
    pub fps: Option<u32>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub tbr: Option<f64>,
    pub vbr: Option<f64>,
    pub abr: Option<f64>,
    pub asr: Option<u32>,
    pub has_video: bool,
    pub has_audio: bool,
    pub quality: Option<i32>,
    pub format_note: Option<String>,
    pub rows: Option<u32>,
    pub columns: Option<u32>,
    pub fragments: Option<Vec<Fragment>>,
    pub source_preference: Option<i32>,
    pub language: Option<String>,
    pub dynamic_range: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub protocol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct Fragment {
    pub url: String,
    pub duration: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct PlaylistEntry {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: Option<u64>,
    pub index: u32,
    pub uploader: Option<String>,
    pub is_music: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct ClipRange {
    pub id: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "config")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub enum PostProcessStep {
    FFmpegConvert {
        target_format: String,
        #[serde(default)]
        audio_only: bool,
        extra_args: Option<Vec<String>>,
    },
    EmbedThumbnail {
        thumbnail_url: String,
    },
    EmbedMetadata {
        title: Option<String>,
        artist: Option<String>,
        album: Option<String>,
    },
    MoveFile {
        destination: String,
    },
    // Placeholders: {input} and {output} are replaced with file paths.
    CustomCommand {
        command: String,
        args: Vec<String>,
    },
    ConcatFiles {
        files: Vec<String>,
        output_path: String,
        delete_sources: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct DownloadRequest {
    pub url: String,
    pub backend: Option<String>,
    pub id: Option<String>,
    #[serde(default)]
    pub quality: QualitySettings,
    pub output: OutputSettings,
    #[serde(default)]
    pub options: DownloadOptions,
    #[serde(default)]
    pub post_process: Vec<PostProcessStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct QualitySettings {
    #[serde(default = "default_format")]
    pub format: String,
    pub max_height: Option<u32>,
    pub prefer_codec: Option<String>,
    #[serde(default)]
    pub audio_only: bool,
    pub audio_format: Option<String>,
}

fn default_format() -> String {
    "bestvideo+bestaudio/best".to_string()
}

impl Default for QualitySettings {
    fn default() -> Self {
        Self {
            format: default_format(),
            max_height: None,
            prefer_codec: None,
            audio_only: false,
            audio_format: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct OutputSettings {
    pub directory: String,
    pub filename_template: Option<String>,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct DownloadOptions {
    pub cookies_from_browser: Option<String>,
    pub custom_cookies: Option<String>,
    pub proxy: Option<ProxyConfig>,
    pub speed_limit: Option<u64>,
    #[serde(default = "default_true")]
    pub embed_thumbnail: bool,
    #[serde(default = "default_true")]
    pub embed_metadata: bool,
    #[serde(default)]
    pub embed_subtitles: bool,
    pub subtitle_langs: Option<String>,
    pub sponsorblock_remove: Option<String>,
    pub youtube_player_client: Option<String>,
    pub aria2_connections: Option<u32>,
    pub aria2_splits: Option<u32>,
    pub aria2_min_split_size: Option<String>,
    pub aria2_disable_ipv6: Option<bool>,
    pub aria2_custom_args: Option<String>,
    pub max_retries: Option<u32>,
    pub clip_ranges: Option<Vec<ClipRange>>,
    #[serde(default)]
    pub use_aria2: bool,
    #[serde(default)]
    pub force_keyframes_at_cuts: bool,
}

fn default_true() -> bool {
    true
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            cookies_from_browser: None,
            custom_cookies: None,
            proxy: None,
            speed_limit: None,
            embed_thumbnail: true,
            embed_metadata: true,
            embed_subtitles: false,
            subtitle_langs: None,
            sponsorblock_remove: None,
            youtube_player_client: None,
            aria2_connections: Some(constants::DEFAULT_ARIA2_CONNECTIONS),
            aria2_splits: Some(constants::DEFAULT_ARIA2_SPLITS),
            aria2_min_split_size: None,
            aria2_disable_ipv6: None,
            aria2_custom_args: None,
            max_retries: Some(constants::DEFAULT_MAX_RETRIES),
            clip_ranges: None,
            use_aria2: false,
            force_keyframes_at_cuts: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct ProxyConfig {
    pub enabled: bool,
    pub url: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct Job {
    pub id: String,
    pub request: DownloadRequest,
    pub status: JobStatus,
    pub backend: String,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub progress: f64,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed: Option<u64>,
    pub eta: Option<u64>,
    pub temp_files: Vec<String>,
    pub retry_count: u32,
    pub last_error: Option<String>,
    pub title: Option<String>,
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub author_url: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub playlist_id: Option<String>,
    #[serde(default)]
    pub playlist_title: Option<String>,
    #[serde(default)]
    pub playlist_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "data")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub enum JobStatus {
    Queued,
    Resolving,
    Downloading,
    PostProcessing,
    Paused,
    Completed { output_path: String },
    Failed { error: String, retryable: bool },
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobStatus::Completed { .. } | JobStatus::Failed { .. } | JobStatus::Cancelled
        )
    }

    pub fn is_active(&self) -> bool {
        !self.is_terminal()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub enum JobControl {
    Pause,
    Resume,
    Cancel,
    Retry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "data")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub enum JobEvent {
    Added {
        job: Job,
    },
    Started {
        job_id: String,
        backend: String,
    },
    UrlInfoPatched {
        job_id: String,
        patch: UrlInfoPatch,
    },
    Progress {
        job_id: String,
        progress: f64,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        speed: Option<u64>,
        eta: Option<u64>,
    },
    StatusChanged {
        job_id: String,
        status: JobStatus,
    },
    Completed {
        job_id: String,
        output_path: String,
        title: Option<String>,
        thumbnail: Option<String>,
        filesize: Option<u64>,
    },
    Failed {
        job_id: String,
        error: String,
        retryable: bool,
    },
    Cancelled {
        job_id: String,
    },
    Paused {
        job_id: String,
    },
    Resumed {
        job_id: String,
    },
    ThumbnailEmbedFailed {
        job_id: String,
        error: String,
    },
    ThumbnailColorExtracted {
        job_id: String,
        color: (u8, u8, u8),
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "message")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub enum BackendError {
    UnsupportedUrl(String),
    NetworkError(String),
    NotFound(String),
    Forbidden(String),
    Unauthorized(String),
    ServerError(String),
    RateLimited(String),
    ProcessError(String),
    ParseError(String),
    IoError(String),
    Cancelled,
    Paused,
    /// A non-terminal job with the same URL already exists.
    DuplicateUrl(String),
    Other(String),
}

impl BackendError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            BackendError::NetworkError(_)
                | BackendError::ServerError(_)
                | BackendError::RateLimited(_)
                | BackendError::ProcessError(_)
        )
    }
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::UnsupportedUrl(s) => write!(f, "Unsupported URL: {}", s),
            BackendError::NetworkError(s) => write!(f, "Network error: {}", s),
            BackendError::NotFound(s) => write!(f, "Not found: {}", s),
            BackendError::Forbidden(s) => write!(f, "Forbidden: {}", s),
            BackendError::Unauthorized(s) => write!(f, "Unauthorized: {}", s),
            BackendError::ServerError(s) => write!(f, "Server error: {}", s),
            BackendError::RateLimited(s) => write!(f, "Rate limited: {}", s),
            BackendError::ProcessError(s) => write!(f, "Process error: {}", s),
            BackendError::ParseError(s) => write!(f, "Parse error: {}", s),
            BackendError::IoError(s) => write!(f, "IO error: {}", s),
            BackendError::Cancelled => write!(f, "Cancelled"),
            BackendError::Paused => write!(f, "Paused"),
            BackendError::DuplicateUrl(id) => write!(f, "Duplicate URL (active job: {})", id),
            BackendError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for BackendError {}

#[derive(Debug, Clone)]
#[allow(dead_code)] // job_id kept for debugging via Debug derive
pub struct ProgressUpdate {
    pub job_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed: Option<u64>,
    pub eta: Option<u64>,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase", default)]
pub struct DownloadSettings {
    pub download_path: String,
    pub audio_path: String,
    pub use_audio_path: bool,

    pub default_download_mode: String, // "auto" | "audio" | "mute"
    pub default_video_quality: String, // "max" | "4k" | "1080p" etc.
    pub default_audio_quality: String, // "best" | "320" etc.
    pub preferred_video_codec: String, // "any" | "h264" | "h265" | "vp9" | "av1"
    pub preferred_audio_codec: String, // "any" | "opus" | "aac" | "mp3" | "vorbis"
    pub audio_format: String,          // "any" | "mp3" | "m4a" | "opus" | "wav" | "flac"

    pub embed_thumbnail: bool,
    pub embed_metadata: bool, // inverse of clearMetadata
    pub embed_subtitles: bool,
    pub subtitle_languages: String,

    pub sponsor_block: bool,
    pub sponsor_block_skip_sponsors: bool,
    pub sponsor_block_skip_intros: bool,
    pub sponsor_block_skip_self_promo: bool,
    pub sponsor_block_skip_interaction: bool,

    pub cookies_from_browser: String,
    pub custom_cookies: String,

    pub proxy_mode: String, // "none" | "system" | "custom"
    pub custom_proxy_url: String,

    pub download_speed_limit: u64, // bytes/sec, 0 = unlimited
    pub use_aria2: bool,
    pub aria2_connections: u32,
    pub aria2_splits: u32,
    pub aria2_min_split_size: String,
    pub aria2_disable_ipv6: bool,
    pub aria2_custom_args: String,

    pub youtube_player_client: String,
    pub concurrent_downloads: u32,
}

impl DownloadSettings {
    pub fn default_download_path() -> String {
        #[cfg(not(target_os = "android"))]
        {
            dirs::download_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        }
        #[cfg(target_os = "android")]
        {
            "/storage/emulated/0/Download/Comine".to_string()
        }
    }
}

impl Default for DownloadSettings {
    fn default() -> Self {
        Self {
            download_path: Self::default_download_path(),
            audio_path: String::new(),
            use_audio_path: false,
            default_download_mode: "auto".to_string(),
            default_video_quality: "max".to_string(),
            default_audio_quality: "best".to_string(),
            preferred_video_codec: "any".to_string(),
            preferred_audio_codec: "any".to_string(),
            audio_format: "any".to_string(),
            embed_thumbnail: true,
            embed_metadata: true,
            embed_subtitles: false,
            subtitle_languages: String::new(),
            sponsor_block: false,
            sponsor_block_skip_sponsors: true,
            sponsor_block_skip_intros: false,
            sponsor_block_skip_self_promo: false,
            sponsor_block_skip_interaction: false,
            cookies_from_browser: String::new(),
            custom_cookies: String::new(),
            proxy_mode: "none".to_string(),
            custom_proxy_url: String::new(),
            download_speed_limit: 0,
            use_aria2: false,
            aria2_connections: 8,
            aria2_splits: 8,
            aria2_min_split_size: "1M".to_string(),
            aria2_disable_ipv6: false,
            aria2_custom_args: String::new(),
            youtube_player_client: String::new(),
            concurrent_downloads: 3,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct EnqueueOverrides {
    pub download_mode: Option<String>,
    pub video_quality: Option<String>,
    pub audio_quality: Option<String>,

    pub cookies_from_browser: Option<String>,
    pub custom_cookies: Option<String>,

    pub embed_thumbnail: Option<bool>,
    pub clear_metadata: Option<bool>,
    pub embed_subtitles: Option<bool>,
    pub subtitle_languages: Option<String>,

    pub sponsor_block: Option<bool>,
    pub sponsor_block_categories: Option<Vec<String>>,

    pub use_aria2: Option<bool>,
    pub audio_format: Option<String>,
    pub output_template: Option<String>,
    pub clip_ranges: Option<Vec<ClipRange>>,
    pub filename: Option<String>,
    pub output_directory: Option<String>,

    pub dont_show_in_history: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct EnqueueRequest {
    pub url: String,
    pub id: Option<String>,
    #[serde(default)]
    pub overrides: EnqueueOverrides,
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
    pub playlist_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct EnqueuePlaylistRequest {
    pub entries: Vec<EnqueueRequest>,
    pub playlist_id: String,
    pub playlist_title: String,
    pub use_playlist_folder: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    pub url: String,
    pub title: String,
    pub author: String,
    pub author_url: Option<String>,
    pub thumbnail: String,
    pub extension: String,
    pub size: u64,
    pub duration: f64,
    pub file_path: String,
    pub downloaded_at: u64,
    #[serde(rename = "type")]
    pub item_type: String, // "video" | "audio" | "image" | "file"
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
    pub playlist_index: Option<u32>,
    pub converted_format: Option<String>,
    pub download_source: Option<String>,
    #[serde(default)]
    pub is_favourite: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct HistoryQuery {
    pub search: Option<String>,
    pub filter: Option<String>, // "all" | "video" | "audio" | "image" | "file" | "favourites"
    pub sort: Option<String>,   // "date" | "name" | "size" | "duration"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ClearFilter {
    Completed,
    Failed,
    All,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct AppStats {
    pub total_downloads: u64,
    pub total_size_mb: f64,
    pub successful_downloads: u64,
    pub failed_downloads: u64,
    pub first_launch: String,
}
