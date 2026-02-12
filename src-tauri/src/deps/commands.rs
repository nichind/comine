use tauri::{AppHandle, Manager};

use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, ReleaseInfo};

use super::engine::cancel;
use super::specs::{aria2, deno, ffmpeg, gallery_dl, lux, quickjs, ytdlp};

#[allow(unused_imports)]
pub use aria2::get_aria2_path;
#[allow(unused_imports)]
pub use aria2::resolve_aria2_path;
#[allow(unused_imports)]
pub use deno::get_deno_path;
#[allow(unused_imports)]
pub use deno::resolve_deno_path;
#[allow(unused_imports)]
pub use ffmpeg::get_ffmpeg_path;
#[allow(unused_imports)]
pub use ffmpeg::get_ffprobe_path;
#[allow(unused_imports)]
pub use ffmpeg::resolve_ffmpeg_path;
#[allow(unused_imports)]
pub use ffmpeg::resolve_ffprobe_path;
#[allow(unused_imports)]
pub use gallery_dl::get_gallery_dl_path;
#[allow(unused_imports)]
pub use gallery_dl::resolve_gallery_dl_path;
#[allow(unused_imports)]
pub use lux::get_lux_path;
#[allow(unused_imports)]
pub use lux::resolve_lux_path;
#[allow(unused_imports)]
pub use quickjs::get_quickjs_path;
#[allow(unused_imports)]
pub use quickjs::resolve_quickjs_path;
#[allow(unused_imports)]
pub use ytdlp::get_ytdlp_path;
#[allow(unused_imports)]
pub use ytdlp::resolve_ytdlp_path;

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
pub async fn check_gallery_dl(
    app: AppHandle,
    check_updates: Option<bool>,
) -> Result<DependencyStatus, String> {
    gallery_dl::check_gallery_dl(app, check_updates).await
}

#[tauri::command]
pub async fn install_gallery_dl(
    app: AppHandle,
    version: Option<String>,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    let result = gallery_dl::install_gallery_dl(app.clone(), version, proxy_config).await?;

    // Dynamically register gallery-dl backend now that it's installed.
    // This avoids requiring an app restart after install.
    #[cfg(not(target_os = "android"))]
    {
        use crate::orchestrator::manager::JobManager;
        use std::sync::Arc;

        if let Some(manager) = app.try_state::<Arc<JobManager>>() {
            if let Some(path) = gallery_dl::resolve_gallery_dl_path(&app) {
                manager
                    .register_backend(Arc::new(
                        crate::orchestrator::backends::gallery_dl::GalleryDlBackend::new(path),
                    ))
                    .await;
                tracing::info!("gallery-dl backend registered after install");
            }
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn uninstall_gallery_dl(app: AppHandle) -> Result<(), String> {
    gallery_dl::uninstall_gallery_dl(app.clone()).await?;

    #[cfg(not(target_os = "android"))]
    {
        use crate::orchestrator::manager::JobManager;
        use std::sync::Arc;

        if let Some(manager) = app.try_state::<Arc<JobManager>>() {
            manager.unregister_backend("gallery-dl").await;
            tracing::info!("gallery-dl backend unregistered after uninstall");
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn get_gallery_dl_releases(
    proxy_config: Option<ProxyConfig>,
) -> Result<Vec<ReleaseInfo>, String> {
    gallery_dl::get_gallery_dl_releases(proxy_config).await
}

#[tauri::command]
pub async fn cancel_dep_install(dep: String) -> Result<(), String> {
    const KNOWN_DEPS: &[&str] = &[
        "ytdlp",
        "ffmpeg",
        "aria2",
        "deno",
        "quickjs",
        "lux",
        "gallery-dl",
    ];
    if KNOWN_DEPS.contains(&dep.as_str()) {
        cancel::cancel(&dep);
        Ok(())
    } else {
        Err(format!("Unknown dependency: {}", dep))
    }
}
