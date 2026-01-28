use crate::types::{NotificationData, NotificationMonitor, NotificationPosition};
use crate::utils::lock_or_recover;
use log::info;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::webview::Color;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Debug)]
struct NotificationInfo {
    slot: usize,
    position: NotificationPosition,
    width: u32,
    height: u32,
    offset: i32,
    monitor_width: i32,
    monitor_height: i32,
    monitor_x: i32,
    monitor_y: i32,
}

struct NotificationManager {
    notifications: HashMap<String, NotificationInfo>,
    occupied: Vec<bool>,
    last_creation: std::time::Instant,
}

impl NotificationManager {
    fn new() -> Self {
        Self {
            notifications: HashMap::new(),
            occupied: vec![false; 10],
            last_creation: std::time::Instant::now(),
        }
    }

    fn allocate_slot(&mut self, window_id: &str, info: NotificationInfo) -> usize {
        let slot = self.occupied.iter().position(|&x| !x).unwrap_or(0);
        self.occupied[slot] = true;
        let mut info = info;
        info.slot = slot;
        self.notifications.insert(window_id.to_string(), info);
        self.last_creation = std::time::Instant::now();
        slot
    }

    fn should_debounce(&self) -> bool {
        self.last_creation.elapsed() < std::time::Duration::from_millis(100)
    }

    fn free_slot(&mut self, window_id: &str) -> Vec<(String, NotificationInfo)> {
        if let Some(freed_info) = self.notifications.remove(window_id) {
            let freed_slot = freed_info.slot;
            self.occupied[freed_slot] = false;

            let mut to_reposition: Vec<(String, NotificationInfo)> = Vec::new();

            for (id, info) in &self.notifications {
                if info.slot > freed_slot {
                    let mut new_info = info.clone();
                    new_info.slot = info.slot - 1;
                    to_reposition.push((id.clone(), new_info));
                }
            }

            for (id, new_info) in &to_reposition {
                if let Some(info) = self.notifications.get_mut(id) {
                    self.occupied[info.slot] = false;
                    info.slot = new_info.slot;
                    self.occupied[new_info.slot] = true;
                }
            }

            to_reposition
        } else {
            Vec::new()
        }
    }
}

lazy_static::lazy_static! {
    static ref NOTIFICATION_MANAGER: Mutex<NotificationManager> = Mutex::new(NotificationManager::new());
}

fn calculate_position(
    monitor_width: i32,
    monitor_height: i32,
    monitor_x: i32,
    monitor_y: i32,
    width: u32,
    height: u32,
    margin: i32,
    slot: usize,
    position: &NotificationPosition,
    offset: i32,
) -> (i32, i32) {
    let slot_height = (height as i32) + 8;

    let x = match position {
        NotificationPosition::TopLeft | NotificationPosition::BottomLeft => monitor_x + margin,
        NotificationPosition::TopCenter | NotificationPosition::BottomCenter => {
            monitor_x + (monitor_width / 2) - (width as i32 / 2)
        }
        NotificationPosition::TopRight | NotificationPosition::BottomRight => {
            monitor_x + monitor_width - (width as i32) - margin
        }
    };

    let y = match position {
        NotificationPosition::TopLeft
        | NotificationPosition::TopCenter
        | NotificationPosition::TopRight => {
            monitor_y + margin + offset + (slot as i32 * slot_height)
        }
        NotificationPosition::BottomLeft
        | NotificationPosition::BottomCenter
        | NotificationPosition::BottomRight => {
            monitor_y + monitor_height
                - (height as i32)
                - margin
                - offset
                - (slot as i32 * slot_height)
        }
    };

    (x, y)
}

