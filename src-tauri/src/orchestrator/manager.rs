use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
#[allow(unused_imports)]
use log::{debug, error, info, warn};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

use crate::orchestrator::backends::{Backend, BackendRegistry, SpawnContext};
use crate::orchestrator::store::JobStore;
use crate::orchestrator::types::*;

struct RunningJob {
    cancel_token: CancellationToken,
    task_handle: tokio::task::JoinHandle<()>,
}

pub struct JobManager {
    app: AppHandle,
    store: Arc<JobStore>,

    registry: RwLock<BackendRegistry>,
    jobs: DashMap<String, Job>,
    running: DashMap<String, RunningJob>,
    last_progress_emit: DashMap<String, u64>,

    max_concurrent: AtomicU32,
    active_count: AtomicU32,
    global_speed_limit: AtomicU64,
}

impl JobManager {
    pub fn new(app: AppHandle, store: Arc<JobStore>) -> Arc<Self> {
        Arc::new(Self {
            app,
            store,
            registry: RwLock::new(BackendRegistry::new()),
            jobs: DashMap::new(),
            running: DashMap::new(),
            last_progress_emit: DashMap::new(),
            max_concurrent: AtomicU32::new(constants::DEFAULT_MAX_CONCURRENT),
            active_count: AtomicU32::new(0),
            global_speed_limit: AtomicU64::new(0),
        })
    }

    pub async fn register_backend(&self, backend: Arc<dyn Backend>) {
        let mut registry = self.registry.write().await;
        registry.register(backend);
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

    pub fn update_job_title(&self, job_id: &str, title: &str) {
        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.title = Some(title.to_string());
        }
    }

