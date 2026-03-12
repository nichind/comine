//! librqbit-based torrent/magnet download backend.
//!
//! On iOS, aria2 subprocess is unavailable, so this backend takes Priority::Absolute for
//! magnet links and .torrent URLs. On desktop, aria2 is preferred (richer feature set), so
//! this backend returns Priority::None and acts only as a fallback that the caller can force
//! by explicitly naming it.
//!
//! The `librqbit::Session` is created lazily on first download to avoid noisy DHT bootstrap
//! when the backend is never actually used (i.e. on desktop where aria2 handles torrents).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::OnceCell;
use tracing::{error, info, warn};

use crate::orchestrator::backends::{
    extract_magnet_name, is_torrent_url, Backend, BackendCapabilities, MetadataEvent, SpawnContext,
};
use crate::orchestrator::types::*;

pub struct LibrqbitBackend {
    download_dir: String,
    session: OnceCell<Arc<librqbit::Session>>,
}

impl LibrqbitBackend {
    /// Create a new backend. The librqbit session is NOT started yet — it will be
    /// initialised lazily on the first `spawn()` call to avoid DHT noise at boot.
    pub fn new(download_dir: &str) -> Self {
        info!(target: "librqbit", download_dir = %download_dir, "LibrqbitBackend created");
        Self {
            download_dir: download_dir.to_string(),
            session: OnceCell::new(),
        }
    }

    /// Get or create the librqbit session.
    async fn session(&self) -> Result<&Arc<librqbit::Session>, BackendError> {
        self.session
            .get_or_try_init(|| async {
                let path = PathBuf::from(&self.download_dir);
                info!(target: "librqbit", path = %path.display(), "Creating download directory");

                tokio::fs::create_dir_all(&path)
                    .await
                    .map_err(|e| {
                        error!(target: "librqbit", error = %e, path = %path.display(), "Failed to create download dir");
                        BackendError::Other(format!("Failed to create download dir: {}", e))
                    })?;

                info!(target: "librqbit", "Initialising librqbit session...");

                let session = librqbit::Session::new_with_opts(
                    path,
                    librqbit::SessionOptions {
                        disable_dht: false,
                        enable_upnp_port_forwarding: false,
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| {
                    error!(target: "librqbit", error = %e, "Failed to create librqbit session");
                    BackendError::Other(format!("Failed to create librqbit session: {}", e))
                })?;

                info!(target: "librqbit", "librqbit session initialised successfully");
                Ok(session)
            })
            .await
    }
}

#[async_trait]
impl Backend for LibrqbitBackend {
    fn name(&self) -> &str {
        "librqbit"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            name: "librqbit".into(),
            streaming_resolve: false,
            playlists: false,
            pause_resume: false,
            multi_connection: true,
            format_selection: false,
            subtitles: false,
            speed_limit: false,
            proxy: false,
            cookies: false,
            torrent_magnet: true,
            post_processing: false,
        }
    }

    fn priority(&self, url: &str) -> Priority {
        if !is_torrent_url(url) {
            return Priority::None;
        }

        // On iOS, aria2 subprocess is unavailable, so take absolute priority for torrents.
        #[cfg(target_os = "ios")]
        {
            Priority::Absolute
        }

        // On all other platforms, aria2 is preferred because it has broader feature support.
        #[cfg(not(target_os = "ios"))]
        {
            Priority::None
        }
    }

    async fn resolve(
        &self,
        url: &str,
        _settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        info!(target: "librqbit", url = %url, "resolve called");
        if !is_torrent_url(url) {
            return Err(BackendError::UnsupportedUrl(url.to_string()));
        }

        let mut info = UrlInfo::simple(url, extract_magnet_name(url), self.name());
        info.content_type = ContentType::Torrent;
        Ok(info)
    }

