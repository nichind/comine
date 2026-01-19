#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VideoInfo {
    pub title: String,
    pub uploader: Option<String>,
    pub channel: Option<String>,
    pub creator: Option<String>,
    pub uploader_id: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub filesize: Option<u64>,
    pub ext: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlaylistEntry {
    pub id: String,
    pub url: String,
    pub title: String,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub uploader: Option<String>,
    pub is_music: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlaylistInfo {
    pub is_playlist: bool,
    pub id: Option<String>,
    pub title: String,
    pub uploader: Option<String>,
    pub thumbnail: Option<String>,
    pub total_count: usize,
    pub entries: Vec<PlaylistEntry>,
    pub has_more: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VideoFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: Option<String>,
    pub fps: Option<f64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub tbr: Option<f64>,
    pub vbr: Option<f64>,
    pub abr: Option<f64>,
    pub asr: Option<u32>,
    pub format_note: Option<String>,
    pub has_video: bool,
    pub has_audio: bool,
    pub quality: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VideoFormats {
    pub title: String,
    pub author: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    pub formats: Vec<VideoFormat>,
    pub view_count: Option<u64>,
    pub like_count: Option<u64>,
    pub description: Option<String>,
    pub upload_date: Option<String>,
    pub channel_url: Option<String>,
    pub channel_id: Option<String>,
    pub storyboards: Option<Vec<Storyboard>>,
    pub chapters: Option<Vec<Chapter>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Storyboard {
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub cols: u32,
    pub rows: u32,
    pub fragment_count: u32,
    pub fragment_duration: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Chapter {
    pub title: String,
    pub start_time: f64,
    pub end_time: f64,
}

#[derive(Serialize, Clone)]
pub struct DownloadProgress {
    pub url: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClipRange {
    pub start: f64,
    pub end: f64,
}

#[derive(Serialize, Clone)]
pub struct DownloadFilePath {
    pub url: String,
    pub file_path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileInfo {
    pub filename: String,
    pub size: u64,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NotificationData {
    pub title: String,
    pub body: String,
    pub thumbnail: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub compact: bool,
    #[serde(default)]
    pub is_playlist: bool,
    #[serde(default)]
    pub is_channel: bool,
    #[serde(default)]
    pub is_file: bool,
    pub file_info: Option<FileInfo>,
    #[serde(default = "default_download_label")]
    pub download_label: String,
    #[serde(default = "default_dismiss_label")]
    pub dismiss_label: String,
}

fn default_download_label() -> String {
    "Download".to_string()
}

fn default_dismiss_label() -> String {
    "Dismiss".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationPosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    #[default]
    BottomRight,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum NotificationMonitor {
    #[default]
    Primary,
    Cursor,
}

// ==================== Dependencies ====================

#[derive(Serialize, Clone, Default)]
pub struct ReleaseInfo {
    pub tag: String,
    pub name: String,
    pub published_at: String,
}

#[derive(Serialize, Clone, Default)]
pub struct DependencyStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub update_available: Option<String>,
}

impl DependencyStatus {
    pub fn not_installed() -> Self {
        Self::default()
    }

    pub fn embedded(description: &str) -> Self {
        Self {
            installed: true,
            version: Some("embedded".to_string()),
            path: Some(description.to_string()),
            update_available: None,
        }
    }

    pub fn installed(version: String, path: String) -> Self {
        Self {
            installed: true,
            version: Some(version),
            path: Some(path),
            update_available: None,
        }
    }

    pub fn with_update(mut self, update_version: Option<String>) -> Self {
        self.update_available = update_version;
        self
    }
}

#[derive(Serialize, Clone, Default)]
pub struct InstallProgress {
    pub stage: String,
    pub progress: u8,
    pub downloaded: u64,
    pub total: u64,
    pub speed: f64,
    pub message: String,
}
