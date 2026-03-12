use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use futures::StreamExt;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

use crate::orchestrator::backends::{is_torrent_url, Backend, BackendRegistry, SpawnContext};
use crate::orchestrator::store::JobStore;
use crate::orchestrator::types::*;

/// RAII guard that increments active count on creation and decrements on drop.
/// Prevents job starvation from missed decrements on any code path (including panics).
struct ActiveJobSlot(Arc<AtomicU32>);

impl ActiveJobSlot {
    fn try_acquire(counter: &Arc<AtomicU32>, max: u32) -> Option<Self> {
        loop {
            let current = counter.load(Ordering::Acquire);
            if current >= max {
                return None;
            }
            if counter
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Some(Self(Arc::clone(counter)));
            }
        }
    }
}

impl Drop for ActiveJobSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

struct RunningJob {
    cancel_token: CancellationToken,
    task_handle: tokio::task::JoinHandle<()>,
    _slot: ActiveJobSlot,
}

pub struct JobManager {
    pub(crate) app: AppHandle,
    store: Arc<JobStore>,

    registry: RwLock<BackendRegistry>,
    jobs: DashMap<String, Job>,
    running: DashMap<String, RunningJob>,
    active_resolves: DashMap<String, CancellationToken>,
    last_progress_emit: DashMap<String, u64>,

    max_concurrent: AtomicU32,
    active_count: Arc<AtomicU32>,
    global_speed_limit: AtomicU64,

    download_settings: RwLock<DownloadSettings>,
    settings_synced: AtomicBool,
    settings_notify: tokio::sync::Notify,

    pub history: Arc<crate::orchestrator::history::HistoryStore>,
    pub stats: Arc<crate::orchestrator::stats::StatsStore>,

    backends_ready: tokio::sync::watch::Receiver<bool>,

    persist_notify: Arc<tokio::sync::Notify>,
    persist_cancel: CancellationToken,
}

