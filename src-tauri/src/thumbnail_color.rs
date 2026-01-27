use crate::utils::lock_or_recover;

use log::{debug, warn};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;
use tauri::Manager;
use tokio::sync::{broadcast, Mutex as AsyncMutex};

const THUMBNAIL_COLOR_CACHE_SIZE: std::num::NonZeroUsize =
    unsafe { std::num::NonZeroUsize::new_unchecked(500) };

static THUMBNAIL_COLOR_CACHE: std::sync::LazyLock<Mutex<lru::LruCache<String, [u8; 3]>>> =
    std::sync::LazyLock::new(|| Mutex::new(lru::LruCache::new(THUMBNAIL_COLOR_CACHE_SIZE)));

const THUMBNAIL_COLOR_DISK_CACHE_FILE: &str = "thumbnail_colors.json";

#[derive(Default)]
struct ThumbnailColorDiskCache {
    loaded: bool,
    dirty: bool,
    map: HashMap<String, [u8; 3]>,
}

static THUMBNAIL_COLOR_DISK_CACHE: std::sync::LazyLock<Mutex<ThumbnailColorDiskCache>> =
    std::sync::LazyLock::new(|| Mutex::new(ThumbnailColorDiskCache::default()));

static THUMBNAIL_COLOR_INFLIGHT: std::sync::LazyLock<
    AsyncMutex<HashMap<String, broadcast::Sender<Result<[u8; 3], String>>>>,
> = std::sync::LazyLock::new(|| AsyncMutex::new(HashMap::new()));

static THUMBNAIL_COLOR_FLUSH_SCHEDULED: AtomicBool = AtomicBool::new(false);

fn extract_yt_video_id(url: &str) -> Option<&str> {
    let markers = ["i.ytimg.com/vi/", "i.ytimg.com/vi_webp/"];
    for marker in markers {
        if let Some(start) = url.find(marker) {
            let after_marker = &url[start + marker.len()..];
            if let Some(end) = after_marker.find('/') {
                let video_id = &after_marker[..end];
                if !video_id.is_empty() {
                    return Some(video_id);
                }
            }
        }
    }
    None
}

fn make_cache_key(url: &str) -> String {
    // Treat absolute filesystem paths as local-file cache entries.
    let path = Path::new(url);
    if path.is_absolute() {
        return format!("file:{}", path.to_string_lossy());
    }

    extract_yt_video_id(url)
        .map(|id| format!("yt:{}", id))
        .unwrap_or_else(|| url.to_string())
}

fn compute_dominant_color(img: image::DynamicImage) -> [u8; 3] {
    use image::GenericImageView;

    let small = img.resize(50, 50, image::imageops::FilterType::Triangle);
    let (width, height) = small.dimensions();

    let mut best_color = [99u8, 102u8, 241u8];
    let mut best_score: f32 = 0.0;

    for y in 0..height {
        for x in 0..width {
            if (x + y) % 4 != 0 {
                continue;
            }

            let pixel = small.get_pixel(x, y);
            let [r, g, b, a] = pixel.0;
            if a < 128 {
                continue;
            }

            let max_c = r.max(g).max(b) as f32;
            let min_c = r.min(g).min(b) as f32;
            let lightness = (max_c + min_c) / 2.0 / 255.0;
            let saturation = if max_c == min_c {
                0.0
            } else {
                (max_c - min_c) / (1.0 - (2.0 * lightness - 1.0).abs()) / 255.0
            };

            let lightness_score = 1.0 - (lightness - 0.5).abs() * 2.0;
            let score = saturation * lightness_score * (1.0 - (lightness - 0.4).abs());

            if score > best_score && saturation > 0.2 {
                best_score = score;
                best_color = [r, g, b];
            }
        }
    }

    // Boost saturation slightly (keeps existing UI look).
    let boost_factor = 1.2f32;
    let r = best_color[0] as f32 / 255.0;
    let g = best_color[1] as f32 / 255.0;
    let b = best_color[2] as f32 / 255.0;

    let max_c = r.max(g).max(b);
    let min_c = r.min(g).min(b);
    let l = (max_c + min_c) / 2.0;

    if max_c != min_c {
        let d = max_c - min_c;
        let mut s = if l > 0.5 {
            d / (2.0 - max_c - min_c)
        } else {
            d / (max_c + min_c)
        };

        let h = if max_c == r {
            ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
        } else if max_c == g {
            ((b - r) / d + 2.0) / 6.0
        } else {
            ((r - g) / d + 4.0) / 6.0
        };

        s = (s * boost_factor).min(1.0);

        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;

        fn hue2rgb(p: f32, q: f32, mut t: f32) -> f32 {
            if t < 0.0 {
                t += 1.0;
            }
            if t > 1.0 {
                t -= 1.0;
            }
            if t < 1.0 / 6.0 {
                return p + (q - p) * 6.0 * t;
            }
            if t < 0.5 {
                return q;
            }
            if t < 2.0 / 3.0 {
                return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
            }
            p
        }

        best_color = [
            (hue2rgb(p, q, h + 1.0 / 3.0) * 255.0) as u8,
            (hue2rgb(p, q, h) * 255.0) as u8,
            (hue2rgb(p, q, h - 1.0 / 3.0) * 255.0) as u8,
        ];
    }

    best_color
}

