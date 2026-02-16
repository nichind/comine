use std::path::PathBuf;

#[cfg(not(target_os = "android"))]
use tauri::AppHandle;
#[cfg(target_os = "android")]
use tauri::{AppHandle, Manager};
use tracing::{error, info, warn};

use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, ReleaseInfo};

use crate::deps::engine::download::fetch_json;
use crate::deps::engine::installer::{
    self, get_bin_dir as get_bin_dir_default, ExtractStrategy, GitHubRelease, InstallPlan,
};
use crate::deps::engine::verify::run_capture_async;

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
        get_bin_dir_default(app)
    }
}

pub fn get_ytdlp_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_bin_dir(app)?.join(BINARY_NAME))
}

pub fn resolve_ytdlp_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(managed) = get_ytdlp_path(app) {
        if managed.exists() {
            return Some(managed);
        }
    }
    crate::deps::engine::verify::find_in_system_path(BINARY_NAME)
}

async fn fetch_latest_release(proxy_config: &ProxyConfig) -> Result<GitHubRelease, String> {
    crate::deps::engine::installer::fetch_github_latest_release("yt-dlp/yt-dlp", proxy_config).await
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
        let ytdlp_path = match resolve_ytdlp_path(&app) {
            Some(path) => path,
            None => return Ok(DependencyStatus::not_installed()),
        };

        match run_capture_async(&ytdlp_path, &["--version"]).await {
            Ok(output) if output.status_code == Some(0) => {
                let version = output.stdout.trim().to_string();
                info!("yt-dlp version: {}", version);
                let disk_size = tokio::fs::metadata(&ytdlp_path).await.ok().map(|m| m.len());

                let update_available = if check_updates.unwrap_or(false) {
                    match fetch_latest_release(&ProxyConfig::default()).await {
                        Ok(release) => {
                            if crate::deps::updater::is_remote_newer(&release.tag_name, &version) {
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

                Ok(
                    DependencyStatus::installed(version, ytdlp_path.to_string_lossy().to_string())
                        .with_update(update_available)
                        .with_disk_size(disk_size),
                )
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
        let config = proxy_config.unwrap_or_default();

        let target_version = match version {
            Some(v) => v,
            None => {
                let release = fetch_latest_release(&config).await?;
                release.tag_name
            }
        };

        info!("Target version: {}", target_version);

        let ytdlp_path = get_ytdlp_path(&app)?;
        let download_url = format!(
            "https://github.com/yt-dlp/yt-dlp/releases/download/{}/{}",
            target_version, RELEASE_ASSET
        );
        let sums_url = format!(
            "https://github.com/yt-dlp/yt-dlp/releases/download/{}/SHA2-256SUMS",
            target_version
        );

        installer::run_install(
            &app,
            InstallPlan {
                dep_name: "ytdlp",
                display_name: "yt-dlp",
                event_name: EVENT_PROGRESS,
                version: target_version,
                download_urls: vec![download_url],
                checksum_urls: vec![sums_url],
                checksum_filename_hint: Some(RELEASE_ASSET),
                temp_archive: ytdlp_path.clone(),
                binary_path: ytdlp_path,
                extract: ExtractStrategy::None,
                extra_executables: vec![],
                verify_args: vec!["--version"],
                custom_verify: None,
            },
            &config,
        )
        .await
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

pub async fn self_update_ytdlp(app: AppHandle) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        let _ = app;
        return Ok("embedded".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let ytdlp_path = match resolve_ytdlp_path(&app) {
            Some(path) => path,
            None => return Err("yt-dlp is not installed".to_string()),
        };

        info!("Running yt-dlp --update (self-update within current channel)");

        let output = run_capture_async(&ytdlp_path, &["--update"]).await?;

        if output.status_code == Some(0) {
            let version_output = run_capture_async(&ytdlp_path, &["--version"]).await?;
            let new_version = version_output.stdout.trim().to_string();
            info!("yt-dlp self-update complete, version: {}", new_version);
            Ok(new_version)
        } else {
            let error = if !output.stderr.is_empty() {
                output.stderr
            } else {
                output.stdout
            };
            warn!("yt-dlp self-update returned non-zero: {}", error);
            let version_output = run_capture_async(&ytdlp_path, &["--version"]).await?;
            Ok(version_output.stdout.trim().to_string())
        }
    }
}

pub async fn update_ytdlp_channel(app: AppHandle, channel: String) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        use crate::orchestrator::backends::android_jni::update_ytdlp_channel_jni;
        let _ = app;

        let channel = channel.clone();
        tokio::task::spawn_blocking(move || -> Result<String, String> {
            update_ytdlp_channel_jni(&channel)
        })
        .await
        .map_err(|e| format!("Task panicked: {}", e))?
    }

    #[cfg(not(target_os = "android"))]
    {
        let ytdlp_path = match resolve_ytdlp_path(&app) {
            Some(path) => path,
            None => return Err("yt-dlp is not installed".to_string()),
        };

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
