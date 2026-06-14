use std::path::PathBuf;

use tauri::AppHandle;
use tracing::{error, info, warn};

use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, ReleaseInfo};

use crate::deps::engine::download::fetch_json;
use crate::deps::engine::installer::{
    self, get_bin_dir, ExtractStrategy, GitHubRelease, InstallPlan,
};
use crate::deps::engine::verify::run_capture_async;

const EVENT_PROGRESS: &str = "gallery-dl-install-progress";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "gallery-dl.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "gallery-dl";

#[cfg(target_os = "windows")]
const RELEASE_ASSET: &str = "gallery-dl.exe";
#[cfg(any(target_os = "linux", target_os = "macos"))]
const RELEASE_ASSET: &str = "gallery-dl.bin";
// gallery-dl doesn't provide a standalone macOS binary; users must install via pip/brew.
// We'll still attempt the Codeberg release but fall back gracefully.

pub fn get_gallery_dl_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_bin_dir(app)?.join(BINARY_NAME))
}

pub fn resolve_gallery_dl_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(managed) = get_gallery_dl_path(app) {
        if managed.exists() {
            return Some(managed);
        }
    }
    crate::deps::engine::verify::find_in_system_path(BINARY_NAME)
}

async fn fetch_latest_release(proxy_config: &ProxyConfig) -> Result<GitHubRelease, String> {
    let url = "https://codeberg.org/api/v1/repos/mikf/gallery-dl/releases/latest";
    fetch_json(url, proxy_config)
        .await
        .map_err(|e| e.to_string())
}

pub async fn check_gallery_dl(
    app: AppHandle,
    check_updates: Option<bool>,
) -> Result<DependencyStatus, String> {
    #[cfg(target_os = "android")]
    {
        let _ = check_updates;
        return Ok(DependencyStatus::not_installed());
    }

    #[cfg(not(target_os = "android"))]
    {
        let gdl_path = match resolve_gallery_dl_path(&app) {
            Some(path) => path,
            None => return Ok(DependencyStatus::not_installed()),
        };

        match run_capture_async(&gdl_path, &["--version"]).await {
            Ok(output) if output.status_code == Some(0) => {
                let version = output.stdout.trim().to_string();
                info!("gallery-dl version: {}", version);
                let disk_size = tokio::fs::metadata(&gdl_path).await.ok().map(|m| m.len());

                let update_available = if check_updates.unwrap_or(false) {
                    match fetch_latest_release(&ProxyConfig::default()).await {
                        Ok(release) => {
                            let latest = release.tag_name.trim_start_matches('v').to_string();
                            if crate::deps::updater::is_remote_newer(&latest, &version) {
                                Some(latest)
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
                    DependencyStatus::installed(version, gdl_path.to_string_lossy().to_string())
                        .with_update(update_available)
                        .with_disk_size(disk_size),
                )
            }
            Ok(output) => {
                warn!("gallery-dl exists but failed to run: {}", output.stderr);
                Ok(DependencyStatus::not_installed())
            }
            Err(e) => {
                error!("Failed to execute gallery-dl: {}", e);
                Ok(DependencyStatus {
                    path: Some(gdl_path.to_string_lossy().to_string()),
                    ..DependencyStatus::not_installed()
                })
            }
        }
    }
}

pub async fn install_gallery_dl(
    app: AppHandle,
    version: Option<String>,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        let _ = (version, proxy_config);
        return Err("gallery-dl installation on Android is not supported.".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let config = proxy_config.unwrap_or_default();

        let target_version = match version {
            Some(v) => v,
            None => {
                let release = fetch_latest_release(&config).await?;
                release.tag_name.clone()
            }
        };

        info!("Target gallery-dl version: {}", target_version);

        let gdl_path = get_gallery_dl_path(&app)?;
        let download_url = format!(
            "https://codeberg.org/mikf/gallery-dl/releases/download/{}/{}",
            target_version, RELEASE_ASSET
        );

        installer::run_install(
            &app,
            InstallPlan {
                dep_name: "gallery-dl",
                display_name: "gallery-dl",
                event_name: EVENT_PROGRESS,
                version: target_version,
                download_urls: vec![download_url],
                checksum_urls: vec![],
                checksum_filename_hint: Some(RELEASE_ASSET),
                temp_archive: gdl_path.clone(),
                binary_path: gdl_path,
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

pub async fn uninstall_gallery_dl(app: AppHandle) -> Result<(), String> {
    let gdl_path = get_gallery_dl_path(&app)?;

    if gdl_path.exists() {
        tokio::fs::remove_file(&gdl_path)
            .await
            .map_err(|e| format!("Failed to remove gallery-dl: {}", e))?;
    }

    Ok(())
}

pub async fn get_gallery_dl_releases(
    proxy_config: Option<ProxyConfig>,
) -> Result<Vec<ReleaseInfo>, String> {
    let config = proxy_config.unwrap_or_default();
    let url = "https://codeberg.org/api/v1/repos/mikf/gallery-dl/releases?limit=10";

    let releases: Vec<GitHubRelease> = fetch_json(url, &config)
        .await
        .map_err(|e| format!("Failed to fetch releases: {}", e))?;

    Ok(releases
        .into_iter()
        .map(|r| ReleaseInfo {
            tag: r.tag_name,
            name: r.name,
            published_at: r.published_at,
        })
        .collect())
}
