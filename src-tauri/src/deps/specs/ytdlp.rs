use std::path::PathBuf;

use log::{error, info, warn};
use tauri::{AppHandle, Manager};

use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, InstallProgress, ReleaseInfo};

use crate::deps::engine::cancel;
use crate::deps::engine::checksum::try_fetch_sha256;
use crate::deps::engine::download::{download_file_with_checksum, fetch_json};
use crate::deps::engine::progress::ProgressEmitter;
use crate::deps::engine::verify::run_capture_async;

#[cfg(unix)]
use crate::deps::engine::fs::make_executable;

const EVENT_PROGRESS: &str = "ytdlp-install-progress";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "yt-dlp.exe";
#[cfg(target_os = "android")]
const BINARY_NAME: &str = "libytdlp.so";
#[cfg(all(not(target_os = "windows"), not(target_os = "android")))]
const BINARY_NAME: &str = "yt-dlp";

#[cfg(target_os = "windows")]
const RELEASE_ASSET: &str = "yt-dlp.exe";
#[cfg(target_os = "macos")]
const RELEASE_ASSET: &str = "yt-dlp_macos";
#[cfg(target_os = "linux")]
const RELEASE_ASSET: &str = "yt-dlp_linux";
#[cfg(target_os = "android")]
const RELEASE_ASSET: &str = "";

fn get_bin_dir(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    {
        let cache_dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| format!("Failed to get app cache dir: {}", e))?;
        info!("Using Android cache dir: {:?}", cache_dir);
        Ok(cache_dir.join("bin"))
    }

    #[cfg(not(target_os = "android"))]
    {
        let app_data = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {}", e))?;
        Ok(app_data.join("bin"))
    }
}

pub fn get_ytdlp_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_bin_dir(app)?.join(BINARY_NAME))
}

#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: String,
    published_at: String,
}

async fn fetch_latest_release(proxy_config: &ProxyConfig) -> Result<GitHubRelease, String> {
    fetch_json::<GitHubRelease>(
        "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest",
        proxy_config,
    )
    .await
    .map_err(|e| format!("Failed to fetch latest release: {}", e))
}

pub async fn check_ytdlp(
    app: AppHandle,
    check_updates: Option<bool>,
) -> Result<DependencyStatus, String> {
    #[cfg(target_os = "android")]
    {
        let _ = check_updates;
        return Ok(DependencyStatus::embedded("youtubedl-android library"));
    }

    #[cfg(not(target_os = "android"))]
    {
        let ytdlp_path = get_ytdlp_path(&app)?;

        if ytdlp_path.exists() {
            match run_capture_async(&ytdlp_path, &["--version"]).await {
                Ok(output) if output.status_code == Some(0) => {
                    let version = output.stdout.trim().to_string();
                    info!("yt-dlp version: {}", version);
                    let disk_size = tokio::fs::metadata(&ytdlp_path).await.ok().map(|m| m.len());

                    let update_available = if check_updates.unwrap_or(false) {
                        match fetch_latest_release(&ProxyConfig::default()).await {
                            Ok(release) => {
                                if release.tag_name != version {
                                    Some(release.tag_name)
                                } else {
                                    None
                                }
                            }
                            Err(_) => None,
                        }
                    } else {
                        None
                    };

                    Ok(DependencyStatus::installed(
                        version,
                        ytdlp_path.to_string_lossy().to_string(),
                    )
                    .with_update(update_available)
                    .with_disk_size(disk_size))
                }
                Ok(output) => {
                    warn!("yt-dlp exists but failed to run: {}", output.stderr);
                    Ok(DependencyStatus::not_installed())
                }
                Err(e) => {
                    error!("Failed to execute yt-dlp: {}", e);
                    Ok(DependencyStatus {
                        path: Some(ytdlp_path.to_string_lossy().to_string()),
                        ..DependencyStatus::not_installed()
                    })
                }
            }
        } else {
            Ok(DependencyStatus::not_installed())
        }
    }
}

