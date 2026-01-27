pub mod backends;
pub mod convert;
pub mod manager;
pub mod store;
pub mod thumbnail;
pub mod types;

use self::manager::JobManager;
use self::store::JobStore;
use self::types::{DownloadRequest, Job, JobControl, ResolveResult};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

// Commands
#[tauri::command]
pub async fn resolve_url(
    state: State<'_, Arc<JobManager>>,
    url: String,
    settings: Option<types::ResolveSettings>,
) -> Result<ResolveResult, String> {
    state
        .resolve_url(&url, settings.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_job(
    state: State<'_, Arc<JobManager>>,
    request: DownloadRequest,
) -> Result<String, String> {
    state.start_job(request).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_jobs(state: State<'_, Arc<JobManager>>) -> Vec<Job> {
    state.get_all_jobs()
}

#[tauri::command]
pub async fn control_job(
    state: State<'_, Arc<JobManager>>,
    job_id: String,
    action: JobControl,
) -> Result<(), String> {
    state
        .control_job(&job_id, action)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_job_settings(
    state: State<'_, Arc<JobManager>>,
    max_concurrent: Option<u32>,
    speed_limit: Option<u64>,
) {
    if let Some(max) = max_concurrent {
        state.set_max_concurrent(max);
    }
    if let Some(limit) = speed_limit {
        state.set_global_speed_limit(limit);
    }
}

pub fn init(app: &AppHandle) -> Arc<JobManager> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));

    let store = Arc::new(JobStore::new(app_data_dir));
    let manager = JobManager::new(app.clone(), store);

    // Initialize Android backend with app handle (for JNI calls to Kotlin)
    #[cfg(target_os = "android")]
    {
        backends::init_android(app.clone());
        // Store manager reference for JNI callbacks from Kotlin
        backends::set_job_manager(Arc::clone(&manager));
    }

    // Register backends + load persisted jobs
    let manager_clone = Arc::clone(&manager);
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        // Register aria2 backend (available on both Desktop and Android)
        // Desktop: Uses aria2c binary
        // Android: Uses libaria2c.so from youtubedl-android via JNI
        if let Some(aria2) = crate::orchestrator::backends::aria2::Aria2Backend::new() {
            manager_clone.register_backend(Arc::new(aria2)).await;
        } else {
            tracing::info!(
                "aria2 not available, direct file downloads will use alternative backends"
            );
        }

        // Register direct backend (fallback for simple HTTP downloads)
        manager_clone
            .register_backend(Arc::new(
                crate::orchestrator::backends::direct::DirectBackend::new(),
            ))
            .await;

        // Register yt-dlp backend (desktop)
        #[cfg(not(target_os = "android"))]
        {
            match crate::deps::get_ytdlp_path(&app_clone) {
                Ok(path) => {
                    manager_clone
                        .register_backend(Arc::new(
                            crate::orchestrator::backends::ytdlp::YtdlpBackend::new(
                                app_clone.clone(),
                                path,
                            ),
                        ))
                        .await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to get yt-dlp path for orchestrator backend");
                }
            }
        }

        // Android uses yt-dlp via JNI (youtubedl-android)
        #[cfg(target_os = "android")]
        {
            manager_clone
                .register_backend(Arc::new(
                    crate::orchestrator::backends::ytdlp::YtdlpBackend::new_android(),
                ))
                .await;
        }

        if let Err(e) = manager_clone.load_persisted().await {
            tracing::warn!(error = %e, "Failed to load persisted orchestrator jobs");
        }

        // Kick the scheduler in case there are queued jobs.
        manager_clone.try_start_next().await;
    });

    manager
}
