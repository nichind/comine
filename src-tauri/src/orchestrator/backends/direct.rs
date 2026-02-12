//! Direct HTTP file download backend.
//!
//! Limitations:
//! - No pause/resume support (would need Range header tracking)

use std::path::PathBuf;

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::orchestrator::backends::{
    extract_filename_from_url, has_file_extension, http_status_to_error, Backend,
    BackendCapabilities, SpawnContext, DIRECT_FILE_EXTENSIONS,
};
use crate::orchestrator::types::*;

pub struct DirectBackend {
    client: Client,
}

impl DirectBackend {
    pub fn new() -> Self {
        Self {
            client: crate::utils::http_client().expect("Failed to create HTTP client"),
        }
    }
}

impl Default for DirectBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Backend for DirectBackend {
    fn name(&self) -> &str {
        "direct"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            name: "direct".into(),
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

    fn priority(&self, url: &str) -> Priority {
        if has_file_extension(url, DIRECT_FILE_EXTENSIONS) {
            Priority::High
        } else if url.starts_with("http://") || url.starts_with("https://") {
            Priority::Low
        } else {
            Priority::None
        }
    }

    async fn resolve(
        &self,
        url: &str,
        _settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        crate::orchestrator::backends::resolve_http_file(url, "direct").await
    }

    async fn spawn(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        let url = &ctx.job.request.url;
        let output_dir = &ctx.job.request.output.directory;

        let filename = ctx
            .job
            .request
            .output
            .filename
            .clone()
            .or_else(|| ctx.job.title.clone())
            .unwrap_or_else(|| extract_filename_from_url(url));

        let output_path = PathBuf::from(output_dir).join(&filename);

        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| BackendError::IoError(e.to_string()))?;
        }

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| BackendError::NetworkError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(http_status_to_error(resp.status().as_u16(), url));
        }

        let total_size = resp.content_length();

        let mut file = File::create(&output_path)
            .await
            .map_err(|e| BackendError::IoError(e.to_string()))?;

        let mut downloaded: u64 = 0;
        let mut stream = resp.bytes_stream();
        let mut last_update = std::time::Instant::now();
        let mut last_downloaded: u64 = 0;

        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    drop(file);
                    let _ = tokio::fs::remove_file(&output_path).await;
                    return Err(BackendError::Cancelled);
                }
                chunk = stream.next() => {
                    match chunk {
                        Some(Ok(bytes)) => {
                            file.write_all(&bytes)
                                .await
                                .map_err(|e| BackendError::IoError(e.to_string()))?;

                            downloaded += bytes.len() as u64;

                            if last_update.elapsed().as_millis() >= 100 {
                                let elapsed_secs = last_update.elapsed().as_secs_f64().max(0.001);
                                let bytes_delta = downloaded.saturating_sub(last_downloaded);
                                let speed = Some((bytes_delta as f64 / elapsed_secs) as u64);
                                let eta = total_size.and_then(|t| {
                                    speed.filter(|&s| s > 0).map(|s| {
                                        t.saturating_sub(downloaded) / s
                                    })
                                });

                                let _ = ctx.progress_tx.send(ProgressUpdate {
                                    job_id: ctx.job.id.clone(),
                                    downloaded_bytes: downloaded,
                                    total_bytes: total_size,
                                    speed,
                                    eta,
                                });

                                last_update = std::time::Instant::now();
                                last_downloaded = downloaded;
                            }
                        }
                        Some(Err(e)) => {
                            return Err(BackendError::NetworkError(e.to_string()));
                        }
                        None => break,
                    }
                }
            }
        }

        file.flush()
            .await
            .map_err(|e| BackendError::IoError(e.to_string()))?;

        Ok(output_path.to_string_lossy().to_string())
    }
}
