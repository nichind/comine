use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::orchestrator::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
#[serde(rename_all = "camelCase")]
pub struct BackendCapabilities {
    pub name: String,
    pub streaming_resolve: bool,
    pub playlists: bool,
    pub pause_resume: bool,
    pub multi_connection: bool,
    pub format_selection: bool,
    pub subtitles: bool,
    pub speed_limit: bool,
    pub proxy: bool,
    pub cookies: bool,
    pub torrent_magnet: bool,
    pub post_processing: bool,
}

pub mod aria2;
pub mod common;
pub mod direct;
pub mod gallery_dl;
pub mod ytdlp;

#[cfg(target_os = "android")]
pub mod android_jni;

pub use common::{
    extract_filename_from_url, extract_magnet_name, guess_mime_type, has_file_extension,
    http_status_to_error, is_torrent_url, parse_size_str, resolve_effective_proxy,
    resolve_http_file, DIRECT_FILE_EXTENSIONS,
};

#[cfg(not(target_os = "android"))]
pub use common::{apply_args_to_command, graceful_shutdown};

#[cfg(target_os = "android")]
pub use android_jni::{get_activity, get_jni_env, wait_for_jni_ready};

#[cfg(target_os = "android")]
pub use ytdlp::{cancel_ytdlp, init_android, pause_android_download};

#[cfg(target_os = "android")]
pub use aria2::cancel_aria2;

#[derive(Debug, Clone)]
pub enum MetadataEvent {
    Patch(UrlInfoPatch),
    PostProcessing,
}

#[derive(Clone)]
pub struct SpawnContext {
    pub job: Job,
    pub cancel_token: CancellationToken,
    pub progress_tx: tokio::sync::mpsc::UnboundedSender<ProgressUpdate>,
    pub metadata_tx: tokio::sync::mpsc::UnboundedSender<MetadataEvent>,
    pub effective_speed_limit: Option<u64>,
}

#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;

    fn capabilities(&self) -> BackendCapabilities;

    // Contract: must not perform network calls.
    fn priority(&self, url: &str) -> Priority;

    async fn resolve(&self, url: &str, settings: &ResolveSettings)
        -> Result<UrlInfo, BackendError>;

    async fn spawn(&self, ctx: SpawnContext) -> Result<String, BackendError>;

    async fn resolve_stream(
        &self,
        url: &str,
        settings: &ResolveSettings,
        _cancel_token: CancellationToken,
    ) -> StreamingResolveHandle {
        let result = self.resolve(url, settings).await;
        wrap_resolve_as_stream(self.name().to_string(), result)
    }
}

pub struct StreamingResolveHandle {
    pub events: Pin<Box<dyn Stream<Item = ResolveEvent> + Send>>,
}

