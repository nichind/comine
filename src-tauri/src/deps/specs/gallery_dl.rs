use std::path::PathBuf;

use tauri::AppHandle;
use tracing::{error, info, warn};

use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, ReleaseInfo};

use crate::deps::engine::download::fetch_json;
use crate::deps::engine::installer::{self, get_bin_dir, GitHubRelease};
#[cfg(all(desktop, not(target_os = "macos")))]
use crate::deps::engine::installer::{ExtractStrategy, InstallPlan};
use crate::deps::engine::verify::run_capture_async;

const EVENT_PROGRESS: &str = "gallery-dl-install-progress";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "gallery-dl.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "gallery-dl";

// gallery-dl only ships a standalone binary for Windows and Linux.
// The "gallery-dl.bin" release asset is a Linux ELF binary — it cannot run on macOS.
// On macOS, users must install gallery-dl via pip or Homebrew; we detect it in the system PATH.
#[cfg(target_os = "windows")]
const RELEASE_ASSET: &str = "gallery-dl.exe";
#[cfg(target_os = "linux")]
const RELEASE_ASSET: &str = "gallery-dl.bin";

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
    installer::fetch_github_latest_release("mikf/gallery-dl", proxy_config).await
}

/// Delete a managed gallery-dl binary that failed to execute.
///
/// This clears the stale/corrupt file so that the next install attempt downloads
/// a fresh binary rather than encountering the same "cannot execute binary file" loop.
/// System-PATH binaries are never touched — we only remove our own managed copy.
async fn delete_managed_binary_if_corrupt(managed_path: &PathBuf, fail_reason: &str) {
    if !managed_path.exists() {
        return;
    }
    warn!(
        "gallery-dl managed binary failed to run ({}); deleting {:?} so next install \
         fetches a fresh copy",
        fail_reason, managed_path
    );
    if let Err(e) = tokio::fs::remove_file(managed_path).await {
        warn!(
            "Failed to delete corrupt gallery-dl binary at {:?}: {}",
            managed_path, e
        );
    } else {
        info!("Deleted corrupt gallery-dl binary — re-download will be required");
    }
}

pub async fn check_gallery_dl(
    app: AppHandle,
    check_updates: Option<bool>,
) -> Result<DependencyStatus, String> {
    #[cfg(mobile)]
    {
        let _ = check_updates;
        return Ok(DependencyStatus::not_installed());
    }

    #[cfg(desktop)]
    {
        let gdl_path = match resolve_gallery_dl_path(&app) {
            Some(path) => path,
            None => return Ok(DependencyStatus::not_installed()),
        };

        // Determine whether this is the managed binary (in our bin dir) so we know
        // whether it is safe to delete on failure.
        let managed_path = get_gallery_dl_path(&app).ok();
        let is_managed = managed_path
            .as_ref()
            .is_some_and(|m| m == &gdl_path);

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
                // Binary exists but could not be executed — most commonly caused by an
                // architecture mismatch (e.g., Linux ELF downloaded on macOS, or wrong CPU
                // arch). Delete the managed copy so the next install re-downloads correctly.
                warn!(
                    "gallery-dl exists but failed to run: {}",
                    output.stderr
                );
                if is_managed {
                    if let Some(ref mp) = managed_path {
                        delete_managed_binary_if_corrupt(mp, &output.stderr).await;
                    }
                }
                Ok(DependencyStatus::not_installed())
            }
            Err(e) => {
                error!("Failed to execute gallery-dl: {}", e);
                // An OS-level execution error (e.g., ENOEXEC) also indicates a corrupt or
                // incompatible binary — clean it up if it is our managed copy.
                if is_managed {
                    if let Some(ref mp) = managed_path {
                        delete_managed_binary_if_corrupt(mp, &e).await;
                    }
                }
                Ok(DependencyStatus::not_installed())
            }
        }
    }
}

pub async fn install_gallery_dl(
    app: AppHandle,
    version: Option<String>,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(mobile)]
    {
        let _ = (version, proxy_config);
        return Err("gallery-dl installation on mobile is not supported.".to_string());
    }

    // gallery-dl does not publish a standalone macOS binary in its GitHub releases.
    // The "gallery-dl.bin" asset is a Linux ELF and cannot run on macOS regardless of
    // architecture. Users must install via `pip install gallery-dl` or `brew install gallery-dl`.
    #[cfg(target_os = "macos")]
    {
        let _ = (version, proxy_config, app);
        return Err(
            "gallery-dl does not provide a standalone macOS binary. \
             Please install it via Homebrew (`brew install gallery-dl`) \
             or pip (`pip install gallery-dl`)."
                .to_string(),
        );
    }

    #[cfg(all(desktop, not(target_os = "macos")))]
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
            "https://github.com/mikf/gallery-dl/releases/download/{}/{}",
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
    let url = "https://api.github.com/repos/mikf/gallery-dl/releases?per_page=10";

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
