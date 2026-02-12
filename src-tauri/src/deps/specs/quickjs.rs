use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tracing::{info, warn};

use crate::proxy::ProxyConfig;
use crate::types::DependencyStatus;

use crate::deps::engine::installer::{self, get_bin_dir, ExtractStrategy, InstallPlan};
use crate::deps::engine::verify::run_capture_async;

use regex::Regex;

const EVENT_PROGRESS: &str = "quickjs-install-progress";
const FALLBACK_VERSION: &str = "2025-09-13";

const QUICKJS_MIRROR_BASE_ENV: &str = "COMINE_QUICKJS_MIRROR_BASE";

pub fn get_quickjs_path(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let binary_name = "qjs.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "qjs";

    Ok(get_bin_dir(app)?.join(binary_name))
}

pub fn resolve_quickjs_path(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(managed) = get_quickjs_path(app) {
        if managed.exists() {
            return Some(managed);
        }
    }
    #[cfg(target_os = "windows")]
    let binary_name = "qjs.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "qjs";

    crate::deps::engine::verify::find_in_system_path(binary_name)
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
            Ok(CmdOutputAsync {
                status_code,
                stdout,
                stderr,
            }) => {
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
    let quickjs_path = match resolve_quickjs_path(&app) {
        Some(path) => path,
        None => return Ok(DependencyStatus::not_installed()),
    };

    match verify_quickjs(&quickjs_path).await {
        Ok(()) => {
            let disk_size = tokio::fs::metadata(&quickjs_path)
                .await
                .ok()
                .map(|m| m.len());

            let version = match run_capture_async(&quickjs_path, &["-h"]).await {
                Ok(output) if output.status_code == Some(0) => {
                    let text = format!("{}\n{}", output.stdout, output.stderr);
                    let re = Regex::new(r"QuickJS version\s+([0-9]{4}-[0-9]{2}-[0-9]{2})").ok();
                    re.and_then(|re| re.captures(&text))
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
}

pub async fn install_quickjs(
    app: AppHandle,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    let config = proxy_config.unwrap_or_default();
    let version = fetch_latest_version(&config).await;
    info!("QuickJS latest version: {}", version);

    let quickjs_path = get_quickjs_path(&app)?;
    let bin_dir = get_bin_dir(&app)?;

    #[cfg(target_os = "windows")]
    let dest_name = "qjs.exe";
    #[cfg(not(target_os = "windows"))]
    let dest_name = "qjs";

    installer::run_install(
        &app,
        InstallPlan {
            dep_name: "quickjs",
            display_name: "QuickJS",
            event_name: EVENT_PROGRESS,
            version: version.clone(),
            download_urls: build_download_urls(&version),
            checksum_urls: vec![],
            checksum_filename_hint: Some("qjs"),
            temp_archive: bin_dir.join(format!("quickjs_{}.zip", uuid::Uuid::new_v4())),
            binary_path: quickjs_path,
            extract: ExtractStrategy::Zip {
                matcher: is_quickjs_file,
                dest_name,
            },
            extra_executables: vec![],
            verify_args: vec![],
            custom_verify: Some(Box::new(|path| {
                let path = path.to_path_buf();
                Box::pin(async move {
                    verify_quickjs(&path)
                        .await
                        .map_err(|e| format!("QuickJS verification failed: {}", e))
                })
            })),
        },
        &config,
    )
    .await
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
