use tauri::AppHandle;
#[cfg(not(target_os = "android"))]
use tracing::error;
use tracing::info;

use tauri::Manager;

#[tauri::command]
#[allow(unused_variables)]
pub async fn set_window_effect(app: AppHandle, effect_type: String) -> Result<(), String> {
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

            let effect_type = match effect_type.as_str() {
                "acrylic" | "blur" | "mica" | "mica-dark" | "tabbed" | "tabbed-dark" => "vibrancy-under-window".to_string(),
                "mica-light" | "tabbed-light" => "vibrancy-under-window".to_string(),
                other => other.to_string(),
            };

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
pub async fn set_acrylic(app: AppHandle, enable: bool) -> Result<(), String> {
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
