mod deps;
mod logs;
mod notifications;
mod orchestrator;
mod proxy;
// #[cfg(not(target_os = "android"))]
// mod relay;
#[cfg(not(target_os = "android"))]
mod server;
mod thumbnail_color;
#[cfg(not(target_os = "android"))]
mod tray;
mod types;
mod utils;

#[cfg(target_os = "android")]
use log::info;
#[cfg(not(target_os = "android"))]
use log::{error, info};

use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;

#[cfg(target_os = "windows")]
use utils::CommandHideConsole;

#[tauri::command]
#[allow(unused_variables)]
async fn get_media_duration(app: AppHandle, file_path: String) -> Result<f64, String> {
    #[cfg(target_os = "android")]
    {
        return Err("get_media_duration not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        use std::process::Stdio;

        if !std::path::Path::new(&file_path).exists() {
            return Err(format!("File not found: {}", file_path));
        }

        let deps_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("deps");

        let ffprobe_path = if cfg!(target_os = "windows") {
            deps_dir.join("ffprobe.exe")
        } else {
            deps_dir.join("ffprobe")
        };

        let ffprobe_cmd = if ffprobe_path.exists() {
            ffprobe_path.to_string_lossy().to_string()
        } else {
            "ffprobe".to_string()
        };

        let mut cmd = tokio::process::Command::new(&ffprobe_cmd);
        cmd.args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            &file_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ffprobe failed: {}", stderr));
        }

        let duration_str = String::from_utf8_lossy(&output.stdout);
        duration_str
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("Failed to parse duration: {}", duration_str))
    }
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct MediaFormatInfo {
    pub format_name: Option<String>,
    pub format_long_name: Option<String>,
    pub duration: Option<f64>,
    pub size: Option<u64>,
    pub bit_rate: Option<u64>,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct MediaStreamInfo {
    pub index: Option<u64>,
    pub codec_type: Option<String>,
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub profile: Option<String>,

    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,

    pub width: Option<u32>,
    pub height: Option<u32>,
    pub r_frame_rate: Option<String>,
    pub avg_frame_rate: Option<String>,

    pub bit_rate: Option<u64>,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct MediaTechnicalInfo {
    pub format: Option<MediaFormatInfo>,
    pub streams: Vec<MediaStreamInfo>,
}

#[tauri::command]
#[allow(unused_variables)]
async fn get_media_technical_info(
    app: AppHandle,
    file_path: String,
) -> Result<MediaTechnicalInfo, String> {
    #[cfg(target_os = "android")]
    {
        return Err("get_media_technical_info not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        use std::process::Stdio;

        if !std::path::Path::new(&file_path).exists() {
            return Err(format!("File not found: {}", file_path));
        }

        let deps_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("deps");

        let ffprobe_path = if cfg!(target_os = "windows") {
            deps_dir.join("ffprobe.exe")
        } else {
            deps_dir.join("ffprobe")
        };

        let ffprobe_cmd = if ffprobe_path.exists() {
            ffprobe_path.to_string_lossy().to_string()
        } else {
            "ffprobe".to_string()
        };

        let mut cmd = tokio::process::Command::new(&ffprobe_cmd);
        cmd.args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            &file_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ffprobe failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let value: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| format!("Failed to parse ffprobe JSON: {}", e))?;

        fn parse_u64(v: &serde_json::Value) -> Option<u64> {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
        }
        fn parse_u32(v: &serde_json::Value) -> Option<u32> {
            v.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .or_else(|| v.as_str().and_then(|s| s.parse::<u32>().ok()))
        }
        fn parse_f64(v: &serde_json::Value) -> Option<f64> {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        }

        let format = value.get("format").and_then(|f| {
            Some(MediaFormatInfo {
                format_name: f
                    .get("format_name")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string()),
                format_long_name: f
                    .get("format_long_name")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string()),
                duration: f.get("duration").and_then(parse_f64),
                size: f.get("size").and_then(parse_u64),
                bit_rate: f.get("bit_rate").and_then(parse_u64),
            })
        });

        let mut streams: Vec<MediaStreamInfo> = Vec::new();
        if let Some(arr) = value.get("streams").and_then(|s| s.as_array()) {
            for s in arr {
                streams.push(MediaStreamInfo {
                    index: s.get("index").and_then(parse_u64),
                    codec_type: s
                        .get("codec_type")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string()),
                    codec_name: s
                        .get("codec_name")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string()),
                    codec_long_name: s
                        .get("codec_long_name")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string()),
                    profile: s
                        .get("profile")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string()),
                    sample_rate: s.get("sample_rate").and_then(parse_u32),
                    channels: s.get("channels").and_then(parse_u32),
                    channel_layout: s
                        .get("channel_layout")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string()),
                    width: s.get("width").and_then(parse_u32),
                    height: s.get("height").and_then(parse_u32),
                    r_frame_rate: s
                        .get("r_frame_rate")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string()),
                    avg_frame_rate: s
                        .get("avg_frame_rate")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string()),
                    bit_rate: s.get("bit_rate").and_then(parse_u64),
                });
            }
        }

        Ok(MediaTechnicalInfo { format, streams })
    }
}

