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
    extract_filename_from_response,
    extract_filename_from_url,
    // Common helpers
    has_file_extension,
    http_status_to_error,
    Backend,
    BackendCapabilities,
    SpawnContext,
    DIRECT_FILE_EXTENSIONS,
};
use crate::orchestrator::types::*;

pub struct DirectBackend {
    client: Client,
}

impl DirectBackend {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent(constants::USER_AGENT)
                .build()
                .expect("Failed to create HTTP client"),
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
            supports_pause: false,
            supports_resume: false,
            supports_progress: true,
            supports_speed_limit: false,
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
        let resp = self
            .client
            .head(url)
            .send()
            .await
            .map_err(|e| BackendError::NetworkError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(http_status_to_error(resp.status().as_u16(), url));
        }

        let content_length = resp
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let filename = extract_filename_from_response(url, &resp);

        Ok(UrlInfo::with_file_info(
            url,
            Some(filename),
            "direct",
            content_length,
            content_type,
        ))
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
                                let speed = Some(downloaded / last_update.elapsed().as_secs().max(1));
                                let eta = total_size.map(|t| {
                                    if downloaded > 0 {
                                        ((t.saturating_sub(downloaded)) as f64 / speed.unwrap_or(1) as f64) as u64
                                    } else {
                                        0
                                    }
                                });

                                let _ = ctx.progress_tx.send(ProgressUpdate {
                                    job_id: ctx.job.id.clone(),
                                    downloaded_bytes: downloaded,
                                    total_bytes: total_size,
                                    speed,
                                    eta,
                                });

                                last_update = std::time::Instant::now();
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
