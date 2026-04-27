//! Shared installer framework that eliminates boilerplate across dependency specs.
//!
//! Each dependency spec only needs to provide the varying parts (URLs, names,
//! matchers, version logic) via [`InstallPlan`], and this module handles the
//! entire fetch → checksum → download → extract → permissions → verify → done
//! pipeline.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::proxy::ProxyConfig;

use super::cancel;
use super::checksum::try_fetch_sha256;
use super::download::download_file_with_checksum;
use super::progress::ProgressEmitter;

/// Common helper: resolve `<app_data_dir>/bin`.
///
/// On Android, yt-dlp uses `app_cache_dir` instead – callers that need that
/// should provide their own `bin_dir` via [`InstallPlan`].
pub fn get_bin_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    Ok(app_data.join("bin"))
}

pub fn checksum_url_candidates(url: &str) -> Vec<String> {
    vec![
        format!("{}.sha256", url),
        format!("{}.sha256sum", url),
        format!("{}.sha256.txt", url),
        format!("{}.sha256sum.txt", url),
    ]
}

pub enum ExtractStrategy {
    Zip {
        matcher: super::extract::FileMatcher,
        dest_name: &'static str,
    },
    ZipMultiple {
        matchers: Vec<(super::extract::FileMatcher, &'static str)>,
    },
    #[allow(dead_code)]
    Custom(
        Box<
            dyn FnOnce(
                    &Path,
                    &Path,
                ) -> std::pin::Pin<
                    Box<dyn std::future::Future<Output = Result<(), String>> + Send>,
                > + Send,
        >,
    ),
    /// No extraction needed – the downloaded file is the binary itself (e.g., yt-dlp).
    None,
}

pub struct InstallPlan {
    pub dep_name: &'static str,
    pub display_name: &'static str,
    pub event_name: &'static str,
    pub version: String,
    pub download_urls: Vec<String>,
    pub checksum_urls: Vec<String>,
    pub checksum_filename_hint: Option<&'static str>,
    pub temp_archive: PathBuf,
    pub binary_path: PathBuf,
    pub extract: ExtractStrategy,
    #[allow(dead_code)]
    pub extra_executables: Vec<PathBuf>,
    pub verify_args: Vec<&'static str>,
    /// Custom verification function. If set, replaces the default
    /// "run binary with verify_args and check exit code 0" logic.
    pub custom_verify: Option<
        Box<
            dyn FnOnce(
                    &Path,
                ) -> std::pin::Pin<
                    Box<dyn std::future::Future<Output = Result<(), String>> + Send>,
                > + Send,
        >,
    >,
}

pub async fn run_install(
    app: &AppHandle,
    plan: InstallPlan,
    proxy_config: &ProxyConfig,
) -> Result<String, String> {
    let progress = ProgressEmitter::new(app, plan.event_name);
    let cancel_token = cancel::reset_token(plan.dep_name);

    if let Some(parent) = plan.temp_archive.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create bin directory: {}", e))?;
    }

    progress.stage(
        "fetching",
        0,
        format!("Fetching {} {}...", plan.display_name, plan.version),
    );

    let mut download_errors: Vec<String> = Vec::new();
    let mut downloaded = false;

    for url in &plan.download_urls {
        if cancel_token.is_cancelled() {
            let _ = tokio::fs::remove_file(&plan.temp_archive).await;
            return Err("Cancelled".to_string());
        }

        let checksum_urls = if plan.checksum_urls.is_empty() {
            checksum_url_candidates(url)
        } else {
            plan.checksum_urls.clone()
        };
        let expected_sha256 =
            try_fetch_sha256(&checksum_urls, proxy_config, plan.checksum_filename_hint).await;

        match download_file_with_checksum(
            url,
            &plan.temp_archive,
            &progress,
            plan.display_name,
            &plan.version,
            Some(proxy_config),
            expected_sha256.as_deref(),
            Some(&cancel_token),
        )
        .await
        {
            Ok(()) => {
                downloaded = true;
                break;
            }
            Err(e) => {
                tracing::warn!("{} download failed from {}: {}", plan.display_name, url, e);
                download_errors.push(format!("{} => {}", url, e));
            }
        }
    }

    if !downloaded {
        return Err(format!(
            "Failed to download {}. Errors: {}",
            plan.display_name,
            download_errors.join(" | ")
        ));
    }

    check_cancelled(&cancel_token, &plan.temp_archive).await?;

    progress.stage(
        "extracting",
        90,
        format!("Extracting {}...", plan.display_name),
    );