impl JobManager {
    pub fn new(
        app: AppHandle,
        store: Arc<JobStore>,
        history: Arc<crate::orchestrator::history::HistoryStore>,
        stats: Arc<crate::orchestrator::stats::StatsStore>,
        backends_ready: tokio::sync::watch::Receiver<bool>,
    ) -> Arc<Self> {
        let persist_notify = Arc::new(tokio::sync::Notify::new());
        let persist_cancel = CancellationToken::new();
        let manager = Arc::new(Self {
            app,
            store: store.clone(),
            registry: RwLock::new(BackendRegistry::new()),
            jobs: DashMap::new(),
            running: DashMap::new(),
            active_resolves: DashMap::new(),
            last_progress_emit: DashMap::new(),
            max_concurrent: AtomicU32::new(constants::DEFAULT_MAX_CONCURRENT),
            active_count: Arc::new(AtomicU32::new(0)),
            global_speed_limit: AtomicU64::new(0),
            download_settings: RwLock::new(DownloadSettings::default()),
            settings_synced: AtomicBool::new(false),
            settings_notify: tokio::sync::Notify::new(),
            history,
            stats,
            backends_ready,
            persist_notify: persist_notify.clone(),
            persist_cancel: persist_cancel.clone(),
        });

        // Spawn debounced persist task — coalesces rapid persist() calls into
        // at most one disk write per DEBOUNCE_MS interval.
        {
            let jobs = manager.jobs.clone();
            let store = store;
            let notify = persist_notify;
            let cancel = persist_cancel;
            tauri::async_runtime::spawn(async move {
                const DEBOUNCE_MS: u64 = 500;
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = notify.notified() => {}
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(DEBOUNCE_MS)).await;
                    let snapshot: Vec<Job> = jobs.iter().map(|r| r.value().clone()).collect();
                    if let Err(e) = store.save_jobs(&snapshot) {
                        error!("Failed to persist jobs: {}", e);
                    }
                }
            });
        }

        manager
    }

    pub async fn register_backend(&self, backend: Arc<dyn Backend>) {
        let mut registry = self.registry.write().await;
        registry.register(backend);
    }

    pub async fn unregister_backend(&self, name: &str) -> bool {
        let mut registry = self.registry.write().await;
        registry.unregister(name)
    }

    pub async fn load_persisted(&self) -> Result<(), BackendError> {
        let loaded = self.store.load_jobs()?;
        for job in loaded {
            self.jobs.insert(job.id.clone(), job);
        }
        Ok(())
    }

    pub fn get_all_jobs(&self) -> Vec<Job> {
        self.jobs.iter().map(|r| r.value().clone()).collect()
    }

    pub fn apply_url_info_patch(&self, job_id: &str, patch: UrlInfoPatch) {
        let mut emit_patch = UrlInfoPatch {
            // Only emit title/thumbnail when they actually changed (these are also the only
            // currently-persisted metadata fields on Job).
            title: None,
            thumbnail: None,
            duration: patch.duration,
            uploader: patch.uploader,
            channel: patch.channel,
            channel_url: patch.channel_url,
            extractor: patch.extractor,
            webpage_url: patch.webpage_url,
            is_playlist: patch.is_playlist,
            content_type: patch.content_type,
        };

        let mut should_persist = false;

        if let Some(mut job) = self.jobs.get_mut(job_id) {
            if let Some(t) = patch.title {
                if job.title.as_deref() != Some(&t) {
                    job.title = Some(t.clone());
                    emit_patch.title = Some(t);
                    should_persist = true;
                }
            }

            if let Some(th) = patch.thumbnail {
                if job.thumbnail.as_deref() != Some(&th) {
                    job.thumbnail = Some(th.clone());
                    emit_patch.thumbnail = Some(th);
                    should_persist = true;
                }
            }

            let author = emit_patch.uploader.as_ref().or(emit_patch.channel.as_ref());
            if let Some(a) = author {
                job.author = Some(a.clone());
            }
            if let Some(ref url) = emit_patch.channel_url {
                job.author_url = Some(url.clone());
            }
            if let Some(d) = emit_patch.duration {
                job.duration = Some(d as f64);
            }
            if let Some(ct) = emit_patch.content_type {
                job.content_type = Some(ct);
            }
        } else {
            // Job might not be in memory yet (race with UI placeholder merge); still forward.
            emit_patch.title = patch.title;
            emit_patch.thumbnail = patch.thumbnail;
        }

        let has_any = emit_patch.title.is_some()
            || emit_patch.thumbnail.is_some()
            || emit_patch.duration.is_some()
            || emit_patch.uploader.is_some()
            || emit_patch.channel.is_some()
            || emit_patch.channel_url.is_some()
            || emit_patch.extractor.is_some()
            || emit_patch.webpage_url.is_some()
            || emit_patch.is_playlist.is_some()
            || emit_patch.content_type.is_some();

        if has_any {
            self.emit_event(JobEvent::UrlInfoPatched {
                job_id: job_id.to_string(),
                patch: emit_patch,
            });
        }

        if should_persist {
            self.persist();
        }
    }

    pub fn set_max_concurrent(&self, max: u32) {
        let max = max.max(1);
        self.max_concurrent.store(max, Ordering::SeqCst);
        info!("Updated max concurrent: {}", max);
    }

    pub fn set_global_speed_limit(&self, limit_bytes_per_sec: u64) {
        self.global_speed_limit
            .store(limit_bytes_per_sec, Ordering::SeqCst);
        info!("Updated global speed limit: {}", limit_bytes_per_sec);
    }

    async fn wait_ready(&self) {
        let mut rx = self.backends_ready.clone();
        // wait_for checks the current value first, then subscribes — no race.
        let _ = rx.wait_for(|&ready| ready).await;
    }

    pub async fn wait_ready_pub(&self) {
        self.wait_ready().await;
    }

    pub async fn resolve_url(
        &self,
        url: &str,
        settings: ResolveSettings,
    ) -> Result<ResolveResult, BackendError> {
        self.wait_ready().await;
        let registry = self.registry.read().await;
        let candidates = registry.candidates_for(url);
        if candidates.is_empty() {
            return Err(BackendError::UnsupportedUrl(url.to_string()));
        }

        for (backend, _) in candidates {
            match backend.resolve(url, &settings).await {
                Ok(info) => {
                    return Ok(ResolveResult {
                        backend: backend.name().to_string(),
                        info,
                    })
                }
                Err(e) => {
                    debug!("Resolve failed for backend {}: {}", backend.name(), e);
                }
            }
        }

        Err(BackendError::Other(
            "No backend could resolve this URL".to_string(),
        ))
    }

    pub async fn resolve_url_stream(
        &self,
        resolve_id: &str,
        url: &str,
        settings: ResolveSettings,
    ) -> Result<(), BackendError> {
        self.wait_ready().await;
        let registry = self.registry.read().await;
        let candidates = registry.candidates_for(url);
        if candidates.is_empty() {
            return Err(BackendError::UnsupportedUrl(url.to_string()));
        }

        #[allow(clippy::unwrap_used)]
        let (backend, _) = candidates.first().unwrap();
        let backend = Arc::clone(backend);
        drop(registry);

        let cancel_token = CancellationToken::new();
        self.active_resolves
            .insert(resolve_id.to_string(), cancel_token.clone());

        let handle = backend.resolve_stream(url, &settings, cancel_token).await;
        let mut stream = handle.events;

        let event_name = format!("resolve-event:{}", resolve_id);
        let app = self.app.clone();
        let resolve_id_owned = resolve_id.to_string();
        let active_resolves = self.active_resolves.clone();

        tokio::spawn(async move {
            while let Some(event) = stream.next().await {
                if let Err(e) = app.emit(&event_name, &event) {
                    warn!("Failed to emit resolve event: {}", e);
                }

                match &event {
                    ResolveEvent::Complete { .. }
                    | ResolveEvent::Error { .. }
                    | ResolveEvent::Cancelled => {
                        active_resolves.remove(&resolve_id_owned);
                        break;
                    }
                    _ => {}
                }
            }

            active_resolves.remove(&resolve_id_owned);
        });

        Ok(())
    }

    pub fn cancel_resolve(&self, resolve_id: &str) -> bool {
        if let Some((_, token)) = self.active_resolves.remove(resolve_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub async fn start_job(
        self: &Arc<Self>,
        request: DownloadRequest,
    ) -> Result<String, BackendError> {
        self.wait_ready().await;

        // Reject if an active (non-terminal) job with the same URL already exists.
        if let Some(existing) = self
            .jobs
            .iter()
            .find(|r| r.request.url == request.url && r.status.is_active())
        {
            info!(
                "Rejected duplicate URL: {} (active job: {})",
                request.url, existing.id
            );
            return Err(BackendError::DuplicateUrl(existing.id.clone()));
        }

        let job_id = request
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let backend = if let Some(ref b) = request.backend {
            b.clone()
        } else {
            let registry = self.registry.read().await;
            let candidates = registry.candidates_for(&request.url);
            // If the highest-priority candidate is a domain specialist (High priority),
            // always prefer it — it knows the URL best and will ignore irrelevant
            // options like format selection that don't apply (e.g., gallery-dl for kemono).
            // Otherwise, find the first candidate whose capabilities match the request.
            let chosen = candidates
                .first()
                .filter(|(_, p)| *p == Priority::High)
                .or_else(|| {
                    candidates
                        .iter()
                        .find(|(b, _)| request_compatible_with(&request, &b.capabilities()))
                })
                .or(candidates.first());
            chosen
                .map(|(b, _)| b.name().to_string())
                .unwrap_or_else(|| "ytdlp".to_string())
        };

        let content_type = if is_torrent_url(&request.url) {
            Some(ContentType::Torrent)
        } else {
            None
        };

        let job = Job {
            id: job_id.clone(),
            request,
            status: JobStatus::Queued,
            backend,
            created_at: now_ms(),
            started_at: None,
            completed_at: None,
            progress: 0.0,
            downloaded_bytes: 0,
            total_bytes: None,
            speed: None,
            eta: None,
            temp_files: Vec::new(),
            retry_count: 0,
            last_error: None,
            title: None,
            thumbnail: None,
            author: None,
            author_url: None,
            duration: None,
            playlist_id: None,
            playlist_title: None,
            playlist_index: None,
            content_type,
            skip_proxy: false,
        };

        self.jobs.insert(job_id.clone(), job.clone());
        self.emit_event(JobEvent::Added { job });
        self.persist();
        self.schedule_try_start_next();

        Ok(job_id)
    }

    fn stop_running_job(self: &Arc<Self>, job_id: &str, status: JobStatus, event: JobEvent) {
        if let Some((_, running)) = self.running.remove(job_id) {
            running.cancel_token.cancel();
            running.task_handle.abort();
            // ActiveJobSlot is dropped here via `running._slot`, decrementing active_count
        }
        self.update_job_status(job_id, status);
        self.emit_event(event);
        self.last_progress_emit.remove(job_id);
        self.persist();
        self.schedule_try_start_next();
    }

    pub async fn control_job(
        self: &Arc<Self>,
        job_id: &str,
        action: JobControl,
    ) -> Result<(), BackendError> {
        match action {
            JobControl::Cancel => {
                #[cfg(target_os = "android")]
                {
                    let backend_name = self.jobs.get(job_id).map(|j| j.backend.clone());
                    match backend_name.as_deref() {
                        Some("aria2") => {
                            if let Err(e) = crate::orchestrator::backends::cancel_aria2(job_id) {
                                warn!("Failed to cancel Android aria2 download {}: {}", job_id, e);
                            }
                        }
                        _ => {
                            if let Err(e) = crate::orchestrator::backends::cancel_ytdlp(job_id) {
                                warn!("Failed to cancel Android ytdlp download {}: {}", job_id, e);
                            }
                        }
                    }
                }

                self.stop_running_job(
                    job_id,
                    JobStatus::Cancelled,
                    JobEvent::Cancelled {
                        job_id: job_id.to_string(),
                    },
                );
                Ok(())
            }
            JobControl::Pause => {
                #[cfg(target_os = "android")]
                {
                    if let Err(e) = crate::orchestrator::backends::pause_android_download(job_id) {
                        warn!("Failed to pause Android download {}: {}", job_id, e);
                    }
                }

                self.stop_running_job(
                    job_id,
                    JobStatus::Paused,
                    JobEvent::Paused {
                        job_id: job_id.to_string(),
                    },
                );
                Ok(())
            }
            JobControl::Resume => {
                self.update_job_status(job_id, JobStatus::Queued);
                self.emit_event(JobEvent::Resumed {
                    job_id: job_id.to_string(),
                });
                self.persist();
                self.schedule_try_start_next();
                Ok(())
            }
            JobControl::Retry => {
                if let Some(mut job) = self.jobs.get_mut(job_id) {
                    job.retry_count = 0;
                    job.last_error = None;
                    job.progress = 0.0;
                    job.downloaded_bytes = 0;
                    job.total_bytes = None;
                    job.speed = None;
                    job.eta = None;
                    job.status = JobStatus::Queued;
                }
                self.persist();
                self.schedule_try_start_next();
                Ok(())
            }
        }
    }

    pub fn move_to_top(&self, job_id: &str) {
        let min_created = self
            .jobs
            .iter()
            .filter(|r| matches!(r.status, JobStatus::Queued))
            .map(|r| r.created_at)
            .min()
            .unwrap_or(0);

        if let Some(mut job) = self.jobs.get_mut(job_id) {
            if matches!(job.status, JobStatus::Queued) {
                job.created_at = min_created.saturating_sub(1);
                self.persist();
            }
        }
    }

    pub async fn sync_settings(&self, settings: DownloadSettings) {
        if settings.concurrent_downloads > 0 {
            self.set_max_concurrent(settings.concurrent_downloads);
        }
        self.set_global_speed_limit(settings.download_speed_limit);
        *self.download_settings.write().await = settings;
        if !self.settings_synced.swap(true, Ordering::SeqCst) {
            self.settings_notify.notify_waiters();
        }
        info!("Download settings synced from frontend");
    }

    pub async fn clear_cookies(&self) {
        let mut settings = self.download_settings.write().await;
        settings.cookies_from_browser = String::new();
        settings.custom_cookies = String::new();
        info!("Cookies cleared from download settings");
    }

    pub async fn build_request_from_enqueue(
        &self,
        req: &EnqueueRequest,
    ) -> Result<DownloadRequest, BackendError> {
        // Wait for the frontend to sync settings at least once (up to 5s)
        if !self.settings_synced.load(Ordering::SeqCst) {
            let timeout = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.settings_notify.notified(),
            )
            .await;
            if timeout.is_err() {
                return Err(BackendError::Other(
                    "Timed out waiting for download settings to be synced from the frontend"
                        .to_string(),
                ));
            }
        }
        let s = self.download_settings.read().await;
        let o = &req.overrides;

        let mode = o
            .download_mode
            .as_deref()
            .unwrap_or(&s.default_download_mode);
        let audio_only = mode == "audio";

        let directory = if let Some(ref dir) = o.output_directory {
            dir.clone()
        } else if audio_only && s.use_audio_path && !s.audio_path.is_empty() {
            s.audio_path.clone()
        } else if !s.download_path.is_empty() {
            s.download_path.clone()
        } else {
            DownloadSettings::default_download_path()
        };

        if directory.is_empty() {
            return Err(BackendError::Other(
                "No download directory configured and could not determine system Downloads folder"
                    .to_string(),
            ));
        }

        let quality_str = o
            .video_quality
            .as_deref()
            .unwrap_or(&s.default_video_quality);
        let (format, max_height) = build_format_and_height(
            quality_str,
            audio_only,
            &s.preferred_video_codec,
            &s.preferred_audio_codec,
        );

        let proxy = build_proxy_config(&s.proxy_mode, &s.custom_proxy_url);

        let sponsorblock_remove = build_sponsorblock_string(
            o.sponsor_block.unwrap_or(s.sponsor_block),
            o.sponsor_block_categories.as_deref(),
            &s,
        );

        let cookies_from_browser = o
            .cookies_from_browser
            .clone()
            .or_else(|| non_empty_string(&s.cookies_from_browser));
        let custom_cookies = o
            .custom_cookies
            .clone()
            .or_else(|| non_empty_string(&s.custom_cookies));

        let speed_limit = if s.download_speed_limit > 0 {
            Some(s.download_speed_limit)
        } else {
            None
        };

        let embed_thumbnail = o.embed_thumbnail.unwrap_or(s.embed_thumbnail);
        let embed_metadata = !o.clear_metadata.unwrap_or(!s.embed_metadata);
        let embed_subtitles = o.embed_subtitles.unwrap_or(s.embed_subtitles);
        let subtitle_langs = o
            .subtitle_languages
            .clone()
            .or_else(|| non_empty_string(&s.subtitle_languages));

        let use_aria2 = o.use_aria2.unwrap_or(s.use_aria2);

        let request = DownloadRequest {
            url: req.url.clone(),
            backend: None,
            id: req.id.clone(),
            quality: QualitySettings {
                format,
                max_height,
                prefer_codec: None,
                audio_only,
                audio_format: {
                    let af = o.audio_format.as_deref().unwrap_or(&s.audio_format);
                    if audio_only && af != "any" && !af.is_empty() {
                        Some(af.to_string())
                    } else {
                        None
                    }
                },
            },
            output: OutputSettings {
                directory,
                filename_template: o.output_template.clone(),
                filename: o.filename.clone(),
            },
            options: DownloadOptions {
                cookies_from_browser,
                custom_cookies,
                proxy,
                speed_limit,
                embed_thumbnail,
                embed_metadata,
                embed_subtitles,
                subtitle_langs,
                sponsorblock_remove,
                youtube_player_client: non_empty_string(&s.youtube_player_client),
                aria2_connections: Some(s.aria2_connections),
                aria2_splits: Some(s.aria2_splits),
                aria2_min_split_size: non_empty_string(&s.aria2_min_split_size),
                aria2_disable_ipv6: Some(s.aria2_disable_ipv6),
                aria2_custom_args: non_empty_string(&s.aria2_custom_args),
                max_retries: Some(3),
                clip_ranges: o.clip_ranges.clone(),
                use_aria2,
                force_keyframes_at_cuts: false,
                torrent_selected_files: o.torrent_selected_files.clone(),
            },
            post_process: Vec::new(),
        };

        Ok(request)
    }

    pub async fn enqueue_url(
        self: &Arc<Self>,
        req: EnqueueRequest,
    ) -> Result<String, BackendError> {
        let playlist_id = req.playlist_id.clone();
        let playlist_title = req.playlist_title.clone();
        let playlist_index = req.playlist_index;
        let prefetched_title = req.overrides.title.clone();
        let prefetched_thumbnail = req.overrides.thumbnail.clone();
        let request = self.build_request_from_enqueue(&req).await?;
        let job_id = self.start_job(request).await?;

        if let Some(mut job) = self.jobs.get_mut(&job_id) {
            if playlist_id.is_some() || playlist_title.is_some() || playlist_index.is_some() {
                job.playlist_id = playlist_id;
                job.playlist_title = playlist_title;
                job.playlist_index = playlist_index;
            }
            if let Some(title) = prefetched_title {
                job.title = Some(title);
            }
            if let Some(thumb) = prefetched_thumbnail {
                job.thumbnail = Some(thumb);
            }
        }

        Ok(job_id)
    }

    pub async fn enqueue_playlist(
        self: &Arc<Self>,
        req: EnqueuePlaylistRequest,
    ) -> Result<Vec<String>, BackendError> {
        let mut ids = Vec::with_capacity(req.entries.len());
        for mut entry in req.entries {
            if entry.playlist_id.is_none() {
                entry.playlist_id = Some(req.playlist_id.clone());
            }
            if entry.playlist_title.is_none() {
                entry.playlist_title = Some(req.playlist_title.clone());
            }
            match self.enqueue_url(entry).await {
                Ok(id) => ids.push(id),
                Err(BackendError::DuplicateUrl(_)) => {
                    continue;
                }
                Err(e) => {
                    warn!("Failed to enqueue playlist entry: {}", e);
                }
            }
        }
        Ok(ids)
    }

    pub async fn pause_all(self: &Arc<Self>) {
        let active_ids: Vec<String> = self
            .jobs
            .iter()
            .filter(|r| r.status.is_active())
            .map(|r| r.id.clone())
            .collect();
        for job_id in active_ids {
            let _ = self.control_job(&job_id, JobControl::Pause).await;
        }
    }

    pub async fn resume_all(self: &Arc<Self>) {
        let paused_ids: Vec<String> = self
            .jobs
            .iter()
            .filter(|r| matches!(r.status, JobStatus::Paused))
            .map(|r| r.id.clone())
            .collect();
        for job_id in paused_ids {
            let _ = self.control_job(&job_id, JobControl::Resume).await;
        }
    }

    pub async fn cancel_all(self: &Arc<Self>) {
        let active_ids: Vec<String> = self
            .jobs
            .iter()
            .filter(|r| r.status.is_active())
            .map(|r| r.id.clone())
            .collect();
        for job_id in active_ids {
            let _ = self.control_job(&job_id, JobControl::Cancel).await;
        }
    }

    pub async fn retry_all_failed(self: &Arc<Self>) {
        let failed_ids: Vec<String> = self
            .jobs
            .iter()
            .filter(|r| matches!(r.status, JobStatus::Failed { .. }))
            .map(|r| r.id.clone())
            .collect();
        for job_id in failed_ids {
            let _ = self.control_job(&job_id, JobControl::Retry).await;
        }
    }

    // NOTE: control_playlist was removed — it was a non-functional stub.
    // When playlist-level batch ops are needed, store playlist_id on Job
    // and implement proper filtering. The frontend currently passes individual job IDs.

    pub fn clear_terminal_jobs(&self, filter: ClearFilter) {
        let to_remove: Vec<String> = self
            .jobs
            .iter()
            .filter(|r| match &filter {
                ClearFilter::Completed => matches!(r.status, JobStatus::Completed { .. }),
                ClearFilter::Failed => {
                    matches!(r.status, JobStatus::Failed { .. } | JobStatus::Cancelled)
                }
                ClearFilter::All => r.status.is_terminal(),
            })
            .map(|r| r.id.clone())
            .collect();
        for id in to_remove {
            self.jobs.remove(&id);
        }
        self.persist();
    }

    fn compute_job_speed_limit(&self, job_limit: Option<u64>) -> Option<u64> {
        if let Some(limit) = job_limit {
            if limit > 0 {
                return Some(limit);
            }
        }

        let global = self.global_speed_limit.load(Ordering::SeqCst);
        if global > 0 {
            let active = self.active_count.load(Ordering::SeqCst).max(1);
            return Some(global / active as u64);
        }

        None
    }

    pub(crate) async fn try_start_next(self: &Arc<Self>) {
        let max = self.max_concurrent.load(Ordering::SeqCst);

        loop {
            let slot = match ActiveJobSlot::try_acquire(&self.active_count, max) {
                Some(s) => s,
                None => return,
            };

            let next = self
                .jobs
                .iter()
                .filter(|r| matches!(r.status, JobStatus::Queued))
                .min_by_key(|r| r.created_at)
                .map(|r| r.value().clone());

            if let Some(job) = next {
                self.spawn_job(job, slot).await;
            } else {
                break;
            }
        }
    }

    fn schedule_try_start_next(self: &Arc<Self>) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            manager.try_start_next().await;
        });
    }

    async fn spawn_job(self: &Arc<Self>, job: Job, slot: ActiveJobSlot) {
        let job_id = job.id.clone();
        let backend_name = job.backend.clone();

        let registry = self.registry.read().await;
        let backend = match registry.get(&backend_name) {
            Some(b) => b,
            None => {
                let candidates = registry.candidates_for(&job.request.url);
                if candidates.is_empty() {
                    drop(registry);
                    let error = BackendError::UnsupportedUrl(job.request.url.clone());
                    self.update_job_status(
                        &job_id,
                        JobStatus::Failed {
                            error: error.to_string(),
                            retryable: false,
                        },
                    );
                    if let Some(mut j) = self.jobs.get_mut(&job_id) {
                        j.completed_at = Some(now_ms());
                        j.last_error = Some(error.to_string());
                    }
                    self.emit_event(JobEvent::Failed {
                        job_id: job_id.clone(),
                        error: error.to_string(),
                        retryable: false,
                    });
                    self.persist();
                    self.schedule_try_start_next();
                    return;
                }
                candidates[0].0.clone()
            }
        };
        drop(registry);

        self.update_job_status(&job_id, JobStatus::Downloading);
        if let Some(mut j) = self.jobs.get_mut(&job_id) {
            j.started_at = Some(now_ms());
            j.backend = backend.name().to_string();
        }

        self.emit_event(JobEvent::Started {
            job_id: job_id.clone(),
            backend: backend.name().to_string(),
        });

        let cancel_token = CancellationToken::new();
        let (progress_tx, progress_rx) = mpsc::unbounded_channel();
        let (metadata_tx, metadata_rx) = mpsc::unbounded_channel();

        let manager_clone = Arc::clone(self);
        let job_id_clone = job_id.clone();
        tokio::spawn(async move {
            manager_clone
                .handle_progress(job_id_clone, progress_rx)
                .await;
        });

        let manager_clone = Arc::clone(self);
        let job_id_clone = job_id.clone();
        tokio::spawn(async move {
            manager_clone
                .handle_metadata(job_id_clone, metadata_rx)
                .await;
        });

        let effective_speed_limit = self.compute_job_speed_limit(job.request.options.speed_limit);
        let ctx = SpawnContext {
            job: job.clone(),
            cancel_token: cancel_token.clone(),
            progress_tx,
            metadata_tx,
            effective_speed_limit,
        };

        let manager_clone = Arc::clone(self);
        let job_id_clone = job_id.clone();
        let task_handle = tokio::spawn(async move {
            match backend.spawn(ctx).await {
                Ok(output_path) => {
                    manager_clone.complete_job(&job_id_clone, output_path);
                }
                Err(BackendError::Cancelled) | Err(BackendError::Paused) => {
                    debug!("Job {} was cancelled/paused", job_id_clone);
                }
                Err(e) => {
                    manager_clone.handle_job_failure(&job_id_clone, e);
                }
            }
        });

        self.running.insert(
            job_id,
            RunningJob {
                cancel_token,
                task_handle,
                _slot: slot,
            },
        );
    }

    async fn handle_progress(
        self: &Arc<Self>,
        job_id: String,
        mut rx: mpsc::UnboundedReceiver<ProgressUpdate>,
    ) {
        while let Some(update) = rx.recv().await {
            let now = now_ms();
            let should_emit = self
                .last_progress_emit
                .get(&job_id)
                .map(|last| now.saturating_sub(*last) >= constants::PROGRESS_THROTTLE_MS)
                .unwrap_or(true);

            let mut current_progress = 0.0;
            let mut current_total: Option<u64> = None;

            if let Some(mut job) = self.jobs.get_mut(&job_id) {
                job.downloaded_bytes = update.downloaded_bytes;
                if let Some(t) = update.total_bytes {
                    if t > 0 {
                        job.total_bytes = Some(t);
                    }
                }
                job.speed = update.speed;
                job.eta = update.eta;

                if let Some(total) = job.total_bytes {
                    if total > 0 {
                        job.progress = (job.downloaded_bytes as f64 / total as f64) * 100.0;
                    }
                }
                current_progress = job.progress;
                current_total = job.total_bytes;
            }

            if should_emit {
                self.last_progress_emit.insert(job_id.clone(), now);
                if let Some(job) = self.jobs.get(&job_id) {
                    debug!(
                        "Progress event for {}: {:.1}%, downloaded={}, total={:?}, speed={:?}",
                        job_id,
                        current_progress,
                        update.downloaded_bytes,
                        current_total,
                        update.speed
                    );
                    self.emit_event(JobEvent::Progress {
                        job_id: job_id.clone(),
                        progress: job.progress,
                        downloaded_bytes: update.downloaded_bytes,
                        total_bytes: job.total_bytes,
                        speed: update.speed,
                        eta: update.eta,
                    });
                }
            }
        }
    }

    async fn handle_metadata(
        self: &Arc<Self>,
        job_id: String,
        mut rx: mpsc::UnboundedReceiver<crate::orchestrator::backends::MetadataEvent>,
    ) {
        use crate::orchestrator::backends::MetadataEvent;
        while let Some(event) = rx.recv().await {
            match event {
                MetadataEvent::Patch(patch) => {
                    self.apply_url_info_patch(&job_id, patch);
                }
                MetadataEvent::PostProcessing => {
                    self.set_post_processing(&job_id);
                }
                MetadataEvent::FilePath(path) => {
                    info!("FilePathResolved for {}: {}", job_id, path);
                    // Emit to frontend so play-while-downloading can work.
                    self.emit_event(JobEvent::FilePathResolved {
                        job_id: job_id.clone(),
                        output_path: path,
                    });
                }
            }
        }
    }

    fn complete_job(self: &Arc<Self>, job_id: &str, output_path: String) {
        self.running.remove(job_id);

        let title_from_filename = std::path::Path::new(&output_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|name| !name.starts_with("http") && !name.contains("%("));

        let path_meta = std::fs::metadata(&output_path).ok();
        let is_directory = path_meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);

        let (actual_filesize, file_count) = if is_directory {
            // Recursively compute total size and file count for directory downloads
            let mut total_size: u64 = 0;
            let mut count: u32 = 0;
            if let Ok(entries) = std::fs::read_dir(&output_path) {
                for entry in entries.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        if meta.is_file() {
                            total_size += meta.len();
                            count += 1;
                        }
                    }
                }
            }
            (Some(total_size), Some(count))
        } else {
            (path_meta.as_ref().map(|m| m.len()), None)
        };

        let extension = if is_directory {
            String::new()
        } else {
            std::path::Path::new(&output_path)
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default()
        };

        let (title, thumbnail, filesize, history_item) =
            if let Some(mut job) = self.jobs.get_mut(job_id) {
                job.status = JobStatus::Completed {
                    output_path: output_path.clone(),
                };
                job.completed_at = Some(now_ms());
                job.progress = 100.0;

                let filesize = actual_filesize.or(job.total_bytes);
                if let Some(fs) = filesize {
                    job.total_bytes = Some(fs);
                }

                if job.title.is_none() {
                    job.title = title_from_filename;
                }

                let item_type = Self::resolve_item_type(&job);

                let history = HistoryItem {
                    id: uuid::Uuid::new_v4().to_string(),
                    url: job.request.url.clone(),
                    title: job.title.clone().unwrap_or_default(),
                    author: job.author.clone().unwrap_or_default(),
                    author_url: job.author_url.clone(),
                    thumbnail: job.thumbnail.clone().unwrap_or_default(),
                    extension: extension.clone(),
                    size: filesize.unwrap_or(0),
                    duration: job.duration.unwrap_or(0.0),
                    file_path: output_path.clone(),
                    downloaded_at: now_ms(),
                    item_type: item_type.to_string(),
                    playlist_id: job.playlist_id.clone(),
                    playlist_title: job.playlist_title.clone(),
                    playlist_index: job.playlist_index,
                    converted_format: job.request.quality.audio_format.clone(),
                    download_source: Some(job.backend.clone()),
                    is_favourite: false,
                    is_directory,
                    file_count,
                    podcast_path: None,
                    podcast_subtitle_path: None,
                    podcast_status: None,
                };

                (
                    job.title.clone(),
                    job.thumbnail.clone(),
                    filesize,
                    Some(history),
                )
            } else {
                (title_from_filename, None, actual_filesize, None)
            };

        if let Some(item) = history_item {
            let history = self.history.clone();
            let stats = self.stats.clone();
            let app = self.app.clone();
            let completion_size = filesize.unwrap_or(0);
            tauri::async_runtime::spawn(async move {
                let added = history.add(item).await;
                stats.record_completion(completion_size).await;
                let _ = app.emit("history-item-added", &added);
                let history_stats = history.compute_stats().await;
                let _ = app.emit("history-stats-changed", &history_stats);
                #[cfg(desktop)]
                {
                    if let Err(e) = crate::tray::rebuild_menu_async(&app).await {
                        tracing::warn!("[Tray] Failed to rebuild after download: {}", e);
                    }
                    // Auto-trigger podcast generation for YouTube downloads if enabled
                    crate::orchestrator::podcast::maybe_auto_generate_podcast(
                        app.clone(),
                        history.clone(),
                        added,
                    )
                    .await;
                }
            });
        }

        self.emit_event(JobEvent::StatusChanged {
            job_id: job_id.to_string(),
            status: JobStatus::Completed {
                output_path: output_path.clone(),
            },
        });

        self.emit_event(JobEvent::Completed {
            job_id: job_id.to_string(),
            output_path,
            title,
            thumbnail,
            filesize,
        });

        self.last_progress_emit.remove(job_id);
        self.persist();
        self.schedule_try_start_next();
    }

    /// Map content_type + job metadata into the history item_type string.
    fn resolve_item_type(job: &Job) -> &'static str {
        // If we have an explicit content_type from resolve, use it
        if let Some(ct) = &job.content_type {
            return match ct {
                ContentType::Audio => "audio",
                ContentType::Image => "image",
                ContentType::Gallery => "gallery",
                ContentType::Torrent => "torrent",
                ContentType::File => "file",
                ContentType::Video => {
                    if job.request.quality.audio_only {
                        "audio"
                    } else {
                        "video"
                    }
                }
                _ => "video",
            };
        }

        // Infer from backend name for backends that don't send content_type patches
        match job.backend.as_str() {
            "gallery-dl" => "image",
            "aria2" => {
                let url = &job.request.url;
                if url.starts_with("magnet:")
                    || url.ends_with(".torrent")
                    || url.contains(".torrent?")
                {
                    "torrent"
                } else {
                    "file"
                }
            }
            _ => {
                if job.request.quality.audio_only {
                    "audio"
                } else {
                    "video"
                }
            }
        }
    }

    fn handle_job_failure(self: &Arc<Self>, job_id: &str, error: BackendError) {
        self.running.remove(job_id);

        let retryable = error.is_retryable();

        // Read all needed job state in a single DashMap access to avoid TOCTOU races.
        let job_state = self.jobs.get(job_id).map(|j| {
            (
                j.request
                    .options
                    .max_retries
                    .unwrap_or(constants::DEFAULT_MAX_RETRIES),
                j.retry_count,
                j.backend.clone(),
            )
        });

        let Some((max_retries, retry_count, current_backend)) = job_state else {
            warn!("handle_job_failure: job {} not found", job_id);
            return;
        };

        if retryable && retry_count < max_retries {
            let is_proxy_error = error.is_proxy_error();
            if let Some(mut job) = self.jobs.get_mut(job_id) {
                job.retry_count += 1;
                job.last_error = Some(error.to_string());
                job.status = JobStatus::Queued;
                // On a proxy error, set skip_proxy so the next attempt omits --proxy.
                if is_proxy_error && !job.skip_proxy {
                    warn!(
                        "Proxy error for job {} — next retry will skip proxy",
                        job_id
                    );
                    job.skip_proxy = true;
                }
            }

            let delay_secs = 3 * 2u64.pow(retry_count);
            let manager = Arc::clone(self);
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
                manager.try_start_next().await;
            });
        } else {
            // Max retries reached - try fallback to next backend.
            // This must be async-safe (no block_on inside runtime).
            let manager = Arc::clone(self);
            let job_id = job_id.to_string();
            let current_backend = current_backend.clone();
            let is_proxy_error = error.is_proxy_error();
            let error_str = error.to_string();

            tokio::spawn(async move {
                let fallback_backend = manager
                    .find_fallback_backend(&job_id, &current_backend)
                    .await;

                if let Some(fallback) = fallback_backend {
                    info!(
                        "Falling back from {} to {} for job {}",
                        current_backend, fallback, job_id
                    );
                    if let Some(mut job) = manager.jobs.get_mut(&job_id) {
                        job.backend = fallback;
                        job.retry_count = 0;
                        job.last_error = Some(format!("Previous backend failed: {}", error_str));
                        job.status = JobStatus::Queued;
                    }
                    manager.schedule_try_start_next();
                } else {
                    // Build a user-facing error message. Proxy errors get a more helpful hint.
                    let final_error = if is_proxy_error {
                        format!(
                            "All backends failed — proxy may be blocking the connection. {}",
                            error_str
                        )
                    } else {
                        format!("All backends failed: {}", error_str)
                    };

                    manager.update_job_status(
                        &job_id,
                        JobStatus::Failed {
                            error: final_error.clone(),
                            retryable,
                        },
                    );

                    if let Some(mut job) = manager.jobs.get_mut(&job_id) {
                        job.completed_at = Some(now_ms());
                    }

                    manager.stats.record_failure().await;

                    manager.emit_event(JobEvent::Failed {
                        job_id: job_id.clone(),
                        error: final_error,
                        retryable,
                    });

                    manager.schedule_try_start_next();
                }

                manager.last_progress_emit.remove(&job_id);
                manager.persist();
            });

            return;
        }

        self.last_progress_emit.remove(job_id);
        self.persist();
    }

    async fn find_fallback_backend(&self, job_id: &str, current: &str) -> Option<String> {
        use crate::orchestrator::backends::{has_file_extension, DIRECT_FILE_EXTENSIONS};

        let job = self.jobs.get(job_id)?;
        let url = job.request.url.clone();
        let request = job.request.clone();
        drop(job);

        // Only allow the "direct" backend as a fallback when the URL looks like a real
        // file download. Without this guard, platform URLs (YouTube, etc.) would fall
        // back to direct HTTP GET and download the HTML page instead of the video.
        let url_has_file_ext = has_file_extension(&url, DIRECT_FILE_EXTENSIONS);

        let registry = self.registry.read().await;
        let candidates = registry.candidates_for(&url);

        // First pass: prefer a backend that is capability-compatible with the request.
        for (backend, _priority) in &candidates {
            let name = backend.name();
            if name == current {
                continue;
            }
            if name == "direct" && !url_has_file_ext {
                debug!(
                    "Skipping 'direct' fallback for job {} — URL has no recognized file extension",
                    job_id
                );
                continue;
            }
            let caps = backend.capabilities();
            if request_compatible_with(&request, &caps) {
                return Some(name.to_string());
            }
        }

        // Second pass: accept any compatible backend.
        for (backend, _priority) in &candidates {
            let name = backend.name();
            if name == current {
                continue;
            }
            if name == "direct" && !url_has_file_ext {
                continue;
            }
            return Some(name.to_string());
        }

        None
    }

    fn update_job_status(&self, job_id: &str, status: JobStatus) {
        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.status = status.clone();
        }
        self.emit_event(JobEvent::StatusChanged {
            job_id: job_id.to_string(),
            status,
        });
    }

    fn emit_event(&self, event: JobEvent) {
        if let Err(e) = self.app.emit("job-event", &event) {
            error!("Failed to emit job event: {}", e);
        }
    }

    fn persist(&self) {
        self.persist_notify.notify_one();
    }

    pub fn set_post_processing(&self, job_id: &str) {
        self.update_job_status(job_id, JobStatus::PostProcessing);
    }

    pub async fn get_all_capabilities(
        &self,
    ) -> Vec<crate::orchestrator::backends::BackendCapabilities> {
        let registry = self.registry.read().await;
        registry.all_capabilities()
    }
}