pub fn wrap_resolve_as_stream(
    backend_name: String,
    result: Result<UrlInfo, BackendError>,
) -> StreamingResolveHandle {
    let (tx, rx) = mpsc::unbounded_channel();

    let _ = tx.send(ResolveEvent::Started {
        backend: backend_name,
    });

    match result {
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

    StreamingResolveHandle {
        events: Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx)),
    }
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
        let name = backend.name().to_string();
        // Replace any existing backend with the same name (supports hot-reload after install)
        self.backends.retain(|b| b.name() != name);
        tracing::info!(backend = %name, "Registered backend");
        self.backends.push(backend);
    }

    pub fn unregister(&mut self, name: &str) -> bool {
        let before = self.backends.len();
        self.backends.retain(|b| b.name() != name);
        let removed = self.backends.len() < before;
        if removed {
            tracing::info!(backend = %name, "Unregistered backend");
        }
        removed
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

    pub fn all_capabilities(&self) -> Vec<BackendCapabilities> {
        self.backends.iter().map(|b| b.capabilities()).collect()
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub use common::is_video_hosting_site as is_known_video_site;

#[cfg(test)]
mod tests {
    use super::*;

    struct MockBackend {
        backend_name: &'static str,
        prio: Priority,
    }

    impl MockBackend {
        fn new(name: &'static str, prio: Priority) -> Arc<dyn Backend> {
            Arc::new(Self {
                backend_name: name,
                prio,
            })
        }
    }

    #[async_trait]
    impl Backend for MockBackend {
        fn name(&self) -> &str {
            self.backend_name
        }

        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities {
                name: self.backend_name.to_string(),
                streaming_resolve: false,
                playlists: false,
                pause_resume: false,
                multi_connection: false,
                format_selection: false,
                subtitles: false,
                speed_limit: false,
                proxy: false,
                cookies: false,
                torrent_magnet: false,
                post_processing: false,
            }
        }

        fn priority(&self, _url: &str) -> Priority {
            self.prio.clone()
        }

        async fn resolve(
            &self,
            _url: &str,
            _settings: &ResolveSettings,
        ) -> Result<UrlInfo, BackendError> {
            Err(BackendError::UnsupportedUrl("mock".into()))
        }

        async fn spawn(&self, _ctx: SpawnContext) -> Result<String, BackendError> {
            Err(BackendError::UnsupportedUrl("mock".into()))
        }
    }

    #[test]
    fn test_registry_new_is_empty() {
        let reg = BackendRegistry::new();
        assert!(reg.backends.is_empty());
        assert!(reg.get("anything").is_none());
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = BackendRegistry::new();
        reg.register(MockBackend::new("ytdlp", Priority::High));
        reg.register(MockBackend::new("aria2", Priority::Medium));

        assert_eq!(reg.backends.len(), 2);
        assert!(reg.get("ytdlp").is_some());
        assert_eq!(reg.get("ytdlp").unwrap().name(), "ytdlp");
        assert!(reg.get("aria2").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn test_register_replaces_duplicate() {
        let mut reg = BackendRegistry::new();
        reg.register(MockBackend::new("ytdlp", Priority::Low));
        reg.register(MockBackend::new("ytdlp", Priority::High));

        assert_eq!(reg.backends.len(), 1);
        let (_, prio) = &reg.candidates_for("https://youtube.com")[0];
        assert_eq!(*prio, Priority::High);
    }

    #[test]
    fn test_unregister() {
        let mut reg = BackendRegistry::new();
        reg.register(MockBackend::new("ytdlp", Priority::High));
        reg.register(MockBackend::new("aria2", Priority::Medium));

        assert!(reg.unregister("ytdlp"));
        assert_eq!(reg.backends.len(), 1);
        assert!(reg.get("ytdlp").is_none());
        assert!(reg.get("aria2").is_some());
    }

    #[test]
    fn test_unregister_nonexistent_returns_false() {
        let mut reg = BackendRegistry::new();
        assert!(!reg.unregister("nope"));
    }

    #[test]
    fn test_candidates_for_sorted_by_priority() {
        let mut reg = BackendRegistry::new();
        reg.register(MockBackend::new("low", Priority::Low));
        reg.register(MockBackend::new("high", Priority::High));
        reg.register(MockBackend::new("medium", Priority::Medium));

        let candidates = reg.candidates_for("https://example.com");
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].0.name(), "high");
        assert_eq!(candidates[1].0.name(), "medium");
        assert_eq!(candidates[2].0.name(), "low");
    }

    #[test]
    fn test_candidates_for_excludes_none_priority() {
        let mut reg = BackendRegistry::new();
        reg.register(MockBackend::new("good", Priority::High));
        reg.register(MockBackend::new("skip", Priority::None));

        let candidates = reg.candidates_for("https://example.com");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].0.name(), "good");
    }

    #[test]
    fn test_candidates_for_empty_registry() {
        let reg = BackendRegistry::new();
        assert!(reg.candidates_for("https://example.com").is_empty());
    }

    #[test]
    fn test_all_capabilities() {
        let mut reg = BackendRegistry::new();
        reg.register(MockBackend::new("a", Priority::High));
        reg.register(MockBackend::new("b", Priority::Low));

        let caps = reg.all_capabilities();
        assert_eq!(caps.len(), 2);
        let names: Vec<&str> = caps.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
    }

    #[test]
    fn test_default_impl() {
        let reg = BackendRegistry::default();
        assert!(reg.backends.is_empty());
    }
}
