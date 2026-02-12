use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::image::Image;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tracing::info;

use crate::orchestrator::manager::JobManager;
use crate::orchestrator::types::HistoryItem;
use crate::store_utils;
use crate::window_manager;

struct TrayStrings {
    show: &'static str,
    download_clipboard: &'static str,
    start_on_boot: &'static str,
    watch_clipboard: &'static str,
    pause_all: &'static str,
    resume_all: &'static str,
    recent_downloads: &'static str,
    no_recent_downloads: &'static str,
    quit: &'static str,
}

fn get_tray_strings(lang: &str) -> TrayStrings {
    match lang {
        "ru" => TrayStrings {
            show: "Показать Comine",
            download_clipboard: "Скачать из буфера",
            start_on_boot: "Запуск при старте",
            watch_clipboard: "Следить за буфером",
            pause_all: "Приостановить все",
            resume_all: "Возобновить все",
            recent_downloads: "Недавние загрузки",
            no_recent_downloads: "Нет недавних загрузок",
            quit: "Выход",
        },
        _ => TrayStrings {
            show: "Show Comine",
            download_clipboard: "Download from clipboard",
            start_on_boot: "Start on boot",
            watch_clipboard: "Watch clipboard",
            pause_all: "Pause all",
            resume_all: "Resume all",
            recent_downloads: "Recent downloads",
            no_recent_downloads: "No recent downloads",
            quit: "Quit",
        },
    }
}

pub struct TrayState {
    pub recent_paths: Mutex<HashMap<String, String>>,
}

fn show_or_recreate_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("window-shown", ());
    } else {
        if let Err(e) = window_manager::recreate_main_window(app) {
            tracing::error!("Failed to recreate main window from tray: {}", e);
        }
    }
}

fn build_tray_menu(
    app: &AppHandle,
    prefetched_history: Option<Vec<HistoryItem>>,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let lang = store_utils::get_str(app, "language", "en");
    let s = get_tray_strings(&lang);

    let show = MenuItemBuilder::with_id("show", s.show).build(app)?;
    let download = MenuItemBuilder::with_id("download", s.download_clipboard).build(app)?;

    let sep1 = tauri::menu::PredefinedMenuItem::separator(app)?;

    let autostart_enabled = {
        #[cfg(not(target_os = "android"))]
        {
            use tauri_plugin_autostart::ManagerExt;
            app.autolaunch().is_enabled().unwrap_or(false)
        }
        #[cfg(target_os = "android")]
        {
            false
        }
    };
    let watch_clipboard = store_utils::get_bool(app, "watchClipboard", true);

    let autostart_check = CheckMenuItemBuilder::with_id("autostart", s.start_on_boot)
        .checked(autostart_enabled)
        .build(app)?;
    let watch_check = CheckMenuItemBuilder::with_id("watch_clipboard", s.watch_clipboard)
        .checked(watch_clipboard)
        .build(app)?;

    let sep2 = tauri::menu::PredefinedMenuItem::separator(app)?;

    // Pause / Resume — always enabled, handlers are no-ops when no active jobs
    let pause = MenuItemBuilder::with_id("pause_all", s.pause_all).build(app)?;
    let resume = MenuItemBuilder::with_id("resume_all", s.resume_all).build(app)?;

    let sep3 = tauri::menu::PredefinedMenuItem::separator(app)?;

    let mut recent_sub = SubmenuBuilder::with_id(app, "recent", s.recent_downloads);
    let mut recent_paths = HashMap::new();

    if let Some(manager) = app.try_state::<Arc<JobManager>>() {
        let items = prefetched_history.unwrap_or_else(|| manager.history.get_all_blocking());
        let count = items.len().min(10);
        if count > 0 {
            for item in items.iter().take(10) {
                let id = format!("recent_{}", item.id);
                let label = if item.title.is_empty() {
                    item.file_path
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or("Unknown")
                        .to_string()
                } else {
                    truncate_str(&item.title, 60)
                };
                let menu_item = MenuItemBuilder::with_id(&id, &label).build(app)?;
                recent_sub = recent_sub.item(&menu_item);
                recent_paths.insert(id, item.file_path.clone());
            }
        } else {
            let empty = MenuItemBuilder::with_id("recent_empty", s.no_recent_downloads)
                .enabled(false)
                .build(app)?;
            recent_sub = recent_sub.item(&empty);
        }
    } else {
        let empty = MenuItemBuilder::with_id("recent_empty", s.no_recent_downloads)
            .enabled(false)
            .build(app)?;
        recent_sub = recent_sub.item(&empty);
    }

    let recent_submenu = recent_sub.build()?;

    if let Some(state) = app.try_state::<Arc<TrayState>>() {
        *state.recent_paths.lock().unwrap() = recent_paths;
    }

    let sep4 = tauri::menu::PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id("quit", s.quit).build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&download)
        .item(&sep1)
        .item(&autostart_check)
        .item(&watch_check)
        .item(&sep2)
        .item(&pause)
        .item(&resume)
        .item(&sep3)
        .item(&recent_submenu)
        .item(&sep4)
        .item(&quit)
        .build()?;

    Ok(menu)
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{}…", truncated)
    }
}