#[tauri::command]
#[allow(unused_variables)]
async fn generate_local_thumbnail(
    app: AppHandle,
    file_path: String,
    item_id: String,
) -> Result<String, String> {
    orchestrator::thumbnail::generate_local_thumbnail(&app, &file_path, &item_id).await
}

#[tauri::command]
#[allow(unused_variables)]
async fn set_window_effect(app: AppHandle, effect_type: String) -> Result<(), String> {
    info!("Setting window effect: {}", effect_type);

    #[cfg(target_os = "windows")]
    {
        use tauri::utils::config::{Color, WindowEffectsConfig};
        use tauri_utils::WindowEffect;

        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_effects(None::<WindowEffectsConfig>);

            if effect_type != "none" && !effect_type.starts_with("vibrancy-") {
                let effect = match effect_type.as_str() {
                    "blur" => WindowEffect::Blur,
                    "mica" => WindowEffect::Mica,
                    "mica-dark" => WindowEffect::MicaDark,
                    "mica-light" => WindowEffect::MicaLight,
                    "tabbed" => WindowEffect::Tabbed,
                    "tabbed-dark" => WindowEffect::TabbedDark,
                    "tabbed-light" => WindowEffect::TabbedLight,
                    _ => WindowEffect::Acrylic,
                };

                // Only Acrylic supports color tinting
                let color = if effect_type == "acrylic" {
                    Some(Color(19, 19, 19, 163))
                } else {
                    None
                };

                let effects_config = WindowEffectsConfig {
                    effects: vec![effect],
                    state: None,
                    radius: None,
                    color,
                };

                // Force redraw when switching between effect APIs
                let _ = window.set_decorations(true);
                let _ = window.set_decorations(false);

                if let Err(e) = window.set_effects(Some(effects_config)) {
                    error!("Failed to set window effect: {:?}", e);
                    return Err(format!("Failed to set window effect: {:?}", e));
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        use tauri::utils::config::WindowEffectsConfig;
        use tauri_utils::{WindowEffect, WindowEffectState};

        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_effects(None::<WindowEffectsConfig>);

            if effect_type != "none" && effect_type.starts_with("vibrancy-") {
                let effect = match effect_type.as_str() {
                    "vibrancy-titlebar" => WindowEffect::Titlebar,
                    "vibrancy-selection" => WindowEffect::Selection,
                    "vibrancy-menu" => WindowEffect::Menu,
                    "vibrancy-popover" => WindowEffect::Popover,
                    "vibrancy-sidebar" => WindowEffect::Sidebar,
                    "vibrancy-header" => WindowEffect::HeaderView,
                    "vibrancy-sheet" => WindowEffect::Sheet,
                    "vibrancy-window" => WindowEffect::WindowBackground,
                    "vibrancy-hud" => WindowEffect::HudWindow,
                    "vibrancy-fullscreen" => WindowEffect::FullScreenUI,
                    "vibrancy-tooltip" => WindowEffect::Tooltip,
                    "vibrancy-content" => WindowEffect::ContentBackground,
                    "vibrancy-under-window" => WindowEffect::UnderWindowBackground,
                    "vibrancy-under-page" => WindowEffect::UnderPageBackground,
                    _ => WindowEffect::WindowBackground,
                };

                let effects_config = WindowEffectsConfig {
                    effects: vec![effect],
                    state: Some(WindowEffectState::FollowsWindowActiveState),
                    radius: Some(12.0),
                    color: None,
                };

                if let Err(e) = window.set_effects(Some(effects_config)) {
                    error!("Failed to set window effect: {:?}", e);
                    return Err(format!("Failed to set window effect: {:?}", e));
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
#[allow(unused_variables)]
async fn set_acrylic(app: AppHandle, enable: bool) -> Result<(), String> {
    set_window_effect(
        app,
        if enable {
            "acrylic".to_string()
        } else {
            "none".to_string()
        },
    )
    .await
}

#[tauri::command]
async fn resolve_proxy_config(config: proxy::ProxyConfig) -> Result<proxy::ResolvedProxy, String> {
    info!(
        "Resolving proxy config: mode={}, custom_url={}, retry_without_proxy={}",
        config.mode, config.custom_url, config.retry_without_proxy
    );
    Ok(proxy::resolve_proxy(&config))
}

#[tauri::command]
async fn validate_proxy_url(url: String) -> Result<(), String> {
    proxy::validate_proxy_url(&url)
}

#[tauri::command]
async fn detect_system_proxy() -> Result<proxy::ResolvedProxy, String> {
    Ok(proxy::detect_system_proxy())
}

#[tauri::command]
#[cfg(not(target_os = "android"))]
async fn get_disk_space(path: String) -> Result<utils::DiskSpaceInfo, String> {
    let actual_path = if path.is_empty() {
        dirs::download_dir()
            .ok_or("Could not find Downloads folder")?
            .to_string_lossy()
            .to_string()
    } else {
        path
    };
    utils::get_disk_space_for_path(&actual_path)
        .ok_or_else(|| "Could not determine disk space".to_string())
}

#[tauri::command]
#[cfg(target_os = "android")]
async fn get_disk_space(_path: String) -> Result<utils::DiskSpaceInfo, String> {
    Err("Not supported on Android".to_string())
}

#[tauri::command]
async fn get_default_download_dir() -> Result<String, String> {
    #[cfg(not(target_os = "android"))]
    {
        dirs::download_dir()
            .map(|p| p.to_string_lossy().to_string())
            .ok_or_else(|| "Could not determine Downloads folder".to_string())
    }
    #[cfg(target_os = "android")]
    {
        // On Android, use Downloads/Comine to avoid permission issues
        Ok("/storage/emulated/0/Download/Comine".to_string())
    }
}

#[tauri::command]
#[cfg(target_os = "android")]
async fn open_file(path: String) -> Result<bool, String> {
    use jni::objects::JValue;

    log::info!("open_file called with path: {}", path);

    let result = std::thread::spawn(move || {
        let mut env = crate::orchestrator::backends::ytdlp::get_jni_env()?;
        let activity = crate::orchestrator::backends::ytdlp::get_activity()?;

        let j_path = env
            .new_string(&path)
            .map_err(|e| format!("Failed to create path string: {}", e))?;

        log::info!("Calling MainActivity.openFile via JNI");

        let result = env
            .call_method(
                activity.as_obj(),
                "openFile",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&j_path)],
            )
            .map_err(|e| format!("JNI call failed: {}", e))?;

        let success = result
            .z()
            .map_err(|e| format!("Failed to get boolean result: {}", e))?;
        log::info!("MainActivity.openFile returned: {}", success);
        Ok::<bool, String>(success)
    })
    .join()
    .map_err(|_| "Thread panicked".to_string())??;

    Ok(result)
}

#[tauri::command]
#[cfg(not(target_os = "android"))]
async fn open_file(_path: String) -> Result<bool, String> {
    Err("Use shell-open plugin on desktop".to_string())
}

#[tauri::command]
#[cfg(target_os = "android")]
async fn open_folder(path: String) -> Result<bool, String> {
    use jni::objects::JValue;

    let result = std::thread::spawn(move || {
        let mut env = crate::orchestrator::backends::ytdlp::get_jni_env()?;
        let activity = crate::orchestrator::backends::ytdlp::get_activity()?;

        let j_path = env
            .new_string(&path)
            .map_err(|e| format!("Failed to create path string: {}", e))?;

        let result = env
            .call_method(
                activity.as_obj(),
                "openFolder",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&j_path)],
            )
            .map_err(|e| format!("JNI call failed: {}", e))?;

        result
            .z()
            .map_err(|e| format!("Failed to get boolean result: {}", e))
    })
    .join()
    .map_err(|_| "Thread panicked".to_string())??;

    Ok(result)
}

#[tauri::command]
#[cfg(not(target_os = "android"))]
async fn open_folder(_path: String) -> Result<bool, String> {
    Err("Use shell-open plugin on desktop".to_string())
}

#[tauri::command]
async fn check_ip(proxy_config: Option<proxy::ProxyConfig>) -> Result<IpCheckResult, String> {
    let config = proxy_config.unwrap_or_default();
    let resolved = proxy::resolve_proxy(&config);

    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5));

    if !resolved.url.is_empty() {
        let proxy =
            reqwest::Proxy::all(&resolved.url).map_err(|e| format!("Invalid proxy: {}", e))?;
        builder = builder.proxy(proxy);
    } else {
        builder = builder.no_proxy();
    }

    let client = builder
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let response = client
        .get("https://api.ipify.org?format=json")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let ip = data["ip"].as_str().ok_or("No IP in response")?.to_string();

    Ok(IpCheckResult {
        ip,
        proxy_used: !resolved.url.is_empty(),
        proxy_source: resolved.source,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct IpCheckResult {
    ip: String,
    proxy_used: bool,
    proxy_source: String,
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn autostart_enable(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .enable()
        .map_err(|e| format!("Failed to enable autostart: {}", e))
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn autostart_disable(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .disable()
        .map_err(|e| format!("Failed to disable autostart: {}", e))
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn autostart_is_enabled(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| format!("Failed to check autostart status: {}", e))
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn autostart_enable(_app: AppHandle) -> Result<(), String> {
    Err("Autostart not supported on Android".to_string())
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn autostart_disable(_app: AppHandle) -> Result<(), String> {
    Err("Autostart not supported on Android".to_string())
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn autostart_is_enabled(_app: AppHandle) -> Result<bool, String> {
    Ok(false)
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub body: Option<String>,
    pub date: Option<String>,
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn check_for_update(
    app: AppHandle,
    allow_prerelease: bool,
) -> Result<UpdateCheckResult, String> {
    use tauri_plugin_updater::UpdaterExt;

    info!(
        "Checking for updates with allow_prerelease={}",
        allow_prerelease
    );

    let endpoint_url = if allow_prerelease {
        let client = reqwest::Client::new();
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

        format!(
            "https://github.com/nichind/comine/releases/download/{}/latest.json",
            tag
        )
    } else {
        "https://github.com/nichind/comine/releases/latest/download/latest.json".to_string()
    };

    info!("Using update endpoint: {}", endpoint_url);

    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint_url
            .parse()
            .map_err(|e| format!("Invalid URL: {}", e))?])
        .map_err(|e| format!("Failed to set endpoints: {}", e))?
        .build()
        .map_err(|e| format!("Failed to build updater: {}", e))?;

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
async fn download_and_install_update(
    app: AppHandle,
    window: tauri::Window,
    allow_prerelease: bool,
) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;

    info!(
        "Starting update download with allow_prerelease={}",
        allow_prerelease
    );

    let endpoint_url = if allow_prerelease {
        let client = reqwest::Client::new();
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

        format!(
            "https://github.com/nichind/comine/releases/download/{}/latest.json",
            tag
        )
    } else {
        "https://github.com/nichind/comine/releases/latest/download/latest.json".to_string()
    };

    info!("Using update endpoint: {}", endpoint_url);

    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint_url
            .parse()
            .map_err(|e| format!("Invalid URL: {}", e))?])
        .map_err(|e| format!("Failed to set endpoints: {}", e))?
        .build()
        .map_err(|e| format!("Failed to build updater: {}", e))?;

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
async fn check_for_update(
    _app: AppHandle,
    _allow_prerelease: bool,
) -> Result<UpdateCheckResult, String> {
    Err("Use Android update mechanism".to_string())
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn download_and_install_update(
    _app: AppHandle,
    _window: tauri::Window,
    _allow_prerelease: bool,
) -> Result<(), String> {
    Err("Use Android update mechanism".to_string())
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn server_start(app: AppHandle, port: u16) {
    server::start_server(app, port);
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn server_stop() {
    server::stop_server();
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn server_is_running() -> bool {
    server::is_running()
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn push_queue_status(items: Vec<server::QueueItem>) {
    server::update_queue(items);
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn push_history_status(items: Vec<server::HistoryItem>) {
    server::update_history(items);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                ])
                .level(log::LevelFilter::Debug)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());

    #[cfg(not(target_os = "android"))]
    let builder = builder.plugin(
        tauri_plugin_autostart::Builder::new()
            .args(["--minimized"])
            .build(),
    );

    let builder = builder.invoke_handler(tauri::generate_handler![
        get_media_duration,
        get_media_technical_info,
        thumbnail_color::get_cached_thumbnail_color,
        thumbnail_color::extract_thumbnail_color,
        thumbnail_color::extract_local_thumbnail_color,
        set_window_effect,
        set_acrylic,
        open_file,
        open_folder,
        notifications::show_notification_window,
        notifications::reveal_notification_window,
        notifications::close_notification_window,
        notifications::close_all_notifications,
        notifications::notification_action,
        logs::get_log_file_path,
        logs::append_log,
        logs::cleanup_old_logs,
        logs::open_logs_folder,
        logs::get_logs_folder_path,
        logs::read_session_logs,
        logs::get_session_log_count,
        resolve_proxy_config,
        validate_proxy_url,
        detect_system_proxy,
        get_disk_space,
        get_default_download_dir,
        check_ip,
        check_for_update,
        download_and_install_update,
        deps::check_ytdlp,
        deps::install_ytdlp,
        deps::uninstall_ytdlp,
        deps::get_ytdlp_releases,
        deps::check_ffmpeg,
        deps::install_ffmpeg,
        deps::uninstall_ffmpeg,
        deps::check_aria2,
        deps::install_aria2,
        deps::uninstall_aria2,
        deps::check_deno,
        deps::install_deno,
        deps::uninstall_deno,
        deps::check_quickjs,
        deps::install_quickjs,
        deps::uninstall_quickjs,
        deps::check_lux,
        deps::install_lux,
        deps::uninstall_lux,
        autostart_enable,
        autostart_disable,
        autostart_is_enabled,
        orchestrator::resolve_url,
        orchestrator::start_job,
        orchestrator::get_jobs,
        orchestrator::control_job,
        orchestrator::update_job_settings,
        orchestrator::convert::convert_local_file,
        orchestrator::convert::cancel_conversion
    ]);

    #[cfg(not(target_os = "android"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_media_duration,
        get_media_technical_info,
        generate_local_thumbnail,
        thumbnail_color::get_cached_thumbnail_color,
        thumbnail_color::extract_thumbnail_color,
        thumbnail_color::extract_local_thumbnail_color,
        set_window_effect,
        set_acrylic,
        open_file,
        open_folder,
        notifications::show_notification_window,
        notifications::reveal_notification_window,
        notifications::close_notification_window,
        notifications::close_all_notifications,
        notifications::notification_action,
        logs::get_log_file_path,
        logs::append_log,
        logs::cleanup_old_logs,
        logs::open_logs_folder,
        logs::get_logs_folder_path,
        logs::read_session_logs,
        logs::get_session_log_count,
        resolve_proxy_config,
        validate_proxy_url,
        detect_system_proxy,
        get_disk_space,
        get_default_download_dir,
        check_ip,
        check_for_update,
        download_and_install_update,
        deps::check_ytdlp,
        deps::install_ytdlp,
        deps::uninstall_ytdlp,
        deps::get_ytdlp_releases,
        deps::check_ffmpeg,
        deps::install_ffmpeg,
        deps::uninstall_ffmpeg,
        deps::check_aria2,
        deps::install_aria2,
        deps::uninstall_aria2,
        deps::check_deno,
        deps::install_deno,
        deps::uninstall_deno,
        deps::check_quickjs,
        deps::install_quickjs,
        deps::uninstall_quickjs,
        deps::check_lux,
        deps::install_lux,
        deps::uninstall_lux,
        autostart_enable,
        autostart_disable,
        autostart_is_enabled,
        server_start,
        server_stop,
        server_is_running,
        push_queue_status,
        push_history_status,
        orchestrator::resolve_url,
        orchestrator::start_job,
        orchestrator::get_jobs,
        orchestrator::control_job,
        orchestrator::update_job_settings,
        orchestrator::convert::convert_local_file,
        orchestrator::convert::cancel_conversion
    ]);

    let builder = builder
        .setup(|app| {
            let manager = orchestrator::init(app.handle());
            app.manage(manager);

            #[cfg(not(target_os = "android"))]
            {
                tray::setup(app.handle())?;

                let start_minimized = std::env::args().any(|arg| arg == "--minimized");
                if start_minimized {
                    if let Some(window) = app.get_webview_window("main") {
                        use tauri_plugin_store::StoreExt;
                        let should_minimize = app
                            .store("settings.json")
                            .ok()
                            .and_then(|store| store.get("startMinimized"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);

                        if should_minimize {
                            let _ = window.hide();
                        }
                    }
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            let label = window.label();
            if label != "main" {
                return;
            }

            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    let _ = window.emit("close-requested", ());
                    api.prevent_close();
                }
                tauri::WindowEvent::Focused(false) => {
                    if let Ok(visible) = window.is_visible() {
                        if !visible {
                            let _ = window.emit("window-hidden", ());
                        }
                    }
                }
                _ => {}
            }
        });

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use crate::utils::lock_or_recover;
    use std::sync::{Arc, Mutex};

    #[test]
    fn lock_or_recover_handles_poisoned_mutex() {
        let mutex = Arc::new(Mutex::new(vec![1, 2, 3]));
        let mutex_clone = Arc::clone(&mutex);

        let handle = std::thread::spawn(move || {
            let _guard = mutex_clone.lock().unwrap();
            panic!("intentional panic to poison mutex");
        });

        let _ = handle.join();
        assert!(mutex.is_poisoned());

        let guard = lock_or_recover(&mutex);
        assert_eq!(*guard, vec![1, 2, 3]);
    }

    #[test]
    fn lock_or_recover_normal_case() {
        let mutex = Mutex::new(42);
        let guard = lock_or_recover(&mutex);
        assert_eq!(*guard, 42);
    }
}