impl Drop for JobManager {
    fn drop(&mut self) {
        info!("JobManager dropping — flushing final state");

        let running_ids: Vec<String> = self.running.iter().map(|r| r.key().clone()).collect();
        for job_id in &running_ids {
            if let Some((_, running)) = self.running.remove(job_id) {
                running.cancel_token.cancel();
                running.task_handle.abort();
            }
            // Mark in-flight jobs as Paused so they resume on next launch.
            if let Some(mut job) = self.jobs.get_mut(job_id) {
                if matches!(
                    job.status,
                    JobStatus::Downloading | JobStatus::Resolving | JobStatus::PostProcessing
                ) {
                    job.status = JobStatus::Paused;
                }
            }
        }

        let resolve_ids: Vec<String> = self
            .active_resolves
            .iter()
            .map(|r| r.key().clone())
            .collect();
        for id in resolve_ids {
            if let Some((_, token)) = self.active_resolves.remove(&id) {
                token.cancel();
            }
        }

        self.persist_cancel.cancel();

        // Final synchronous flush — guaranteed durable write before process exit.
        let snapshot: Vec<Job> = self.jobs.iter().map(|r| r.value().clone()).collect();
        if let Err(e) = self.store.save_jobs_sync(&snapshot) {
            error!("Failed to flush jobs on shutdown: {}", e);
        } else {
            info!("Jobs flushed to disk on shutdown");
        }
    }
}

