#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub filename: String,
    pub size: u64,
    pub mime_type: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
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

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub tag: String,
    pub name: String,
    pub published_at: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct DependencyStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub update_available: Option<String>,
    pub disk_size: Option<u64>,
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
            disk_size: None,
        }
    }

    pub fn installed(version: String, path: String) -> Self {
        Self {
            installed: true,
            version: Some(version),
            path: Some(path),
            update_available: None,
            disk_size: None,
        }
    }

    pub fn with_update(mut self, update_version: Option<String>) -> Self {
        self.update_available = update_version;
        self
    }

    pub fn with_disk_size(mut self, disk_size: Option<u64>) -> Self {
        self.disk_size = disk_size;
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
