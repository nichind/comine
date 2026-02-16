use std::path::PathBuf;

#[cfg(not(target_os = "android"))]
use tauri::AppHandle;
#[cfg(target_os = "android")]
use tauri::{AppHandle, Manager};

use crate::proxy::ProxyConfig;
use crate::types::DependencyStatus;

use crate::deps::engine::installer::{self, get_bin_dir, ExtractStrategy, InstallPlan};
use crate::deps::engine::verify::run_capture_async;

const EVENT_PROGRESS: &str = "ffmpeg-install-progress";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "ffmpeg.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "ffmpeg";

#[cfg(target_os = "windows")]
const FFPROBE_NAME: &str = "ffprobe.exe";
#[cfg(not(target_os = "windows"))]
const FFPROBE_NAME: &str = "ffprobe";

fn get_binary_path(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    {
        let bin_dir = get_bin_dir(app)?;
        let custom_path = bin_dir.join(name);
        if custom_path.exists() {
            return Ok(custom_path);
        }

        let app_data = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {}", e))?;

        if let Some(parent) = app_data.parent() {
            let lib_path = parent.join("no_backup").join(name);
            if lib_path.exists() {
                return Ok(lib_path);
            }
        }

        Ok(custom_path)
    }

    #[cfg(not(target_os = "android"))]
    Ok(get_bin_dir(app)?.join(name))
}

pub fn get_ffmpeg_path(app: &AppHandle) -> Result<PathBuf, String> {
    get_binary_path(app, BINARY_NAME)
}

pub fn get_ffprobe_path(app: &AppHandle) -> Result<PathBuf, String> {
    get_binary_path(app, FFPROBE_NAME)
}

pub fn resolve_ffmpeg_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(managed) = get_ffmpeg_path(app) {
        if managed.exists() {
            return Some(managed);
        }
    }
    crate::deps::engine::verify::find_in_system_path(BINARY_NAME)
}

pub fn resolve_ffprobe_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(managed) = get_ffprobe_path(app) {
        if managed.exists() {
            return Some(managed);
        }
    }
    crate::deps::engine::verify::find_in_system_path(FFPROBE_NAME)
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

pub async fn check_ffmpeg(app: AppHandle, check_updates: Option<bool>) -> Result<DependencyStatus, String> {
    let ffmpeg_path = match resolve_ffmpeg_path(&app) {
        Some(path) => path,
        None => {
            #[cfg(target_os = "android")]
            return Ok(DependencyStatus::embedded("youtubedl-android library"));

            #[cfg(not(target_os = "android"))]
            return Ok(DependencyStatus::not_installed());
        }
    };
    #[cfg(not(target_os = "android"))]
    let ffprobe_path = resolve_ffprobe_path(&app);

    let _ = check_updates;

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

            #[cfg(not(target_os = "android"))]
            let disk_size = {
                let ffmpeg_size = tokio::fs::metadata(&ffmpeg_path)
                    .await
                    .ok()
                    .map(|m| m.len())
                    .unwrap_or(0);
                let ffprobe_size = match &ffprobe_path {
                    Some(p) => tokio::fs::metadata(p)
                        .await
                        .ok()
                        .map(|m| m.len())
                        .unwrap_or(0),
                    None => 0,
                };
                Some(ffmpeg_size.saturating_add(ffprobe_size))
            };

            #[cfg(target_os = "android")]
            let disk_size = None;

            Ok(
                DependencyStatus::installed(version, ffmpeg_path.to_string_lossy().to_string())
                    .with_disk_size(disk_size),
            )
        }
        _ => {
            #[cfg(target_os = "android")]
            return Ok(DependencyStatus::embedded(
                "youtubedl-android library (not found in path)",
            ));
            #[cfg(not(target_os = "android"))]
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

    let output = crate::utils::new_command("tar")
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
        let config = proxy_config.unwrap_or_default();
        let ffmpeg_path = get_ffmpeg_path(&app)?;
        let ffprobe_path = get_ffprobe_path(&app)?;
        let bin_dir = get_bin_dir(&app)?;

        #[cfg(target_os = "windows")]
		let download_url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
        #[cfg(target_os = "macos")]
        let download_url = "https://evermeet.cx/ffmpeg/getrelease/zip";
        #[cfg(target_os = "linux")]
		let download_url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz";

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let extract = ExtractStrategy::ZipMultiple {
            matchers: vec![
                (is_ffmpeg_file as fn(&str) -> bool, BINARY_NAME),
                (is_ffprobe_file as fn(&str) -> bool, FFPROBE_NAME),
            ],
        };

        #[cfg(target_os = "linux")]
        let extract = ExtractStrategy::Custom(Box::new(|archive_path, bin_dir| {
            let archive_path = archive_path.to_path_buf();
            let bin_dir = bin_dir.to_path_buf();
            Box::pin(async move { extract_tar_xz_ffmpeg(&archive_path, &bin_dir).await })
        }));

        installer::run_install(
            &app,
            InstallPlan {
                dep_name: "ffmpeg",
                display_name: "ffmpeg",
                event_name: EVENT_PROGRESS,
                version: "latest".to_string(),
                download_urls: vec![download_url.to_string()],
                checksum_urls: vec![],
                checksum_filename_hint: Some("ffmpeg"),
                temp_archive: bin_dir.join("ffmpeg_temp.archive"),
                binary_path: ffmpeg_path,
                extract,
                extra_executables: vec![ffprobe_path],
                verify_args: vec!["-version"],
                custom_verify: None,
            },
            &config,
        )
        .await
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
