use tauri::AppHandle;
use tauri::Emitter;

#[cfg(target_os = "android")]
use tracing::info;
#[cfg(not(target_os = "android"))]
use tracing::{error, info};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub body: Option<String>,
    pub date: Option<String>,
}

#[cfg(not(target_os = "android"))]
async fn resolve_update_endpoint(allow_prerelease: bool) -> Result<String, String> {
    if allow_prerelease {
        let client = crate::utils::http_client()?;
        let response = client
            .get("https://api.github.com/repos/nichind/comine/releases")
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "comine-updater")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch releases: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("GitHub API error: {}", response.status()));
        }

        let releases: Vec<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse releases: {}", e))?;

        let latest = releases.first().ok_or("No releases found")?;
        let tag = latest["tag_name"]
            .as_str()
            .ok_or("No tag_name in release")?;

        Ok(format!(
            "https://github.com/nichind/comine/releases/download/{}/latest.json",
            tag
        ))
    } else {
        Ok("https://github.com/nichind/comine/releases/latest/download/latest.json".to_string())
    }
}

#[cfg(not(target_os = "android"))]
async fn build_updater(
    app: &AppHandle,
    allow_prerelease: bool,
) -> Result<tauri_plugin_updater::Updater, String> {
    use tauri_plugin_updater::UpdaterExt;

    let endpoint_url = resolve_update_endpoint(allow_prerelease).await?;
    info!("Using update endpoint: {}", endpoint_url);

    app.updater_builder()
        .endpoints(vec![endpoint_url
            .parse()
            .map_err(|e| format!("Invalid URL: {}", e))?])
        .map_err(|e| format!("Failed to set endpoints: {}", e))?
        .build()
        .map_err(|e| format!("Failed to build updater: {}", e))
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    allow_prerelease: bool,
) -> Result<UpdateCheckResult, String> {
    info!(
        "Checking for updates with allow_prerelease={}",
        allow_prerelease
    );

    let updater = build_updater(&app, allow_prerelease).await?;

    let update = updater
        .check()
        .await
        .map_err(|e| format!("Update check failed: {}", e))?;

    match update {
        Some(update) => {
            info!(
                "Update available: {} (current: {})",
                update.version, update.current_version
            );
            let date_str = update.date.map(|d| d.to_string());
            Ok(UpdateCheckResult {
                available: true,
                version: Some(update.version.clone()),
                body: Some(update.body.clone().unwrap_or_default()),
                date: date_str,
            })
        }
        None => {
            info!("No update available");
            Ok(UpdateCheckResult {
                available: false,
                version: None,
                body: None,
                date: None,
            })
        }
    }
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn download_and_install_update(
    app: AppHandle,
    window: tauri::Window,
    allow_prerelease: bool,
) -> Result<(), String> {
    info!(
        "Starting update download with allow_prerelease={}",
        allow_prerelease
    );

    let updater = build_updater(&app, allow_prerelease).await?;

    let update = updater
        .check()
        .await
        .map_err(|e| format!("Update check failed: {}", e))?
        .ok_or("No update available")?;

    info!("Downloading update version {}", update.version);

    let window_for_progress = window.clone();
    let window_for_finish = window.clone();
    let mut started = false;

    update
        .download_and_install(
            move |chunk_length, content_length| {
                if !started {
                    started = true;
                    let _ = window_for_progress.emit(
                        "update-download-progress",
                        serde_json::json!({
                            "event": "started",
                            "contentLength": content_length
                        }),
                    );
                }

                let _ = window_for_progress.emit(
                    "update-download-progress",
                    serde_json::json!({
                        "event": "progress",
                        "chunkLength": chunk_length
                    }),
                );
            },
            move || {
                info!("Download finished, verifying and installing...");
                let _ = window_for_finish.emit(
                    "update-download-progress",
                    serde_json::json!({
                        "event": "finished"
                    }),
                );
            },
        )
        .await
        .map_err(|e| {
            let err_str = e.to_string();
            error!("Update install failed: {}", err_str);
            if err_str.contains("signature") || err_str.contains("Signature") {
                "Update signature verification failed. The release may not be properly signed."
                    .to_string()
            } else {
                format!("Update failed: {}", err_str)
            }
        })?;

    info!("Update installed successfully, restarting app...");
    app.restart();
}

#[cfg(target_os = "android")]
#[tauri::command]
pub async fn check_for_update(
    _app: AppHandle,
    _allow_prerelease: bool,
) -> Result<UpdateCheckResult, String> {
    Err("Use Android update mechanism".to_string())
}

#[cfg(target_os = "android")]
#[tauri::command]
pub async fn download_and_install_update(
    _app: AppHandle,
    _window: tauri::Window,
    _allow_prerelease: bool,
) -> Result<(), String> {
    Err("Use Android update mechanism".to_string())
}