pub async fn install_ytdlp(
    app: AppHandle,
    version: Option<String>,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        let _ = (version, proxy_config);
        return Ok("embedded".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let ytdlp_path = get_ytdlp_path(&app)?;
        let bin_dir = get_bin_dir(&app)?;

        tokio::fs::create_dir_all(&bin_dir)
            .await
            .map_err(|e| format!("Failed to create bin directory: {}", e))?;

        let progress = ProgressEmitter::new(&app, EVENT_PROGRESS);

        progress.emit(InstallProgress {
            stage: "fetching".to_string(),
            progress: 0,
            downloaded: 0,
            total: 0,
            speed: 0.0,
            message: "Fetching latest release info...".to_string(),
        });

        let config = proxy_config.unwrap_or_default();
        let cancel_token = cancel::reset_token("ytdlp");

        let target_version = match version {
            Some(v) => v,
            None => {
                let release = fetch_latest_release(&config).await?;
                release.tag_name
            }
        };

        info!("Target version: {}", target_version);

        let download_url = format!(
            "https://github.com/yt-dlp/yt-dlp/releases/download/{}/{}",
            target_version, RELEASE_ASSET
        );

        let sums_url = format!(
            "https://github.com/yt-dlp/yt-dlp/releases/download/{}/SHA2-256SUMS",
            target_version
        );
        let expected_sha256 = try_fetch_sha256(&vec![sums_url], &config, Some(RELEASE_ASSET)).await;

        download_file_with_checksum(
            &download_url,
            &ytdlp_path,
            &progress,
            "yt-dlp",
            &target_version,
            Some(&config),
            expected_sha256.as_deref(),
            Some(&cancel_token),
        )
        .await?;

        if cancel_token.is_cancelled() {
            let _ = tokio::fs::remove_file(&ytdlp_path).await;
            return Err("Cancelled".to_string());
        }

        let metadata = tokio::fs::metadata(&ytdlp_path)
            .await
            .map_err(|e| format!("Downloaded file not found: {}", e))?;
        let total_size = metadata.len();

        #[cfg(unix)]
        {
            progress.emit(InstallProgress {
                stage: "permissions".to_string(),
                progress: 95,
                downloaded: total_size,
                total: total_size,
                speed: 0.0,
                message: "Setting executable permissions...".to_string(),
            });

            make_executable(&ytdlp_path).await?;
        }

        progress.emit(InstallProgress {
            stage: "verifying".to_string(),
            progress: 98,
            downloaded: total_size,
            total: total_size,
            speed: 0.0,
            message: "Verifying installation...".to_string(),
        });

        match run_capture_async(&ytdlp_path, &["--version"]).await {
            Ok(output) if output.status_code == Some(0) => {
                info!("yt-dlp verified: {}", output.stdout.trim());
            }
            Ok(output) => {
                return Err(format!("yt-dlp verification failed: {}", output.stderr));
            }
            Err(e) => {
                return Err(format!("yt-dlp verification failed: {}", e));
            }
        }

        progress.emit(InstallProgress {
            stage: "complete".to_string(),
            progress: 100,
            downloaded: total_size,
            total: total_size,
            speed: 0.0,
            message: format!("yt-dlp {} installed successfully!", target_version),
        });

        Ok(ytdlp_path.to_string_lossy().to_string())
    }
}

pub async fn uninstall_ytdlp(app: AppHandle) -> Result<(), String> {
    let ytdlp_path = get_ytdlp_path(&app)?;

    if ytdlp_path.exists() {
        tokio::fs::remove_file(&ytdlp_path)
            .await
            .map_err(|e| format!("Failed to remove yt-dlp: {}", e))?;
    }

    Ok(())
}

pub async fn get_ytdlp_releases(
    proxy_config: Option<ProxyConfig>,
) -> Result<Vec<ReleaseInfo>, String> {
    #[cfg(target_os = "android")]
    {
        let _ = proxy_config;
        return Ok(vec![ReleaseInfo {
            tag: "embedded".to_string(),
            name: "youtubedl-android (embedded)".to_string(),
            published_at: "".to_string(),
        }]);
    }

    #[cfg(not(target_os = "android"))]
    {
        let config = proxy_config.unwrap_or_default();
        let releases: Vec<GitHubRelease> = fetch_json(
            "https://api.github.com/repos/yt-dlp/yt-dlp/releases?per_page=10",
            &config,
        )
        .await?;

        Ok(releases
            .into_iter()
            .map(|r| ReleaseInfo {
                tag: r.tag_name,
                name: r.name,
                published_at: r.published_at,
            })
            .collect())
    }
}

pub async fn update_ytdlp_channel(app: AppHandle, channel: String) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        let _ = (app, channel);
        return Err("Not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let ytdlp_path = get_ytdlp_path(&app)?;

        if !ytdlp_path.exists() {
            return Err("yt-dlp is not installed".to_string());
        }

        let channel = channel.trim().to_string();
        let channel_lc = channel.to_lowercase();
        let effective_channel = if channel_lc == "nightly" {
            "master".to_string()
        } else {
            channel.clone()
        };

        let valid_channels = ["stable", "nightly", "master"];
        if !valid_channels.contains(&channel_lc.as_str()) && !channel.contains('@') {
            return Err(format!(
                "Invalid channel '{}'. Use 'stable', 'nightly', 'master', or 'REPO@TAG'",
                channel
            ));
        }

        info!("Updating yt-dlp to channel: {}", effective_channel);

        let output = run_capture_async(&ytdlp_path, &["--update-to", &effective_channel]).await?;

        if output.status_code == Some(0) {
            let version_output = run_capture_async(&ytdlp_path, &["--version"]).await?;
            let new_version = version_output.stdout.trim().to_string();
            info!("yt-dlp updated to: {}", new_version);
            Ok(new_version)
        } else {
            let error = if !output.stderr.is_empty() {
                output.stderr
            } else {
                output.stdout
            };
            Err(format!("Update failed: {}", error))
        }
    }
}