/// Check whether a backend's capabilities satisfy the download request's needs.
///
/// Returns `true` if the request doesn't require anything the backend can't do.
/// This is intentionally lenient — a request with no special options is compatible
/// with every backend.
fn request_compatible_with(
    request: &DownloadRequest,
    caps: &crate::orchestrator::backends::BackendCapabilities,
) -> bool {
    let opts = &request.options;
    let quality = &request.quality;

    if (quality.audio_only
        || quality.max_height.is_some()
        || quality.prefer_codec.is_some()
        || quality.audio_format.is_some()
        || quality.format != "best")
        && !caps.format_selection
    {
        return false;
    }

    if (opts.embed_subtitles || opts.subtitle_langs.is_some()) && !caps.subtitles {
        return false;
    }

    if opts.proxy.as_ref().is_some_and(|p| p.enabled) && !caps.proxy {
        return false;
    }

    if opts.speed_limit.is_some_and(|l| l > 0) && !caps.speed_limit {
        return false;
    }

    if (opts.cookies_from_browser.is_some() || opts.custom_cookies.is_some()) && !caps.cookies {
        return false;
    }

    if !request.post_process.is_empty() && !caps.post_processing {
        return false;
    }

    if opts.clip_ranges.as_ref().is_some_and(|c| !c.is_empty()) && !caps.post_processing {
        return false;
    }

    if opts.sponsorblock_remove.is_some() && !caps.post_processing {
        return false;
    }

    if (request.url.starts_with("magnet:")
        || request.url.ends_with(".torrent")
        || request.url.contains(".torrent?"))
        && !caps.torrent_magnet
    {
        return false;
    }

    true
}

