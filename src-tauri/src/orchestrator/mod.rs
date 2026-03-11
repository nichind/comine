pub mod backends;
pub mod convert;
pub mod history;
pub mod manager;
pub mod podcast;
pub mod stats;
pub mod store;
pub mod thumbnail;
pub mod types;

use self::manager::JobManager;
use self::store::JobStore;
use self::types::{
    BackendError, ClearFilter, DownloadRequest, DownloadSettings, EnqueuePlaylistRequest,
    EnqueueRequest, Job, JobControl, ResolveResult,
};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub async fn resolve_url(
    state: State<'_, Arc<JobManager>>,
    url: String,
    settings: Option<types::ResolveSettings>,
) -> Result<ResolveResult, BackendError> {
    state.resolve_url(&url, settings.unwrap_or_default()).await
}

#[tauri::command]
pub async fn resolve_url_stream(
    state: State<'_, Arc<JobManager>>,
    resolve_id: String,
    url: String,
    settings: Option<types::ResolveSettings>,
) -> Result<(), BackendError> {
    let settings = settings.unwrap_or_default();
    state.resolve_url_stream(&resolve_id, &url, settings).await
}

#[tauri::command]
pub fn cancel_resolve_stream(
    state: State<'_, Arc<JobManager>>,
    resolve_id: String,
) -> Result<(), String> {
    state.cancel_resolve(&resolve_id);
    Ok(())
}

#[tauri::command]
pub async fn start_job(
    state: State<'_, Arc<JobManager>>,
    request: DownloadRequest,
) -> Result<String, BackendError> {
    state.start_job(request).await
}

#[tauri::command]
pub async fn get_jobs(state: State<'_, Arc<JobManager>>) -> Result<Vec<Job>, String> {
    state.wait_ready_pub().await;
    Ok(state.get_all_jobs())
}

