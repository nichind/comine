use std::path::Path;

#[cfg(desktop)]
use futures_util::StreamExt;

#[cfg(desktop)]
use tracing::{debug, error, info, warn};

#[cfg(desktop)]
use tokio::io::AsyncWriteExt;

#[cfg(desktop)]
use tokio_util::sync::CancellationToken;

#[cfg(desktop)]
use crate::proxy::{proxy_strategies, ProxyConfig};

#[cfg(desktop)]
use crate::types::InstallProgress;

#[cfg(desktop)]
use super::progress::ProgressEmitter;

use crate::deps::error::{DepsError, DepsResult, DownloadError};

#[cfg(desktop)]
const HTTP_TIMEOUT_SECS: u64 = 600;
#[cfg(desktop)]
const HTTP_CONNECT_TIMEOUT_SECS: u64 = 60;
#[cfg(desktop)]
const TCP_KEEPALIVE_SECS: u64 = 30;
#[cfg(desktop)]
const MAX_REDIRECTS: usize = 10;
#[cfg(desktop)]
const MAX_RETRY_ATTEMPTS: u32 = 5;
#[cfg(desktop)]
const PROGRESS_EMIT_INTERVAL_MS: u128 = 100;
#[cfg(desktop)]
use crate::orchestrator::types::constants::USER_AGENT;
#[cfg(desktop)]
const RATELIMIT_DELAY_SECS: u64 = 10;
#[cfg(desktop)]
const RATELIMIT_MAX_DELAY_SECS: u64 = 60;

#[cfg(desktop)]
fn http_client(proxy_url: Option<&str>) -> Result<reqwest::Client, DownloadError> {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .connect_timeout(std::time::Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS))
        .pool_max_idle_per_host(4)
        .tcp_keepalive(std::time::Duration::from_secs(TCP_KEEPALIVE_SECS))
        .tcp_nodelay(true)
        .gzip(true)
        .brotli(true)
        .danger_accept_invalid_certs(false);

    match proxy_url {
        Some(url) if !url.is_empty() => {
            let proxy = reqwest::Proxy::all(url)
                .map_err(|e| DownloadError::Request(format!("Invalid proxy URL: {e}")))?;
            builder = builder.proxy(proxy);
            debug!("HTTP client using proxy: {}", url);
        }
        _ => {
            builder = builder.no_proxy();
            debug!("HTTP client with no proxy");
        }
    }

    builder
        .build()
        .map_err(|e| DownloadError::Request(format!("Failed to build HTTP client: {e}")))
}

#[cfg(desktop)]
async fn fetch(
    url: &str,
    proxy_config: &ProxyConfig,
    cancel: Option<&CancellationToken>,
) -> Result<reqwest::Response, DownloadError> {
    let strategies = proxy_strategies(proxy_config);
    let mut last_error = DownloadError::Request("No strategies available".to_string());

    for (strategy_name, proxy_url) in &strategies {
        info!("Trying download {} - {}", strategy_name, url);

        let client = match http_client(proxy_url.as_deref()) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to create client {}: {}", strategy_name, e);
                last_error = e;
                continue;
            }
        };

        for attempt in 0..MAX_RETRY_ATTEMPTS {
            if cancel.is_some_and(|t| t.is_cancelled()) {
                return Err(DownloadError::Cancelled);
            }

            if attempt > 0 {
                let delay = std::time::Duration::from_millis(1000 * 2u64.pow(attempt));
                info!("Retry {} after {:?}...", attempt, delay);
                tokio::time::sleep(delay).await;
            }

            match client.get(url).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        info!("Download started successfully {}", strategy_name);
                        return Ok(response);
                    } else if status.as_u16() == 429 || status.as_u16() == 503 {
                        let delay = RATELIMIT_DELAY_SECS * 2u64.pow(attempt.min(3));
                        let delay = delay.min(RATELIMIT_MAX_DELAY_SECS);
                        warn!("Rate limited ({}), waiting {}s...", status, delay);
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                        last_error = DownloadError::RateLimited;
                    } else if status.as_u16() == 403 {
                        last_error = DownloadError::AccessDenied;
                        break;
                    } else {
                        warn!(
                            "HTTP {} from {} (strategy: {})",
                            status.as_u16(),
                            url,
                            strategy_name
                        );
                        last_error = DownloadError::http(status.as_u16(), status.to_string());

                        // Client errors (e.g. 404 for missing checksum sidecars) won't fix themselves.
                        if status.is_client_error() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    let err_str = e.to_string();
                    warn!("Request error: {}", err_str);

                    if err_str.contains("timeout") {
                        last_error = DownloadError::Timeout;
                    } else if err_str.contains("connection") || err_str.contains("proxy") {
                        last_error = DownloadError::Connection(err_str.clone());
                        if attempt >= 2 {
                            break;
                        }
                    } else {
                        last_error = DownloadError::Request(err_str);
                    }
                }
            }
        }
    }

    Err(last_error)
}