    async fn spawn(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        let url = ctx.job.request.url.clone();
        let job_id = ctx.job.id.clone();
        let output_dir = ctx.job.request.output.directory.clone();

        info!(
            target: "librqbit",
            job_id = %job_id,
            url = %url,
            output_dir = %output_dir,
            "Starting torrent download"
        );

        info!(target: "librqbit", job_id = %job_id, "Getting/creating session...");
        let session = self.session().await?;
        info!(target: "librqbit", job_id = %job_id, "Session ready");

        // Convert 1-based torrent_selected_files to the 0-based indices librqbit expects.
        let only_files: Option<Vec<usize>> =
            ctx.job.request.options.torrent_selected_files.as_ref().map(|selected| {
                let files: Vec<usize> = selected.iter().map(|&i| i.saturating_sub(1) as usize).collect();
                info!(target: "librqbit", job_id = %job_id, ?files, "Selected files (0-based)");
                files
            });

        let add_opts = librqbit::AddTorrentOptions {
            output_folder: Some(output_dir.clone()),
            only_files,
            ..Default::default()
        };

        info!(target: "librqbit", job_id = %job_id, "Adding torrent to session...");
        let response = session
            .add_torrent(librqbit::AddTorrent::from_url(&url), Some(add_opts))
            .await
            .map_err(|e| {
                error!(target: "librqbit", job_id = %job_id, error = %e, "Failed to add torrent");
                BackendError::Other(format!("Failed to add torrent: {}", e))
            })?;

        info!(target: "librqbit", job_id = %job_id, "Torrent added, getting handle...");
        let handle = response
            .into_handle()
            .ok_or_else(|| {
                error!(target: "librqbit", job_id = %job_id, "No handle — torrent was already managed or list-only");
                BackendError::Other("Torrent was already managed or list-only".to_string())
            })?;

        let torrent_id = handle.id();
        info!(target: "librqbit", job_id = %job_id, torrent_id = torrent_id, "Got torrent handle");

        // Wait for metadata to arrive so we know the total size.
        info!(target: "librqbit", job_id = %job_id, "Waiting for metadata/initialization...");
        handle
            .wait_until_initialized()
            .await
            .map_err(|e| {
                error!(target: "librqbit", job_id = %job_id, error = %e, "Torrent init/metadata failed");
                BackendError::Other(format!("Torrent init failed: {}", e))
            })?;

        info!(target: "librqbit", job_id = %job_id, name = ?handle.name(), "Metadata received, torrent initialized");

        // Send output path early so the frontend can enable play-while-downloading.
        let early_output_path = resolve_output_path(&handle, &output_dir);
        info!(target: "librqbit", job_id = %job_id, path = %early_output_path, "Early output path");
        let _ = ctx.metadata_tx.send(MetadataEvent::FilePath(early_output_path));

        // Progress loop: poll stats every second, forward ProgressUpdate, honour cancellation.
        info!(target: "librqbit", job_id = %job_id, "Starting progress loop");
        let poll_result = run_progress_loop(&ctx, &handle).await;

        match poll_result {
            Ok(()) => {
                let output_path = resolve_output_path(&handle, &output_dir);
                info!(
                    target: "librqbit",
                    job_id = %job_id,
                    output_path = %output_path,
                    "Torrent download complete"
                );
                Ok(output_path)
            }
            Err(BackendError::Cancelled) => {
                if let Err(e) = session
                    .delete(librqbit::api::TorrentIdOrHash::Id(torrent_id), false)
                    .await
                {
                    warn!(
                        target: "librqbit",
                        job_id = %job_id,
                        error = %e,
                        "Failed to remove cancelled torrent from session"
                    );
                }
                Err(BackendError::Cancelled)
            }
            Err(e) => {
                error!(target: "librqbit", job_id = %job_id, error = %e, "Download failed");
                Err(e)
            }
        }
    }
}

/// Poll the torrent stats every second, emit ProgressUpdates, and wait for completion.
async fn run_progress_loop(
    ctx: &SpawnContext,
    handle: &Arc<librqbit::ManagedTorrent>,
) -> Result<(), BackendError> {
    let job_id = ctx.job.id.clone();
    let mut last_progress_bytes: u64 = 0;
    let mut last_tick = tokio::time::Instant::now();
    let mut log_counter: u32 = 0;

    loop {
        tokio::select! {
            _ = ctx.cancel_token.cancelled() => {
                info!(target: "librqbit", job_id = %job_id, "Download cancelled");
                return Err(BackendError::Cancelled);
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                let stats = handle.stats();

                let elapsed_secs = last_tick.elapsed().as_secs_f64().max(0.001);
                let speed_bytes = stats.progress_bytes.saturating_sub(last_progress_bytes);
                let speed = Some((speed_bytes as f64 / elapsed_secs) as u64);

                let total_bytes = if stats.total_bytes > 0 {
                    Some(stats.total_bytes)
                } else {
                    None
                };

                let eta = total_bytes.and_then(|total| {
                    speed.filter(|&s| s > 0).map(|s| {
                        total.saturating_sub(stats.progress_bytes) / s
                    })
                });

                // Log every 5 seconds for diagnostics
                log_counter += 1;
                if log_counter % 5 == 1 {
                    info!(
                        target: "librqbit",
                        job_id = %job_id,
                        progress_bytes = stats.progress_bytes,
                        total_bytes = stats.total_bytes,
                        finished = stats.finished,
                        speed = ?speed,
                        "Progress tick"
                    );
                }

                let _ = ctx.progress_tx.send(ProgressUpdate {
                    job_id: job_id.clone(),
                    downloaded_bytes: stats.progress_bytes,
                    total_bytes,
                    speed,
                    eta,
                });

                last_progress_bytes = stats.progress_bytes;
                last_tick = tokio::time::Instant::now();

                if stats.finished {
                    info!(target: "librqbit", job_id = %job_id, "Torrent finished");
                    break;
                }
            }
        }
    }

    handle
        .wait_until_completed()
        .await
        .map_err(|e| BackendError::Other(format!("Torrent completion wait failed: {}", e)))?;

    Ok(())
}

fn resolve_output_path(handle: &Arc<librqbit::ManagedTorrent>, fallback_dir: &str) -> String {
    let torrent_name = handle.name();
    match torrent_name {
        Some(name) if !name.is_empty() => {
            let candidate = PathBuf::from(fallback_dir).join(&name);
            candidate.to_string_lossy().to_string()
        }
        _ => fallback_dir.to_string(),
    }
}
