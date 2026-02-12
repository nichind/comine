use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_store::StoreExt;
use tokio::sync::RwLock;
use url::Url;

use crate::notifications;
use crate::orchestrator::manager::JobManager;
use crate::orchestrator::types::{ProxyConfig, ResolveSettings};
use crate::store_utils::{get_bool, get_str, get_string_array};
use crate::types::{NotificationData, NotificationMonitor, NotificationPosition};
use crate::url_utils;

const POLL_INTERVAL_MS: u64 = 500;

pub struct ClipboardWatcherState {
    running: AtomicBool,
    last_text: RwLock<String>,
}

impl ClipboardWatcherState {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            last_text: RwLock::new(String::new()),
        }
    }
}

pub struct InputFocusState {
    pub focused: AtomicBool,
}

impl InputFocusState {
    pub fn new() -> Self {
        Self {
            focused: AtomicBool::new(false),
        }
    }
}

#[tauri::command]
pub fn set_url_input_focused(app: AppHandle, focused: bool) {
    if let Some(state) = app.try_state::<Arc<InputFocusState>>() {
        state.focused.store(focused, Ordering::Relaxed);
    }
}

#[tauri::command]
pub async fn start_clipboard_watcher(app: AppHandle) -> Result<(), String> {
    let state = app.state::<Arc<ClipboardWatcherState>>();
    if state.running.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let state_clone = state.inner().clone();
    let app_clone = app.clone();

    tokio::spawn(async move {
        tracing::info!("[Clipboard] Watcher started");
        while state_clone.running.load(Ordering::Relaxed) {
            match app_clone.clipboard().read_text() {
                Ok(text) if !text.is_empty() => {
                    let mut last_text = state_clone.last_text.write().await;

                    if *last_text != text {
                        *last_text = text.clone();

                        if is_http_url(&text) {
                            handle_clipboard_url(&app_clone, &text).await;
                        }
                    }
                }
                Err(e) => {
                    tracing::trace!("[Clipboard] read_text failed: {}", e);
                }
                _ => {}
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
        tracing::info!("[Clipboard] Watcher stopped");
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_clipboard_watcher(app: AppHandle) -> Result<(), String> {
    let state = app.state::<Arc<ClipboardWatcherState>>();
    state.running.store(false, Ordering::SeqCst);
    Ok(())
}

fn is_http_url(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.len() > 4096 || trimmed.contains('\n') {
        return false;
    }
    match Url::parse(trimmed) {
        Ok(url) => matches!(url.scheme(), "http" | "https"),
        Err(_) => false,
    }
}

fn build_resolve_settings(app: &AppHandle) -> ResolveSettings {
    let cookies_from_browser = {
        let v = get_str(app, "cookiesFromBrowser", "");
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    };
    let custom_cookies = {
        let v = get_str(app, "customCookies", "");
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    };

    let proxy_mode = get_str(app, "proxyMode", "none");
    let custom_proxy_url = get_str(app, "customProxyUrl", "");
    let proxy = match proxy_mode.as_str() {
        "system" => Some(ProxyConfig {
            enabled: true,
            url: None,
            username: None,
            password: None,
        }),
        "custom" => Some(ProxyConfig {
            enabled: true,
            url: if custom_proxy_url.is_empty() {
                None
            } else {
                Some(custom_proxy_url)
            },
            username: None,
            password: None,
        }),
        _ => None,
    };

    let youtube_player_client = {
        let use_it = get_bool(app, "usePlayerClientForExtraction", false);
        if use_it {
            let client = get_str(app, "youtubePlayerClient", "");
            if client.is_empty() {
                None
            } else {
                Some(client)
            }
        } else {
            None
        }
    };

    ResolveSettings {
        cookies_from_browser,
        custom_cookies,
        proxy,
        youtube_player_client,
        flat_playlist: true,
        page_size: Some(50),
        cursor: None,
    }
}

fn get_notification_position(app: &AppHandle) -> Option<NotificationPosition> {
    app.store("settings.json")
        .ok()
        .and_then(|s| s.get("notificationPosition"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

fn get_notification_monitor(app: &AppHandle) -> Option<NotificationMonitor> {
    app.store("settings.json")
        .ok()
        .and_then(|s| s.get("notificationMonitor"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

fn get_notification_offset(app: &AppHandle) -> Option<i32> {
    app.store("settings.json")
        .ok()
        .and_then(|s| s.get("notificationOffset"))
        .and_then(|v| v.as_i64().map(|n| n as i32))
}

/// The unified clipboard URL handler — runs entirely in Rust.
/// Handles media URLs, file URLs, resolving, classification, and notification display.
async fn handle_clipboard_url(app: &AppHandle, text: &str) {
    let watch_clipboard = get_bool(app, "watchClipboard", true);
    if !watch_clipboard {
        return;
    }

    let patterns = get_string_array(app, "clipboardPatterns");

    if url_utils::is_valid_media_url(text, &patterns) {
        handle_detected_url(app, text).await;
    } else {
        let watch_files = get_bool(app, "watchClipboardForFiles", false);
        if watch_files {
            let (is_file, filename) = url_utils::is_direct_file_url(text);
            if is_file {
                handle_detected_file_url(app, text, filename.as_deref()).await;
            }
        }
    }
}

async fn handle_detected_file_url(app: &AppHandle, raw_url: &str, detected_filename: Option<&str>) {
    let file_notifs = get_bool(app, "fileDownloadNotifications", true);
    if !file_notifs {
        return;
    }

    // If the main window is visible, emit to frontend for toast handling
    if is_main_window_visible(app) {
        let _ = app.emit(
            "clipboard-url-file",
            serde_json::json!({
                "url": raw_url,
                "filename": detected_filename,
            }),
        );
        return;
    }

    let notifs_enabled = get_bool(app, "notificationsEnabled", true);
    if !notifs_enabled {
        return;
    }

    let proxy_mode = get_str(app, "proxyMode", "none");
    let custom_proxy_url = get_str(app, "customProxyUrl", "");
    let proxy_config = if proxy_mode != "none" {
        Some(crate::proxy::ProxyConfig {
            mode: proxy_mode,
            custom_url: custom_proxy_url,
            retry_without_proxy: false,
        })
    } else {
        None
    };

    match crate::check_file_url(raw_url.to_string(), proxy_config).await {
        Ok(file_info) => {
            if !file_info.is_file {
                return;
            }
            let filename = if file_info.filename.is_empty() {
                detected_filename.unwrap_or("file").to_string()
            } else {
                file_info.filename
            };

            let body = format_size(file_info.size);

            tracing::info!(
                "[Clipboard] File URL detected: {} ({} bytes)",
                filename,
                file_info.size
            );

            let data = NotificationData {
                title: filename.clone(),
                body,
                thumbnail: None,
                url: Some(raw_url.to_string()),
                compact: get_bool(app, "compactNotifications", false),
                is_playlist: false,
                is_channel: false,
                is_file: true,
                file_info: Some(crate::types::FileInfo {
                    filename,
                    size: file_info.size,
                    mime_type: file_info.mime_type,
                }),
                download_label: "Download".to_string(),
                dismiss_label: "Dismiss".to_string(),
            };

            let _ = notifications::show_notification_window(
                app.clone(),
                data,
                get_notification_position(app),
                get_notification_monitor(app),
                get_notification_offset(app),
            )
            .await;
        }
        Err(e) => {
            tracing::warn!("[Clipboard] Failed to check file URL: {}", e);
        }
    }
}

async fn handle_detected_url(app: &AppHandle, raw_url: &str) {
    let ignore_mixes = get_bool(app, "ignoreMixes", false);
    let url = url_utils::clean_url(raw_url, ignore_mixes);
    tracing::info!(
        "[Clipboard] handleDetectedUrl: {}",
        &url[..url.len().min(80)]
    );

    // If the URL input is focused, just paste the URL — don't resolve
    let input_focused = app
        .try_state::<Arc<InputFocusState>>()
        .map(|s| s.focused.load(Ordering::Relaxed))
        .unwrap_or(false);

    if input_focused {
        let _ = app.emit("clipboard-url-paste", &url);
        return;
    }

    // Notify frontend of resolve start (for loading toast) — only if window exists
    let has_window = is_main_window_visible(app);
    if has_window {
        let _ = app.emit("clipboard-url-resolving", &url);
    }

    let is_channel = url_utils::is_likely_channel(&url);
    let is_playlist = !is_channel && url_utils::is_likely_playlist(&url, ignore_mixes);

    let compact = get_bool(app, "compactNotifications", false);
    let position = get_notification_position(app);
    let monitor = get_notification_monitor(app);
    let offset = get_notification_offset(app);

    let manager = match app.try_state::<Arc<JobManager>>() {
        Some(m) => m.inner().clone(),
        None => {
            tracing::error!("[Clipboard] JobManager not available for resolve");
            if has_window {
                let _ = app.emit("clipboard-url-resolved", &url);
            }
            return;
        }
    };

    let resolve_timeout = std::time::Duration::from_secs(30);
    let resolve_settings = build_resolve_settings(app);

    if is_channel {
        match tokio::time::timeout(resolve_timeout, manager.resolve_url(&url, resolve_settings))
            .await
        {
            Err(_) => {
                tracing::warn!(
                    "[Clipboard] Channel resolve timed out for {}",
                    &url[..url.len().min(80)]
                );
                if has_window {
                    let _ = app.emit("clipboard-url-resolved", &url);
                }
                return;
            }
            Ok(Ok(result)) => {
                let info = &result.info;
                let channel_name = info
                    .channel
                    .as_deref()
                    .or(info.uploader.as_deref())
                    .or(info.title.as_deref())
                    .unwrap_or("Channel");
                let handle = info
                    .channel_id
                    .as_ref()
                    .map(|id| format!("@{}", id))
                    .unwrap_or_default();
                let total_count = info
                    .playlist_count
                    .unwrap_or_else(|| info.entries.as_ref().map(|e| e.len() as u32).unwrap_or(0));

                if total_count > 0 {
                    let body = if handle.is_empty() {
                        format!("{} videos", total_count)
                    } else {
                        format!("{} videos • {}", total_count, handle)
                    };

                    show_resolve_notification(
                        app,
                        &url,
                        channel_name,
                        &body,
                        info.thumbnail.as_deref(),
                        compact,
                        false,
                        true,
                        position,
                        monitor,
                        offset,
                    )
                    .await;
                    if has_window {
                        let _ = app.emit("clipboard-url-resolved", &url);
                    }
                    return;
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("[Clipboard] Channel resolve failed: {}", e);
            }
        }
        // Fall through to video resolve if channel resolve fails or has 0 items
    }

    if is_playlist {
        let resolve_settings = build_resolve_settings(app);
        match tokio::time::timeout(resolve_timeout, manager.resolve_url(&url, resolve_settings))
            .await
        {
            Err(_) => {
                tracing::warn!(
                    "[Clipboard] Playlist resolve timed out for {}",
                    &url[..url.len().min(80)]
                );
                if has_window {
                    let _ = app.emit("clipboard-url-resolved", &url);
                }
                return;
            }
            Ok(Ok(result)) => {
                let info = &result.info;
                let total_count = info
                    .playlist_count
                    .unwrap_or_else(|| info.entries.as_ref().map(|e| e.len() as u32).unwrap_or(0));

                if info.is_playlist && total_count > 0 {
                    let title = info.title.as_deref().unwrap_or("Playlist detected");
                    let body = format!("{} videos", total_count);

                    show_resolve_notification(
                        app, &url, title, &body, None, compact, true, false, position, monitor,
                        offset,
                    )
                    .await;
                    if has_window {
                        let _ = app.emit("clipboard-url-resolved", &url);
                    }
                    return;
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("[Clipboard] Playlist resolve failed: {}", e);
            }
        }
        // Fall through to video resolve
    }

    let resolve_settings = build_resolve_settings(app);
    match tokio::time::timeout(resolve_timeout, manager.resolve_url(&url, resolve_settings)).await {
        Err(_) => {
            tracing::warn!(
                "[Clipboard] Video resolve timed out for {}",
                &url[..url.len().min(80)]
            );
        }
        Ok(Ok(result)) => {
            let info = &result.info;
            let thumbnail = info
                .thumbnail
                .as_deref()
                .map(|s| s.to_string())
                .or_else(|| url_utils::get_quick_thumbnail(&url));

            let mut duration_str = String::new();
            if let Some(dur) = info.duration {
                if dur > 0 {
                    let mins = dur / 60;
                    let secs = dur % 60;
                    duration_str = format!(" • {}:{:02}", mins, secs);
                }
            }

            let is_twitter = url.contains("twitter.com") || url.contains("x.com");
            let author_display = if is_twitter {
                info.channel_id
                    .as_ref()
                    .map(|id| format!("@{}", id))
                    .or_else(|| info.uploader.clone())
                    .or_else(|| info.channel.clone())
                    .unwrap_or_default()
            } else {
                info.uploader
                    .clone()
                    .or_else(|| info.channel.clone())
                    .unwrap_or_default()
            };

            let title = info.title.as_deref().unwrap_or("Media Detected");
            let body = format!("{}{}", author_display, duration_str);

            show_resolve_notification(
                app,
                &url,
                title,
                &body,
                thumbnail.as_deref(),
                compact,
                false,
                false,
                position,
                monitor,
                offset,
            )
            .await;
        }
        Ok(Err(e)) => {
            tracing::warn!("[Clipboard] Video resolve failed: {}", e);
        }
    }

    if has_window {
        let _ = app.emit("clipboard-url-resolved", &url);
    }
}

/// Resolve succeeded — ALWAYS show a desktop notification popup.
async fn show_resolve_notification(
    app: &AppHandle,
    url: &str,
    title: &str,
    body: &str,
    thumbnail: Option<&str>,
    compact: bool,
    is_playlist: bool,
    is_channel: bool,
    position: Option<NotificationPosition>,
    monitor: Option<NotificationMonitor>,
    offset: Option<i32>,
) {
    let notifs_enabled = get_bool(app, "notificationsEnabled", true);
    if !notifs_enabled {
        tracing::debug!("[Clipboard] Notifications disabled, skipping popup");
        return;
    }

    tracing::info!(
        "[Clipboard] Showing notification: title={}, body={}, thumbnail={:?}",
        title,
        body,
        thumbnail
    );

    let data = NotificationData {
        title: title.to_string(),
        body: body.to_string(),
        thumbnail: thumbnail.map(|s| s.to_string()),
        url: Some(url.to_string()),
        compact,
        is_playlist,
        is_channel,
        is_file: false,
        file_info: None,
        download_label: "Download".to_string(),
        dismiss_label: "Dismiss".to_string(),
    };

    let _ =
        notifications::show_notification_window(app.clone(), data, position, monitor, offset).await;
}

fn is_main_window_visible(app: &AppHandle) -> bool {
    if let Some(window) = app.get_webview_window("main") {
        window.is_visible().unwrap_or(false)
    } else {
        false
    }
}

fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "Unknown size".to_string();
    }
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}
