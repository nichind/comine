use std::path::PathBuf;

use log::{info, warn};
use tauri::{AppHandle, Manager};

use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, InstallProgress};

use crate::deps::engine::cancel;
use crate::deps::engine::checksum::try_fetch_sha256;
use crate::deps::engine::download::{download_file_with_checksum, fetch_json};
use crate::deps::engine::extract::{extract_from_zip, ZipExtractConfig};
use crate::deps::engine::progress::ProgressEmitter;
use crate::deps::engine::verify::run_capture_async;

#[cfg(unix)]
use crate::deps::engine::fs::make_executable;

const EVENT_PROGRESS: &str = "aria2-install-progress";
const FALLBACK_VERSION: &str = "1.37.0";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "aria2c.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "aria2c";

fn get_bin_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    Ok(app_data.join("bin"))
}

pub fn get_aria2_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_bin_dir(app)?.join(BINARY_NAME))
}

#[cfg(target_os = "windows")]
fn is_aria2_file(name: &str) -> bool {
    name.ends_with("/aria2c.exe") || name == "aria2c.exe"
}

#[cfg(target_os = "linux")]
fn is_aria2_file(name: &str) -> bool {
    (name.ends_with("/aria2c") || name == "aria2c") && !name.ends_with(".exe")
}

#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

async fn fetch_latest_version(proxy_config: &ProxyConfig) -> String {
    match fetch_json::<GitHubRelease>(
        "https://api.github.com/repos/aria2/aria2/releases/latest",
        proxy_config,
    )
    .await
    {
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
        let aria2_path = get_aria2_path(&app)?;

        if aria2_path.exists() {
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

                    Ok(DependencyStatus::installed(
                        version,
                        aria2_path.to_string_lossy().to_string(),
                    )
                    .with_disk_size(disk_size))
                }
                _ => Ok(DependencyStatus::not_installed()),
            }
        } else {
            Ok(DependencyStatus::not_installed())
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
        let aria2_path = get_aria2_path(&app)?;
        let bin_dir = get_bin_dir(&app)?;

        tokio::fs::create_dir_all(&bin_dir)
            .await
            .map_err(|e| format!("Failed to create bin directory: {}", e))?;

        let config = proxy_config.unwrap_or_default();
        let progress = ProgressEmitter::new(&app, EVENT_PROGRESS);

        progress.emit(InstallProgress {
            stage: "fetching".to_string(),
            progress: 0,
            downloaded: 0,
            total: 0,
            speed: 0.0,
            message: "Fetching latest aria2 version...".to_string(),
        });

        let version = fetch_latest_version(&config).await;
        info!("Installing aria2 version: {}", version);
        let cancel_token = cancel::reset_token("aria2");

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

        let temp_archive = bin_dir.join("aria2_temp.zip");
        let checksum_urls = vec![
            format!("{}.sha256", download_url),
            format!("{}.sha256sum", download_url),
            format!("{}.sha256.txt", download_url),
            format!("{}.sha256sum.txt", download_url),
        ];
        let expected_sha256 = try_fetch_sha256(&checksum_urls, &config, Some("aria2")).await;

        download_file_with_checksum(
            &download_url,
            &temp_archive,
            &progress,
            "aria2",
            &version,
            Some(&config),
            expected_sha256.as_deref(),
            Some(&cancel_token),
        )
        .await?;

        if cancel_token.is_cancelled() {
            let _ = tokio::fs::remove_file(&temp_archive).await;
            return Err("Cancelled".to_string());
        }

        progress.emit(InstallProgress {
            stage: "extracting".to_string(),
            progress: 90,
            downloaded: 0,
            total: 0,
            speed: 0.0,
            message: "Extracting aria2...".to_string(),
        });

        extract_from_zip(
            &temp_archive,
            &bin_dir,
            ZipExtractConfig {
                matcher: is_aria2_file,
                dest_name: BINARY_NAME,
                extract_all: false,
            },
        )
        .await?;

        let _ = tokio::fs::remove_file(&temp_archive).await;

        #[cfg(unix)]
        {
            make_executable(&aria2_path).await?;
        }

        progress.emit(InstallProgress {
            stage: "verifying".to_string(),
            progress: 95,
            downloaded: 0,
            total: 0,
            speed: 0.0,
            message: "Verifying installation...".to_string(),
        });

        match run_capture_async(&aria2_path, &["--version"]).await {
            Ok(output) if output.status_code == Some(0) => {
                info!("aria2 installed successfully");
            }
            Ok(output) => {
                return Err(format!("aria2 verification failed: {}", output.stderr));
            }
            Err(e) => {
                return Err(format!("aria2 verification failed: {}", e));
            }
        }

        progress.emit(InstallProgress {
            stage: "complete".to_string(),
            progress: 100,
            downloaded: 0,
            total: 0,
            speed: 0.0,
            message: "aria2 installed successfully!".to_string(),
        });

        Ok(aria2_path.to_string_lossy().to_string())
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
