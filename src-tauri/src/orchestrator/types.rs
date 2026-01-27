//! Core types for the job orchestration system.
//! Public types can optionally export TypeScript bindings via the `ts-export` feature.

use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

pub mod constants {
    pub const DEFAULT_MAX_CONCURRENT: u32 = 2;
    pub const DEFAULT_MAX_RETRIES: u32 = 3;
    pub const DEFAULT_ARIA2_CONNECTIONS: u32 = 8;
    pub const DEFAULT_ARIA2_SPLITS: u32 = 8;
    pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    pub const PROGRESS_THROTTLE_MS: u64 = 250;
    #[allow(dead_code)] // May be used for resolve timeout feature
    pub const RESOLVE_TIMEOUT_SECS: u64 = 30;
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolveSettings {
    pub cookies_from_browser: Option<String>,
    pub custom_cookies: Option<String>,
    pub proxy: Option<ProxyConfig>,
    pub youtube_player_client: Option<String>,
    #[serde(default)]
    pub flat_playlist: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct ResolveResult {
    pub backend: String,
    pub info: UrlInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct UrlInfo {
    pub url: String,
    pub title: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: Option<u64>,
    pub filesize: Option<u64>,
    pub extractor: String,
    pub is_playlist: bool,
    pub playlist_count: Option<u32>,
    pub formats: Option<Vec<VideoFormat>>,
    pub mime_type: Option<String>,
    pub entries: Option<Vec<PlaylistEntry>>,
    pub uploader: Option<String>,
    pub channel: Option<String>,
    pub view_count: Option<u64>,
    pub like_count: Option<u64>,
    pub description: Option<String>,
    pub upload_date: Option<String>,
    pub channel_url: Option<String>,
    pub channel_id: Option<String>,
    pub storyboards: Option<Vec<Storyboard>>,
    pub chapters: Option<Vec<Chapter>>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct DownloadRequest {
    pub url: String,
    pub backend: Option<String>,
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
    "best".to_string()
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
    pub max_retries: Option<u32>,
    pub clip_ranges: Option<Vec<ClipRange>>,
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
            max_retries: Some(constants::DEFAULT_MAX_RETRIES),
            clip_ranges: None,
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
    pub post_process_index: usize,
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

    #[allow(dead_code)] // Logical complement to is_terminal, useful for debugging
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            JobStatus::Downloading | JobStatus::PostProcessing | JobStatus::Resolving
        )
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
        .unwrap()
        .as_millis() as u64
}