fn cache_color(app: &AppHandle, cache_key: String, color: [u8; 3]) {
    {
        let mut cache = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
        cache.put(cache_key.clone(), color);
    }

    {
        let mut disk = lock_or_recover(&THUMBNAIL_COLOR_DISK_CACHE);
        if disk.map.len() > 3000 {
            let keys: Vec<String> = disk.map.keys().take(1000).cloned().collect();
            for k in keys {
                disk.map.remove(&k);
            }
        }
        disk.map.insert(cache_key, color);
        disk.dirty = true;
    }

    schedule_thumbnail_color_disk_flush(app.clone());
}

/// Computes a dominant color for a *local* image file.
/// This is used for locally generated thumbnails (e.g. extracted from the downloaded file).
#[tauri::command]
pub async fn extract_local_thumbnail_color(
    app: AppHandle,
    path: String,
) -> Result<[u8; 3], String> {
    ensure_thumbnail_color_disk_cache_loaded(&app).await?;

    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("File not found: {}", path));
    }

    let cache_key = format!("file:{}", p.to_string_lossy());

    {
        let mut cache = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
        if let Some(&color) = cache.get(&cache_key) {
            return Ok(color);
        }
    }

    let bytes = tokio::fs::read(&p)
        .await
        .map_err(|e| format!("Failed to read image: {}", e))?;

    let img =
        image::load_from_memory(&bytes).map_err(|e| format!("Failed to decode image: {}", e))?;
    let color = compute_dominant_color(img);

    cache_color(&app, cache_key.clone(), color);
    Ok(color)
}

fn thumbnail_color_cache_path(app: &AppHandle) -> Result<PathBuf, String> {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("Failed to get cache dir: {}", e))?;
    Ok(cache_dir.join(THUMBNAIL_COLOR_DISK_CACHE_FILE))
}

async fn ensure_thumbnail_color_disk_cache_loaded(app: &AppHandle) -> Result<(), String> {
    {
        let disk = lock_or_recover(&THUMBNAIL_COLOR_DISK_CACHE);
        if disk.loaded {
            return Ok(());
        }
    }

    let path = thumbnail_color_cache_path(app)?;

    let loaded_map: HashMap<String, [u8; 3]> = match tokio::fs::read(&path).await {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(map) => map,
            Err(e) => {
                warn!(
                    "Failed to parse thumbnail color disk cache ({}): {}",
                    path.display(),
                    e
                );
                HashMap::new()
            }
        },
        Err(_) => HashMap::new(),
    };

    let mut disk = lock_or_recover(&THUMBNAIL_COLOR_DISK_CACHE);
    disk.map = loaded_map;
    disk.loaded = true;
    disk.dirty = false;
    Ok(())
}

