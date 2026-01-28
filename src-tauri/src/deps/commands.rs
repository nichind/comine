//! Tauri command wrappers for dependency management.

use tauri::AppHandle;

use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, ReleaseInfo};

use super::engine::cancel;
use super::specs::{aria2, deno, ffmpeg, lux, quickjs, ytdlp};

#[allow(unused_imports)]
pub use aria2::get_aria2_path;
#[allow(unused_imports)]
pub use deno::get_deno_path;
pub use ffmpeg::get_ffmpeg_path;
#[allow(unused_imports)]
pub use lux::get_lux_path;
#[allow(unused_imports)]
pub use quickjs::get_quickjs_path;
pub use ytdlp::get_ytdlp_path;

#[tauri::command]
pub async fn check_ytdlp(
    app: AppHandle,
    check_updates: Option<bool>,
) -> Result<DependencyStatus, String> {
    ytdlp::check_ytdlp(app, check_updates).await
}

#[tauri::command]
pub async fn install_ytdlp(
    app: AppHandle,
    version: Option<String>,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    ytdlp::install_ytdlp(app, version, proxy_config).await
}

#[tauri::command]
pub async fn uninstall_ytdlp(app: AppHandle) -> Result<(), String> {
    ytdlp::uninstall_ytdlp(app).await
}

#[tauri::command]
pub async fn get_ytdlp_releases(
    proxy_config: Option<ProxyConfig>,
) -> Result<Vec<ReleaseInfo>, String> {
    ytdlp::get_ytdlp_releases(proxy_config).await
}

#[tauri::command]
pub async fn update_ytdlp_channel(app: AppHandle, channel: String) -> Result<String, String> {
    ytdlp::update_ytdlp_channel(app, channel).await
}

#[tauri::command]
pub async fn check_ffmpeg(app: AppHandle) -> Result<DependencyStatus, String> {
    ffmpeg::check_ffmpeg(app).await
}

#[tauri::command]
pub async fn install_ffmpeg(
    app: AppHandle,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    ffmpeg::install_ffmpeg(app, proxy_config).await
}

#[tauri::command]
pub async fn uninstall_ffmpeg(app: AppHandle) -> Result<(), String> {
    ffmpeg::uninstall_ffmpeg(app).await
}

#[tauri::command]
pub async fn check_aria2(app: AppHandle) -> Result<DependencyStatus, String> {
    aria2::check_aria2(app).await
}

#[tauri::command]
pub async fn install_aria2(
    app: AppHandle,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    aria2::install_aria2(app, proxy_config).await
}

#[tauri::command]
pub async fn uninstall_aria2(app: AppHandle) -> Result<(), String> {
    aria2::uninstall_aria2(app).await
}

#[tauri::command]
pub async fn check_deno(app: AppHandle) -> Result<DependencyStatus, String> {
    deno::check_deno(app).await
}

#[tauri::command]
pub async fn install_deno(
    app: AppHandle,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    deno::install_deno(app, proxy_config).await
}

#[tauri::command]
pub async fn uninstall_deno(app: AppHandle) -> Result<(), String> {
    deno::uninstall_deno(app).await
}

#[tauri::command]
pub async fn check_quickjs(app: AppHandle) -> Result<DependencyStatus, String> {
    quickjs::check_quickjs(app).await
}

#[tauri::command]
pub async fn install_quickjs(
    app: AppHandle,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    quickjs::install_quickjs(app, proxy_config).await
}

#[tauri::command]
pub async fn uninstall_quickjs(app: AppHandle) -> Result<(), String> {
    quickjs::uninstall_quickjs(app).await
}

#[tauri::command]
pub async fn check_lux(app: AppHandle) -> Result<DependencyStatus, String> {
    lux::check_lux(app).await
}

#[tauri::command]
pub async fn install_lux(
    app: AppHandle,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    lux::install_lux(app, proxy_config).await
}

#[tauri::command]
pub async fn uninstall_lux(app: AppHandle) -> Result<(), String> {
    lux::uninstall_lux(app).await
}

#[tauri::command]
pub async fn cancel_dep_install(dep: String) -> Result<(), String> {
    match dep.as_str() {
        "ytdlp" => {
            cancel::cancel("ytdlp");
            Ok(())
        }
        "ffmpeg" => {
            cancel::cancel("ffmpeg");
            Ok(())
        }
        "aria2" => {
            cancel::cancel("aria2");
            Ok(())
        }
        "deno" => {
            cancel::cancel("deno");
            Ok(())
        }
        "quickjs" => {
            cancel::cancel("quickjs");
            Ok(())
        }
        "lux" => {
            cancel::cancel("lux");
            Ok(())
        }
        _ => Err(format!("Unknown dependency: {}", dep)),
    }
}