    match plan.extract {
        ExtractStrategy::Zip { matcher, dest_name } => {
            let parent = plan.temp_archive.parent().ok_or_else(|| {
                format!("No parent directory for {}", plan.temp_archive.display())
            })?;
            super::extract::extract_from_zip(
                &plan.temp_archive,
                parent,
                super::extract::ZipExtractConfig {
                    matcher,
                    dest_name,
                    extract_all: false,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        ExtractStrategy::ZipMultiple { matchers } => {
            let parent = plan.temp_archive.parent().ok_or_else(|| {
                format!("No parent directory for {}", plan.temp_archive.display())
            })?;
            let matchers_slice: Vec<_> = matchers.iter().map(|(m, n)| (*m, *n)).collect();
            super::extract::extract_from_zip_multiple(&plan.temp_archive, parent, &matchers_slice)
                .await
                .map_err(|e| e.to_string())?;
        }
        ExtractStrategy::Custom(custom_fn) => {
            let bin_dir = plan
                .temp_archive
                .parent()
                .ok_or_else(|| format!("No parent directory for {}", plan.temp_archive.display()))?
                .to_path_buf();
            custom_fn(&plan.temp_archive, &bin_dir).await?;
        }
        ExtractStrategy::None => {
            // Downloaded file is the binary itself – nothing to extract.
        }
    }

    if plan.temp_archive != plan.binary_path {
        let _ = tokio::fs::remove_file(&plan.temp_archive).await;
    }

    #[cfg(unix)]
    {
        super::fs::make_executable(&plan.binary_path).await?;
        for extra in &plan.extra_executables {
            if extra.exists() {
                super::fs::make_executable(extra).await?;
            }
        }
    }

    progress.stage(
        "verifying",
        95,
        format!("Verifying {}...", plan.display_name),
    );

    if let Some(custom_verify) = plan.custom_verify {
        if let Err(e) = custom_verify(&plan.binary_path).await {
            delete_corrupt_binary(&plan.binary_path, plan.display_name).await;
            return Err(e);
        }
    } else {
        let args: Vec<&str> = plan.verify_args.to_vec();
        match super::verify::run_capture_async(&plan.binary_path, &args).await {
            Ok(output) if output.status_code == Some(0) => {
                info!("{} installed successfully", plan.display_name);
            }
            Ok(output) => {
                let reason = if output.stderr.is_empty() {
                    format!("exited with code {:?}", output.status_code)
                } else {
                    output.stderr.clone()
                };
                tracing::error!(
                    "{} verification failed after install: {}",
                    plan.display_name,
                    reason
                );
                delete_corrupt_binary(&plan.binary_path, plan.display_name).await;
                return Err(format!(
                    "{} verification failed: {}",
                    plan.display_name, reason
                ));
            }
            Err(e) => {
                tracing::error!(
                    "{} verification error (binary may be incompatible): {}",
                    plan.display_name,
                    e
                );
                delete_corrupt_binary(&plan.binary_path, plan.display_name).await;
                return Err(format!("{} verification failed: {}", plan.display_name, e));
            }
        }
    }

    progress.stage(
        "complete",
        100,
        format!("{} installed successfully!", plan.display_name),
    );

    Ok(plan.binary_path.to_string_lossy().to_string())
}

/// Delete a binary that failed verification so future install attempts re-download it fresh.
///
/// Verification failures often indicate the wrong architecture was downloaded (e.g., x86_64
/// binary on an ARM Mac). Leaving the corrupt file on disk causes a loop: the startup check
/// finds it, fails to run it, and subsequent installs fail at verification again.
async fn delete_corrupt_binary(binary_path: &Path, display_name: &str) {
    if !binary_path.exists() {
        return;
    }
    tracing::warn!(
        "Deleting corrupt/incompatible {} binary at {:?} so next install re-downloads it",
        display_name,
        binary_path
    );
    if let Err(e) = tokio::fs::remove_file(binary_path).await {
        tracing::warn!(
            "Failed to delete corrupt {} binary at {:?}: {}",
            display_name,
            binary_path,
            e
        );
    } else {
        tracing::info!(
            "Deleted corrupt {} binary — next install will fetch a fresh copy",
            display_name
        );
    }
}

async fn check_cancelled(token: &CancellationToken, temp_archive: &Path) -> Result<(), String> {
    if token.is_cancelled() {
        let _ = tokio::fs::remove_file(temp_archive).await;
        Err("Cancelled".to_string())
    } else {
        Ok(())
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct GitHubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub published_at: String,
}

pub async fn fetch_github_latest_release(
    repo: &str,
    proxy_config: &ProxyConfig,
) -> Result<GitHubRelease, String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    super::download::fetch_json::<GitHubRelease>(&url, proxy_config)
        .await
        .map_err(|e| format!("Failed to fetch latest release from {}: {}", repo, e))
}