#[tauri::command]
pub async fn show_notification_window(
    app: AppHandle,
    data: NotificationData,
    position: Option<NotificationPosition>,
    monitor: Option<NotificationMonitor>,
    offset: Option<i32>,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return Err("Notification windows not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NOTIFICATION_COUNTER: AtomicU32 = AtomicU32::new(0);

        let should_wait = {
            let manager = lock_or_recover(&NOTIFICATION_MANAGER);
            manager.should_debounce()
        };

        if should_wait {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        let notification_id = NOTIFICATION_COUNTER.fetch_add(1, Ordering::Relaxed);
        let window_label = format!("notification-{}", notification_id);

        let position = position.unwrap_or_default();
        let monitor_setting = monitor.unwrap_or_default();
        let offset = offset.unwrap_or(48);

        info!(
            "Using position: {:?}, monitor: {:?}, offset: {}",
            position, monitor_setting, offset
        );

        let monitors = app.available_monitors().map_err(|e| e.to_string())?;

        let target_monitor = match monitor_setting {
            NotificationMonitor::Primary => monitors.into_iter().next(),
            NotificationMonitor::Cursor => {
                if let Some(main_window) = app.get_webview_window("main") {
                    if let Ok(cursor_pos) = main_window.cursor_position() {
                        monitors
                            .into_iter()
                            .find(|m: &tauri::Monitor| {
                                let pos = m.position();
                                let size = m.size();
                                let cx = cursor_pos.x as i32;
                                let cy = cursor_pos.y as i32;
                                cx >= pos.x
                                    && cx < pos.x + size.width as i32
                                    && cy >= pos.y
                                    && cy < pos.y + size.height as i32
                            })
                            .or_else(|| app.available_monitors().ok()?.into_iter().next())
                    } else {
                        monitors.into_iter().next()
                    }
                } else {
                    monitors.into_iter().next()
                }
            }
        }
        .ok_or("No monitor found")?;

        let monitor_size = target_monitor.size();
        let monitor_position = target_monitor.position();

        let width: u32 = if data.compact { 280 } else { 320 };
        let height: u32 = if data.compact { 48 } else { 100 };
        let margin: i32 = 16;

        let notif_info = NotificationInfo {
            slot: 0,
            position: position.clone(),
            width,
            height,
            offset,
            monitor_width: monitor_size.width as i32,
            monitor_height: monitor_size.height as i32,
            monitor_x: monitor_position.x,
            monitor_y: monitor_position.y,
        };

        let slot = {
            let mut manager = lock_or_recover(&NOTIFICATION_MANAGER);
            manager.allocate_slot(&window_label, notif_info)
        };

        let (x, y) = calculate_position(
            monitor_size.width as i32,
            monitor_size.height as i32,
            monitor_position.x,
            monitor_position.y,
            width,
            height,
            margin,
            slot,
            &position,
            offset,
        );

        let title_encoded = urlencoding::encode(&data.title);
        let body_encoded = urlencoding::encode(&data.body);
        let thumbnail_encoded = data
            .thumbnail
            .as_ref()
            .map(|t| urlencoding::encode(t).to_string())
            .unwrap_or_default();
        let url_encoded = data
            .url
            .as_ref()
            .map(|u| urlencoding::encode(u).to_string())
            .unwrap_or_default();
        let compact = if data.compact { "1" } else { "0" };
        let is_playlist = if data.is_playlist { "1" } else { "0" };
        let is_channel = if data.is_channel { "1" } else { "0" };
        let is_file = if data.is_file { "1" } else { "0" };
        let download_label = urlencoding::encode(&data.download_label);
        let dismiss_label = urlencoding::encode(&data.dismiss_label);

        // Serialize file_info as JSON and encode it
        let file_info_encoded = data
            .file_info
            .as_ref()
            .map(|fi| {
                urlencoding::encode(&serde_json::to_string(fi).unwrap_or_default()).to_string()
            })
            .unwrap_or_default();

        let notification_url = format!(
            "/notification?title={}&body={}&thumbnail={}&url={}&window_id={}&compact={}&dl={}&dm={}&is_playlist={}&is_channel={}&is_file={}&file_info={}",
            title_encoded, body_encoded, thumbnail_encoded, url_encoded, window_label, compact, download_label, dismiss_label, is_playlist, is_channel, is_file, file_info_encoded
        );

        info!(
            "Creating notification window at ({}, {}) slot {} position {:?}",
            x, y, slot, position
        );

        WebviewWindowBuilder::new(
            &app,
            &window_label,
            WebviewUrl::App(notification_url.into()),
        )
        .title("Comine: Notification")
        .inner_size(width as f64, height as f64)
        .min_inner_size(width as f64, height as f64)
        .max_inner_size(width as f64, height as f64)
        .position(x as f64, y as f64)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focused(false)
        .visible(false)
        .shadow(false)
        .focusable(false)
        .background_color(Color(0, 0, 0, 0))
        .build()
        .map_err(|e| format!("Failed to create notification window: {}", e))?;

        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{
                GetWindowLongW, SetWindowLongPtrW, SetWindowLongW, GWL_EXSTYLE, GWLP_HWNDPARENT,
                WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            };

            if let Some(window) = app.get_webview_window(&window_label) {
                if let Ok(raw_hwnd) = window.hwnd() {
                    unsafe {
                        let hwnd = HWND(raw_hwnd.0 as *mut _);

                        // Add WS_EX_TOOLWINDOW to hide from taskbar,
                        // WS_EX_NOACTIVATE to prevent activation on click
                        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                        SetWindowLongW(
                            hwnd,
                            GWL_EXSTYLE,
                            (ex_style & !(WS_EX_APPWINDOW.0 as i32))
                                | WS_EX_TOOLWINDOW.0 as i32
                                | WS_EX_NOACTIVATE.0 as i32,
                        );

                        if let Some(main_window) = app.get_webview_window("main") {
                            if let Ok(main_hwnd_raw) = main_window.hwnd() {
                                let main_hwnd = HWND(main_hwnd_raw.0 as *mut _);
                                let _ = SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, main_hwnd.0 as isize);
                            }
                        }
                    }
                }
            }
        }

        {
            let app_handle = app.clone();
            let window_id = window_label.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(750)).await;
                if let Some(window) = app_handle.get_webview_window(&window_id) {
                    if window.is_visible().ok() == Some(false) {
                        let _ = window.show();
                    }
                }
            });
        }

        info!("Notification window created: {}", window_label);
        Ok(())
    }
}

