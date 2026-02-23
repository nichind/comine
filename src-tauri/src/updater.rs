use tauri::AppHandle;
use tauri::Emitter;
#[cfg(any(target_os = "android", target_os = "linux"))]
use tauri::Manager;
use tracing::info;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub body: Option<String>,
    pub date: Option<String>,
    pub download_url: Option<String>,
    pub is_prerelease: bool,
}

impl UpdateCheckResult {
    fn none() -> Self {
        Self {
            available: false,
            version: None,
            body: None,
            date: None,
            download_url: None,
            is_prerelease: false,
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
async fn resolve_update_endpoint(allow_prerelease: bool) -> Result<String, String> {
    if !allow_prerelease {
        return Ok(
            "https://github.com/nichind/comine/releases/latest/download/latest.json".to_string(),
        );
    }

    let client = crate::utils::http_client()?;
    let response = client
        .get("https://api.github.com/repos/nichind/comine/releases")
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "comine-updater")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch releases: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }

    let releases: Vec<serde_json::Value> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse releases: {e}"))?;

    let tag = releases
        .first()
        .and_then(|r| r["tag_name"].as_str())
        .ok_or("No releases found")?;

    Ok(format!(
        "https://github.com/nichind/comine/releases/download/{tag}/latest.json"
    ))
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
async fn build_updater(
    app: &AppHandle,
    allow_prerelease: bool,
) -> Result<tauri_plugin_updater::Updater, String> {
    use tauri_plugin_updater::UpdaterExt;

    let url = resolve_update_endpoint(allow_prerelease).await?;
    info!("Update endpoint: {url}");

    app.updater_builder()
        .endpoints(vec![url
            .parse()
            .map_err(|e| format!("Invalid URL: {e}"))?])
        .map_err(|e| format!("Failed to configure updater: {e}"))?
        .build()
        .map_err(|e| format!("Failed to build updater: {e}"))
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    allow_prerelease: bool,
) -> Result<UpdateCheckResult, String> {
    info!("Checking for updates (allow_prerelease={allow_prerelease})");

    let updater = build_updater(&app, allow_prerelease).await?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("Update check failed: {e}"))?;

    match update {
        Some(u) => {
            info!(
                "Update available: {} (current: {})",
                u.version, u.current_version
            );
            Ok(UpdateCheckResult {
                available: true,
                version: Some(u.version.clone()),
                body: Some(u.body.clone().unwrap_or_default()),
                date: u.date.map(|d| d.to_string()),
                download_url: None,
                is_prerelease: false,
            })
        }
        None => {
            info!("No update available");
            Ok(UpdateCheckResult::none())
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
#[tauri::command]
#[allow(unused_variables)]
pub async fn download_and_install_update(
    app: AppHandle,
    window: tauri::Window,
    allow_prerelease: bool,
    apk_url: Option<String>,
) -> Result<(), String> {
    let updater = build_updater(&app, allow_prerelease).await?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("Update check failed: {e}"))?
        .ok_or("No update available")?;

    info!("Downloading update v{}", update.version);

    let progress_window = window.clone();
    let finish_window = window.clone();
    let mut started = false;

    update
        .download_and_install(
            move |chunk_length, content_length| {
                if !started {
                    started = true;
                    let _ = progress_window.emit(
                        "update-download-progress",
                        serde_json::json!({ "event": "started", "contentLength": content_length }),
                    );
                }
                let _ = progress_window.emit(
                    "update-download-progress",
                    serde_json::json!({ "event": "progress", "chunkLength": chunk_length }),
                );
            },
            move || {
                let _ = finish_window.emit(
                    "update-download-progress",
                    serde_json::json!({ "event": "finished" }),
                );
            },
        )
        .await
        .map_err(|e| {
            let msg = e.to_string();
            tracing::error!("Update install failed: {msg}");
            if msg.to_ascii_lowercase().contains("signature") {
                "Update signature verification failed".to_string()
            } else {
                format!("Update failed: {msg}")
            }
        })?;

    info!("Update installed, restarting");
    app.restart();
}

#[cfg(any(target_os = "android", target_os = "linux"))]
const PLATFORM: &str = if cfg!(target_os = "linux") {
    "Linux"
} else {
    "Android"
};

#[cfg(any(target_os = "android", target_os = "linux"))]
fn parse_version(v: &str) -> (Vec<u32>, Option<String>) {
    let v = v.strip_prefix('v').unwrap_or(v);
    let (base, pre) = v
        .split_once('-')
        .map_or((v, None), |(b, p)| (b, Some(p.to_string())));
    let parts = base.split('.').filter_map(|s| s.parse().ok()).collect();
    (parts, pre)
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn is_newer_version(remote: &str, local: &str) -> bool {
    let (rv, rp) = parse_version(remote);
    let (lv, lp) = parse_version(local);

    for i in 0..rv.len().max(lv.len()) {
        let r = rv.get(i).copied().unwrap_or(0);
        let l = lv.get(i).copied().unwrap_or(0);
        match r.cmp(&l) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
        }
    }

    match (&lp, &rp) {
        (Some(_), None) => true,
        (Some(l), Some(r)) => r > l,
        _ => false,
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
async fn fetch_latest_release(allow_prerelease: bool) -> Result<serde_json::Value, String> {
    let client = crate::utils::http_client()?;
    let url = if allow_prerelease {
        "https://api.github.com/repos/nichind/comine/releases"
    } else {
        "https://api.github.com/repos/nichind/comine/releases/latest"
    };

    let response = client
        .get(url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "comine-updater")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch releases: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }

    if allow_prerelease {
        let releases: Vec<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse releases: {e}"))?;
        releases
            .into_iter()
            .next()
            .ok_or_else(|| "No releases found".to_string())
    } else {
        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse release: {e}"))
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn find_asset_by_extension(assets: &[serde_json::Value], ext: &str) -> Option<String> {
    assets.iter().find_map(|asset| {
        let name = asset["name"].as_str().unwrap_or("");
        if name.ends_with(ext) && !name.contains("updater") && !name.ends_with(".sig") {
            asset["browser_download_url"].as_str().map(String::from)
        } else {
            None
        }
    })
}

#[cfg(any(target_os = "android", target_os = "linux"))]
async fn stream_download(
    url: &str,
    dest: &std::path::Path,
    window: &tauri::Window,
) -> Result<u64, String> {
    use futures_util::StreamExt;

    let client = crate::utils::http_client()?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    let _ = window.emit(
        "update-download-progress",
        serde_json::json!({ "event": "started", "contentLength": response.content_length() }),
    );

    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| format!("Failed to create file: {e}"))?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Stream error: {e}"))?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| format!("Write error: {e}"))?;
        downloaded += chunk.len() as u64;
        let _ = window.emit(
            "update-download-progress",
            serde_json::json!({ "event": "progress", "chunkLength": chunk.len() }),
        );
    }

    tokio::io::AsyncWriteExt::shutdown(&mut file)
        .await
        .map_err(|e| format!("Flush error: {e}"))?;

    let _ = window.emit(
        "update-download-progress",
        serde_json::json!({ "event": "finished" }),
    );

    info!(
        "[{PLATFORM}] Downloaded {downloaded} bytes to {}",
        dest.display()
    );
    Ok(downloaded)
}

#[cfg(target_os = "linux")]
enum LinuxInstallType {
    AppImage(String),
    Deb,
    Rpm,
    Unknown,
}

#[cfg(target_os = "linux")]
fn detect_linux_install_type() -> LinuxInstallType {
    if let Ok(path) = std::env::var("APPIMAGE") {
        return LinuxInstallType::AppImage(path);
    }
    if cmd_succeeds("dpkg", &["-s", "comine"]) {
        return LinuxInstallType::Deb;
    }
    if cmd_succeeds("rpm", &["-q", "comine"]) {
        return LinuxInstallType::Rpm;
    }
    LinuxInstallType::Unknown
}

#[cfg(target_os = "linux")]
fn cmd_succeeds(program: &str, args: &[&str]) -> bool {
    std::process::Command::new(program)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn install_linux_update(
    app: &AppHandle,
    path: &std::path::Path,
    install_type: &LinuxInstallType,
) -> Result<(), String> {
    match install_type {
        LinuxInstallType::AppImage(current) => {
            use std::os::unix::fs::PermissionsExt;
            let dest = std::path::PathBuf::from(current);
            std::fs::copy(path, &dest).map_err(|e| format!("Failed to replace AppImage: {e}"))?;
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("Failed to set permissions: {e}"))?;
            let _ = std::fs::remove_file(path);
            info!(
                "[Linux] AppImage replaced at {}, restarting",
                dest.display()
            );
            app.restart();
        }
        _ => {
            info!("[Linux] Opening package installer for {}", path.display());
            std::process::Command::new("xdg-open")
                .arg(path)
                .spawn()
                .map_err(|e| format!("Failed to open package: {e}"))?;
            Ok(())
        }
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    allow_prerelease: bool,
) -> Result<UpdateCheckResult, String> {
    info!("[{PLATFORM}] Checking for updates (allow_prerelease={allow_prerelease})");

    let current = app
        .config()
        .version
        .clone()
        .unwrap_or_else(|| "0.0.0".to_string());

    let release = fetch_latest_release(allow_prerelease).await?;
    let tag = release["tag_name"]
        .as_str()
        .ok_or("No tag_name in release")?;
    let version = tag.strip_prefix('v').unwrap_or(tag).to_string();

    if !is_newer_version(&version, &current) {
        info!("[{PLATFORM}] Up to date (remote={version}, current={current})");
        return Ok(UpdateCheckResult::none());
    }

    let download_url = release["assets"].as_array().and_then(|assets| {
        #[cfg(target_os = "android")]
        {
            find_asset_by_extension(assets, ".apk")
        }
        #[cfg(target_os = "linux")]
        {
            let ext = match detect_linux_install_type() {
                LinuxInstallType::Deb => ".deb",
                LinuxInstallType::Rpm => ".rpm",
                _ => ".AppImage",
            };
            find_asset_by_extension(assets, ext)
        }
    });

    info!("[{PLATFORM}] Update available: {current} -> {version}");

    Ok(UpdateCheckResult {
        available: true,
        version: Some(version),
        body: release["body"].as_str().map(String::from),
        date: release["published_at"].as_str().map(String::from),
        download_url,
        is_prerelease: release["prerelease"].as_bool().unwrap_or(false),
    })
}

#[cfg(any(target_os = "android", target_os = "linux"))]
#[tauri::command]
pub async fn download_and_install_update(
    app: AppHandle,
    window: tauri::Window,
    allow_prerelease: bool,
    apk_url: Option<String>,
) -> Result<(), String> {
    let url = match apk_url {
        Some(ref u) if !u.is_empty() => u.clone(),
        _ => check_for_update(app.clone(), allow_prerelease)
            .await?
            .download_url
            .ok_or("No download URL found in release")?,
    };

    info!("[{PLATFORM}] Starting download: {url}");

    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("Failed to get cache dir: {e}"))?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("Failed to create cache dir: {e}"))?;

    #[cfg(target_os = "linux")]
    {
        let install_type = detect_linux_install_type();
        let ext = match &install_type {
            LinuxInstallType::Deb => "deb",
            LinuxInstallType::Rpm => "rpm",
            _ => "AppImage",
        };
        let dest = cache_dir.join(format!("comine-update.{ext}"));
        stream_download(&url, &dest, &window).await?;
        return install_linux_update(&app, &dest, &install_type);
    }

    #[cfg(target_os = "android")]
    {
        let dest = cache_dir.join("update.apk");
        stream_download(&url, &dest, &window).await?;
        let path = dest.to_str().ok_or("Invalid path")?.to_string();
        tokio::task::spawn_blocking(move || install_apk_jni(&path))
            .await
            .map_err(|e| format!("JNI task panicked: {e}"))?
    }
}

#[cfg(target_os = "android")]
fn install_apk_jni(path: &str) -> Result<(), String> {
    use crate::orchestrator::backends::android_jni::{get_activity, get_jni_env};
    use jni::objects::JValue;

    let mut env = get_jni_env()?;
    let activity = get_activity()?;
    let j_path = env
        .new_string(path)
        .map_err(|e| format!("JNI string error: {e}"))?;

    env.call_method(
        activity.as_obj(),
        "installApk",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&j_path)],
    )
    .map_err(|e| format!("JNI installApk failed: {e}"))?;

    Ok(())
}
