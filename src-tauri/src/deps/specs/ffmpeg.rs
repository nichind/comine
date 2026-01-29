use std::path::PathBuf;

use log::info;
use tauri::{AppHandle, Manager};

use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, InstallProgress};

use crate::deps::engine::cancel;
use crate::deps::engine::checksum::try_fetch_sha256;
use crate::deps::engine::download::download_file_with_checksum;
use crate::deps::engine::extract::{extract_from_zip_multiple, FileMatcher};
use crate::deps::engine::progress::ProgressEmitter;
use crate::deps::engine::verify::run_capture_async;

#[cfg(unix)]
use crate::deps::engine::fs::make_executable;

const EVENT_PROGRESS: &str = "ffmpeg-install-progress";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "ffmpeg.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "ffmpeg";

#[cfg(target_os = "windows")]
const FFPROBE_NAME: &str = "ffprobe.exe";
#[cfg(not(target_os = "windows"))]
const FFPROBE_NAME: &str = "ffprobe";

fn get_bin_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    Ok(app_data.join("bin"))
}

pub fn get_ffmpeg_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_bin_dir(app)?.join(BINARY_NAME))
}

fn get_ffprobe_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_bin_dir(app)?.join(FFPROBE_NAME))
}

#[cfg(target_os = "windows")]
fn is_ffmpeg_file(name: &str) -> bool {
    name.ends_with("/ffmpeg.exe") || name == "ffmpeg.exe"
}

#[cfg(target_os = "windows")]
fn is_ffprobe_file(name: &str) -> bool {
    name.ends_with("/ffprobe.exe") || name == "ffprobe.exe"
}

#[cfg(not(target_os = "windows"))]
fn is_ffmpeg_file(name: &str) -> bool {
    (name.ends_with("/ffmpeg") || name == "ffmpeg") && !name.ends_with(".exe")
}

#[cfg(not(target_os = "windows"))]
fn is_ffprobe_file(name: &str) -> bool {
    (name.ends_with("/ffprobe") || name == "ffprobe") && !name.ends_with(".exe")
}