#[tauri::command]
pub async fn control_job(
    state: State<'_, Arc<JobManager>>,
    job_id: String,
    action: JobControl,
) -> Result<(), BackendError> {
    state.control_job(&job_id, action).await
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

#[tauri::command]
pub fn move_job_to_top(state: State<'_, Arc<JobManager>>, job_id: String) {
    state.move_to_top(&job_id);
}

#[tauri::command]
pub async fn get_backend_capabilities(
    state: State<'_, Arc<JobManager>>,
) -> Result<Vec<backends::BackendCapabilities>, String> {
    Ok(state.get_all_capabilities().await)
}

#[tauri::command]
pub async fn sync_download_settings(
    state: State<'_, Arc<JobManager>>,
    settings: DownloadSettings,
) -> Result<(), String> {
    state.sync_settings(settings).await;
    Ok(())
}

#[tauri::command]
pub async fn enqueue_url(
    state: State<'_, Arc<JobManager>>,
    request: EnqueueRequest,
) -> Result<String, BackendError> {
    state.enqueue_url(request).await
}

#[tauri::command]
pub async fn enqueue_playlist(
    state: State<'_, Arc<JobManager>>,
    request: EnqueuePlaylistRequest,
) -> Result<Vec<String>, BackendError> {
    state.enqueue_playlist(request).await
}

#[tauri::command]
pub async fn pause_all_jobs(state: State<'_, Arc<JobManager>>) -> Result<(), String> {
    state.pause_all().await;
    Ok(())
}

#[tauri::command]
pub async fn resume_all_jobs(state: State<'_, Arc<JobManager>>) -> Result<(), String> {
    state.resume_all().await;
    Ok(())
}

#[tauri::command]
pub async fn cancel_all_jobs(state: State<'_, Arc<JobManager>>) -> Result<(), String> {
    state.cancel_all().await;
    Ok(())
}

#[tauri::command]
pub async fn retry_all_failed(state: State<'_, Arc<JobManager>>) -> Result<(), String> {
    state.retry_all_failed().await;
    Ok(())
}

#[tauri::command]
pub fn clear_terminal_jobs(
    state: State<'_, Arc<JobManager>>,
    filter: ClearFilter,
) -> Result<(), String> {
    state.clear_terminal_jobs(filter);
    Ok(())
}

#[tauri::command]
pub async fn get_history(
    state: State<'_, Arc<JobManager>>,
) -> Result<Vec<types::HistoryItem>, String> {
    Ok(state.history.get_all().await)
}

#[tauri::command]
pub async fn remove_history_item(
    state: State<'_, Arc<JobManager>>,
    id: String,
) -> Result<bool, String> {
    let removed = state.history.remove(&id).await;
    if removed {
        emit_history_stats(&state).await;
    }
    Ok(removed)
}

#[tauri::command]
pub async fn clear_history(state: State<'_, Arc<JobManager>>) -> Result<(), String> {
    state.history.clear().await;
    emit_history_stats(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn toggle_history_favourite(
    state: State<'_, Arc<JobManager>>,
    id: String,
) -> Result<Option<bool>, String> {
    let result = state.history.toggle_favourite(&id).await;
    emit_history_stats(&state).await;
    Ok(result)
}

#[tauri::command]
pub async fn set_history_favourite(
    state: State<'_, Arc<JobManager>>,
    ids: Vec<String>,
    value: bool,
) -> Result<(), String> {
    state.history.set_favourite(&ids, value).await;
    emit_history_stats(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn update_history_duration(
    state: State<'_, Arc<JobManager>>,
    id: String,
    duration: f64,
) -> Result<(), String> {
    state.history.update_duration(&id, duration).await;
    Ok(())
}

#[tauri::command]
pub async fn export_history(state: State<'_, Arc<JobManager>>) -> Result<String, String> {
    Ok(state.history.export().await)
}

#[tauri::command]
pub async fn import_history(
    state: State<'_, Arc<JobManager>>,
    items: Vec<types::HistoryItem>,
) -> Result<usize, String> {
    let count = state.history.import(items).await;
    emit_history_stats(&state).await;
    Ok(count)
}

#[tauri::command]
pub async fn restore_history_from_frontend(
    state: State<'_, Arc<JobManager>>,
    items: Vec<types::HistoryItem>,
) -> Result<(), String> {
    state.history.restore_from_frontend(items).await;
    emit_history_stats(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn get_app_stats(state: State<'_, Arc<JobManager>>) -> Result<types::AppStats, String> {
    Ok(state.stats.get().await)
}

#[tauri::command]
pub async fn get_history_stats(
    state: State<'_, Arc<JobManager>>,
) -> Result<types::HistoryStats, String> {
    Ok(state.history.compute_stats().await)
}

#[tauri::command]
pub async fn fetch_broadcasts(
    state: State<'_, Arc<JobManager>>,
) -> Result<Vec<types::Broadcast>, String> {
    Ok(state.stats.fetch_broadcasts(&state.app).await)
}

async fn emit_history_stats(state: &JobManager) {
    let stats = state.history.compute_stats().await;
    let _ = state.app.emit("history-stats-changed", &stats);
}

pub fn init(app: &AppHandle) -> Arc<JobManager> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));

    let db = crate::database::Database::new(&app_data_dir)
        .unwrap_or_else(|e| panic!("Failed to initialize database: {}", e));

    let store = Arc::new(JobStore::new(db.clone()));
    let history_store = history::HistoryStore::new(db.clone());
    let stats_store = stats::StatsStore::new(db);
    let (ready_tx, ready_rx) = tokio::sync::watch::channel(false);
    let manager = JobManager::new(app.clone(), store, history_store, stats_store, ready_rx);

    #[cfg(target_os = "android")]
    {
        backends::init_android(app.clone());
    }

    let manager_clone = Arc::clone(&manager);
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        manager_clone
            .stats
            .backfill_from_history(&manager_clone.history)
            .await;
        manager_clone.stats.start(app_clone.clone());
        if let Some(aria2) = crate::orchestrator::backends::aria2::Aria2Backend::new(&app_clone) {
            manager_clone.register_backend(Arc::new(aria2)).await;
        } else {
            tracing::info!(
                "aria2 not available, direct file downloads will use alternative backends"
            );
        }

        manager_clone
            .register_backend(Arc::new(
                crate::orchestrator::backends::direct::DirectBackend::new(),
            ))
            .await;

        // librqbit backend — handles torrents/magnets natively (no subprocess).
        // On iOS it takes Priority::Absolute for torrent URLs (aria2 is unavailable there).
        // On other platforms it returns Priority::None so aria2 wins; it can still be forced
        // explicitly by name if needed.
        // Session is lazy — DHT only starts on first download.
        {
            let default_dir = app_clone
                .path()
                .download_dir()
                .or_else(|_| app_clone.path().document_dir())
                .or_else(|_| app_clone.path().app_data_dir())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string());

            let backend = crate::orchestrator::backends::librqbit::LibrqbitBackend::new(&default_dir);
            manager_clone.register_backend(Arc::new(backend)).await;
        }

        #[cfg(desktop)]
        {
            match crate::deps::resolve_ytdlp_path(&app_clone) {
                Some(path) => {
                    manager_clone
                        .register_backend(Arc::new(
                            crate::orchestrator::backends::ytdlp::YtdlpBackend::new(
                                app_clone.clone(),
                                path,
                            ),
                        ))
                        .await;
                }
                None => {
                    tracing::warn!("yt-dlp not found (managed or system), backend not available");
                }
            }
        }

        #[cfg(desktop)]
        {
            match crate::deps::resolve_gallery_dl_path(&app_clone) {
                Some(path) => {
                    manager_clone
                        .register_backend(Arc::new(
                            crate::orchestrator::backends::gallery_dl::GalleryDlBackend::new(path),
                        ))
                        .await;
                }
                None => {
                    tracing::info!("gallery-dl not installed, gallery backend not available");
                }
            }
        }

        #[cfg(target_os = "android")]
        {
            manager_clone
                .register_backend(Arc::new(
                    crate::orchestrator::backends::ytdlp::YtdlpBackend::new_android(
                        app_clone.clone(),
                    ),
                ))
                .await;
        }

        if let Err(e) = manager_clone.load_persisted().await {
            tracing::warn!(error = %e, "Failed to load persisted orchestrator jobs");
        }

        let _ = ready_tx.send(true);

        manager_clone.try_start_next().await;
    });

    manager
}
