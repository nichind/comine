use std::path::PathBuf;

use log::info;
use tauri::{AppHandle, Manager};

use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, InstallProgress};

use crate::deps::engine::checksum::try_fetch_sha256;
use crate::deps::engine::cancel;
use crate::deps::engine::download::download_file_with_checksum;
use crate::deps::engine::extract::{extract_from_zip, ZipExtractConfig};
use crate::deps::engine::progress::ProgressEmitter;
use crate::deps::engine::verify::run_capture_async;

#[cfg(unix)]
use crate::deps::engine::fs::make_executable;

const EVENT_PROGRESS: &str = "deno-install-progress";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "deno.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "deno";

fn get_bin_dir(app: &AppHandle) -> Result<PathBuf, String> {
	let app_data = app
		.path()
		.app_data_dir()
		.map_err(|e| format!("Failed to get app data dir: {}", e))?;
	Ok(app_data.join("bin"))
}

pub fn get_deno_path(app: &AppHandle) -> Result<PathBuf, String> {
	Ok(get_bin_dir(app)?.join(BINARY_NAME))
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

pub async fn check_deno(app: AppHandle) -> Result<DependencyStatus, String> {
	let deno_path = get_deno_path(&app)?;

	if deno_path.exists() {
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

				Ok(
					DependencyStatus::installed(version, deno_path.to_string_lossy().to_string())
						.with_disk_size(disk_size),
				)
			}
			_ => Ok(DependencyStatus::not_installed()),
		}
	} else {
		Ok(DependencyStatus::not_installed())
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

		let deno_path = get_deno_path(&app)?;
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
			message: "Downloading Deno...".to_string(),
		});

		let config = proxy_config.unwrap_or_default();
		let cancel_token = cancel::reset_token("deno");
		let temp_archive = bin_dir.join("deno_temp.zip");
		let checksum_urls = vec![
			format!("{}.sha256", download_url),
			format!("{}.sha256sum", download_url),
			format!("{}.sha256.txt", download_url),
			format!("{}.sha256sum.txt", download_url),
		];
		let expected_sha256 = try_fetch_sha256(&checksum_urls, &config, Some("deno"))
			.await;

		download_file_with_checksum(
			download_url,
			&temp_archive,
			&progress,
			"Deno",
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
			message: "Extracting Deno...".to_string(),
		});

		extract_from_zip(
			&temp_archive,
			&bin_dir,
			ZipExtractConfig {
				matcher: is_deno_file,
				dest_name: BINARY_NAME,
				extract_all: false,
			},
		)
		.await?;

		let _ = tokio::fs::remove_file(&temp_archive).await;

		#[cfg(unix)]
		{
			make_executable(&deno_path).await?;
		}

		progress.emit(InstallProgress {
			stage: "verifying".to_string(),
			progress: 95,
			downloaded: 0,
			total: 0,
			speed: 0.0,
			message: "Verifying installation...".to_string(),
		});

		match run_capture_async(&deno_path, &["--version"]).await {
			Ok(output) if output.status_code == Some(0) => {
				info!(
					"Deno installed successfully: {}",
					output.stdout.lines().next().unwrap_or("unknown")
				);
			}
			Ok(output) => {
				return Err(format!("Deno verification failed: {}", output.stderr));
			}
			Err(e) => {
				return Err(format!("Failed to run deno: {}", e));
			}
		}

		progress.emit(InstallProgress {
			stage: "complete".to_string(),
			progress: 100,
			downloaded: 0,
			total: 0,
			speed: 0.0,
			message: "Deno installed successfully!".to_string(),
		});

		Ok(deno_path.to_string_lossy().to_string())
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