#[tauri::command]
pub async fn reveal_notification_window(app: AppHandle, window_id: String) -> Result<(), String> {
    #[cfg(not(target_os = "android"))]
    {
        info!("Revealing notification window: {}", window_id);
        if let Some(window) = app.get_webview_window(&window_id) {
            #[cfg(target_os = "windows")]
            {
                use windows::Win32::Foundation::HWND;
                use windows::Win32::UI::WindowsAndMessaging::{
                    GetWindowLongW, SetWindowLongPtrW, SetWindowLongW, SetWindowPos, GWL_EXSTYLE,
                    GWLP_HWNDPARENT, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
                    SWP_NOZORDER, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
                };

                if let Ok(raw_hwnd) = window.hwnd() {
                    unsafe {
                        let hwnd = HWND(raw_hwnd.0 as *mut _);
                        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                        SetWindowLongW(
                            hwnd,
                            GWL_EXSTYLE,
                            (ex_style & !(WS_EX_APPWINDOW.0 as i32))
                                | WS_EX_TOOLWINDOW.0 as i32
                                | WS_EX_NOACTIVATE.0 as i32,
                        );

                        if let Some(main_window) = app.get_webview_window("main") {
                            if let Ok(main_hwnd_raw) = main_window.hwnd() {
                                let main_hwnd = HWND(main_hwnd_raw.0 as *mut _);
                                let _ = SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, main_hwnd.0 as isize);
                            }
                        }

                        let _ = SetWindowPos(
                            hwnd,
                            HWND(std::ptr::null_mut()),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE
                                | SWP_NOSIZE
                                | SWP_NOZORDER
                                | SWP_NOOWNERZORDER
                                | SWP_FRAMECHANGED,
                        );
                    }
                }
            }

            let _ = window.set_always_on_top(true);
            let _ = window.show();
            info!("Notification window revealed");
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn close_notification_window(app: AppHandle, window_id: String) -> Result<(), String> {
    #[cfg(not(target_os = "android"))]
    {
        info!("Closing notification: {}", window_id);

        if let Some(window) = app.get_webview_window(&window_id) {
            window.close().map_err(|e| e.to_string())?;
        }

        let to_reposition = {
            let mut manager = lock_or_recover(&NOTIFICATION_MANAGER);
            manager.free_slot(&window_id)
        };

        let margin: i32 = 16;
        for (id, info) in to_reposition {
            if let Some(window) = app.get_webview_window(&id) {
                let (new_x, new_y) = calculate_position(
                    info.monitor_width,
                    info.monitor_height,
                    info.monitor_x,
                    info.monitor_y,
                    info.width,
                    info.height,
                    margin,
                    info.slot,
                    &info.position,
                    info.offset,
                );
                info!("Repositioning {} to slot {} (y={})", id, info.slot, new_y);
                let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                    x: new_x,
                    y: new_y,
                }));
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn close_all_notifications(app: AppHandle) -> Result<(), String> {
    #[cfg(not(target_os = "android"))]
    {
        info!("Closing all notification windows");

        let window_ids: Vec<String> = {
            let manager = lock_or_recover(&NOTIFICATION_MANAGER);
            manager.notifications.keys().cloned().collect()
        };

        for window_id in window_ids {
            if let Some(window) = app.get_webview_window(&window_id) {
                let _ = window.close();
            }
            let mut manager = lock_or_recover(&NOTIFICATION_MANAGER);
            manager.free_slot(&window_id);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn notification_action(
    app: AppHandle,
    window_id: String,
    url: Option<String>,
    metadata: Option<serde_json::Value>,
    keep_open: Option<bool>,
) -> Result<(), String> {
    #[cfg(not(target_os = "android"))]
    {
        info!(
            "Notification action triggered: window_id={}, url={:?}, has_metadata={}, keep_open={:?}",
            window_id,
            url,
            metadata.is_some(),
            keep_open
        );

        if let Some(video_url) = &url {
            let payload = serde_json::json!({
                "url": video_url,
                "metadata": metadata
            });

            if let Some(main_window) = app.get_webview_window("main") {
                let _ = main_window.emit("notification-start-download", payload);
                info!("Emitted notification-start-download to main window");
            } else {
                let _ = app.emit("notification-start-download", payload);
                info!("Emitted notification-start-download globally");
            }
        }

        if !keep_open.unwrap_or(false) {
            close_notification_window(app, window_id).await?;
        }
    }
    Ok(())
}
