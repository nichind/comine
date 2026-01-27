use std::path::{Path, PathBuf};

use log::{info, warn};
use tauri::{AppHandle, Manager};

use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, InstallProgress};

use crate::deps::engine::checksum::try_fetch_sha256;
use crate::deps::engine::cancel;
use crate::deps::engine::download::download_file_with_checksum;
use crate::deps::engine::extract::{extract_from_zip, ZipExtractConfig};
use crate::deps::engine::progress::ProgressEmitter;
use crate::deps::engine::verify::run_capture_async;

use regex::Regex;

#[cfg(unix)]
use crate::deps::engine::fs::make_executable;

const EVENT_PROGRESS: &str = "quickjs-install-progress";
const FALLBACK_VERSION: &str = "2025-09-13";

const QUICKJS_MIRROR_BASE_ENV: &str = "COMINE_QUICKJS_MIRROR_BASE";

fn get_bin_dir(app: &AppHandle) -> Result<PathBuf, String> {
	let app_data = app
		.path()
		.app_data_dir()
		.map_err(|e| format!("Failed to get app data dir: {}", e))?;
	Ok(app_data.join("bin"))
}

pub fn get_quickjs_path(app: &AppHandle) -> Result<PathBuf, String> {
	#[cfg(target_os = "windows")]
	let binary_name = "qjs.exe";
	#[cfg(not(target_os = "windows"))]
	let binary_name = "qjs";

	Ok(get_bin_dir(app)?.join(binary_name))
}

#[cfg(target_os = "windows")]
fn is_quickjs_file(name: &str) -> bool {
	name == "qjs"
		|| name == "qjs.com"
		|| name == "qjs.exe"
		|| name.ends_with("/qjs")
		|| name.ends_with("/qjs.com")
		|| name.ends_with("/qjs.exe")
}

#[cfg(not(target_os = "windows"))]
fn is_quickjs_file(name: &str) -> bool {
	(name == "qjs" || name.ends_with("/qjs") || name == "qjs.com" || name.ends_with("/qjs.com"))
		&& !name.ends_with(".exe")
}

async fn verify_quickjs(quickjs_path: &Path) -> Result<(), String> {
	use crate::deps::engine::verify::{run_capture_async, CmdOutputAsync};

	let attempts: &[(&str, &[&str])] = &[
		("std_print", &["--std", "-e", "print('ok')"]),
		("print", &["-e", "print('ok')"]),
	];

	let mut errors: Vec<String> = Vec::new();

	for (label, args) in attempts {
		match run_capture_async(quickjs_path, args).await {
			Ok(CmdOutputAsync { status_code, stdout, stderr }) => {
				if status_code == Some(0) && stdout.contains("ok") {
					return Ok(());
				}
				errors.push(format!(
					"attempt={} exit={:?} stdout={} stderr={}",
					label,
					status_code,
					stdout.trim(),
					stderr.trim()
				));
			}
			Err(e) => {
				errors.push(format!("attempt={} spawn_error={}", label, e));
			}
		}
	}

	Err(errors.join(" | "))
}

fn build_download_url(version: &str) -> String {
	// Use Cosmopolitan build for Windows - it's statically linked and works on all
	// Windows versions without requiring libwinpthread-1.dll or other MinGW DLLs
	#[cfg(target_os = "windows")]
	let url = format!(
		"https://bellard.org/quickjs/binary_releases/quickjs-cosmo-{}.zip",
		version
	);
	#[cfg(target_os = "macos")]
	let url = format!(
		"https://bellard.org/quickjs/binary_releases/quickjs-cosmo-{}.zip",
		version
	);
	#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
	let url = format!(
		"https://bellard.org/quickjs/binary_releases/quickjs-linux-x86_64-{}.zip",
		version
	);
	#[cfg(all(target_os = "linux", target_arch = "x86"))]
	let url = format!(
		"https://bellard.org/quickjs/binary_releases/quickjs-linux-i686-{}.zip",
		version
	);
	#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
	let url = format!(
		"https://bellard.org/quickjs/binary_releases/quickjs-cosmo-{}.zip",
		version
	);

	#[cfg(not(any(
		target_os = "windows",
		target_os = "macos",
		all(target_os = "linux", target_arch = "x86_64"),
		all(target_os = "linux", target_arch = "x86"),
		all(target_os = "linux", target_arch = "aarch64")
	)))]
	let url = format!(
		"https://bellard.org/quickjs/binary_releases/quickjs-cosmo-{}.zip",
		version
	);

	url
}

fn build_download_urls(version: &str) -> Vec<String> {
	let mut urls = Vec::new();

	if let Ok(base) = std::env::var(QUICKJS_MIRROR_BASE_ENV) {
		let base = base.trim().trim_end_matches('/');
		if !base.is_empty() {
			urls.push(format!("{}/quickjs-cosmo-{}.zip", base, version));
		}
	}

	urls.push(build_download_url(version));
	urls
}

async fn fetch_latest_version(proxy_config: &ProxyConfig) -> String {
	use crate::deps::engine::download::fetch_json;

	#[derive(serde::Deserialize)]
	struct LatestVersion {
		version: String,
	}

	match fetch_json::<LatestVersion>(
		"https://bellard.org/quickjs/binary_releases/LATEST.json",
		proxy_config,
	)
	.await
	{
		Ok(latest) => latest.version,
		Err(e) => {
			warn!("Failed to fetch QuickJS LATEST.json: {}, using fallback", e);
			FALLBACK_VERSION.to_string()
		}
	}
}