#[tauri::command]
pub fn rebuild_tray_menu(app: AppHandle) -> Result<(), String> {
    rebuild_menu_inner(&app).map_err(|e| format!("Failed to rebuild tray menu: {}", e))
}

fn rebuild_menu_inner(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let menu = build_tray_menu(app, None)?;
    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

/// Async version — fetches history without blocking the tokio runtime
pub async fn rebuild_menu_async(app: &AppHandle) -> Result<(), String> {
    let history = if let Some(manager) = app.try_state::<Arc<JobManager>>() {
        Some(manager.history.get_all().await)
    } else {
        None
    };
    let menu =
        build_tray_menu(app, history).map_err(|e| format!("Failed to build tray menu: {}", e))?;
    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_menu(Some(menu))
            .map_err(|e| format!("Failed to set tray menu: {}", e))?;
    }
    Ok(())
}

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(Arc::new(TrayState {
        recent_paths: Mutex::new(HashMap::new()),
    }));

    let menu = build_tray_menu(app, None)?;

    #[cfg(target_os = "windows")]
    let icon = Image::from_path("icons/icon.ico")
        .or_else(|_| Image::from_path("icons/32x32.png"))
        .unwrap_or_else(|_| {
            Image::from_bytes(include_bytes!("../icons/icon.ico"))
                .expect("Failed to load embedded icon")
        });

    #[cfg(not(target_os = "windows"))]
    let icon = Image::from_path("icons/icon.png")
        .or_else(|_| Image::from_path("icons/32x32.png"))
        .unwrap_or_else(|_| {
            Image::from_bytes(include_bytes!("../icons/32x32.png"))
                .expect("Failed to load embedded icon")
        });

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Comine")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref().to_string();
            handle_menu_event(app, &id);
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_or_recreate_window(tray.app_handle());
            }
        })
        .build(app)?;

    info!("System tray initialized");
    Ok(())
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "show" => {
            show_or_recreate_window(app);
        }
        "download" => {
            if app.get_webview_window("main").is_some() {
                let _ = app.emit("tray-download-clipboard", ());
            } else {
                if let Err(e) = window_manager::recreate_main_window(app) {
                    tracing::error!("Failed to recreate window for clipboard download: {}", e);
                    return;
                }
                let app_clone = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;
                    let _ = app_clone.emit("tray-download-clipboard", ());
                });
            }
        }
        "autostart" => {
            #[cfg(not(target_os = "android"))]
            {
                use tauri_plugin_autostart::ManagerExt;
                let currently = app.autolaunch().is_enabled().unwrap_or(false);
                if currently {
                    let _ = app.autolaunch().disable();
                } else {
                    let _ = app.autolaunch().enable();
                }
            }
            let _ = rebuild_menu_inner(app);
        }
        "watch_clipboard" => {
            let current = store_utils::get_bool(app, "watchClipboard", true);
            store_utils::set_bool(app, "watchClipboard", !current);
            let _ = app.emit(
                "setting-changed",
                serde_json::json!({
                    "key": "watchClipboard",
                    "value": !current
                }),
            );
            let _ = rebuild_menu_inner(app);
        }
        "pause_all" => {
            if let Some(manager) = app.try_state::<Arc<JobManager>>() {
                let m = manager.inner().clone();
                tauri::async_runtime::spawn(async move {
                    m.pause_all().await;
                });
            }
        }
        "resume_all" => {
            if let Some(manager) = app.try_state::<Arc<JobManager>>() {
                let m = manager.inner().clone();
                tauri::async_runtime::spawn(async move {
                    m.resume_all().await;
                });
            }
        }
        "quit" => {
            app.exit(0);
        }
        other if other.starts_with("recent_") => {
            let file_path = app
                .try_state::<Arc<TrayState>>()
                .and_then(|state| state.recent_paths.lock().ok()?.get(other).cloned());
            if let Some(path) = file_path {
                tracing::info!("[Tray] Revealing recent download: {}", path);
                tauri::async_runtime::spawn(async move {
                    let _ = crate::reveal_file_internal(path).await;
                });
            } else {
                tracing::warn!("[Tray] No path found for menu id: {}", other);
            }
        }
        _ => {}
    }
}
