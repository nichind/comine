mod clipboard;
mod database;
mod deps;
mod logs;
mod media_info;
mod notifications;
mod orchestrator;
mod proxy;
mod server;
mod store_utils;
mod thumbnail_color;
#[cfg(not(target_os = "android"))]
mod tray;
#[cfg(not(target_os = "android"))]
mod tray_icon;
mod types;
mod updater;
mod url_utils;
mod utils;
mod window_effects;
#[cfg(not(target_os = "android"))]
pub(crate) mod window_manager;

use tracing::info;

use tauri::AppHandle;
use tauri::Emitter;
use tauri::Listener;
use tauri::Manager;

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
async fn get_disk_space(path: String) -> Result<utils::DiskSpaceInfo, String> {
    let actual_path = if path.is_empty() {
        #[cfg(not(target_os = "android"))]
        {
            dirs::download_dir()
                .ok_or("Could not find Downloads folder")?
                .to_string_lossy()
                .to_string()
        }
        #[cfg(target_os = "android")]
        {
            "/storage/emulated/0/Download/Comine".to_string()
        }
    } else {
        path
    };
    utils::get_disk_space_for_path(&actual_path).ok_or_else(|| {
        "Could not determine disk space. Check permissions or path validity.".to_string()
    })
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

    tracing::info!("open_file called with path: {}", path);

    let result = std::thread::spawn(move || {
        let mut env = crate::orchestrator::backends::android_jni::get_jni_env()?;
        let activity = crate::orchestrator::backends::android_jni::get_activity()?;

        let j_path = env
            .new_string(&path)
            .map_err(|e| format!("Failed to create path string: {}", e))?;

        tracing::info!("Calling MainActivity.openFile via JNI");

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
        tracing::info!("MainActivity.openFile returned: {}", success);
        Ok::<bool, String>(success)
    })
    .join()
    .map_err(|_| "Thread panicked".to_string())??;

    Ok(result)
}

#[tauri::command]
#[cfg(not(target_os = "android"))]
async fn open_file(path: String) -> Result<bool, String> {
    tracing::info!("open_file called with path: {}", path);
    opener::open(std::path::Path::new(&path))
        .map(|_| true)
        .map_err(|e| format!("Failed to open file: {}", e))
}

#[tauri::command]
#[cfg(target_os = "android")]
async fn open_folder(path: String) -> Result<bool, String> {
    use jni::objects::JValue;

    let result = std::thread::spawn(move || {
        let mut env = crate::orchestrator::backends::android_jni::get_jni_env()?;
        let activity = crate::orchestrator::backends::android_jni::get_activity()?;

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
async fn open_folder(path: String) -> Result<bool, String> {
    tracing::info!("open_folder called with path: {}", path);
    opener::open(std::path::Path::new(&path))
        .map(|_| true)
        .map_err(|e| format!("Failed to open folder: {}", e))
}

#[tauri::command]
#[cfg(not(target_os = "android"))]
async fn reveal_file(path: String) -> Result<bool, String> {
    tracing::info!("reveal_file called with path: {}", path);
    let p = std::path::PathBuf::from(&path);
    #[allow(unused_variables)]
    if let Some(parent) = p.parent() {
        #[cfg(target_os = "windows")]
        {
            let status = crate::utils::new_std_command("explorer")
                .arg("/select,")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("Failed to reveal file: {}", e))?;
            drop(status);
            Ok(true)
        }
        #[cfg(target_os = "macos")]
        {
            crate::utils::new_std_command("open")
                .arg("-R")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("Failed to reveal file: {}", e))?;
            Ok(true)
        }
        #[cfg(target_os = "linux")]
        {
            crate::utils::new_std_command("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("Failed to reveal file: {}", e))?;
            Ok(true)
        }
    } else {
        Err("Could not determine parent directory".to_string())
    }
}

#[tauri::command]
#[cfg(target_os = "android")]
async fn reveal_file(_path: String) -> Result<bool, String> {
    Ok(false)
}

#[cfg(not(target_os = "android"))]
pub(crate) async fn reveal_file_internal(path: String) -> Result<bool, String> {
    reveal_file(path).await
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_directory: bool,
}

#[tauri::command]
async fn list_directory_contents(path: String) -> Result<Vec<DirectoryEntry>, String> {
    let dir = std::path::Path::new(&path);
    if !dir.is_dir() {
        return Err("Path is not a directory".to_string());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Failed to read directory: {}", e))?;
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        result.push(DirectoryEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry.path().to_string_lossy().to_string(),
            size: if meta.is_file() { meta.len() } else { 0 },
            is_directory: meta.is_dir(),
        });
    }
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(result)
}

#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FileUrlInfo {
    is_file: bool,
    filename: String,
    size: u64,
    mime_type: String,
    supports_resume: bool,
}