pub async fn check_quickjs(app: AppHandle) -> Result<DependencyStatus, String> {
	let quickjs_path = get_quickjs_path(&app)?;

	if quickjs_path.exists() {
		match verify_quickjs(&quickjs_path).await {
			Ok(()) => {
				let disk_size = tokio::fs::metadata(&quickjs_path).await.ok().map(|m| m.len());

				let version = match run_capture_async(&quickjs_path, &["-h"]).await {
					Ok(output) if output.status_code == Some(0) => {
						let text = format!("{}\n{}", output.stdout, output.stderr);
						let re = Regex::new(r"QuickJS version\s+([0-9]{4}-[0-9]{2}-[0-9]{2})")
							.ok();
						re
							.and_then(|re| re.captures(&text))
							.and_then(|c| c.get(1))
							.map(|m| m.as_str().to_string())
							.unwrap_or_else(|| "installed".to_string())
					}
					_ => "installed".to_string(),
				};

				Ok(
					DependencyStatus::installed(version, quickjs_path.to_string_lossy().to_string())
						.with_disk_size(disk_size),
				)
			}
			Err(_) => Ok(DependencyStatus::not_installed()),
		}
	} else {
		Ok(DependencyStatus::not_installed())
	}
}

pub async fn install_quickjs(
	app: AppHandle,
	proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
	let quickjs_path = get_quickjs_path(&app)?;
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
		message: "Fetching latest QuickJS version...".to_string(),
	});

	let config = proxy_config.unwrap_or_default();
	let cancel_token = cancel::reset_token("quickjs");
	let version = fetch_latest_version(&config).await;

	info!("QuickJS latest version: {}", version);

	let temp_archive = bin_dir.join(format!("quickjs_{}.zip", uuid::Uuid::new_v4()));

	let download_urls = build_download_urls(&version);
	let mut download_errors: Vec<String> = Vec::new();
	let mut downloaded = false;

	for url in &download_urls {
		if cancel_token.is_cancelled() {
			let _ = tokio::fs::remove_file(&temp_archive).await;
			return Err("Cancelled".to_string());
		}

		let checksum_urls = vec![
			format!("{}.sha256", url),
			format!("{}.sha256sum", url),
			format!("{}.sha256.txt", url),
			format!("{}.sha256sum.txt", url),
		];
		let expected_sha256 = try_fetch_sha256(&checksum_urls, &config, Some("qjs"))
			.await;

		match download_file_with_checksum(
			url,
			&temp_archive,
			&progress,
			"QuickJS",
			&version,
			Some(&config),
			expected_sha256.as_deref(),
			Some(&cancel_token),
		)
		.await
		{
			Ok(()) => {
				downloaded = true;
				break;
			}
			Err(e) => {
				warn!("QuickJS download failed from {}: {}", url, e);
				download_errors.push(format!("{} => {}", url, e));
			}
		}
	}

	if !downloaded {
		return Err(format!(
			"Failed to download QuickJS. If bellard.org is blocked in your region, set {} to a mirror base URL. Errors: {}",
			QUICKJS_MIRROR_BASE_ENV,
			download_errors.join(" | ")
		));
	}

	progress.emit(InstallProgress {
		stage: "extracting".to_string(),
		progress: 90,
		downloaded: 0,
		total: 0,
		speed: 0.0,
		message: "Extracting QuickJS...".to_string(),
	});

	#[cfg(target_os = "windows")]
	let dest_name = "qjs.exe";
	#[cfg(not(target_os = "windows"))]
	let dest_name = "qjs";

	extract_from_zip(
		&temp_archive,
		&bin_dir,
		ZipExtractConfig {
			matcher: is_quickjs_file,
			dest_name,
			extract_all: false,
		},
	)
	.await?;

	let _ = tokio::fs::remove_file(&temp_archive).await;

	#[cfg(unix)]
	{
		make_executable(&quickjs_path).await?;
	}

	progress.emit(InstallProgress {
		stage: "verifying".to_string(),
		progress: 95,
		downloaded: 0,
		total: 0,
		speed: 0.0,
		message: "Verifying installation...".to_string(),
	});

	verify_quickjs(&quickjs_path)
		.await
		.map_err(|e| format!("QuickJS verification failed: {}", e))?;

	info!("QuickJS installed successfully");

	progress.emit(InstallProgress {
		stage: "complete".to_string(),
		progress: 100,
		downloaded: 0,
		total: 0,
		speed: 0.0,
		message: "QuickJS installed successfully!".to_string(),
	});

	Ok(quickjs_path.to_string_lossy().to_string())
}

pub async fn uninstall_quickjs(app: AppHandle) -> Result<(), String> {
	let quickjs_path = get_quickjs_path(&app)?;

	if quickjs_path.exists() {
		tokio::fs::remove_file(&quickjs_path)
			.await
			.map_err(|e| format!("Failed to remove QuickJS: {}", e))?;
	}

	Ok(())
}