pub async fn check_ffmpeg(app: AppHandle) -> Result<DependencyStatus, String> {
    #[cfg(target_os = "android")]
    {
        return Ok(DependencyStatus::embedded("youtubedl-android library"));
    }

    #[cfg(not(target_os = "android"))]
    {
        let ffmpeg_path = get_ffmpeg_path(&app)?;
        let ffprobe_path = get_ffprobe_path(&app)?;

        if ffmpeg_path.exists() {
            match run_capture_async(&ffmpeg_path, &["-version"]).await {
                Ok(output) if output.status_code == Some(0) => {
                    let version = output
                        .stdout
                        .lines()
                        .next()
                        .and_then(|line| line.strip_prefix("ffmpeg version "))
                        .map(|v| v.split_whitespace().next().unwrap_or("unknown"))
                        .unwrap_or("unknown")
                        .to_string();

                    let disk_size = {
                        let ffmpeg_size = tokio::fs::metadata(&ffmpeg_path)
                            .await
                            .ok()
                            .map(|m| m.len())
                            .unwrap_or(0);
                        let ffprobe_size = tokio::fs::metadata(&ffprobe_path)
                            .await
                            .ok()
                            .map(|m| m.len())
                            .unwrap_or(0);
                        Some(ffmpeg_size.saturating_add(ffprobe_size))
                    };

                    Ok(DependencyStatus::installed(
                        version,
                        ffmpeg_path.to_string_lossy().to_string(),
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

#[cfg(target_os = "linux")]
async fn extract_tar_xz_ffmpeg(
    archive_path: &std::path::Path,
    bin_dir: &std::path::Path,
) -> Result<(), String> {
    use std::process::Stdio;

    let output = tokio::process::Command::new("tar")
        .args([
            "-xJf",
            archive_path.to_str().ok_or("Invalid path")?,
            "-C",
            bin_dir.to_str().ok_or("Invalid path")?,
            "--strip-components=2",
            "--wildcards",
            "*/bin/ffmpeg",
            "*/bin/ffprobe",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to run tar: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tar extraction failed: {}", stderr));
    }

    Ok(())
}

pub async fn install_ffmpeg(
    app: AppHandle,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = proxy_config;
        return Err(
			"ffmpeg installation is not supported on this platform. On Android, install via Termux."
				.to_string(),
		);
    }

    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        let ffmpeg_path = get_ffmpeg_path(&app)?;
        let ffprobe_path = get_ffprobe_path(&app)?;
        let bin_dir = get_bin_dir(&app)?;

        tokio::fs::create_dir_all(&bin_dir)
            .await
            .map_err(|e| format!("Failed to create bin directory: {}", e))?;

        let progress = ProgressEmitter::new(&app, EVENT_PROGRESS);

        progress.emit(InstallProgress {
            stage: "downloading".to_string(),
            progress: 0,
            downloaded: 0,
            total: 0,
            speed: 0.0,
            message: "Downloading ffmpeg...".to_string(),
        });

        #[cfg(target_os = "windows")]
		let download_url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
        #[cfg(target_os = "macos")]
        let download_url = "https://evermeet.cx/ffmpeg/getrelease/zip";
        #[cfg(target_os = "linux")]
		let download_url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz";

        let temp_archive = bin_dir.join("ffmpeg_temp.archive");
        let config = proxy_config.unwrap_or_default();
        let cancel_token = cancel::reset_token("ffmpeg");
        let checksum_urls = vec![
            format!("{}.sha256", download_url),
            format!("{}.sha256sum", download_url),
            format!("{}.sha256.txt", download_url),
            format!("{}.sha256sum.txt", download_url),
        ];
        let expected_sha256 = try_fetch_sha256(&checksum_urls, &config, Some("ffmpeg")).await;

        download_file_with_checksum(
            download_url,
            &temp_archive,
            &progress,
            "ffmpeg",
            "latest",
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
            message: "Extracting ffmpeg...".to_string(),
        });

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let matchers: &[(FileMatcher, &'static str)] = &[
                (is_ffmpeg_file, BINARY_NAME),
                (is_ffprobe_file, FFPROBE_NAME),
            ];
            extract_from_zip_multiple(&temp_archive, &bin_dir, matchers).await?;
        }

        #[cfg(target_os = "linux")]
        {
            extract_tar_xz_ffmpeg(&temp_archive, &bin_dir).await?;
        }

        let _ = tokio::fs::remove_file(&temp_archive).await;

        #[cfg(unix)]
        {
            make_executable(&ffmpeg_path).await?;
            if ffprobe_path.exists() {
                make_executable(&ffprobe_path).await?;
            }
        }

        progress.emit(InstallProgress {
            stage: "verifying".to_string(),
            progress: 95,
            downloaded: 0,
            total: 0,
            speed: 0.0,
            message: "Verifying installation...".to_string(),
        });

        match run_capture_async(&ffmpeg_path, &["-version"]).await {
            Ok(output) if output.status_code == Some(0) => {
                info!("ffmpeg installed successfully");
            }
            Ok(output) => {
                return Err(format!("ffmpeg verification failed: {}", output.stderr));
            }
            Err(e) => {
                return Err(format!("ffmpeg verification failed: {}", e));
            }
        }

        progress.emit(InstallProgress {
            stage: "complete".to_string(),
            progress: 100,
            downloaded: 0,
            total: 0,
            speed: 0.0,
            message: "ffmpeg installed successfully!".to_string(),
        });

        Ok(ffmpeg_path.to_string_lossy().to_string())
    }
}

pub async fn uninstall_ffmpeg(app: AppHandle) -> Result<(), String> {
    let ffmpeg_path = get_ffmpeg_path(&app)?;
    let ffprobe_path = get_ffprobe_path(&app)?;

    if ffmpeg_path.exists() {
        tokio::fs::remove_file(&ffmpeg_path)
            .await
            .map_err(|e| format!("Failed to remove ffmpeg: {}", e))?;
    }

    if ffprobe_path.exists() {
        let _ = tokio::fs::remove_file(&ffprobe_path).await;
    }

    Ok(())
}