#[cfg(desktop)]
pub async fn download_file_with_checksum(
    url: &str,
    dest: &Path,
    progress: &ProgressEmitter,
    dep_name: &'static str,
    version: &str,
    proxy_config: Option<&ProxyConfig>,
    expected_sha256: Option<&str>,
    cancel: Option<&CancellationToken>,
) -> DepsResult<()> {
    let config = proxy_config.cloned().unwrap_or_default();

    if cancel.is_some_and(|t| t.is_cancelled()) {
        return Err(DepsError::Download(DownloadError::Cancelled));
    }

    if dest.exists() {
        info!("Removing existing file: {:?}", dest);
        if let Err(e) = tokio::fs::remove_file(dest).await {
            warn!("Failed to remove existing file: {}", e);
        }
    }

    info!("Starting download: {} -> {:?}", url, dest);

    let response = fetch(url, &config, cancel).await.map_err(|e| {
        error!("Failed to download {}: {}", dep_name, e);
        DepsError::Download(e)
    })?;

    let total_size = response.content_length().unwrap_or(0);
    info!("Download response OK, expected size: {} bytes", total_size);

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = tokio::fs::File::create(dest).await?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    let start_time = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now();

    progress.emit(InstallProgress {
        stage: "downloading".to_string(),
        progress: 0,
        downloaded: 0,
        total: total_size,
        speed: 0.0,
        message: format!("Downloading {} {}...", dep_name, version),
    });

    while let Some(chunk) = stream.next().await {
        if cancel.is_some_and(|t| t.is_cancelled()) {
            let _ = tokio::fs::remove_file(dest).await;
            return Err(DepsError::Download(DownloadError::Cancelled));
        }

        let chunk =
            chunk.map_err(|e| DepsError::Download(DownloadError::Request(e.to_string())))?;
        file.write_all(&chunk).await?;

        downloaded += chunk.len() as u64;

        if last_emit.elapsed().as_millis() >= PROGRESS_EMIT_INTERVAL_MS {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                downloaded as f64 / elapsed
            } else {
                0.0
            };

            let pct = if total_size > 0 {
                ((downloaded as f64 / total_size as f64) * 100.0) as u8
            } else {
                0
            };

            progress.emit(InstallProgress {
                stage: "downloading".to_string(),
                progress: pct,
                downloaded,
                total: total_size,
                speed,
                message: format!("Downloading {} {}...", dep_name, version),
            });

            last_emit = std::time::Instant::now();
        }
    }

    file.flush().await?;
    drop(file);

    let metadata = tokio::fs::metadata(dest).await?;
    let actual_size = metadata.len();
    info!(
        "Download complete: {} bytes written to {:?}",
        actual_size, dest
    );

    if total_size > 0 && actual_size != total_size {
        error!(
            "Size mismatch! Expected {} bytes, got {} bytes",
            total_size, actual_size
        );
        let _ = tokio::fs::remove_file(dest).await;
        return Err(DepsError::Download(DownloadError::size_mismatch(
            total_size,
            actual_size,
        )));
    }

    if actual_size == 0 {
        let _ = tokio::fs::remove_file(dest).await;
        return Err(DepsError::Download(DownloadError::EmptyResponse));
    }

    if let Some(expected) = expected_sha256 {
        info!("Verifying checksum for {}...", dep_name);
        progress.emit(InstallProgress {
            stage: "verifying".to_string(),
            progress: 85,
            downloaded: actual_size,
            total: actual_size,
            speed: 0.0,
            message: format!("Verifying {} checksum...", dep_name),
        });

        super::checksum::verify_sha256(dest, expected, dep_name).await?;
    }

    Ok(())
}

#[cfg(desktop)]
pub async fn fetch_json<T: serde::de::DeserializeOwned>(
    url: &str,
    proxy_config: &ProxyConfig,
) -> DepsResult<T> {
    let response = fetch(url, proxy_config, None).await?;
    response
        .json::<T>()
        .await
        .map_err(|e| DepsError::other(format!("Failed to parse JSON: {}", e)))
}

#[cfg(desktop)]
pub async fn fetch_text(url: &str, proxy_config: &ProxyConfig) -> DepsResult<String> {
    let response = fetch(url, proxy_config, None).await?;
    response
        .text()
        .await
        .map_err(|e| DepsError::other(format!("Failed to read text: {}", e)))
}

#[cfg(mobile)]
pub async fn fetch_json<T: serde::de::DeserializeOwned>(
    _url: &str,
    _proxy_config: &crate::proxy::ProxyConfig,
) -> DepsResult<T> {
    Err(DepsError::UnsupportedPlatform(
        "fetch_json not supported on mobile",
    ))
}

#[cfg(mobile)]
pub async fn fetch_text(
    _url: &str,
    _proxy_config: &crate::proxy::ProxyConfig,
) -> DepsResult<String> {
    Err(DepsError::UnsupportedPlatform(
        "fetch_text not supported on mobile",
    ))
}

#[cfg(mobile)]
pub async fn download_file_with_checksum(
    _url: &str,
    _dest: &std::path::Path,
    _progress: &super::progress::ProgressEmitter,
    _dep_name: &'static str,
    _version: &str,
    _proxy_config: Option<&crate::proxy::ProxyConfig>,
    _expected_sha256: Option<&str>,
    _cancel: Option<&tokio_util::sync::CancellationToken>,
) -> DepsResult<()> {
    Err(DepsError::UnsupportedPlatform(
        "Downloading dependencies is not supported on mobile",
    ))
}
