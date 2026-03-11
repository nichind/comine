use std::path::PathBuf;

use tauri::AppHandle;
use tracing::info;

use crate::proxy::ProxyConfig;
use crate::types::DependencyStatus;

use crate::deps::engine::installer::{self, get_bin_dir, ExtractStrategy, InstallPlan};
use crate::deps::engine::verify::run_capture_async;

const EVENT_PROGRESS: &str = "lux-install-progress";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "lux.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "lux";

pub fn get_lux_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_bin_dir(app)?.join(BINARY_NAME))
}

pub fn resolve_lux_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(managed) = get_lux_path(app) {
        if managed.exists() {
            return Some(managed);
        }
    }
    crate::deps::engine::verify::find_in_system_path(BINARY_NAME)
}

#[cfg(target_os = "windows")]
fn is_lux_file(name: &str) -> bool {
    name == "lux.exe" || name.ends_with("/lux.exe")
}

fn build_asset_name(version: &str) -> String {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return format!("lux_{}_Windows_x86_64.zip", version);

    #[cfg(all(target_os = "windows", target_arch = "x86"))]
    return format!("lux_{}_Windows_i386.zip", version);

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return format!("lux_{}_Darwin_x86_64.tar.gz", version);

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return format!("lux_{}_Darwin_arm64.tar.gz", version);

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return format!("lux_{}_Linux_x86_64.tar.gz", version);

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return format!("lux_{}_Linux_arm64.tar.gz", version);

    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
    )))]
    return String::new();
}

pub async fn check_lux(
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
        let lux_path = match resolve_lux_path(&app) {
            Some(path) => path,
            None => return Ok(DependencyStatus::not_installed()),
        };

        match run_capture_async(&lux_path, &["-v"]).await {
            Ok(output) if output.status_code == Some(0) => {
                let version = output
                    .stdout
                    .trim()
                    .strip_prefix("lux: version ")
                    .and_then(|s| s.split(',').next())
                    .unwrap_or("installed")
                    .trim()
                    .to_string();
                let disk_size = tokio::fs::metadata(&lux_path).await.ok().map(|m| m.len());

                let update_available = if check_updates.unwrap_or(false) {
                    match installer::fetch_github_latest_release(
                        "iawia002/lux",
                        &ProxyConfig::default(),
                    )
                    .await
                    {
                        Ok(release) => {
                            let latest = release.tag_name.trim_start_matches('v').to_string();
                            if !version.is_empty()
                                && crate::deps::updater::is_remote_newer(&latest, &version)
                            {
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

                Ok(DependencyStatus::installed(
                    if version.is_empty() {
                        "installed".to_string()
                    } else {
                        version
                    },
                    lux_path.to_string_lossy().to_string(),
                )
                .with_update(update_available)
                .with_disk_size(disk_size))
            }
            _ => Ok(DependencyStatus::not_installed()),
        }
    }
}

#[cfg(all(desktop, not(target_os = "windows")))]
async fn extract_tar_gz_lux(
    archive_path: &std::path::Path,
    bin_dir: &std::path::Path,
) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let archive_path = archive_path.to_path_buf();
    let bin_dir = bin_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&archive_path)
            .map_err(|e| format!("Failed to open archive: {}", e))?;

        let tar = GzDecoder::new(file);
        let mut archive = Archive::new(tar);

        for entry in archive
            .entries()
            .map_err(|e| format!("Failed to read tar entries: {}", e))?
        {
            let mut entry = entry.map_err(|e| format!("Failed to read tar entry: {}", e))?;
            let path = entry
                .path()
                .map_err(|e| format!("Failed to get entry path: {}", e))?;

            let name = path.to_string_lossy().to_string();

            if name == "lux" || name.ends_with("/lux") {
                let dest_path = bin_dir.join("lux");
                entry
                    .unpack(&dest_path)
                    .map_err(|e| format!("Failed to extract lux: {}", e))?;
                info!("Extracted lux to {:?}", dest_path);
                break;
            }
        }

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

pub async fn install_lux(
    app: AppHandle,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(mobile)]
    {
        let _ = proxy_config;
        return Err("Lux installation on mobile is not supported.".to_string());
    }

    #[cfg(desktop)]
    {
        let config = proxy_config.unwrap_or_default();
        let release = installer::fetch_github_latest_release("iawia002/lux", &config).await?;

        let version = release.tag_name.trim_start_matches('v').to_string();
        info!("Latest lux version: {}", version);

        let asset_name = build_asset_name(&version);
        if asset_name.is_empty() {
            return Err("Lux installation is not supported on this platform.".to_string());
        }

        let download_url = format!(
            "https://github.com/iawia002/lux/releases/download/{}/{}",
            release.tag_name, asset_name
        );

        let lux_path = get_lux_path(&app)?;
        let bin_dir = get_bin_dir(&app)?;

        #[cfg(target_os = "windows")]
        let extract = ExtractStrategy::Zip {
            matcher: is_lux_file as fn(&str) -> bool,
            dest_name: BINARY_NAME,
        };

        #[cfg(not(target_os = "windows"))]
        let extract = ExtractStrategy::Custom(Box::new(|archive_path, bin_dir| {
            let archive_path = archive_path.to_path_buf();
            let bin_dir = bin_dir.to_path_buf();
            Box::pin(async move { extract_tar_gz_lux(&archive_path, &bin_dir).await })
        }));

        #[cfg(target_os = "windows")]
        let temp_archive = bin_dir.join("lux_temp.zip");
        #[cfg(not(target_os = "windows"))]
        let temp_archive = bin_dir.join("lux_temp.tar.gz");

        installer::run_install(
            &app,
            InstallPlan {
                dep_name: "lux",
                display_name: "Lux",
                event_name: EVENT_PROGRESS,
                version,
                download_urls: vec![download_url],
                checksum_urls: vec![],
                checksum_filename_hint: Some("lux"),
                temp_archive,
                binary_path: lux_path,
                extract,
                extra_executables: vec![],
                verify_args: vec!["-v"],
                custom_verify: None,
            },
            &config,
        )
        .await
    }
}

pub async fn uninstall_lux(app: AppHandle) -> Result<(), String> {
    let lux_path = get_lux_path(&app)?;

    if lux_path.exists() {
        tokio::fs::remove_file(&lux_path)
            .await
            .map_err(|e| format!("Failed to remove Lux: {}", e))?;
    }

    Ok(())
}