    pub fn update_job_metadata(&self, job_id: &str, title: Option<&str>, thumbnail: Option<&str>) {
        if let Some(mut job) = self.jobs.get_mut(job_id) {
            if let Some(t) = title {
                job.title = Some(t.to_string());
            }
            if let Some(th) = thumbnail {
                job.thumbnail = Some(th.to_string());
            }
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

    pub async fn resolve_url(
        &self,
        url: &str,
        settings: ResolveSettings,
    ) -> Result<ResolveResult, BackendError> {
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

    pub async fn start_job(
        self: &Arc<Self>,
        request: DownloadRequest,
    ) -> Result<String, BackendError> {
        let job_id = uuid::Uuid::new_v4().to_string();

        let backend = if let Some(ref b) = request.backend {
            b.clone()
        } else {
            let registry = self.registry.read().await;
            let candidates = registry.candidates_for(&request.url);
            candidates
                .first()
                .map(|(b, _)| b.name().to_string())
                .unwrap_or_else(|| "ytdlp".to_string())
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
            post_process_index: 0,
        };

        self.jobs.insert(job_id.clone(), job.clone());
        self.emit_event(JobEvent::Added { job });
        self.persist();
        self.schedule_try_start_next();

        Ok(job_id)
    }

    pub async fn control_job(
        self: &Arc<Self>,
        job_id: &str,
        action: JobControl,
    ) -> Result<(), BackendError> {
        match action {
            JobControl::Cancel => {
                // On Android, delegate to DownloadService (for ytdlp) and Aria2 cancel
                #[cfg(target_os = "android")]
                {
                    // Try both - one will succeed depending on which backend is running
                    if let Err(e) = crate::orchestrator::backends::cancel_android_download(job_id) {
                        warn!("Failed to cancel Android ytdlp download {}: {}", job_id, e);
                    }
                    if let Err(e) = crate::orchestrator::backends::cancel_aria2_android(job_id) {
                        warn!("Failed to cancel Android aria2 download {}: {}", job_id, e);
                    }
                }

                if let Some((_, running)) = self.running.remove(job_id) {
                    running.cancel_token.cancel();
                    running.task_handle.abort();
                    self.active_count.fetch_sub(1, Ordering::SeqCst);
                }
                self.update_job_status(job_id, JobStatus::Cancelled);
                self.emit_event(JobEvent::Cancelled {
                    job_id: job_id.to_string(),
                });
                self.last_progress_emit.remove(job_id);
                self.persist();
                self.schedule_try_start_next();
                Ok(())
            }
            JobControl::Pause => {
                // On Android, delegate to DownloadService
                #[cfg(target_os = "android")]
                {
                    if let Err(e) = crate::orchestrator::backends::pause_android_download(job_id) {
                        warn!("Failed to pause Android download {}: {}", job_id, e);
                    }
                }

                if let Some((_, running)) = self.running.remove(job_id) {
                    running.cancel_token.cancel();
                    running.task_handle.abort();
                    self.active_count.fetch_sub(1, Ordering::SeqCst);
                }
                self.update_job_status(job_id, JobStatus::Paused);
                self.emit_event(JobEvent::Paused {
                    job_id: job_id.to_string(),
                });
                self.last_progress_emit.remove(job_id);
                self.persist();
                self.schedule_try_start_next();
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
        let active = self.active_count.load(Ordering::SeqCst);
        if active >= max {
            return;
        }

        let next = self
            .jobs
            .iter()
            .filter(|r| matches!(r.status, JobStatus::Queued))
            .min_by_key(|r| r.created_at)
            .map(|r| r.value().clone());

        if let Some(job) = next {
            self.spawn_job(job).await;
        }
    }

    fn schedule_try_start_next(self: &Arc<Self>) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            manager.try_start_next().await;
        });
    }

    async fn spawn_job(self: &Arc<Self>, job: Job) {
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

        self.active_count.fetch_add(1, Ordering::SeqCst);
        self.emit_event(JobEvent::Started {
            job_id: job_id.clone(),
            backend: backend.name().to_string(),
        });

        let cancel_token = CancellationToken::new();
        let (progress_tx, progress_rx) = mpsc::unbounded_channel();

        let manager_clone = Arc::clone(self);
        let job_id_clone = job_id.clone();
        tokio::spawn(async move {
            manager_clone
                .handle_progress(job_id_clone, progress_rx)
                .await;
        });

        let effective_speed_limit = self.compute_job_speed_limit(job.request.options.speed_limit);
        let ctx = SpawnContext {
            job: job.clone(),
            cancel_token: cancel_token.clone(),
            progress_tx,
            effective_speed_limit,
        };

        let manager_clone = Arc::clone(self);
        let job_id_clone = job_id.clone();
        let task_handle = tokio::spawn(async move {
            match backend.spawn(ctx).await {
                Ok(output_path) => {
                    manager_clone.complete_job(&job_id_clone, output_path);
                }
                Err(BackendError::Cancelled) => {
                    debug!("Job {} was cancelled", job_id_clone);
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
                    info!(
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

    fn complete_job(self: &Arc<Self>, job_id: &str, output_path: String) {
        self.running.remove(job_id);

        let (mut title, thumbnail, total_bytes) = self
            .jobs
            .get(job_id)
            .map(|j| (j.title.clone(), j.thumbnail.clone(), j.total_bytes))
            .unwrap_or((None, None, None));

        // Extract title from filename if not already set (desktop downloads don't get title from yt-dlp)
        if title.is_none() {
            if let Some(filename) = std::path::Path::new(&output_path).file_stem() {
                let name = filename.to_string_lossy().to_string();
                // Only use filename as title if it's not the URL or a template placeholder
                if !name.starts_with("http") && !name.contains("%(") {
                    title = Some(name);
                }
            }
        }

        // Get actual file size if not known from progress
        let filesize =
            total_bytes.or_else(|| std::fs::metadata(&output_path).ok().map(|m| m.len()));

        self.update_job_status(
            job_id,
            JobStatus::Completed {
                output_path: output_path.clone(),
            },
        );

        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.completed_at = Some(now_ms());
            job.progress = 100.0;
            if filesize.is_some() {
                job.total_bytes = filesize;
            }
            // Update job title if we extracted it from filename
            if title.is_some() && job.title.is_none() {
                job.title = title.clone();
            }
        }

        self.emit_event(JobEvent::Completed {
            job_id: job_id.to_string(),
            output_path,
            title,
            thumbnail,
            filesize,
        });

        self.active_count.fetch_sub(1, Ordering::SeqCst);
        self.last_progress_emit.remove(job_id);
        self.persist();
        self.schedule_try_start_next();
    }

    fn handle_job_failure(self: &Arc<Self>, job_id: &str, error: BackendError) {
        self.running.remove(job_id);

        let retryable = error.is_retryable();
        let max_retries = self
            .jobs
            .get(job_id)
            .map(|j| {
                j.request
                    .options
                    .max_retries
                    .unwrap_or(constants::DEFAULT_MAX_RETRIES)
            })
            .unwrap_or(constants::DEFAULT_MAX_RETRIES);

        let retry_count = self.jobs.get(job_id).map(|j| j.retry_count).unwrap_or(0);
        let current_backend = self.jobs.get(job_id).map(|j| j.backend.clone());

        if retryable && retry_count < max_retries {
            // Retry with same backend
            if let Some(mut job) = self.jobs.get_mut(job_id) {
                job.retry_count += 1;
                job.last_error = Some(error.to_string());
                job.status = JobStatus::Queued;
            }

            let delay_secs = 3 * 2u64.pow(retry_count);
            self.active_count.fetch_sub(1, Ordering::SeqCst);
            let manager = Arc::clone(self);
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
                manager.try_start_next().await;
            });
        } else {
            // Max retries reached - try fallback to next backend
            let fallback_backend = self.find_fallback_backend(job_id, current_backend.as_deref());

            if let Some(fallback) = fallback_backend {
                info!(
                    "Falling back from {:?} to {} for job {}",
                    current_backend, fallback, job_id
                );
                if let Some(mut job) = self.jobs.get_mut(job_id) {
                    job.backend = fallback;
                    job.retry_count = 0; // Reset retry count for new backend
                    job.last_error = Some(format!("Previous backend failed: {}", error));
                    job.status = JobStatus::Queued;
                }
                self.active_count.fetch_sub(1, Ordering::SeqCst);
                self.schedule_try_start_next();
            } else {
                // No fallback available - mark as failed
                self.update_job_status(
                    job_id,
                    JobStatus::Failed {
                        error: error.to_string(),
                        retryable,
                    },
                );

                if let Some(mut job) = self.jobs.get_mut(job_id) {
                    job.completed_at = Some(now_ms());
                }

                self.emit_event(JobEvent::Failed {
                    job_id: job_id.to_string(),
                    error: error.to_string(),
                    retryable,
                });

                self.active_count.fetch_sub(1, Ordering::SeqCst);
                self.schedule_try_start_next();
            }
        }

        self.last_progress_emit.remove(job_id);
        self.persist();
    }

    fn find_fallback_backend(&self, job_id: &str, current: Option<&str>) -> Option<String> {
        let url = self.jobs.get(job_id).map(|j| j.request.url.clone())?;

        // Define fallback chain based on current backend
        let fallback_order: &[&str] = match current {
            Some("aria2") => &["direct"],
            Some("direct") => &[],
            Some("ytdlp") => &["direct"],
            _ => &["direct"],
        };

        let registry = futures::executor::block_on(self.registry.read());

        for &backend_name in fallback_order {
            if let Some(backend) = registry.get(backend_name) {
                // Check if this backend can handle the URL at all
                if backend.priority(&url) != Priority::None {
                    return Some(backend_name.to_string());
                }
            }
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
        let jobs: Vec<Job> = self.jobs.iter().map(|r| r.value().clone()).collect();
        if let Err(e) = self.store.save_jobs(&jobs) {
            error!("Failed to persist jobs: {}", e);
        }
    }

    #[allow(dead_code)]
    pub fn handle_android_started(&self, job_id: &str, title: &str) {
        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.status = JobStatus::Downloading;
            job.started_at = Some(now_ms());
            if !title.is_empty() {
                job.title = Some(title.to_string());
            }
        }
        self.emit_event(JobEvent::Started {
            job_id: job_id.to_string(),
            backend: "android".to_string(),
        });
        self.persist();
    }

    #[allow(dead_code)]
    pub fn handle_android_progress(
        &self,
        job_id: &str,
        progress: f64,
        speed: Option<u64>,
        eta: Option<u64>,
        downloaded_bytes: Option<u64>,
        total_bytes: Option<u64>,
    ) {
        let now = now_ms();
        let should_emit = self
            .last_progress_emit
            .get(job_id)
            .map(|last| now.saturating_sub(*last) >= constants::PROGRESS_THROTTLE_MS)
            .unwrap_or(true);

        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.progress = progress;
            job.speed = speed;
            job.eta = eta;
            if let Some(d) = downloaded_bytes {
                job.downloaded_bytes = d;
            }
            if let Some(t) = total_bytes {
                job.total_bytes = Some(t);
            }
        }

        if should_emit {
            self.last_progress_emit.insert(job_id.to_string(), now);
            if let Some(job) = self.jobs.get(job_id) {
                self.emit_event(JobEvent::Progress {
                    job_id: job_id.to_string(),
                    progress: job.progress,
                    downloaded_bytes: job.downloaded_bytes,
                    total_bytes: job.total_bytes,
                    speed: job.speed,
                    eta: job.eta,
                });
            }
        }
    }

    #[allow(dead_code)]
    pub fn handle_android_completed(
        &self,
        job_id: &str,
        output_path: &str,
        title: Option<String>,
        thumbnail: Option<String>,
    ) {
        self.running.remove(job_id);

        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.status = JobStatus::Completed {
                output_path: output_path.to_string(),
            };
            job.completed_at = Some(now_ms());
            job.progress = 100.0;
            if let Some(t) = title.as_ref() {
                job.title = Some(t.clone());
            }
            if let Some(th) = thumbnail.as_ref() {
                job.thumbnail = Some(th.clone());
            }
        }

        // Get file size from filesystem
        let filesize = std::fs::metadata(output_path).ok().map(|m| m.len());

        self.emit_event(JobEvent::Completed {
            job_id: job_id.to_string(),
            output_path: output_path.to_string(),
            title,
            thumbnail,
            filesize,
        });

        self.active_count.fetch_sub(1, Ordering::SeqCst);
        self.last_progress_emit.remove(job_id);
        self.persist();

        // Try to start next job
        let manager = self.app.state::<Arc<JobManager>>();
        let manager_clone = Arc::clone(&*manager);
        tokio::spawn(async move {
            manager_clone.try_start_next().await;
        });
    }

    #[allow(dead_code)]
    pub fn handle_android_failed(&self, job_id: &str, error: &str) {
        self.running.remove(job_id);

        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.status = JobStatus::Failed {
                error: error.to_string(),
                retryable: false,
            };
            job.completed_at = Some(now_ms());
            job.last_error = Some(error.to_string());
        }

        self.emit_event(JobEvent::Failed {
            job_id: job_id.to_string(),
            error: error.to_string(),
            retryable: false,
        });

        self.active_count.fetch_sub(1, Ordering::SeqCst);
        self.last_progress_emit.remove(job_id);
        self.persist();

        // Try to start next job
        let manager = self.app.state::<Arc<JobManager>>();
        let manager_clone = Arc::clone(&*manager);
        tokio::spawn(async move {
            manager_clone.try_start_next().await;
        });
    }

    #[allow(dead_code)]
    pub fn handle_android_cancelled(&self, job_id: &str) {
        self.running.remove(job_id);

        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.status = JobStatus::Cancelled;
            job.completed_at = Some(now_ms());
        }

        self.emit_event(JobEvent::Cancelled {
            job_id: job_id.to_string(),
        });

        self.active_count.fetch_sub(1, Ordering::SeqCst);
        self.last_progress_emit.remove(job_id);
        self.persist();

        // Try to start next job
        let manager = self.app.state::<Arc<JobManager>>();
        let manager_clone = Arc::clone(&*manager);
        tokio::spawn(async move {
            manager_clone.try_start_next().await;
        });
    }

    #[allow(dead_code)]
    pub fn handle_android_paused(&self, job_id: &str) {
        self.running.remove(job_id);

        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.status = JobStatus::Paused;
        }

        self.emit_event(JobEvent::Paused {
            job_id: job_id.to_string(),
        });

        self.active_count.fetch_sub(1, Ordering::SeqCst);
        self.last_progress_emit.remove(job_id);
        self.persist();

        // Try to start next job
        let manager = self.app.state::<Arc<JobManager>>();
        let manager_clone = Arc::clone(&*manager);
        tokio::spawn(async move {
            manager_clone.try_start_next().await;
        });
    }
}
