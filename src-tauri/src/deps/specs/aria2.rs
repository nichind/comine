use std::path::PathBuf;

use tauri::AppHandle;
use tracing::{info, warn};

use crate::proxy::ProxyConfig;
use crate::types::DependencyStatus;

use crate::deps::engine::installer::{self, get_bin_dir, ExtractStrategy, InstallPlan};
use crate::deps::engine::verify::run_capture_async;

const EVENT_PROGRESS: &str = "aria2-install-progress";
const FALLBACK_VERSION: &str = "1.37.0";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "aria2c.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "aria2c";

pub fn get_aria2_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_bin_dir(app)?.join(BINARY_NAME))
}

pub fn resolve_aria2_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(managed) = get_aria2_path(app) {
        if managed.exists() {
            return Some(managed);
        }
    }
    crate::deps::engine::verify::find_in_system_path(BINARY_NAME)
}

#[cfg(target_os = "windows")]
fn is_aria2_file(name: &str) -> bool {
    name.ends_with("/aria2c.exe") || name == "aria2c.exe"
}

#[cfg(target_os = "linux")]
fn is_aria2_file(name: &str) -> bool {
    (name.ends_with("/aria2c") || name == "aria2c") && !name.ends_with(".exe")
}

async fn fetch_latest_version(proxy_config: &ProxyConfig) -> String {
    match installer::fetch_github_latest_release("aria2/aria2", proxy_config).await {
        Ok(release) => release
            .tag_name
            .strip_prefix("release-")
            .unwrap_or(&release.tag_name)
            .to_string(),
        Err(e) => {
            warn!(
                "Failed to fetch latest aria2 version, using fallback: {}",
                e
            );
            FALLBACK_VERSION.to_string()
        }
    }
}

pub async fn check_aria2(app: AppHandle) -> Result<DependencyStatus, String> {
    #[cfg(target_os = "android")]
    {
        return Ok(DependencyStatus::embedded("youtubedl-android library"));
    }

    #[cfg(not(target_os = "android"))]
    {
        let aria2_path = match resolve_aria2_path(&app) {
            Some(path) => path,
            None => return Ok(DependencyStatus::not_installed()),
        };

        match run_capture_async(&aria2_path, &["--version"]).await {
            Ok(output) if output.status_code == Some(0) => {
                let version = output
                    .stdout
                    .lines()
                    .next()
                    .and_then(|line| line.strip_prefix("aria2 version "))
                    .unwrap_or("unknown")
                    .to_string();
                let disk_size = tokio::fs::metadata(&aria2_path).await.ok().map(|m| m.len());

                Ok(
                    DependencyStatus::installed(version, aria2_path.to_string_lossy().to_string())
                        .with_disk_size(disk_size),
                )
            }
            _ => Ok(DependencyStatus::not_installed()),
        }
    }
}

pub async fn install_aria2(
    app: AppHandle,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let _ = proxy_config;
        return Err(
            "aria2 installation on macOS: please install via 'brew install aria2'".to_string(),
        );
    }

    #[cfg(target_os = "android")]
    {
        let _ = proxy_config;
        return Err(
            "aria2 installation on Android is not supported. Please install via Termux."
                .to_string(),
        );
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = proxy_config;
        return Err("aria2 installation is not supported on this platform.".to_string());
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        let config = proxy_config.unwrap_or_default();
        let version = fetch_latest_version(&config).await;
        info!("Installing aria2 version: {}", version);

        let bin_dir = get_bin_dir(&app)?;
        let aria2_path = get_aria2_path(&app)?;

        #[cfg(target_os = "windows")]
        let download_url = format!(
            "https://github.com/aria2/aria2/releases/download/release-{}/aria2-{}-win-64bit-build1.zip",
            version, version
        );

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let download_url = format!(
            "https://github.com/abcfy2/aria2-static-build/releases/download/{}/aria2-x86_64-linux-musl_static.zip",
            version
        );

        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let download_url = format!(
            "https://github.com/abcfy2/aria2-static-build/releases/download/{}/aria2-aarch64-linux-musl_static.zip",
            version
        );

        installer::run_install(
            &app,
            InstallPlan {
                dep_name: "aria2",
                display_name: "aria2",
                event_name: EVENT_PROGRESS,
                version,
                download_urls: vec![download_url],
                checksum_urls: vec![],
                checksum_filename_hint: Some("aria2"),
                temp_archive: bin_dir.join("aria2_temp.zip"),
                binary_path: aria2_path,
                extract: ExtractStrategy::Zip {
                    matcher: is_aria2_file,
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

pub async fn uninstall_aria2(app: AppHandle) -> Result<(), String> {
    let aria2_path = get_aria2_path(&app)?;

    if aria2_path.exists() {
        tokio::fs::remove_file(&aria2_path)
            .await
            .map_err(|e| format!("Failed to remove aria2: {}", e))?;
    }

    Ok(())
}
