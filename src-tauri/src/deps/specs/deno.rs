use std::path::PathBuf;

use tauri::AppHandle;

use crate::proxy::ProxyConfig;
use crate::types::DependencyStatus;

use crate::deps::engine::installer::{self, get_bin_dir, ExtractStrategy, InstallPlan};
use crate::deps::engine::verify::run_capture_async;

const EVENT_PROGRESS: &str = "deno-install-progress";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "deno.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "deno";

pub fn get_deno_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_bin_dir(app)?.join(BINARY_NAME))
}

pub fn resolve_deno_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(managed) = get_deno_path(app) {
        if managed.exists() {
            return Some(managed);
        }
    }
    crate::deps::engine::verify::find_in_system_path(BINARY_NAME)
}

#[cfg(target_os = "windows")]
fn is_deno_file(name: &str) -> bool {
    name == "deno.exe" || name.ends_with("/deno.exe")
}

#[cfg(not(target_os = "windows"))]
fn is_deno_file(name: &str) -> bool {
    name == "deno" || name.ends_with("/deno")
}

fn get_download_url() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
	return "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip";

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
	return "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-apple-darwin.zip";

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
	return "https://github.com/denoland/deno/releases/latest/download/deno-aarch64-apple-darwin.zip";

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
	return "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-unknown-linux-gnu.zip";

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
	return "https://github.com/denoland/deno/releases/latest/download/deno-aarch64-unknown-linux-gnu.zip";

    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
    )))]
    return "";
}

pub async fn check_deno(app: AppHandle, check_updates: Option<bool>) -> Result<DependencyStatus, String> {
    let deno_path = match resolve_deno_path(&app) {
        Some(path) => path,
        None => return Ok(DependencyStatus::not_installed()),
    };

    match run_capture_async(&deno_path, &["--version"]).await {
        Ok(output) if output.status_code == Some(0) => {
            let version = output
                .stdout
                .lines()
                .next()
                .and_then(|line| line.strip_prefix("deno "))
                .map(|v| v.split_whitespace().next().unwrap_or("unknown"))
                .unwrap_or("unknown")
                .to_string();
            let disk_size = tokio::fs::metadata(&deno_path).await.ok().map(|m| m.len());

            let update_available = if check_updates.unwrap_or(false) {
                match installer::fetch_github_latest_release("denoland/deno", &ProxyConfig::default()).await {
                    Ok(release) => {
                        let latest = release.tag_name.trim_start_matches('v').to_string();
                        if crate::deps::updater::is_remote_newer(&latest, &version) { Some(latest) } else { None }
                    }
                    Err(_) => None,
                }
            } else {
                None
            };

            Ok(
                DependencyStatus::installed(version, deno_path.to_string_lossy().to_string())
                    .with_update(update_available)
                    .with_disk_size(disk_size),
            )
        }
        _ => Ok(DependencyStatus::not_installed()),
    }
}

pub async fn install_deno(
    app: AppHandle,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        let _ = proxy_config;
        return Err("Deno installation on Android is not supported.".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let download_url = get_download_url();
        if download_url.is_empty() {
            return Err("Deno installation is not supported on this platform.".to_string());
        }

        let config = proxy_config.unwrap_or_default();
        let deno_path = get_deno_path(&app)?;
        let bin_dir = get_bin_dir(&app)?;

        installer::run_install(
            &app,
            InstallPlan {
                dep_name: "deno",
                display_name: "Deno",
                event_name: EVENT_PROGRESS,
                version: "latest".to_string(),
                download_urls: vec![download_url.to_string()],
                checksum_urls: vec![],
                checksum_filename_hint: Some("deno"),
                temp_archive: bin_dir.join("deno_temp.zip"),
                binary_path: deno_path,
                extract: ExtractStrategy::Zip {
                    matcher: is_deno_file,
                    dest_name: BINARY_NAME,
                },
                extra_executables: vec![],
                verify_args: vec!["--version"],
                custom_verify: None,
            },
            &config,
        )
        .await
    }
}

pub async fn uninstall_deno(app: AppHandle) -> Result<(), String> {
    let deno_path = get_deno_path(&app)?;

    if deno_path.exists() {
        tokio::fs::remove_file(&deno_path)
            .await
            .map_err(|e| format!("Failed to remove deno: {}", e))?;
    }

    Ok(())
}
