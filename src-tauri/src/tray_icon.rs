use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use image::{Rgba, RgbaImage};
use tauri::{AppHandle, Manager};

use crate::orchestrator::manager::JobManager;
use crate::orchestrator::types::JobStatus;

const POLL_INTERVAL_MS: u64 = 500;
const ICON_BYTES: &[u8] = include_bytes!("../icons/32x32.png");

pub struct TrayIconAnimState {
    running: AtomicBool,
}

impl TrayIconAnimState {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TrayIconMode {
    Normal,
    Resolving,
    Progress(f64), // 0.0 .. 1.0
}

pub fn start_polling(app: &AppHandle) {
    let state = Arc::new(TrayIconAnimState::new());
    if state.running.swap(true, Ordering::SeqCst) {
        return;
    }
    app.manage(state.clone());

    let app_handle = app.clone();
    let base_icon = load_base_icon();

    tauri::async_runtime::spawn(async move {
        let mut last_mode = TrayIconMode::Normal;
        let mut anim_tick: u32 = 0;

        loop {
            if !state.running.load(Ordering::Relaxed) {
                break;
            }

            let (mode, tooltip) = compute_mode_and_tooltip(&app_handle);

            if let Some(tray) = app_handle.tray_by_id("main-tray") {
                let _ = tray.set_tooltip(Some(&tooltip));
            }

            let needs_update = match (&mode, &last_mode) {
                (TrayIconMode::Normal, TrayIconMode::Normal) => false,
                (TrayIconMode::Resolving, TrayIconMode::Resolving) => true, // animate
                (TrayIconMode::Progress(a), TrayIconMode::Progress(b)) => (a - b).abs() > 0.005,
                _ => true,
            };

            if needs_update {
                let icon_img = match &mode {
                    TrayIconMode::Normal => base_icon.clone(),
                    TrayIconMode::Resolving => render_resolving(&base_icon, anim_tick),
                    TrayIconMode::Progress(p) => render_progress(&base_icon, *p),
                };

                set_tray_icon(&app_handle, &icon_img);
            } else if mode == TrayIconMode::Normal && last_mode != TrayIconMode::Normal {
                // Went back to normal — restore original icon
                set_tray_icon(&app_handle, &base_icon);
            }

            last_mode = mode;
            anim_tick = anim_tick.wrapping_add(1);

            tokio::time::sleep(tokio::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
    });
}

fn compute_mode_and_tooltip(app: &AppHandle) -> (TrayIconMode, String) {
    let manager = match app.try_state::<Arc<JobManager>>() {
        Some(m) => m.inner().clone(),
        None => return (TrayIconMode::Normal, "Comine".to_string()),
    };

    let jobs = manager.get_all_jobs();
    let active: Vec<_> = jobs.iter().filter(|j| j.status.is_active()).collect();

    if active.is_empty() {
        return (TrayIconMode::Normal, "Comine".to_string());
    }

    let any_downloading = active
        .iter()
        .any(|j| matches!(j.status, JobStatus::Downloading | JobStatus::PostProcessing));
    let any_resolving = active
        .iter()
        .any(|j| matches!(j.status, JobStatus::Resolving));

    if any_downloading {
        let download_jobs: Vec<_> = active
            .iter()
            .filter(|j| matches!(j.status, JobStatus::Downloading | JobStatus::PostProcessing))
            .collect();

        let total_progress: f64 = download_jobs.iter().map(|j| j.progress).sum();
        let avg = total_progress / download_jobs.len() as f64;

        let total_speed: u64 = download_jobs.iter().filter_map(|j| j.speed).sum();
        let max_eta: Option<u64> = download_jobs.iter().filter_map(|j| j.eta).max();

        let count = download_jobs.len();
        let mut tooltip = format!("Comine — {} downloading", count);

        if total_speed > 0 {
            tooltip.push_str(&format!(" · {}", format_speed(total_speed as f64)));
        }

        if let Some(eta) = max_eta {
            if eta > 0 && eta < 86400 {
                tooltip.push_str(&format!(" · ETA {}", format_eta(eta as f64)));
            }
        }

        (
            TrayIconMode::Progress((avg / 100.0).clamp(0.0, 1.0)),
            tooltip,
        )
    } else if any_resolving {
        let resolving_count = active
            .iter()
            .filter(|j| matches!(j.status, JobStatus::Resolving))
            .count();
        let tooltip = if resolving_count == 1 {
            "Comine — resolving…".to_string()
        } else {
            format!("Comine — {} resolving…", resolving_count)
        };
        (TrayIconMode::Resolving, tooltip)
    } else {
        let tooltip = format!("Comine — {} active", active.len());
        (TrayIconMode::Normal, tooltip)
    }
}

fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_073_741_824.0 {
        format!("{:.1} GB/s", bytes_per_sec / 1_073_741_824.0)
    } else if bytes_per_sec >= 1_048_576.0 {
        format!("{:.1} MB/s", bytes_per_sec / 1_048_576.0)
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.0} KB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

fn format_eta(seconds: f64) -> String {
    let secs = seconds as u64;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}

fn load_base_icon() -> RgbaImage {
    image::load_from_memory(ICON_BYTES)
        .expect("Failed to decode embedded 32x32 icon")
        .to_rgba8()
}

/// Draw a progress fill from bottom to top (tinted with accent color)
fn render_progress(base: &RgbaImage, progress: f64) -> RgbaImage {
    let (w, h) = base.dimensions();
    let mut img = base.clone();
    let fill_height = ((h as f64) * progress).round() as u32;

    // Semi-transparent green overlay on filled portion
    for y in 0..h {
        let from_bottom = h - 1 - y;
        let in_fill = from_bottom < fill_height;
        for x in 0..w {
            let px = img.get_pixel(x, y);
            if px[3] == 0 {
                continue; // skip transparent
            }
            if in_fill {
                // Blend with green tint
                let r = ((px[0] as f32) * 0.4 + 80.0).min(255.0) as u8;
                let g = ((px[1] as f32) * 0.4 + 180.0).min(255.0) as u8;
                let b = ((px[2] as f32) * 0.4 + 80.0).min(255.0) as u8;
                img.put_pixel(x, y, Rgba([r, g, b, px[3]]));
            } else {
                // Dim the unfilled portion
                let r = ((px[0] as f32) * 0.5) as u8;
                let g = ((px[1] as f32) * 0.5) as u8;
                let b = ((px[2] as f32) * 0.5) as u8;
                img.put_pixel(x, y, Rgba([r, g, b, px[3]]));
            }
        }
    }
    img
}

/// Pulsing effect for resolving state
fn render_resolving(base: &RgbaImage, tick: u32) -> RgbaImage {
    let (w, h) = base.dimensions();
    let mut img = base.clone();

    // Pulse factor: oscillates between 0.4 and 1.0
    let phase = (tick as f64 * 0.3).sin() * 0.3 + 0.7; // 0.4..1.0

    for y in 0..h {
        for x in 0..w {
            let px = img.get_pixel(x, y);
            if px[3] == 0 {
                continue;
            }
            let r = ((px[0] as f64) * phase).min(255.0) as u8;
            let g = ((px[1] as f64) * phase).min(255.0) as u8;
            let b = ((px[2] as f64) * phase).min(255.0) as u8;
            img.put_pixel(x, y, Rgba([r, g, b, px[3]]));
        }
    }
    img
}

fn set_tray_icon(app: &AppHandle, img: &RgbaImage) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let (w, h) = img.dimensions();
        let rgba = img.as_raw().to_vec();
        let icon = tauri::image::Image::new_owned(rgba, w, h);
        let _ = tray.set_icon(Some(icon));
    }
}