fn non_empty_string(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn quality_to_max_height(quality: &str) -> Option<u32> {
    match quality {
        "best" | "max" | "" => None,
        "4k" => Some(2160),
        _ => {
            // Parse numeric part from "1080p", "720p", etc.
            quality.trim_end_matches('p').parse::<u32>().ok()
        }
    }
}

fn video_codec_filter(codec: &str) -> &'static str {
    match codec {
        "h264" => "[vcodec^=avc]",
        "h265" => "[vcodec^=hev]",
        "vp9" => "[vcodec^=vp9]",
        "av1" => "[vcodec^=av01]",
        _ => "",
    }
}

fn audio_codec_filter(codec: &str) -> &'static str {
    match codec {
        "opus" => "[acodec=opus]",
        "aac" => "[acodec^=mp4a]",
        "mp3" => "[acodec^=mp3]",
        "vorbis" => "[acodec=vorbis]",
        _ => "",
    }
}

fn build_format_and_height(
    quality: &str,
    audio_only: bool,
    preferred_video_codec: &str,
    preferred_audio_codec: &str,
) -> (String, Option<u32>) {
    let known_presets = [
        "best", "max", "4k", "1440p", "1080p", "720p", "480p", "360p", "240p",
    ];
    let is_preset = quality.is_empty() || known_presets.contains(&quality);

    if !is_preset {
        // Treat as raw yt-dlp format string
        return (quality.to_string(), None);
    }

    let max_height = quality_to_max_height(quality);

    if audio_only {
        let af = audio_codec_filter(preferred_audio_codec);
        let format = if af.is_empty() {
            "bestaudio/best".to_string()
        } else {
            format!("bestaudio{af}/bestaudio/best")
        };
        return (format, None); // max_height is irrelevant for audio-only
    }

    let vf = video_codec_filter(preferred_video_codec);
    let af = audio_codec_filter(preferred_audio_codec);

    let format = if !vf.is_empty() || !af.is_empty() {
        let pv = format!("bestvideo{vf}");
        let pa = format!("bestaudio{af}");
        format!("{pv}+{pa}/{pv}+bestaudio/bestvideo+{pa}/bestvideo+bestaudio/best")
    } else {
        "bestvideo+bestaudio/best".to_string()
    };

    (format, max_height)
}