#[tauri::command]
async fn check_file_url(
    url: String,
    proxy_config: Option<proxy::ProxyConfig>,
) -> Result<FileUrlInfo, String> {
    let resolved = proxy_config
        .map(|c| proxy::resolve_proxy(&c))
        .unwrap_or_default();
    let proxy_url = if resolved.url.is_empty() {
        None
    } else {
        Some(resolved.url.as_str())
    };
    let client = utils::http_client_with_proxy(proxy_url)?;

    let response = client
        .head(&url)
        .send()
        .await
        .map_err(|e| format!("HEAD request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let headers = response.headers();

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let content_length = headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let content_disposition = headers
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let supports_resume = headers
        .get("accept-ranges")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("bytes"))
        .unwrap_or(false);

    let filename = if content_disposition.contains("filename=") {
        content_disposition
            .split("filename=")
            .nth(1)
            .map(|s| s.trim_matches('"').trim_matches('\'').to_string())
            .unwrap_or_default()
    } else {
        url.split('/')
            .last()
            .and_then(|s| s.split('?').next())
            .unwrap_or("")
            .to_string()
    };

    let is_file = content_disposition.contains("attachment")
        || (!content_type.starts_with("text/html")
            && !content_type.starts_with("application/json")
            && content_length > 0)
        || !filename.is_empty() && filename.contains('.');

    Ok(FileUrlInfo {
        is_file,
        filename,
        size: content_length,
        mime_type: content_type,
        supports_resume,
    })
}

#[tauri::command]
async fn clear_cookies(
    state: tauri::State<'_, std::sync::Arc<orchestrator::manager::JobManager>>,
) -> Result<(), String> {
    tracing::info!("Clearing cookies");
    state.clear_cookies().await;
    Ok(())
}

#[tauri::command]
async fn clear_memory_caches() -> Result<(), String> {
    tracing::info!("Clearing in-memory caches");
    if let Ok(mut cache) = thumbnail_color::memory_cache().lock() {
        cache.clear();
    }
    Ok(())
}

#[tauri::command]
async fn clear_cache(app: AppHandle) -> Result<(), String> {
    tracing::info!("Clearing disk caches");
    thumbnail_color::clear_disk_cache(&app).await;
    Ok(())
}

#[tauri::command]
async fn extension_update_status(
    id: String,
    state: String,
    progress: Option<f64>,
    speed: Option<String>,
    eta: Option<String>,
    error: Option<String>,
    file_path: Option<String>,
    duration: Option<f64>,
) -> Result<(), String> {
    // Forward this status to the local HTTP server for the browser extension to poll.
    // The extension polls /status which reads live from JobManager — the data is already
    // up-to-date. This command exists so the frontend can fire-and-forget status updates
    // without errors. In the future this could push to WebSocket-connected extensions.
    tracing::debug!(
        "Extension status update: id={}, state={}, progress={:?}",
        id,
        state,
        progress
    );
    let _ = (speed, eta, error, file_path, duration); // acknowledged
    Ok(())
}

#[tauri::command]
async fn check_ip(proxy_config: Option<proxy::ProxyConfig>) -> Result<IpCheckResult, String> {
    let config = proxy_config.unwrap_or_default();
    let resolved = proxy::resolve_proxy(&config);

    let proxy_url = if resolved.url.is_empty() {
        None
    } else {
        Some(resolved.url.as_str())
    };
    let client = utils::http_client_with_proxy(proxy_url)?;

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
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
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

#[tauri::command]
fn server_start(
    app: AppHandle,
    port: u16,
    token: String,
    manager: tauri::State<'_, std::sync::Arc<crate::orchestrator::manager::JobManager>>,
) {
    server::start_server(app, port, token, manager.inner().clone());
}

#[tauri::command]
fn server_stop() {
    server::stop_server();
}

#[tauri::command]
fn server_is_running() -> bool {
    server::is_running()
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
        media_info::get_media_duration,
        media_info::get_media_technical_info,
        media_info::generate_local_thumbnail,
        orchestrator::thumbnail::get_or_cache_thumbnail,
        thumbnail_color::get_cached_thumbnail_color,
        thumbnail_color::extract_thumbnail_color,
        thumbnail_color::extract_local_thumbnail_color,
        window_effects::set_window_effect,
        window_effects::set_acrylic,
        open_file,
        open_folder,
        list_directory_contents,
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
        updater::check_for_update,
        updater::download_and_install_update,
        deps::check_ytdlp,
        deps::install_ytdlp,
        deps::uninstall_ytdlp,
        deps::get_ytdlp_releases,
        deps::update_ytdlp_channel,
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
        deps::check_gallery_dl,
        deps::install_gallery_dl,
        deps::uninstall_gallery_dl,
        deps::get_gallery_dl_releases,
        deps::cancel_dep_install,
        autostart_enable,
        autostart_disable,
        autostart_is_enabled,
        reveal_file,
        check_file_url,
        clear_cookies,
        clear_memory_caches,
        clear_cache,
        extension_update_status,
        server_start,
        server_stop,
        server_is_running,
        orchestrator::resolve_url,
        orchestrator::resolve_url_stream,
        orchestrator::cancel_resolve_stream,
        orchestrator::start_job,
        orchestrator::get_jobs,
        orchestrator::control_job,
        orchestrator::update_job_settings,
        orchestrator::move_job_to_top,
        orchestrator::get_backend_capabilities,
        orchestrator::sync_download_settings,
        orchestrator::enqueue_url,
        orchestrator::enqueue_playlist,
        orchestrator::pause_all_jobs,
        orchestrator::resume_all_jobs,
        orchestrator::cancel_all_jobs,
        orchestrator::retry_all_failed,
        orchestrator::clear_terminal_jobs,
        orchestrator::get_history,
        orchestrator::remove_history_item,
        orchestrator::clear_history,
        orchestrator::toggle_history_favourite,
        orchestrator::set_history_favourite,
        orchestrator::update_history_duration,
        orchestrator::export_history,
        orchestrator::import_history,
        orchestrator::restore_history_from_frontend,
        orchestrator::get_app_stats,
        orchestrator::get_history_stats,
        orchestrator::fetch_broadcasts,
        orchestrator::convert::convert_local_file,
        orchestrator::convert::cancel_conversion,
        clipboard::start_clipboard_watcher,
        clipboard::stop_clipboard_watcher,
        clipboard::set_url_input_focused,
        #[cfg(not(target_os = "android"))]
        tray::rebuild_tray_menu
    ]);

    let builder = builder
        .setup(|app| {
            let manager = orchestrator::init(app.handle());
            app.manage(manager);
            app.manage(std::sync::Arc::new(clipboard::ClipboardWatcherState::new()));
            app.manage(std::sync::Arc::new(clipboard::InputFocusState::new()));

            #[cfg(not(target_os = "android"))]
            app.manage(std::sync::Arc::new(window_manager::WindowState::new()));

            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    orchestrator::thumbnail::prune_thumbnail_cache(&app_handle).await;
                });
            }

            #[cfg(not(target_os = "android"))]
            {
                tray::setup(app.handle())?;
                tray_icon::start_polling(app.handle());

                let start_minimized_arg = std::env::args().any(|arg| arg == "--minimized");
                use tauri_plugin_store::StoreExt;
                let start_minimized_setting = app
                    .store("settings.json")
                    .ok()
                    .and_then(|store| store.get("startMinimized"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                let should_show_on_ready = !(start_minimized_arg && start_minimized_setting);

                {
                    let app_handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let effect_type = app_handle
                            .store("settings.json")
                            .ok()
                            .and_then(|store| store.get("backgroundType"))
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                            .unwrap_or_else(|| "acrylic".to_string());

                        let effect_type = match effect_type.as_str() {
                            "acrylic" | "blur" | "mica" | "mica-dark" | "mica-light" | "tabbed"
                            | "tabbed-dark" | "tabbed-light" => effect_type,
                            _ if effect_type.starts_with("vibrancy-") => effect_type,
                            _ => "none".to_string(),
                        };

                        let _ = window_effects::set_window_effect(app_handle, effect_type).await;
                    });
                }

                {
                    use std::sync::atomic::Ordering;
                    use std::sync::Arc;
                    use std::time::Duration;
                    use tokio::time::sleep;

                    let window_state = app.state::<Arc<window_manager::WindowState>>();

                    let app_handle = app.handle().clone();
                    let app_handle_for_listener = app_handle.clone();
                    let app_handle_for_event = app_handle.clone();
                    let ws_for_event = window_state.inner().clone();
                    app_handle_for_listener.listen("frontend-ready", move |_| {
                        if !should_show_on_ready {
                            return;
                        }
                        if ws_for_event.was_shown.swap(true, Ordering::SeqCst) {
                            return;
                        }

                        if let Some(window) = app_handle_for_event.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.emit("window-shown", ());
                            let _ = window.set_focus();
                        }
                    });

                    let app_handle = app.handle().clone();
                    let ws_for_timeout = window_state.inner().clone();
                    tauri::async_runtime::spawn(async move {
                        sleep(Duration::from_millis(6000)).await;
                        if !should_show_on_ready {
                            return;
                        }
                        if ws_for_timeout.was_shown.swap(true, Ordering::SeqCst) {
                            return;
                        }

                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.emit("window-shown", ());
                        }
                    });
                }

                if start_minimized_arg {
                    if let Some(window) = app.get_webview_window("main") {
                        if start_minimized_setting {
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
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Prevent the app from exiting when the last window is destroyed
            // (close-to-tray). The tray "Quit" button calls app.exit(0) which
            // sets a non-None code and bypasses this guard.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = &event {
                if code.is_none() {
                    // Only keep the process alive if the user chose tray behavior
                    use tauri_plugin_store::StoreExt;
                    let close_behavior: String = app
                        .store("settings.json")
                        .ok()
                        .and_then(|s| {
                            let v = s.get("closeBehavior")?;
                            Some(v.as_str()?.to_string())
                        })
                        .unwrap_or_else(|| "tray".to_string());

                    if close_behavior == "tray" {
                        api.prevent_exit();
                    }
                }
            }
        });
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
