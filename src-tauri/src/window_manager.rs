use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_store::StoreExt;
use tracing::info;

use crate::window_effects;

/// Shared state for the main window's one-shot show guard.
pub struct WindowState {
    pub was_shown: AtomicBool,
}

impl WindowState {
    pub fn new() -> Self {
        Self {
            was_shown: AtomicBool::new(false),
        }
    }
}

/// Recreate the main webview window after it has been destroyed (e.g. close-to-tray).
/// Mirrors the config from tauri.conf.json and applies window effects from settings.
#[cfg(not(target_os = "android"))]
pub fn recreate_main_window(app: &AppHandle) -> Result<(), String> {
    // If the main window already exists, just show it
    if let Some(existing) = app.get_webview_window("main") {
        let _ = existing.unminimize();
        let _ = existing.show();
        let _ = existing.set_focus();
        let _ = existing.emit("window-shown", ());
        return Ok(());
    }

    info!("Recreating main webview window");

    // Reset the one-shot show guard so frontend-ready can trigger show again
    if let Some(state) = app.try_state::<Arc<WindowState>>() {
        state.was_shown.store(false, Ordering::SeqCst);
    }

    // Build the window with the same config as tauri.conf.json
    let _window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("/".into()))
        .title("Comine")
        .inner_size(1080.0, 760.0)
        .visible(false)
        .transparent(true)
        .decorations(false)
        .shadow(true)
        .resizable(true)
        .center()
        .build()
        .map_err(|e| format!("Failed to recreate main window: {}", e))?;

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let effect_type = app_handle
            .store("settings.json")
            .ok()
            .and_then(|store| store.get("backgroundType"))
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "acrylic".to_string());

        let effect_type = match effect_type.as_str() {
            "acrylic" | "blur" | "mica" | "mica-dark" | "mica-light" | "tabbed" | "tabbed-dark"
            | "tabbed-light" => effect_type,
            _ if effect_type.starts_with("vibrancy-") => effect_type,
            _ => "none".to_string(),
        };

        let _ = window_effects::set_window_effect(app_handle, effect_type).await;
    });

    // Register a fresh frontend-ready listener for this window
    let app_for_listener = app.clone();
    let listener_id = app.listen("frontend-ready", move |_| {
        if let Some(state) = app_for_listener.try_state::<Arc<WindowState>>() {
            if state.was_shown.swap(true, Ordering::SeqCst) {
                return;
            }
        }
        if let Some(w) = app_for_listener.get_webview_window("main") {
            let _ = w.show();
            let _ = w.emit("window-shown", ());
            let _ = w.set_focus();
        }
    });

    // Safety timeout: show window after 6s if frontend-ready never fires
    let app_for_timeout = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(6000)).await;
        if let Some(state) = app_for_timeout.try_state::<Arc<WindowState>>() {
            if state.was_shown.swap(true, Ordering::SeqCst) {
                // Already shown, clean up the listener
                app_for_timeout.unlisten(listener_id);
                return;
            }
        }
        if let Some(w) = app_for_timeout.get_webview_window("main") {
            let _ = w.show();
            let _ = w.emit("window-shown", ());
        }
        app_for_timeout.unlisten(listener_id);
    });

    info!("Main window recreation initiated");
    Ok(())
}