fn schedule_thumbnail_color_disk_flush(app: AppHandle) {
    if THUMBNAIL_COLOR_FLUSH_SCHEDULED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;

        let write_result: Result<(), String> = async {
            ensure_thumbnail_color_disk_cache_loaded(&app).await?;

            let (path, snapshot) = {
                let disk = lock_or_recover(&THUMBNAIL_COLOR_DISK_CACHE);
                if !disk.dirty {
                    return Ok(());
                }
                (thumbnail_color_cache_path(&app)?, disk.map.clone())
            };

            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("Failed to create cache dir: {}", e))?;
            }

            let json = serde_json::to_vec(&snapshot)
                .map_err(|e| format!("Failed to serialize thumbnail color cache: {}", e))?;
            tokio::fs::write(&path, json)
                .await
                .map_err(|e| format!("Failed to write thumbnail color cache: {}", e))?;

            let mut disk = lock_or_recover(&THUMBNAIL_COLOR_DISK_CACHE);
            disk.dirty = false;

            Ok(())
        }
        .await;

        if let Err(e) = write_result {
            warn!("Failed to flush thumbnail color disk cache: {}", e);
        }

        THUMBNAIL_COLOR_FLUSH_SCHEDULED.store(false, Ordering::SeqCst);
    });
}

/// Fast path used by the frontend before doing any network fetch.
/// Returns `None` if we don't have it cached (in-memory or on-disk).
#[tauri::command]
pub async fn get_cached_thumbnail_color(
    app: AppHandle,
    url: String,
) -> Result<Option<[u8; 3]>, String> {
    ensure_thumbnail_color_disk_cache_loaded(&app).await?;

    let cache_key = make_cache_key(&url);

    {
        let mut cache = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
        if let Some(&color) = cache.get(&cache_key) {
            return Ok(Some(color));
        }
    }

    {
        let disk = lock_or_recover(&THUMBNAIL_COLOR_DISK_CACHE);
        if let Some(&color) = disk.map.get(&cache_key) {
            let mut cache = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
            cache.put(cache_key, color);
            return Ok(Some(color));
        }
    }

    Ok(None)
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn extract_thumbnail_color(app: AppHandle, url: String) -> Result<[u8; 3], String> {
    ensure_thumbnail_color_disk_cache_loaded(&app).await?;

    let cache_key = make_cache_key(&url);

    {
        let mut cache = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
        if let Some(&color) = cache.get(&cache_key) {
            return Ok(color);
        }
    }

    {
        let disk = lock_or_recover(&THUMBNAIL_COLOR_DISK_CACHE);
        if let Some(&color) = disk.map.get(&cache_key) {
            let mut cache = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
            cache.put(cache_key, color);
            return Ok(color);
        }
    }

    let mut receiver = None;
    {
        let mut inflight = THUMBNAIL_COLOR_INFLIGHT.lock().await;
        if let Some(sender) = inflight.get(&cache_key) {
            receiver = Some(sender.subscribe());
        } else {
            let (sender, _rx) = broadcast::channel(16);
            inflight.insert(cache_key.clone(), sender);
        }
    }

    if let Some(mut rx) = receiver {
        match rx.recv().await {
            Ok(result) => return result,
            Err(e) => {
                debug!("Thumbnail color inflight recv error ({}): {}", cache_key, e);
            }
        }
    }

    debug!("Thumbnail color cache miss for: {}, fetching...", cache_key);

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to fetch image: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read image bytes: {}", e))?;

    let img =
        image::load_from_memory(&bytes).map_err(|e| format!("Failed to decode image: {}", e))?;
    let best_color = compute_dominant_color(img);

    cache_color(&app, cache_key.clone(), best_color);

    {
        let mut inflight = THUMBNAIL_COLOR_INFLIGHT.lock().await;
        if let Some(sender) = inflight.remove(&cache_key) {
            let _ = sender.send(Ok(best_color));
        }
    }

    Ok(best_color)
}
