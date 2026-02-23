use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

use super::specs::{aria2, deno, ffmpeg, gallery_dl, lux, quickjs, ytdlp};

pub fn is_remote_newer(remote: &str, local: &str) -> bool {
    let remote = remote.strip_prefix('v').unwrap_or(remote);
    let local = local.strip_prefix('v').unwrap_or(local);

    let parse = |s: &str| -> Vec<u64> {
        s.split(|c: char| c == '.' || c == '-')
            .filter_map(|seg| seg.parse::<u64>().ok())
            .collect()
    };

    let rv = parse(remote);
    let lv = parse(local);

    for i in 0..rv.len().max(lv.len()) {
        let r = rv.get(i).copied().unwrap_or(0);
        let l = lv.get(i).copied().unwrap_or(0);
        match r.cmp(&l) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
        }
    }

    false
}

const INITIAL_DELAY_SECS: u64 = 10;
const CHECK_INTERVAL_SECS: u64 = 3600;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DepUpdateInfo {
    pub dep: String,
    pub version: String,
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(INITIAL_DELAY_SECS)).await;

        run_ytdlp_self_update(&app).await;
        run_dep_update_check(&app).await;

        loop {
            tokio::time::sleep(Duration::from_secs(CHECK_INTERVAL_SECS)).await;

            run_ytdlp_self_update(&app).await;
            run_dep_update_check(&app).await;
        }
    });
}

async fn run_ytdlp_self_update(app: &AppHandle) {
    match ytdlp::self_update_ytdlp(app.clone()).await {
        Ok(version) => info!("yt-dlp self-update complete: {}", version),
        Err(e) => warn!("yt-dlp self-update failed: {}", e),
    }
}

async fn run_dep_update_check(app: &AppHandle) {
    let check_enabled = crate::store_utils::get_bool(app, "checkDepUpdates", true);
    if !check_enabled {
        return;
    }

    info!("Checking for dependency updates...");

    let app_clone = app.clone();
    let results = tokio::join!(
        check_dep_update("ytdlp", ytdlp::check_ytdlp(app_clone.clone(), Some(true))),
        check_dep_update(
            "ffmpeg",
            ffmpeg::check_ffmpeg(app_clone.clone(), Some(true))
        ),
        check_dep_update("aria2", aria2::check_aria2(app_clone.clone(), Some(true))),
        check_dep_update("deno", deno::check_deno(app_clone.clone(), Some(true))),
        check_dep_update(
            "quickjs",
            quickjs::check_quickjs(app_clone.clone(), Some(true))
        ),
        check_dep_update("lux", lux::check_lux(app_clone.clone(), Some(true))),
        check_dep_update(
            "gallery_dl",
            gallery_dl::check_gallery_dl(app_clone.clone(), Some(true))
        ),
    );

    let updates: Vec<DepUpdateInfo> = [
        results.0, results.1, results.2, results.3, results.4, results.5, results.6,
    ]
    .into_iter()
    .flatten()
    .collect();

    if updates.is_empty() {
        info!("All dependencies are up to date");
    } else {
        let names: Vec<&str> = updates.iter().map(|u| u.dep.as_str()).collect();
        info!("Updates available for: {}", names.join(", "));
        let _ = app.emit("dep-updates-available", &updates);
    }
}

async fn check_dep_update(
    name: &str,
    check_future: impl std::future::Future<Output = Result<crate::types::DependencyStatus, String>>,
) -> Option<DepUpdateInfo> {
    match check_future.await {
        Ok(status) => {
            if status.installed {
                status.update_available.map(|version| DepUpdateInfo {
                    dep: name.to_string(),
                    version,
                })
            } else {
                None
            }
        }
        Err(e) => {
            warn!("Failed to check {} for updates: {}", name, e);
            None
        }
    }
}
