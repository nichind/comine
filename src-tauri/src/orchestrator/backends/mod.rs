//! Backend trait — separates RESOLUTION from EXECUTION concerns.

use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::orchestrator::types::*;

pub mod aria2;
pub mod common;
pub mod direct;
pub mod ytdlp;

pub use common::{
    extract_filename_from_response, extract_filename_from_url, extract_magnet_name,
    guess_mime_type, has_file_extension, http_status_to_error, is_torrent_url,
    DIRECT_FILE_EXTENSIONS,
};

#[cfg(target_os = "android")]
pub use ytdlp::{
    cancel_android_download, init_android, is_jni_ready, pause_android_download, set_job_manager,
    wait_for_jni_ready,
};

#[cfg(target_os = "android")]
pub use aria2::cancel_aria2_android;

#[cfg(target_os = "android")]
pub use ytdlp::{
    get_activity, get_jni_env, register_pending_job, AndroidJobResult, PENDING_ANDROID_JOBS,
};

#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Part of trait design, will be used for feature checks
pub struct BackendCapabilities {
    pub supports_pause: bool,
    pub supports_resume: bool,
    pub supports_progress: bool,
    pub supports_speed_limit: bool,
}

#[derive(Clone)]
pub struct SpawnContext {
    pub job: Job,
    pub cancel_token: CancellationToken,
    pub progress_tx: tokio::sync::mpsc::UnboundedSender<ProgressUpdate>,
    pub effective_speed_limit: Option<u64>,
}

#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;

    #[allow(dead_code)] // Part of trait design, will be used for feature checks
    fn capabilities(&self) -> BackendCapabilities;

    // Contract: must not perform network calls.
    fn priority(&self, url: &str) -> Priority;

    async fn resolve(&self, url: &str, settings: &ResolveSettings)
        -> Result<UrlInfo, BackendError>;

    async fn spawn(&self, ctx: SpawnContext) -> Result<String, BackendError>;
}

pub struct BackendRegistry {
    backends: Vec<Arc<dyn Backend>>,
}

impl BackendRegistry {
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    pub fn register(&mut self, backend: Arc<dyn Backend>) {
        tracing::info!(backend = backend.name(), "Registered backend");
        self.backends.push(backend);
    }

    pub fn candidates_for(&self, url: &str) -> Vec<(Arc<dyn Backend>, Priority)> {
        let mut candidates: Vec<_> = self
            .backends
            .iter()
            .map(|b| (b.clone(), b.priority(url)))
            .filter(|(_, p)| *p != Priority::None)
            .collect();

        candidates.sort_by(|a, b| b.1.cmp(&a.1));
        candidates
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Backend>> {
        self.backends.iter().find(|b| b.name() == name).cloned()
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Backwards-compatible alias.
pub use common::is_video_hosting_site as is_known_video_site;