fn build_proxy_config(mode: &str, custom_url: &str) -> Option<ProxyConfig> {
    match mode {
        "system" => Some(ProxyConfig {
            enabled: true,
            url: None,
            username: None,
            password: None,
        }),
        "custom" => Some(ProxyConfig {
            enabled: true,
            url: non_empty_string(custom_url),
            username: None,
            password: None,
        }),
        _ => None,
    }
}

fn build_sponsorblock_string(
    enabled: bool,
    categories_override: Option<&[String]>,
    settings: &DownloadSettings,
) -> Option<String> {
    if !enabled {
        return None;
    }

    if let Some(cats) = categories_override {
        if cats.is_empty() {
            return Some("sponsor".to_string());
        }
        return Some(cats.join(","));
    }

    let mut cats = Vec::new();
    if settings.sponsor_block_skip_sponsors {
        cats.push("sponsor");
    }
    if settings.sponsor_block_skip_intros {
        cats.push("intro");
    }
    if settings.sponsor_block_skip_self_promo {
        cats.push("selfpromo");
    }
    if settings.sponsor_block_skip_interaction {
        cats.push("interaction");
    }
    if cats.is_empty() {
        cats.push("sponsor");
    }
    Some(cats.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::backends::BackendCapabilities;
    use pretty_assertions::assert_eq;

    fn full_caps() -> BackendCapabilities {
        BackendCapabilities {
            name: "test".to_string(),
            streaming_resolve: true,
            playlists: true,
            pause_resume: true,
            multi_connection: true,
            format_selection: true,
            subtitles: true,
            speed_limit: true,
            proxy: true,
            cookies: true,
            torrent_magnet: true,
            post_processing: true,
        }
    }

    fn minimal_caps() -> BackendCapabilities {
        BackendCapabilities {
            name: "minimal".to_string(),
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

    fn basic_request() -> DownloadRequest {
        DownloadRequest {
            url: "https://example.com/file.mp4".into(),
            backend: None,
            id: None,
            quality: QualitySettings {
                format: "best".into(),
                ..QualitySettings::default()
            },
            output: OutputSettings {
                directory: "/tmp".into(),
                filename_template: None,
                filename: None,
            },
            options: DownloadOptions::default(),
            post_process: vec![],
        }
    }

    #[test]
    fn test_compatible_basic_request_any_backend() {
        let req = basic_request();
        assert!(request_compatible_with(&req, &full_caps()));
        assert!(request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_audio_only_needs_format_selection() {
        let mut req = basic_request();
        req.quality.audio_only = true;
        assert!(request_compatible_with(&req, &full_caps()));
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_max_height_needs_format_selection() {
        let mut req = basic_request();
        req.quality.max_height = Some(1080);
        assert!(request_compatible_with(&req, &full_caps()));
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_subtitles_need_support() {
        let mut req = basic_request();
        req.options.embed_subtitles = true;
        assert!(request_compatible_with(&req, &full_caps()));
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_proxy_needs_support() {
        let mut req = basic_request();
        req.options.proxy = Some(ProxyConfig {
            enabled: true,
            url: Some("http://proxy:8080".into()),
            username: None,
            password: None,
        });
        assert!(request_compatible_with(&req, &full_caps()));
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_disabled_proxy_ok() {
        let mut req = basic_request();
        req.options.proxy = Some(ProxyConfig {
            enabled: false,
            url: None,
            username: None,
            password: None,
        });
        // Disabled proxy should be fine even without proxy support
        assert!(request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_speed_limit_needs_support() {
        let mut req = basic_request();
        req.options.speed_limit = Some(1_000_000);
        assert!(request_compatible_with(&req, &full_caps()));
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_zero_speed_limit_ok() {
        let mut req = basic_request();
        req.options.speed_limit = Some(0);
        assert!(request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_cookies_need_support() {
        let mut req = basic_request();
        req.options.cookies_from_browser = Some("chrome".into());
        assert!(request_compatible_with(&req, &full_caps()));
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_post_process_needs_support() {
        let mut req = basic_request();
        req.post_process.push(PostProcessStep::FFmpegConvert {
            target_format: "mp4".into(),
            audio_only: false,
            extra_args: None,
        });
        assert!(request_compatible_with(&req, &full_caps()));
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_clip_ranges_need_post_processing() {
        let mut req = basic_request();
        req.options.clip_ranges = Some(vec![ClipRange {
            id: "clip1".into(),
            start: 10.0,
            end: 30.0,
        }]);
        assert!(request_compatible_with(&req, &full_caps()));
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_sponsorblock_needs_post_processing() {
        let mut req = basic_request();
        req.options.sponsorblock_remove = Some("sponsor".into());
        assert!(request_compatible_with(&req, &full_caps()));
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_magnet_needs_torrent_support() {
        let mut req = basic_request();
        req.url = "magnet:?xt=urn:btih:abc123".into();
        assert!(request_compatible_with(&req, &full_caps()));
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_torrent_url() {
        let mut req = basic_request();
        req.url = "https://example.com/file.torrent".into();
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_torrent_url_with_params() {
        let mut req = basic_request();
        req.url = "https://example.com/file.torrent?token=abc".into();
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_compatible_non_default_format_needs_selection() {
        let mut req = basic_request();
        req.quality.format = "bestvideo[height<=720]+bestaudio".into();
        assert!(!request_compatible_with(&req, &minimal_caps()));
    }

    #[test]
    fn test_format_default() {
        let (fmt, height) = build_format_and_height("best", false, "any", "any");
        assert_eq!(fmt, "bestvideo+bestaudio/best");
        assert_eq!(height, None);
    }

    #[test]
    fn test_format_empty_preset() {
        let (fmt, height) = build_format_and_height("", false, "any", "any");
        assert_eq!(fmt, "bestvideo+bestaudio/best");
        assert_eq!(height, None);
    }

    #[test]
    fn test_format_1080p() {
        let (_, height) = build_format_and_height("1080p", false, "any", "any");
        assert_eq!(height, Some(1080));
    }

    #[test]
    fn test_format_4k() {
        let (_, height) = build_format_and_height("4k", false, "any", "any");
        assert_eq!(height, Some(2160));
    }

    #[test]
    fn test_format_audio_only() {
        let (fmt, _) = build_format_and_height("best", true, "any", "any");
        assert_eq!(fmt, "bestaudio/best");
    }

    #[test]
    fn test_format_audio_only_with_codec() {
        let (fmt, _) = build_format_and_height("best", true, "any", "opus");
        assert!(fmt.contains("[acodec=opus]"));
        assert!(fmt.starts_with("bestaudio[acodec=opus]"));
    }

    #[test]
    fn test_format_with_video_codec() {
        let (fmt, _) = build_format_and_height("best", false, "h264", "any");
        assert!(fmt.contains("[vcodec^=avc]"));
    }

    #[test]
    fn test_format_with_both_codecs() {
        let (fmt, _) = build_format_and_height("best", false, "av1", "opus");
        assert!(fmt.contains("[vcodec^=av01]"));
        assert!(fmt.contains("[acodec=opus]"));
    }

    #[test]
    fn test_format_raw_format_string_passthrough() {
        let (fmt, height) =
            build_format_and_height("bestvideo[height<=480]+bestaudio/best", false, "any", "any");
        assert_eq!(fmt, "bestvideo[height<=480]+bestaudio/best");
        assert_eq!(height, None); // raw strings don't produce height
    }

    #[test]
    fn test_proxy_none() {
        assert!(build_proxy_config("none", "").is_none());
    }

    #[test]
    fn test_proxy_system() {
        let p = build_proxy_config("system", "").unwrap();
        assert!(p.enabled);
        assert!(p.url.is_none());
    }

    #[test]
    fn test_proxy_custom() {
        let p = build_proxy_config("custom", "http://proxy:8080").unwrap();
        assert!(p.enabled);
        assert_eq!(p.url.as_deref(), Some("http://proxy:8080"));
    }

    #[test]
    fn test_proxy_custom_empty_url() {
        let p = build_proxy_config("custom", "  ").unwrap();
        assert!(p.enabled);
        assert!(p.url.is_none()); // empty string becomes None via non_empty_string
    }

    #[test]
    fn test_sponsorblock_disabled() {
        let settings = DownloadSettings::default();
        assert!(build_sponsorblock_string(false, None, &settings).is_none());
    }

    #[test]
    fn test_sponsorblock_default_settings() {
        let settings = DownloadSettings::default();
        // Default has sponsor_block_skip_sponsors=true, rest false
        let result = build_sponsorblock_string(true, None, &settings).unwrap();
        assert_eq!(result, "sponsor");
    }

    #[test]
    fn test_sponsorblock_multiple_categories() {
        let mut settings = DownloadSettings::default();
        settings.sponsor_block_skip_sponsors = true;
        settings.sponsor_block_skip_intros = true;
        settings.sponsor_block_skip_self_promo = true;
        let result = build_sponsorblock_string(true, None, &settings).unwrap();
        assert_eq!(result, "sponsor,intro,selfpromo");
    }

    #[test]
    fn test_sponsorblock_all_categories() {
        let mut settings = DownloadSettings::default();
        settings.sponsor_block_skip_sponsors = true;
        settings.sponsor_block_skip_intros = true;
        settings.sponsor_block_skip_self_promo = true;
        settings.sponsor_block_skip_interaction = true;
        let result = build_sponsorblock_string(true, None, &settings).unwrap();
        assert_eq!(result, "sponsor,intro,selfpromo,interaction");
    }

    #[test]
    fn test_sponsorblock_none_selected_falls_back_to_sponsor() {
        let mut settings = DownloadSettings::default();
        settings.sponsor_block_skip_sponsors = false;
        let result = build_sponsorblock_string(true, None, &settings).unwrap();
        assert_eq!(result, "sponsor");
    }

    #[test]
    fn test_sponsorblock_override_categories() {
        let settings = DownloadSettings::default();
        let cats = vec!["music_offtopic".to_string(), "filler".to_string()];
        let result = build_sponsorblock_string(true, Some(&cats), &settings).unwrap();
        assert_eq!(result, "music_offtopic,filler");
    }

    #[test]
    fn test_sponsorblock_override_empty_falls_back() {
        let settings = DownloadSettings::default();
        let cats: Vec<String> = vec![];
        let result = build_sponsorblock_string(true, Some(&cats), &settings).unwrap();
        assert_eq!(result, "sponsor");
    }

    #[test]
    fn test_quality_to_max_height_presets() {
        assert_eq!(quality_to_max_height("best"), None);
        assert_eq!(quality_to_max_height("max"), None);
        assert_eq!(quality_to_max_height(""), None);
        assert_eq!(quality_to_max_height("4k"), Some(2160));
        assert_eq!(quality_to_max_height("1080p"), Some(1080));
        assert_eq!(quality_to_max_height("720p"), Some(720));
        assert_eq!(quality_to_max_height("480p"), Some(480));
        assert_eq!(quality_to_max_height("360p"), Some(360));
        assert_eq!(quality_to_max_height("240p"), Some(240));
    }

    #[test]
    fn test_video_codec_filters() {
        assert_eq!(video_codec_filter("h264"), "[vcodec^=avc]");
        assert_eq!(video_codec_filter("h265"), "[vcodec^=hev]");
        assert_eq!(video_codec_filter("vp9"), "[vcodec^=vp9]");
        assert_eq!(video_codec_filter("av1"), "[vcodec^=av01]");
        assert_eq!(video_codec_filter("unknown"), "");
        assert_eq!(video_codec_filter("any"), "");
    }

    #[test]
    fn test_audio_codec_filters() {
        assert_eq!(audio_codec_filter("opus"), "[acodec=opus]");
        assert_eq!(audio_codec_filter("aac"), "[acodec^=mp4a]");
        assert_eq!(audio_codec_filter("mp3"), "[acodec^=mp3]");
        assert_eq!(audio_codec_filter("vorbis"), "[acodec=vorbis]");
        assert_eq!(audio_codec_filter("unknown"), "");
        assert_eq!(audio_codec_filter("any"), "");
    }

    #[test]
    fn test_non_empty_string() {
        assert_eq!(non_empty_string("hello"), Some("hello".to_string()));
        assert_eq!(non_empty_string("  spaced  "), Some("spaced".to_string()));
        assert_eq!(non_empty_string(""), None);
        assert_eq!(non_empty_string("   "), None);
    }

    #[test]
    fn test_audio_only_max_height_is_none() {
        let (_, height) = build_format_and_height("1080p", true, "any", "any");
        assert_eq!(height, None, "audio-only should never return max_height");
    }

    #[test]
    fn test_video_max_height_is_set() {
        let (_, height) = build_format_and_height("1080p", false, "any", "any");
        assert_eq!(height, Some(1080));
    }

    #[test]
    fn test_audio_only_4k_still_no_height() {
        let (_, height) = build_format_and_height("4k", true, "any", "opus");
        assert_eq!(height, None);
    }
}
