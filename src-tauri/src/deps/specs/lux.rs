use std::path::PathBuf;

use log::info;
use tauri::{AppHandle, Manager};

use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, InstallProgress};

use crate::deps::engine::checksum::try_fetch_sha256;
use crate::deps::engine::cancel;
use crate::deps::engine::download::{download_file_with_checksum, fetch_json};
use crate::deps::engine::progress::ProgressEmitter;
use crate::deps::engine::verify::run_capture_async;

#[cfg(target_os = "windows")]
use crate::deps::engine::extract::{extract_from_zip, ZipExtractConfig};

#[cfg(unix)]
use crate::deps::engine::fs::make_executable;

const EVENT_PROGRESS: &str = "lux-install-progress";

#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "lux.exe";
#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "lux";

fn get_bin_dir(app: &AppHandle) -> Result<PathBuf, String> {
	let app_data = app
		.path()
		.app_data_dir()
		.map_err(|e| format!("Failed to get app data dir: {}", e))?;
	Ok(app_data.join("bin"))
}

pub fn get_lux_path(app: &AppHandle) -> Result<PathBuf, String> {
	Ok(get_bin_dir(app)?.join(BINARY_NAME))
}

#[cfg(target_os = "windows")]
fn is_lux_file(name: &str) -> bool {
	name == "lux.exe" || name.ends_with("/lux.exe")
}

#[derive(serde::Deserialize)]
struct GitHubRelease {
	tag_name: String,
}

async fn fetch_latest_release(proxy_config: &ProxyConfig) -> Result<GitHubRelease, String> {
	fetch_json::<GitHubRelease>(
		"https://api.github.com/repos/iawia002/lux/releases/latest",
		proxy_config,
	)
	.await
	.map_err(|e| format!("Failed to fetch lux release: {}", e))
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

pub async fn check_lux(app: AppHandle) -> Result<DependencyStatus, String> {
	#[cfg(target_os = "android")]
	{
		return Ok(DependencyStatus::not_installed());
	}

	#[cfg(not(target_os = "android"))]
	{
		let lux_path = get_lux_path(&app)?;

		if lux_path.exists() {
			match run_capture_async(&lux_path, &["-v"]).await {
				Ok(output) if output.status_code == Some(0) => {
					// Format: "lux: version X.Y.Z, A fast and simple..."
					let version = output
						.stdout
						.trim()
						.strip_prefix("lux: version ")
						.and_then(|s| s.split(',').next())
						.unwrap_or("installed")
						.trim()
						.to_string();
					let disk_size = tokio::fs::metadata(&lux_path).await.ok().map(|m| m.len());

					Ok(
						DependencyStatus::installed(
							if version.is_empty() {
								"installed".to_string()
							} else {
								version
							},
							lux_path.to_string_lossy().to_string(),
						)
						.with_disk_size(disk_size),
					)
				}
				_ => Ok(DependencyStatus::not_installed()),
			}
		} else {
			Ok(DependencyStatus::not_installed())
		}
	}
}

#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
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
	#[cfg(target_os = "android")]
	{
		let _ = proxy_config;
		return Err("Lux installation on Android is not supported.".to_string());
	}

	#[cfg(not(target_os = "android"))]
	{
		let lux_path = get_lux_path(&app)?;
		let bin_dir = get_bin_dir(&app)?;

		tokio::fs::create_dir_all(&bin_dir)
			.await
			.map_err(|e| format!("Failed to create bin directory: {}", e))?;

		let progress = ProgressEmitter::new(&app, EVENT_PROGRESS);

		progress.emit(InstallProgress {
			stage: "fetching".to_string(),
			progress: 0,
			downloaded: 0,
			total: 0,
			speed: 0.0,
			message: "Fetching latest lux release...".to_string(),
		});

		let config = proxy_config.unwrap_or_default();
		let release = fetch_latest_release(&config).await?;

		let version = release.tag_name.trim_start_matches('v');
		info!("Latest lux version: {}", version);

		let asset_name = build_asset_name(version);
		if asset_name.is_empty() {
			return Err("Lux installation is not supported on this platform.".to_string());
		}

		let download_url = format!(
			"https://github.com/iawia002/lux/releases/download/{}/{}",
			release.tag_name, asset_name
		);

		info!("Downloading lux from: {}", download_url);

		#[cfg(target_os = "windows")]
		let temp_archive = bin_dir.join("lux_temp.zip");
		#[cfg(not(target_os = "windows"))]
		let temp_archive = bin_dir.join("lux_temp.tar.gz");

		let checksum_urls = vec![
			format!("{}.sha256", download_url),
			format!("{}.sha256sum", download_url),
			format!("{}.sha256.txt", download_url),
			format!("{}.sha256sum.txt", download_url),
		];
		let expected_sha256 = try_fetch_sha256(&checksum_urls, &config, Some("lux"))
			.await;
		let cancel_token = cancel::reset_token("lux");

		download_file_with_checksum(
			&download_url,
			&temp_archive,
			&progress,
			"Lux",
			version,
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
			message: "Extracting lux...".to_string(),
		});

		#[cfg(target_os = "windows")]
		{
			extract_from_zip(
				&temp_archive,
				&bin_dir,
				ZipExtractConfig {
					matcher: is_lux_file,
					dest_name: BINARY_NAME,
					extract_all: false,
				},
			)
			.await?;
		}

		#[cfg(not(target_os = "windows"))]
		{
			extract_tar_gz_lux(&temp_archive, &bin_dir).await?;
		}

		let _ = tokio::fs::remove_file(&temp_archive).await;

		#[cfg(unix)]
		{
			make_executable(&lux_path).await?;
		}

		progress.emit(InstallProgress {
			stage: "verifying".to_string(),
			progress: 95,
			downloaded: 0,
			total: 0,
			speed: 0.0,
			message: "Verifying installation...".to_string(),
		});

		match run_capture_async(&lux_path, &["-v"]).await {
			Ok(output) if output.status_code == Some(0) => {
				info!("Lux installed successfully");
			}
			Ok(output) => {
				return Err(format!("Lux verification failed: {}", output.stderr));
			}
			Err(e) => {
				return Err(format!("Failed to run lux: {}", e));
			}
		}

		progress.emit(InstallProgress {
			stage: "complete".to_string(),
			progress: 100,
			downloaded: 0,
			total: 0,
			speed: 0.0,
			message: "Lux installed successfully!".to_string(),
		});

		Ok(lux_path.to_string_lossy().to_string())
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
