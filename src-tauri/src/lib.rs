mod backends;
mod cache;
mod deps;
mod job_engine;
mod logs;
mod notifications;
mod proxy;
// RELAY WIP - DISABLED
// #[cfg(not(target_os = "android"))]
// mod relay;
#[cfg(not(target_os = "android"))]
mod server;
#[cfg(not(target_os = "android"))]
mod tray;
mod types;
mod utils;

use types::{PlaylistInfo, VideoFormats, VideoInfo};
use utils::lock_or_recover;

#[cfg(not(target_os = "android"))]
use backends::{InfoRequest, PlaylistRequest};

#[cfg(not(target_os = "android"))]
use image::{DynamicImage, GenericImageView};
use std::collections::HashMap;
use std::path::PathBuf;
#[cfg(not(target_os = "android"))]
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;
#[cfg(not(target_os = "android"))]
use tauri::Emitter;
use tauri::Manager;
use tokio::sync::{broadcast, Mutex as AsyncMutex};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AndroidBuildYtDlpOptionsRequest {
    url: String,
    settings_json: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AndroidYtDlpJobSettingsPayload {
    format: Option<String>,
    is_audio_only: Option<bool>,

    aria2_connections: Option<u32>,
    aria2_splits: Option<u32>,
    aria2_min_split_size: Option<String>,

    speed_limit: Option<u32>,
    youtube_player_client: Option<String>,
    output_template: Option<String>,

    embed_thumbnail: Option<bool>,
    embed_chapters: Option<bool>,
    embed_subtitles: Option<bool>,
    subtitle_languages: Option<String>,
    sponsor_block: Option<bool>,
    sponsor_block_categories: Option<Vec<String>>,
    remux: Option<bool>,
    convert_to_mp4: Option<bool>,
    clip_ranges: Option<Vec<types::ClipRange>>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AndroidYtDlpOption {
    key: String,
    value: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AndroidBuildYtDlpOptionsResponse {
    options: Vec<AndroidYtDlpOption>,
    // Kotlin replaces this token inside values like -o.
    output_dir_token: String,
}

#[tauri::command]
async fn build_android_ytdlp_options(
    req: AndroidBuildYtDlpOptionsRequest,
) -> Result<AndroidBuildYtDlpOptionsResponse, String> {
    let settings: AndroidYtDlpJobSettingsPayload = serde_json::from_str(&req.settings_json)
        .map_err(|e| format!("Invalid settings JSON: {}", e))?;

    let output_dir_token = "__COMINE_OUTPUT_DIR__".to_string();
    let template = settings
        .output_template
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("%(title)s.%(ext)s");

    let mut options: Vec<AndroidYtDlpOption> = Vec::new();
    fn push_flag(options: &mut Vec<AndroidYtDlpOption>, key: &str) {
        options.push(AndroidYtDlpOption {
            key: key.to_string(),
            value: None,
        });
    }
    fn push_kv(options: &mut Vec<AndroidYtDlpOption>, key: &str, value: String) {
        options.push(AndroidYtDlpOption {
            key: key.to_string(),
            value: Some(value),
        });
    }

    // Output & encoding
    push_kv(
        &mut options,
        "-o",
        format!("{}/{}", output_dir_token, template),
    );
    push_kv(&mut options, "--encoding", "utf-8".to_string());

    // Emit a deterministic file path marker (Kotlin can parse it without guessing)
    push_kv(
        &mut options,
        "--print",
        "after_move:>>>FILEPATH:%(filepath)s".to_string(),
    );

    // Format
    if let Some(fmt) = settings.format.as_deref().filter(|s| !s.trim().is_empty()) {
        push_kv(&mut options, "-f", fmt.trim().to_string());
    }

    // YouTube player client
    if let Some(client) = settings
        .youtube_player_client
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        push_kv(
            &mut options,
            "--extractor-args",
            format!(
                "youtube:player_client={};player_skip=webpage,configs",
                client.trim()
            ),
        );
    }

    // Clip ranges -> download sections
    let has_clip_ranges = settings
        .clip_ranges
        .as_ref()
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if let Some(ranges) = settings.clip_ranges.as_ref() {
        for range in ranges.iter() {
            if range.end <= range.start {
                continue;
            }
            // Android youtubedl-android expects the same --download-sections syntax.
            push_kv(
                &mut options,
                "--download-sections",
                format!("*{}-{}", range.start, range.end),
            );
        }
    }

    if has_clip_ranges {
        // Reduces brief A/V desync at cut boundaries for some sites (e.g. Vimeo).
        push_flag(&mut options, "--force-keyframes-at-cuts");
    }

    let is_audio_only = settings.is_audio_only.unwrap_or(false);
    if is_audio_only {
        push_flag(&mut options, "-x");
        push_kv(&mut options, "--audio-format", "m4a".to_string());

        if settings.embed_thumbnail.unwrap_or(true) {
            push_flag(&mut options, "--embed-thumbnail");
            push_kv(&mut options, "--convert-thumbnails", "jpg".to_string());
        }
    } else {
        // Video post-processing
        if settings.remux.unwrap_or(true) {
            if settings.convert_to_mp4.unwrap_or(false) {
                push_kv(&mut options, "--recode-video", "mp4".to_string());
            } else {
                push_kv(&mut options, "--remux-video", "mp4".to_string());
            }
        }
    }

    if settings.embed_chapters.unwrap_or(true) {
        push_flag(&mut options, "--embed-chapters");
    }

    if settings.embed_subtitles.unwrap_or(false) {
        push_flag(&mut options, "--write-subs");
        push_flag(&mut options, "--write-auto-subs");
        if let Some(langs) = settings
            .subtitle_languages
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            push_kv(&mut options, "--sub-langs", langs.trim().to_string());
        }
        push_flag(&mut options, "--embed-subs");
    }

    if settings.sponsor_block.unwrap_or(false) {
        let cats = settings
            .sponsor_block_categories
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>();
        if cats.is_empty() {
            push_kv(&mut options, "--sponsorblock-remove", "sponsor".to_string());
        } else {
            push_kv(&mut options, "--sponsorblock-remove", cats.join(","));
        }
    }

    // Aria2 integration: executor decides if aria2 is actually available.
    if !has_clip_ranges {
        let aria2_connections = settings.aria2_connections.unwrap_or(8).clamp(1, 16);
        let aria2_splits = settings.aria2_splits.unwrap_or(8).clamp(1, 16);
        let aria2_min_split_size = settings
            .aria2_min_split_size
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("1M")
            .to_string();
        // Match existing working Android behavior.
        push_kv(&mut options, "--downloader", "libaria2c.so".to_string());
        push_kv(
            &mut options,
            "--external-downloader-args",
            format!(
                "aria2c:'-x {} -s {} -k {}'",
                aria2_connections, aria2_splits, aria2_min_split_size
            ),
        );
    }

    if let Some(limit_mbps) = settings.speed_limit {
        if limit_mbps > 0 {
            push_kv(&mut options, "--limit-rate", format!("{}M", limit_mbps));
        }
    }

    // URL always last.
    options.push(AndroidYtDlpOption {
        key: "__URL__".to_string(),
        value: Some(req.url),
    });

    Ok(AndroidBuildYtDlpOptionsResponse {
        options,
        output_dir_token,
    })
}

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
            // Ensure loaded to avoid overwriting the disk cache with an empty map.
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

#[cfg(not(target_os = "android"))]
use log::{debug, error, info, warn};
#[cfg(target_os = "android")]
use log::{debug, info, warn};

#[cfg(target_os = "windows")]
use utils::CommandHideConsole;

#[tauri::command]
#[allow(unused_variables)]
async fn download_video(
    app: AppHandle,
    url: String,
    video_quality: Option<String>,
    download_mode: Option<String>,
    audio_quality: Option<String>,
    convert_to_mp4: Option<bool>,
    remux: Option<bool>,
    clear_metadata: Option<bool>,
    use_aria2: Option<bool>,
    aria2_connections: Option<u32>,
    aria2_splits: Option<u32>,
    aria2_min_split_size: Option<String>,
    aria2_disable_ipv6: Option<bool>,
    aria2_custom_args: Option<String>,
    no_playlist: Option<bool>,
    cookies_from_browser: Option<String>,
    custom_cookies: Option<String>,
    download_path: Option<String>,
    embed_thumbnail: Option<bool>,
    thumbnail_url_for_embed: Option<String>,
    playlist_title: Option<String>,
    proxy_config: Option<proxy::ProxyConfig>,
    sponsor_block: Option<bool>,
    sponsor_block_skip_sponsors: Option<bool>,
    sponsor_block_skip_intros: Option<bool>,
    sponsor_block_skip_self_promo: Option<bool>,
    sponsor_block_skip_interaction: Option<bool>,
    chapters: Option<bool>,
    embed_subtitles: Option<bool>,
    subtitle_languages: Option<String>,
    download_speed_limit: Option<u64>,
    youtube_player_client: Option<String>,
    concurrent_fragments: Option<u32>,
    retries: Option<u32>,
    fragment_retries: Option<u32>,
    download_custom_args: Option<String>,
    post_process_custom_args: Option<String>,
    keep_original: Option<bool>,
    output_template: Option<String>,
    restrict_filenames: Option<bool>,
    windows_filenames: Option<bool>,
    clip_ranges: Option<Vec<types::ClipRange>>,
    registry: tauri::State<'_, job_engine::JobRegistry>,
    window: tauri::Window,
) -> Result<String, String> {
    info!("Starting download for URL: {}", url);
    info!("Cookies from browser param: {:?}", cookies_from_browser);
    info!(
        "Custom cookies param: {:?}",
        custom_cookies
            .as_ref()
            .map(|s| if s.is_empty() { "empty" } else { "set" })
    );

    #[cfg(target_os = "android")]
    {
        return Err(
            "On Android, downloads run via the job-based bridge (window.AndroidYtDlp.startDownloadJob + job-event)"
                .to_string(),
        );
    }

    #[cfg(not(target_os = "android"))]
    {
        backends::download_video_auto(
            &app,
            None,
            window,
            registry.inner().clone(),
            backends::DownloadRequest {
                url,
                video_quality,
                download_mode,
                audio_quality,
                convert_to_mp4,
                remux,
                clear_metadata,
                use_aria2,
                aria2_connections,
                aria2_splits,
                aria2_min_split_size,
                aria2_disable_ipv6,
                aria2_custom_args,
                no_playlist,
                cookies_from_browser,
                custom_cookies,
                download_path,
                embed_thumbnail,
                thumbnail_url_for_embed,
                playlist_title,
                proxy_config,
                sponsor_block,
                sponsor_block_skip_sponsors,
                sponsor_block_skip_intros,
                sponsor_block_skip_self_promo,
                sponsor_block_skip_interaction,
                chapters,
                embed_subtitles,
                subtitle_languages,
                download_speed_limit,
                youtube_player_client,
                concurrent_fragments,
                retries,
                fragment_retries,
                download_custom_args,
                post_process_custom_args,
                keep_original,
                output_template,
                restrict_filenames,
                windows_filenames,
                clip_ranges,
                multi_thread: None,
                thread_count: None,
            },
        )
        .await
    }
}

#[tauri::command]
#[allow(unused_variables)]
async fn get_playlist_info(
    app: AppHandle,
    url: String,
    offset: Option<usize>,
    limit: Option<usize>,
    cookies_from_browser: Option<String>,
    custom_cookies: Option<String>,
    proxy_config: Option<proxy::ProxyConfig>,
    youtube_player_client: Option<String>,
) -> Result<PlaylistInfo, String> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(50);

    info!(
        "Getting playlist info for URL: {} (offset={}, limit={})",
        url, offset, limit
    );

    #[cfg(target_os = "android")]
    {
        return Err(
            "On Android, use window.AndroidYtDlp.getPlaylistInfo() from JavaScript".to_string(),
        );
    }

    #[cfg(not(target_os = "android"))]
    {
        backends::get_playlist_info_auto(
            &app,
            None,
            PlaylistRequest {
                url,
                offset,
                limit,
                cookies_from_browser,
                custom_cookies,
                proxy_config,
                youtube_player_client,
            },
        )
        .await
    }
}

#[tauri::command]
#[allow(unused_variables)]
async fn get_video_info(
    app: AppHandle,
    url: String,
    cookies_from_browser: Option<String>,
    custom_cookies: Option<String>,
    proxy_config: Option<proxy::ProxyConfig>,
    youtube_player_client: Option<String>,
) -> Result<VideoInfo, String> {
    info!("Getting video info for URL: {}", url);

    #[cfg(target_os = "android")]
    {
        return Err(
            "On Android, use window.AndroidYtDlp.getVideoInfo() from JavaScript".to_string(),
        );
    }

    #[cfg(not(target_os = "android"))]
    {
        backends::get_video_info_auto(
            &app,
            None,
            InfoRequest {
                url,
                cookies_from_browser,
                custom_cookies,
                proxy_config,
                youtube_player_client,
            },
        )
        .await
    }
}

#[tauri::command]
#[allow(unused_variables)]
async fn get_video_formats(
    app: AppHandle,
    url: String,
    cookies_from_browser: Option<String>,
    custom_cookies: Option<String>,
    proxy_config: Option<proxy::ProxyConfig>,
    youtube_player_client: Option<String>,
) -> Result<VideoFormats, String> {
    info!("Getting video formats for URL: {}", url);

    #[cfg(target_os = "android")]
    {
        return Err("Format selection not supported on Android yet".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        backends::get_video_formats_auto(
            &app,
            None,
            InfoRequest {
                url,
                cookies_from_browser,
                custom_cookies,
                proxy_config,
                youtube_player_client,
            },
        )
        .await
    }
}

/// Get media file duration using ffprobe (for already downloaded files)
#[tauri::command]
#[allow(unused_variables)]
async fn get_media_duration(app: AppHandle, file_path: String) -> Result<f64, String> {
    #[cfg(target_os = "android")]
    {
        return Err("get_media_duration not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        use std::process::Stdio;

        if !std::path::Path::new(&file_path).exists() {
            return Err(format!("File not found: {}", file_path));
        }

        let deps_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("deps");

        let ffprobe_path = if cfg!(target_os = "windows") {
            deps_dir.join("ffprobe.exe")
        } else {
            deps_dir.join("ffprobe")
        };

        let ffprobe_cmd = if ffprobe_path.exists() {
            ffprobe_path.to_string_lossy().to_string()
        } else {
            "ffprobe".to_string()
        };

        let mut cmd = tokio::process::Command::new(&ffprobe_cmd);
        cmd.args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            &file_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ffprobe failed: {}", stderr));
        }

        let duration_str = String::from_utf8_lossy(&output.stdout);
        duration_str
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("Failed to parse duration: {}", duration_str))
    }
}

#[tauri::command]
#[allow(unused_variables)]
async fn extract_video_thumbnail(app: AppHandle, file_path: String) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        return Err("Not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        use std::path::Path;
        use std::process::Stdio;

        let path = Path::new(&file_path);
        if !path.exists() {
            return Err("File not found".to_string());
        }

        let ffmpeg_path = deps::get_ffmpeg_path(&app)?;
        if !ffmpeg_path.exists() {
            return Err("FFmpeg not installed".to_string());
        }

        let cache_dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| format!("Cache dir error: {}", e))?
            .join("thumbs");

        tokio::fs::create_dir_all(&cache_dir)
            .await
            .map_err(|e| format!("Failed to create cache dir: {}", e))?;

        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("thumb");
        let thumb_path = cache_dir.join(format!("{}.jpg", file_stem));

        if thumb_path.exists() {
            return Ok(thumb_path.to_string_lossy().to_string());
        }

        let mut cmd = tokio::process::Command::new(&ffmpeg_path);
        cmd.args([
            "-i",
            &file_path,
            "-ss",
            "1",
            "-vframes",
            "1",
            "-vf",
            "scale=320:-1",
            "-q:v",
            "3",
            "-y",
            thumb_path.to_str().ok_or("Invalid path")?,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

        if !output.status.success() || !thumb_path.exists() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "Thumbnail extraction failed: {}",
                stderr.lines().take(2).collect::<Vec<_>>().join(" ")
            ));
        }

        Ok(thumb_path.to_string_lossy().to_string())
    }
}

/// Extract YouTube video ID from thumbnail URL (e.g., i.ytimg.com/vi/VIDEO_ID/...)
fn extract_yt_video_id(url: &str) -> Option<&str> {
    // Match patterns like: i.ytimg.com/vi/VIDEO_ID/ or i.ytimg.com/vi_webp/VIDEO_ID/
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

#[tauri::command]
#[allow(unused_variables)]
async fn extract_thumbnail_color(app: AppHandle, url: String) -> Result<[u8; 3], String> {
    use image::GenericImageView;

    ensure_thumbnail_color_disk_cache_loaded(&app).await?;

    // Extract YouTube video ID from thumbnail URL without regex
    let cache_key = extract_yt_video_id(&url)
        .map(|id| format!("yt:{}", id))
        .unwrap_or_else(|| url.clone());

    {
        let mut cache = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
        if let Some(&color) = cache.get(&cache_key) {
            debug!("Thumbnail color cache hit for: {}", cache_key);
            return Ok(color);
        }
    }

    // Disk cache hit (persists across app restarts)
    {
        let disk = lock_or_recover(&THUMBNAIL_COLOR_DISK_CACHE);
        if let Some(&color) = disk.map.get(&cache_key) {
            debug!("Thumbnail color disk cache hit for: {}", cache_key);
            let mut cache = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
            cache.put(cache_key, color);
            return Ok(color);
        }
    }

    // Coalesce inflight extraction
    let mut receiver = None;
    {
        let mut inflight = THUMBNAIL_COLOR_INFLIGHT.lock().await;
        if let Some(sender) = inflight.get(&cache_key) {
            debug!("Thumbnail color inflight - awaiting: {}", cache_key);
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
                // Fall through and compute ourselves.
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

    let small = img.resize(50, 50, image::imageops::FilterType::Triangle);
    let (width, height) = small.dimensions();

    let mut best_color = [99u8, 102u8, 241u8]; // Default accent color
    let mut best_score: f32 = 0.0;

    for y in 0..height {
        for x in 0..width {
            if (x + y) % 4 != 0 {
                continue; // Sample every 4th pixel
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

    // Boost saturation slightly
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

    {
        let mut cache = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
        cache.put(cache_key.clone(), best_color);
    }

    {
        let mut disk = lock_or_recover(&THUMBNAIL_COLOR_DISK_CACHE);
        if disk.map.len() > 3000 {
            let keys: Vec<String> = disk.map.keys().take(1000).cloned().collect();
            for k in keys {
                disk.map.remove(&k);
            }
        }
        disk.map.insert(cache_key.clone(), best_color);
        disk.dirty = true;
    }
    schedule_thumbnail_color_disk_flush(app.clone());

    {
        let mut inflight = THUMBNAIL_COLOR_INFLIGHT.lock().await;
        if let Some(sender) = inflight.remove(&cache_key) {
            let _ = sender.send(Ok(best_color));
        }
    }

    Ok(best_color)
}

#[tauri::command]
#[allow(unused_variables)]
async fn get_cached_thumbnail_color(
    app: AppHandle,
    url: String,
) -> Result<Option<[u8; 3]>, String> {
    ensure_thumbnail_color_disk_cache_loaded(&app).await?;

    // Extract YouTube video ID from thumbnail URL without regex
    let cache_key = extract_yt_video_id(&url)
        .map(|id| format!("yt:{}", id))
        .unwrap_or_else(|| url.clone());

    {
        let mut cache = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
        if let Some(color) = cache.get(&cache_key).copied() {
            return Ok(Some(color));
        }
    }

    let disk = lock_or_recover(&THUMBNAIL_COLOR_DISK_CACHE);
    if let Some(&color) = disk.map.get(&cache_key) {
        let mut cache = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
        cache.put(cache_key, color);
        return Ok(Some(color));
    }

    Ok(None)
}

// ==================== YouTube Music Thumbnail Cropping ====================

/// Check if image has solid color bars on sides (letterboxed)
#[cfg(not(target_os = "android"))]
fn is_letterboxed_thumbnail(img: &DynamicImage) -> bool {
    let (width, height) = img.dimensions();

    if width <= height {
        return false;
    }

    let square_size = height;
    let bar_width = (width - square_size) / 2;

    if bar_width < (width / 20) {
        return false;
    }

    let dark_threshold: u8 = 30;

    let sample_points_left = [
        (bar_width / 4, height / 4),
        (bar_width / 4, height / 2),
        (bar_width / 4, height * 3 / 4),
        (bar_width / 2, height / 4),
        (bar_width / 2, height / 2),
        (bar_width / 2, height * 3 / 4),
        (bar_width * 3 / 4, height / 4),
        (bar_width * 3 / 4, height / 2),
        (bar_width * 3 / 4, height * 3 / 4),
    ];

    let sample_points_right = [
        (width - bar_width / 4, height / 4),
        (width - bar_width / 4, height / 2),
        (width - bar_width / 4, height * 3 / 4),
        (width - bar_width / 2, height / 4),
        (width - bar_width / 2, height / 2),
        (width - bar_width / 2, height * 3 / 4),
        (width - bar_width * 3 / 4, height / 4),
        (width - bar_width * 3 / 4, height / 2),
        (width - bar_width * 3 / 4, height * 3 / 4),
    ];

    let mut dark_count = 0;
    let total_samples = sample_points_left.len() + sample_points_right.len();

    for (x, y) in sample_points_left.iter().chain(sample_points_right.iter()) {
        if *x >= width || *y >= height {
            continue;
        }
        let pixel = img.get_pixel(*x, *y);
        if pixel[0] <= dark_threshold && pixel[1] <= dark_threshold && pixel[2] <= dark_threshold {
            dark_count += 1;
        }
    }

    let required_dark = (total_samples * 7) / 10;
    if dark_count >= required_dark {
        return true;
    }

    let tolerance: i16 = 60;
    let ref_color = img.get_pixel(bar_width / 2, height / 2);

    let mut uniform_count = 0;
    for (x, y) in sample_points_left.iter().chain(sample_points_right.iter()) {
        if *x >= width || *y >= height {
            continue;
        }
        let pixel = img.get_pixel(*x, *y);

        let diff_r = (pixel[0] as i16 - ref_color[0] as i16).abs();
        let diff_g = (pixel[1] as i16 - ref_color[1] as i16).abs();
        let diff_b = (pixel[2] as i16 - ref_color[2] as i16).abs();

        if diff_r <= tolerance && diff_g <= tolerance && diff_b <= tolerance {
            uniform_count += 1;
        }
    }

    let required_uniform = (total_samples * 7) / 10;

    uniform_count >= required_uniform
}

/// Crop a letterboxed thumbnail to its center square
#[cfg(not(target_os = "android"))]
fn crop_to_center_square(img: DynamicImage) -> DynamicImage {
    let (width, height) = img.dimensions();
    let square_size = height;
    let x_offset = (width - square_size) / 2;

    img.crop_imm(x_offset, 0, square_size, square_size)
}

/// Embed a thumbnail into an audio file by downloading it from a URL.
///
/// - Downloads the image
/// - Crops to center square if letterboxed
/// - Encodes as JPEG
/// - Embeds via ffmpeg
#[cfg(not(target_os = "android"))]
async fn embed_thumbnail_from_url(
    app: &AppHandle,
    audio_path: &str,
    thumbnail_url: &str,
) -> Result<(), String> {
    use std::io::Cursor;

    if thumbnail_url.is_empty() {
        return Err("Empty thumbnail URL".to_string());
    }

    let response = reqwest::get(thumbnail_url)
        .await
        .map_err(|e| format!("Failed to download thumbnail: {}", e))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read thumbnail bytes: {}", e))?;

    let img =
        image::load_from_memory(&bytes).map_err(|e| format!("Failed to decode image: {}", e))?;

    let processed = if is_letterboxed_thumbnail(&img) {
        crop_to_center_square(img)
    } else {
        img
    };

    let mut jpeg_bytes: Vec<u8> = Vec::new();
    let mut cursor = Cursor::new(&mut jpeg_bytes);
    processed
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode thumbnail as JPEG: {}", e))?;

    embed_thumbnail_jpeg_bytes(app, audio_path, &jpeg_bytes).await
}

#[cfg(not(target_os = "android"))]
async fn embed_thumbnail_jpeg_bytes(
    app: &AppHandle,
    audio_path: &str,
    jpeg_bytes: &[u8],
) -> Result<(), String> {
    use std::process::Stdio;
    use std::time::{SystemTime, UNIX_EPOCH};

    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("Failed to get cache dir: {}", e))?;
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| format!("Failed to create cache dir: {}", e))?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let thumb_path = cache_dir.join(format!("cover_{}.jpg", stamp));
    tokio::fs::write(&thumb_path, jpeg_bytes)
        .await
        .map_err(|e| format!("Failed to write thumbnail file: {}", e))?;

    let ffmpeg_path = deps::get_ffmpeg_path(app)?;
    if !ffmpeg_path.exists() {
        let _ = tokio::fs::remove_file(&thumb_path).await;
        return Err("FFmpeg not found".to_string());
    }

    let audio_path_buf = std::path::PathBuf::from(audio_path);
    let audio_ext = audio_path_buf
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "mp3".to_string());
    let temp_output = audio_path_buf.with_extension(format!("temp.{}", audio_ext));

    let mut cmd = tokio::process::Command::new(&ffmpeg_path);
    cmd.args([
        "-y",
        "-i",
        audio_path,
        "-i",
        thumb_path
            .to_str()
            .ok_or("Invalid thumbnail path encoding")?,
        "-map",
        "0:a",
        "-map",
        "1:v",
        "-c:a",
        "copy",
        "-c:v",
        "mjpeg",
    ]);

    if audio_ext == "mp3" {
        cmd.args([
            "-id3v2_version",
            "3",
            "-metadata:s:v",
            "title=Album cover",
            "-metadata:s:v",
            "comment=Cover (front)",
        ]);
    } else {
        cmd.args(["-disposition:v:0", "attached_pic"]);
    }

    cmd.arg(temp_output.to_str().ok_or("Invalid output path encoding")?);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    cmd.hide_console();

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    let _ = tokio::fs::remove_file(&thumb_path).await;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = tokio::fs::remove_file(&temp_output).await;
        return Err(format!("FFmpeg failed to embed thumbnail: {}", stderr));
    }

    tokio::fs::rename(&temp_output, audio_path)
        .await
        .map_err(|e| format!("Failed to replace original file: {}", e))?;

    Ok(())
}

/// Set window background effect
/// Supports various effects for Windows and macOS
/// effect_type: "acrylic", "blur", "mica", "mica-dark", "mica-light", "tabbed", "tabbed-dark", "tabbed-light"
///              "vibrancy-*" for macOS vibrancy effects
///              "none" to disable effects
#[tauri::command]
#[allow(unused_variables)]
async fn set_window_effect(app: AppHandle, effect_type: String) -> Result<(), String> {
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

                // Only Acrylic supports color tinting
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

                // Force redraw when switching between effect APIs
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
async fn set_acrylic(app: AppHandle, enable: bool) -> Result<(), String> {
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

// ==================== Proxy Commands ====================

/// Resolve proxy based on configuration from frontend
/// Returns the effective proxy URL, source, and description
#[tauri::command]
async fn resolve_proxy_config(config: proxy::ProxyConfig) -> Result<proxy::ResolvedProxy, String> {
    info!(
        "Resolving proxy config: mode={}, custom_url={}, retry_without_proxy={}",
        config.mode, config.custom_url, config.retry_without_proxy
    );
    Ok(proxy::resolve_proxy(&config))
}

/// Validate a proxy URL syntax
#[tauri::command]
async fn validate_proxy_url(url: String) -> Result<(), String> {
    proxy::validate_proxy_url(&url)
}

/// Detect system proxy (for displaying to user)
#[tauri::command]
async fn detect_system_proxy() -> Result<proxy::ResolvedProxy, String> {
    Ok(proxy::detect_system_proxy())
}

/// Get disk space info for a given path
#[tauri::command]
#[cfg(not(target_os = "android"))]
async fn get_disk_space(path: String) -> Result<utils::DiskSpaceInfo, String> {
    let actual_path = if path.is_empty() {
        dirs::download_dir()
            .ok_or("Could not find Downloads folder")?
            .to_string_lossy()
            .to_string()
    } else {
        path
    };
    utils::get_disk_space_for_path(&actual_path)
        .ok_or_else(|| "Could not determine disk space".to_string())
}

#[tauri::command]
#[cfg(target_os = "android")]
async fn get_disk_space(_path: String) -> Result<utils::DiskSpaceInfo, String> {
    Err("Not supported on Android".to_string())
}

/// Check current public IP (to verify proxy is working)
#[tauri::command]
async fn check_ip(proxy_config: Option<proxy::ProxyConfig>) -> Result<IpCheckResult, String> {
    let config = proxy_config.unwrap_or_default();
    let resolved = proxy::resolve_proxy(&config);

    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5));

    if !resolved.url.is_empty() {
        let proxy =
            reqwest::Proxy::all(&resolved.url).map_err(|e| format!("Invalid proxy: {}", e))?;
        builder = builder.proxy(proxy);
    } else {
        builder = builder.no_proxy();
    }

    let client = builder
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let response = client
        .get("https://api.ipify.org?format=json")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let ip = data["ip"].as_str().ok_or("No IP in response")?.to_string();

    Ok(IpCheckResult {
        ip,
        proxy_used: !resolved.url.is_empty(),
        proxy_source: resolved.source,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct IpCheckResult {
    ip: String,
    proxy_used: bool,
    proxy_source: String,
}

// ==================== File Download Commands ====================

/// Download a file directly (for download manager functionality)
/// Returns the file path on success
#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn download_file(
    app: AppHandle,
    window: tauri::Window,
    url: String,
    filename: String,
    download_path: String,
    proxy_config: Option<proxy::ProxyConfig>,
    connections: Option<u32>,
    splits: Option<u32>,
    min_split_size: Option<String>,
    speed_limit: Option<u64>,
) -> Result<String, String> {
    info!("Starting file download: {} -> {}", url, filename);

    let base_path = if download_path.is_empty() {
        dirs::download_dir()
            .or_else(dirs::home_dir)
            .ok_or("Could not determine download directory")?
    } else {
        std::path::PathBuf::from(&download_path)
    };

    let safe_filename = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();

    let dest_path = base_path.join(&safe_filename);

    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let aria2_path = deps::get_aria2_path(&app)?;
    let use_aria2 = aria2_path.exists();

    let browser_ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    let mut force_reqwest_fallback = false;

    if use_aria2 {
        let connections = connections.unwrap_or(4).clamp(1, 16);
        let splits = splits.unwrap_or(connections).clamp(1, 16);
        let min_split_size = min_split_size.unwrap_or_else(|| "1M".to_string());

        let mut cmd = tokio::process::Command::new(&aria2_path);

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        cmd.arg(&url)
            .arg("-d")
            .arg(base_path.to_string_lossy().to_string())
            .arg("-o")
            .arg(&safe_filename)
            .arg("-x")
            .arg(connections.to_string()) // max connections
            .arg("-s")
            .arg(splits.to_string()) // splits
            .arg("-k")
            .arg(&min_split_size) // min split size
            .arg("--file-allocation=none")
            .arg("--max-tries=10")
            .arg("--retry-wait=3")
            .arg("--max-file-not-found=5")
            .arg("--connect-timeout=30")
            .arg("--timeout=600")
            .arg("--continue=true") // resume support
            .arg("--auto-file-renaming=false")
            .arg("--allow-overwrite=true")
            .arg("--summary-interval=0") // avoid noisy summary blocks
            .arg("--download-result=hide")
            .arg("--console-log-level=warn")
            .arg("--enable-color=false")
            .arg("--user-agent")
            .arg(browser_ua);

        if let Some(limit) = speed_limit {
            if limit > 0 {
                cmd.arg("--max-download-limit")
                    .arg(format!("{}K", limit / 1024));
            }
        }

        if let Some(ref config) = proxy_config {
            let resolved = proxy::resolve_proxy(config);
            if !resolved.url.is_empty() {
                cmd.arg("--all-proxy").arg(&resolved.url);
            }
        }

        info!("Running aria2c with {} connections", connections);
        debug!("aria2c path: {:?}", aria2_path);
        debug!("aria2c dest: {:?}", dest_path);

        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped()) // Capture stdout for progress
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn aria2c: {}", e))?;

        info!("aria2c process spawned, reading progress...");

        #[cfg(not(target_os = "android"))]
        {
            use tokio::io::AsyncReadExt;

            #[derive(Default)]
            struct Aria2Progress {
                percent: u8,
                downloaded: u64,
                total: u64,
                speed_bps: u64,
            }

            fn parse_size_to_bytes(input: &str) -> u64 {
                let s = input.trim();
                if s.is_empty() {
                    return 0;
                }

                // Common aria2 units: B, KiB, MiB, GiB (and iB/s variants)
                let unit_start = s
                    .find(|c: char| c.is_ascii_alphabetic())
                    .unwrap_or(s.len());
                let (num_part, unit_part) = s.split_at(unit_start);

                let num: f64 = num_part.trim().parse::<f64>().unwrap_or(0.0);
                let unit = unit_part.trim().trim_end_matches("/s").trim();

                let mult: f64 = match unit {
                    "B" | "" => 1.0,
                    "KiB" | "K" | "KB" => 1024.0,
                    "MiB" | "M" | "MB" => 1024.0 * 1024.0,
                    "GiB" | "G" | "GB" => 1024.0 * 1024.0 * 1024.0,
                    _ => 1.0,
                };

                (num * mult).max(0.0) as u64
            }

            fn parse_aria2_progress_line(line: &str) -> Option<Aria2Progress> {
                // Example CR line:
                // [#gid 12MiB/50MiB(24%) CN:4 DL:3.1MiB ETA:12s]
                let mut out = Aria2Progress::default();

                if let Some(pct_start) = line.find('(') {
                    if let Some(pct_end) = line[pct_start..].find('%') {
                        if let Ok(p) = line[pct_start + 1..pct_start + pct_end].parse::<u8>() {
                            out.percent = p;
                        }
                    }
                }

                if let Some(slash_idx) = line.find("iB/") {
                    let before_slash = &line[..slash_idx + 2];
                    let size_start = before_slash
                        .rfind([' ', '[', '#'])
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let dl_part = &before_slash[size_start..];
                    out.downloaded = parse_size_to_bytes(dl_part);

                    let after_slash = &line[slash_idx + 3..];
                    if let Some(end) = after_slash.find(['(', ' ', ']', '[']) {
                        let total_part = &after_slash[..end];
                        out.total = parse_size_to_bytes(total_part);
                    }
                }

                if let Some(dl_idx) = line.find("DL:") {
                    let speed_part = &line[dl_idx + 3..];
                    if let Some(end) = speed_part.find([' ', ']', '[']) {
                        out.speed_bps = parse_size_to_bytes(&speed_part[..end]);
                    } else {
                        out.speed_bps = parse_size_to_bytes(speed_part);
                    }
                }

                if out.percent > 0 || out.downloaded > 0 || out.total > 0 || out.speed_bps > 0 {
                    Some(out)
                } else {
                    None
                }
            }

            async fn read_lines_crlf<R: tokio::io::AsyncRead + Unpin>(
                mut reader: R,
                mut on_line: impl FnMut(String),
            ) {
                let mut buf = [0u8; 4096];
                let mut acc: Vec<u8> = Vec::new();

                loop {
                    let n = match reader.read(&mut buf).await {
                        Ok(n) => n,
                        Err(_) => break,
                    };
                    if n == 0 {
                        break;
                    }

                    acc.extend_from_slice(&buf[..n]);

                    while let Some(pos) = acc.iter().position(|&b| b == b'\n' || b == b'\r') {
                        let mut line_bytes: Vec<u8> = acc.drain(..pos).collect();

                        // Drain delimiter
                        let delim = acc.drain(..1).next().unwrap_or(b'\n');
                        if delim == b'\r' {
                            if acc.first() == Some(&b'\n') {
                                let _ = acc.drain(..1).next();
                            }
                        }

                        while matches!(line_bytes.last(), Some(b'\r' | b'\n')) {
                            line_bytes.pop();
                        }

                        let line = String::from_utf8_lossy(&line_bytes).to_string();
                        on_line(line);
                    }

                    if acc.len() > 1024 * 1024 {
                        let line = String::from_utf8_lossy(&acc).to_string();
                        acc.clear();
                        on_line(line);
                    }
                }

                if !acc.is_empty() {
                    let line = String::from_utf8_lossy(&acc).to_string();
                    on_line(line);
                }
            }

            let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
            let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

            let window_task = window.clone();
            let url_task = url.clone();
            let last_error: std::sync::Arc<std::sync::Mutex<Option<String>>> =
                std::sync::Arc::new(std::sync::Mutex::new(None));
            let last_error_stdout = last_error.clone();
            let last_error_stderr = last_error.clone();

            let progress_state: std::sync::Arc<std::sync::Mutex<(u8, std::time::Instant)>> =
                std::sync::Arc::new(std::sync::Mutex::new((0, std::time::Instant::now())));

            const PROGRESS_THROTTLE_MS: u64 = 250;

            let progress_state_stdout = progress_state.clone();
            let window_stdout = window_task.clone();
            let url_stdout = url_task.clone();
            let stdout_task = tokio::spawn(async move {
                read_lines_crlf(stdout, |line| {
                    let trimmed = line.trim().to_string();
                    if trimmed.is_empty() {
                        return;
                    }

                    let is_errorish = trimmed.contains("[ERROR]") || trimmed.contains("Exception") || trimmed.contains("errorCode=");
                    if is_errorish {
                        if let Ok(mut last) = last_error_stdout.lock() {
                            *last = Some(trimmed.clone());
                        }
                        debug!("aria2c: {}", trimmed);
                    }

                    if let Some(p) = parse_aria2_progress_line(&trimmed) {
                        let should_emit = if let Ok(mut s) = progress_state_stdout.lock() {
                            let now = std::time::Instant::now();
                            let percent_changed = p.percent != 0 && p.percent != s.0;
                            let time_ok = now.duration_since(s.1).as_millis() >= PROGRESS_THROTTLE_MS as u128;
                            if percent_changed || time_ok {
                                if p.percent != 0 {
                                    s.0 = p.percent;
                                }
                                s.1 = now;
                                true
                            } else {
                                false
                            }
                        } else {
                            true
                        };

                        if should_emit {
                            let _ = window_stdout.emit(
                                "download-progress",
                                serde_json::json!({
                                    "url": url_stdout,
                                    "progress": p.percent,
                                    "downloadedBytes": p.downloaded,
                                    "totalBytes": p.total,
                                    "speedBps": p.speed_bps,
                                    "message": trimmed,
                                }),
                            );
                        }
                    }
                })
                .await;
            });

            let progress_state_stderr = progress_state.clone();
            let window_stderr = window_task.clone();
            let url_stderr = url_task.clone();
            let stderr_task = tokio::spawn(async move {
                read_lines_crlf(stderr, |line| {
                    let trimmed = line.trim().to_string();
                    if trimmed.is_empty() {
                        return;
                    }

                    let is_errorish = trimmed.contains("[ERROR]") || trimmed.contains("Exception") || trimmed.contains("errorCode=");
                    if is_errorish {
                        if let Ok(mut last) = last_error_stderr.lock() {
                            *last = Some(trimmed.clone());
                        }
                        debug!("aria2c: {}", trimmed);
                    }

                    if let Some(p) = parse_aria2_progress_line(&trimmed) {
                        let should_emit = if let Ok(mut s) = progress_state_stderr.lock() {
                            let now = std::time::Instant::now();
                            let percent_changed = p.percent != 0 && p.percent != s.0;
                            let time_ok = now.duration_since(s.1).as_millis() >= PROGRESS_THROTTLE_MS as u128;
                            if percent_changed || time_ok {
                                if p.percent != 0 {
                                    s.0 = p.percent;
                                }
                                s.1 = now;
                                true
                            } else {
                                false
                            }
                        } else {
                            true
                        };

                        if should_emit {
                            let _ = window_stderr.emit(
                                "download-progress",
                                serde_json::json!({
                                    "url": url_stderr,
                                    "progress": p.percent,
                                    "downloadedBytes": p.downloaded,
                                    "totalBytes": p.total,
                                    "speedBps": p.speed_bps,
                                    "message": trimmed,
                                }),
                            );
                        }
                    }
                })
                .await;
            });

            // Wait for aria2c
            info!("Waiting for aria2c to exit...");
            let status = child
                .wait()
                .await
                .map_err(|e| format!("aria2c failed: {}", e))?;
            info!("aria2c exited with status: {:?}", status);

            let _ = stdout_task.await;
            let _ = stderr_task.await;

            if !status.success() {
                let details = last_error
                    .lock()
                    .ok()
                    .and_then(|g| g.clone())
                    .unwrap_or_else(|| "aria2c exited with non-zero status".to_string());

                let lower = details.to_lowercase();
                let is_tlsish = lower.contains("ssl/tls")
                    || lower.contains("tls handshake")
                    || lower.contains("handshake failure")
                    || lower.contains("connection was forcibly closed")
                    || lower.contains("schannel")
                    || lower.contains("openssl");

                if is_tlsish {
                    warn!("aria2c failed with TLS/handshake error, falling back to reqwest: {}", details);
                    force_reqwest_fallback = true;
                } else {
                    error!("aria2c failed: {}", details);
                    return Err(details);
                }
            }
        }

        if !force_reqwest_fallback {
            info!("aria2c download complete: {:?}", dest_path);
        }
    }

    if !use_aria2 || force_reqwest_fallback {
        info!("Using reqwest fallback for file download");

        let config = proxy_config.unwrap_or_default();
        let resolved = proxy::resolve_proxy(&config);

        let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(3600)); // 1 hour timeout

        if !resolved.url.is_empty() {
            let proxy =
                reqwest::Proxy::all(&resolved.url).map_err(|e| format!("Invalid proxy: {}", e))?;
            builder = builder.proxy(proxy);
        }

        let client = builder
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))?;

        let response = client
            .get(&url)
            .header(reqwest::header::USER_AGENT, browser_ua)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        let total_size = response.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;

        let mut file = tokio::fs::File::create(&dest_path)
            .await
            .map_err(|e| format!("Failed to create file: {}", e))?;

        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let mut stream = response.bytes_stream();
        let start_time = std::time::Instant::now();
        let mut last_emit = std::time::Instant::now();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Write error: {}", e))?;

            downloaded += chunk.len() as u64;

            if last_emit.elapsed().as_millis() >= 100 {
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed_bps: u64 = if elapsed > 0.0 {
                    (downloaded as f64 / elapsed).max(0.0) as u64
                } else {
                    0
                };

                let percent: u8 = if total_size > 0 {
                    (((downloaded as f64 / total_size as f64) * 100.0).round() as i64)
                        .clamp(0, 100) as u8
                } else {
                    0
                };

                let _ = window.emit(
                    "download-progress",
                    serde_json::json!({
                        "url": url,
                        "progress": percent,
                        "downloadedBytes": downloaded,
                        "totalBytes": total_size,
                        "speedBps": speed_bps,
                        "message": "reqwest",
                    }),
                );

                last_emit = std::time::Instant::now();
            }
        }

        file.flush()
            .await
            .map_err(|e| format!("Flush error: {}", e))?;
        info!("reqwest download complete: {:?}", dest_path);
    }

    if !dest_path.exists() {
        return Err("Download failed: file not created".to_string());
    }

    Ok(dest_path.to_string_lossy().to_string())
}

/// Check if a URL is a direct file download (HEAD request)
/// Returns file info if it's a downloadable file
#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn check_file_url(
    url: String,
    proxy_config: Option<proxy::ProxyConfig>,
) -> Result<FileUrlInfo, String> {
    let config = proxy_config.unwrap_or_default();
    let resolved = proxy::resolve_proxy(&config);

    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5));

    if !resolved.url.is_empty() {
        let proxy =
            reqwest::Proxy::all(&resolved.url).map_err(|e| format!("Invalid proxy: {}", e))?;
        builder = builder.proxy(proxy);
    }

    let client = builder
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let response = client
        .head(&url)
        .send()
        .await
        .map_err(|e| format!("HEAD request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let content_length = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let content_disposition = response
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let filename = extract_filename_from_headers(&content_disposition, &url);

    let is_file = !content_type.starts_with("text/html")
        && !content_type.starts_with("application/xhtml")
        && (content_length > 0 || !content_type.starts_with("text/"));

    Ok(FileUrlInfo {
        is_file,
        filename,
        size: content_length,
        mime_type: content_type,
        supports_resume: response.headers().contains_key("accept-ranges"),
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FileUrlInfo {
    is_file: bool,
    filename: String,
    size: u64,
    mime_type: String,
    supports_resume: bool,
}

/// Extract filename from Content-Disposition header or URL
#[cfg(not(target_os = "android"))]
fn extract_filename_from_headers(content_disposition: &str, url: &str) -> String {
    if !content_disposition.is_empty() {
        if let Some(start) = content_disposition.find("filename=") {
            let rest = &content_disposition[start + 9..];
            let filename = if let Some(stripped) = rest.strip_prefix('"') {
                stripped.split('"').next().unwrap_or("")
            } else {
                rest.split(';').next().unwrap_or("").trim()
            };
            if !filename.is_empty() {
                return filename.to_string();
            }
        }
    }

    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(mut segments) = parsed.path_segments() {
            if let Some(last) = segments.next_back() {
                if !last.is_empty() && last.contains('.') {
                    return urlencoding::decode(last)
                        .unwrap_or(std::borrow::Cow::Borrowed(last))
                        .into_owned();
                }
            }
        }
    }

    "download".to_string()
}

// Android stubs for file download commands
#[cfg(target_os = "android")]
#[tauri::command]
async fn download_file(
    _app: AppHandle,
    _window: tauri::Window,
    _url: String,
    _filename: String,
    _download_path: String,
    _proxy_config: Option<proxy::ProxyConfig>,
    _connections: Option<u32>,
    _speed_limit: Option<u64>,
) -> Result<String, String> {
    Err("File downloads are not supported on Android yet".to_string())
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn check_file_url(
    _url: String,
    _proxy_config: Option<proxy::ProxyConfig>,
) -> Result<FileUrlInfo, String> {
    Err("File URL checking is not supported on Android yet".to_string())
}

#[tauri::command]
#[cfg(not(target_os = "android"))]
async fn clear_cookies(app: AppHandle) -> Result<(), String> {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("Failed to get cache dir: {}", e))?;

    let cookie_files = ["custom_cookies.txt", "lux_cookies.txt"];

    for file in &cookie_files {
        let path = cache_dir.join(file);
        if path.exists() {
            let _ = tokio::fs::remove_file(&path).await;
            info!("Deleted cookie file: {:?}", path);
        }
    }

    Ok(())
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn clear_cookies(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
#[cfg(not(target_os = "android"))]
async fn clear_cache(app: AppHandle) -> Result<u32, String> {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("Failed to get cache dir: {}", e))?;

    let mut deleted: u32 = 0;
    let files_to_clean = ["custom_cookies.txt", "cropped_cover.jpg"];

    for file in &files_to_clean {
        let path = cache_dir.join(file);
        if path.exists() && tokio::fs::remove_file(&path).await.is_ok() {
            deleted += 1;
            info!("Deleted cache file: {:?}", path);
        }
    }

    Ok(deleted)
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn clear_cache(_app: AppHandle) -> Result<u32, String> {
    Ok(0)
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn get_cache_stats() -> Result<CacheStats, String> {
    let (video_info_count, playlist_count, formats_count) = {
        let vi = lock_or_recover(&cache::VIDEO_INFO_CACHE);
        let pi = lock_or_recover(&cache::PLAYLIST_INFO_CACHE);
        let vf = lock_or_recover(&cache::VIDEO_FORMATS_CACHE);
        (vi.len(), pi.len(), vf.len())
    };

    // Estimate playlist cache size (rough estimate)
    let playlist_entries_total: usize = {
        let pi = lock_or_recover(&cache::PLAYLIST_INFO_CACHE);
        pi.iter().map(|(_, v)| v.entries.len()).sum()
    };

    Ok(CacheStats {
        video_info_count,
        playlist_count,
        playlist_entries_total,
        formats_count,
    })
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn get_cache_stats() -> Result<CacheStats, String> {
    Ok(CacheStats {
        video_info_count: 0,
        playlist_count: 0,
        playlist_entries_total: 0,
        formats_count: 0,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheStats {
    video_info_count: usize,
    playlist_count: usize,
    playlist_entries_total: usize,
    formats_count: usize,
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn clear_memory_caches() -> Result<(), String> {
    {
        let mut vi = lock_or_recover(&cache::VIDEO_INFO_CACHE);
        vi.clear();
    }
    {
        let mut pi = lock_or_recover(&cache::PLAYLIST_INFO_CACHE);
        pi.clear();
    }
    {
        let mut vf = lock_or_recover(&cache::VIDEO_FORMATS_CACHE);
        vf.clear();
    }
    {
        let mut tc = lock_or_recover(&THUMBNAIL_COLOR_CACHE);
        tc.clear();
    }
    info!("Cleared all in-memory caches");
    Ok(())
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn clear_memory_caches() -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn autostart_enable(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .enable()
        .map_err(|e| format!("Failed to enable autostart: {}", e))
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn autostart_disable(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .disable()
        .map_err(|e| format!("Failed to disable autostart: {}", e))
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn autostart_is_enabled(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| format!("Failed to check autostart status: {}", e))
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn autostart_enable(_app: AppHandle) -> Result<(), String> {
    Err("Autostart not supported on Android".to_string())
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn autostart_disable(_app: AppHandle) -> Result<(), String> {
    Err("Autostart not supported on Android".to_string())
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn autostart_is_enabled(_app: AppHandle) -> Result<bool, String> {
    Ok(false)
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub body: Option<String>,
    pub date: Option<String>,
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn check_for_update(
    app: AppHandle,
    allow_prerelease: bool,
) -> Result<UpdateCheckResult, String> {
    use tauri_plugin_updater::UpdaterExt;

    info!(
        "Checking for updates with allow_prerelease={}",
        allow_prerelease
    );

    let endpoint_url = if allow_prerelease {
        let client = reqwest::Client::new();
        let response = client
            .get("https://api.github.com/repos/nichind/comine/releases")
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "comine-updater")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch releases: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("GitHub API error: {}", response.status()));
        }

        let releases: Vec<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse releases: {}", e))?;

        let latest = releases.first().ok_or("No releases found")?;
        let tag = latest["tag_name"]
            .as_str()
            .ok_or("No tag_name in release")?;

        format!(
            "https://github.com/nichind/comine/releases/download/{}/latest.json",
            tag
        )
    } else {
        "https://github.com/nichind/comine/releases/latest/download/latest.json".to_string()
    };

    info!("Using update endpoint: {}", endpoint_url);

    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint_url
            .parse()
            .map_err(|e| format!("Invalid URL: {}", e))?])
        .map_err(|e| format!("Failed to set endpoints: {}", e))?
        .build()
        .map_err(|e| format!("Failed to build updater: {}", e))?;

    let update = updater
        .check()
        .await
        .map_err(|e| format!("Update check failed: {}", e))?;

    match update {
        Some(update) => {
            info!(
                "Update available: {} (current: {})",
                update.version, update.current_version
            );
            let date_str = update.date.map(|d| d.to_string());
            Ok(UpdateCheckResult {
                available: true,
                version: Some(update.version.clone()),
                body: Some(update.body.clone().unwrap_or_default()),
                date: date_str,
            })
        }
        None => {
            info!("No update available");
            Ok(UpdateCheckResult {
                available: false,
                version: None,
                body: None,
                date: None,
            })
        }
    }
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
async fn download_and_install_update(
    app: AppHandle,
    window: tauri::Window,
    allow_prerelease: bool,
) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;

    info!(
        "Starting update download with allow_prerelease={}",
        allow_prerelease
    );

    let endpoint_url = if allow_prerelease {
        let client = reqwest::Client::new();
        let response = client
            .get("https://api.github.com/repos/nichind/comine/releases")
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "comine-updater")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch releases: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("GitHub API error: {}", response.status()));
        }

        let releases: Vec<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse releases: {}", e))?;

        let latest = releases.first().ok_or("No releases found")?;
        let tag = latest["tag_name"]
            .as_str()
            .ok_or("No tag_name in release")?;

        format!(
            "https://github.com/nichind/comine/releases/download/{}/latest.json",
            tag
        )
    } else {
        "https://github.com/nichind/comine/releases/latest/download/latest.json".to_string()
    };

    info!("Using update endpoint: {}", endpoint_url);

    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint_url
            .parse()
            .map_err(|e| format!("Invalid URL: {}", e))?])
        .map_err(|e| format!("Failed to set endpoints: {}", e))?
        .build()
        .map_err(|e| format!("Failed to build updater: {}", e))?;

    let update = updater
        .check()
        .await
        .map_err(|e| format!("Update check failed: {}", e))?
        .ok_or("No update available")?;

    info!("Downloading update version {}", update.version);

    let window_for_progress = window.clone();
    let window_for_finish = window.clone();
    let mut started = false;

    update
        .download_and_install(
            move |chunk_length, content_length| {
                if !started {
                    started = true;
                    let _ = window_for_progress.emit(
                        "update-download-progress",
                        serde_json::json!({
                            "event": "started",
                            "contentLength": content_length
                        }),
                    );
                }

                let _ = window_for_progress.emit(
                    "update-download-progress",
                    serde_json::json!({
                        "event": "progress",
                        "chunkLength": chunk_length
                    }),
                );
            },
            move || {
                info!("Download finished, verifying and installing...");
                let _ = window_for_finish.emit(
                    "update-download-progress",
                    serde_json::json!({
                        "event": "finished"
                    }),
                );
            },
        )
        .await
        .map_err(|e| {
            let err_str = e.to_string();
            error!("Update install failed: {}", err_str);
            if err_str.contains("signature") || err_str.contains("Signature") {
                "Update signature verification failed. The release may not be properly signed."
                    .to_string()
            } else {
                format!("Update failed: {}", err_str)
            }
        })?;

    info!("Update installed successfully, restarting app...");
    app.restart();
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn check_for_update(
    _app: AppHandle,
    _allow_prerelease: bool,
) -> Result<UpdateCheckResult, String> {
    Err("Use Android update mechanism".to_string())
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn download_and_install_update(
    _app: AppHandle,
    _window: tauri::Window,
    _allow_prerelease: bool,
) -> Result<(), String> {
    Err("Use Android update mechanism".to_string())
}

// ============ Server Commands ============

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn server_start(app: AppHandle, port: u16) {
    server::start_server(app, port);
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn server_stop() {
    server::stop_server();
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn server_is_running() -> bool {
    server::is_running()
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn push_queue_status(items: Vec<server::QueueItem>) {
    server::update_queue(items);
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
fn push_history_status(items: Vec<server::HistoryItem>) {
    server::update_history(items);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .manage(job_engine::JobRegistry::default())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                ])
                .level(log::LevelFilter::Debug)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());

    #[cfg(not(target_os = "android"))]
    let builder = builder.plugin(
        tauri_plugin_autostart::Builder::new()
            .args(["--minimized"])
            .build(),
    );

    let builder = builder.invoke_handler(tauri::generate_handler![
        download_video,
        job_engine::jobs_start,
        job_engine::jobs_cancel,
        get_video_info,
        get_video_formats,
        get_playlist_info,
        get_media_duration,
        extract_video_thumbnail,
        extract_thumbnail_color,
        get_cached_thumbnail_color,
        set_window_effect,
        set_acrylic,
        notifications::show_notification_window,
        notifications::reveal_notification_window,
        notifications::close_notification_window,
        notifications::close_all_notifications,
        notifications::notification_action,
        logs::get_log_file_path,
        logs::append_log,
        logs::cleanup_old_logs,
        logs::open_logs_folder,
        logs::get_logs_folder_path,
        logs::read_session_logs,
        logs::get_session_log_count,
        resolve_proxy_config,
        validate_proxy_url,
        detect_system_proxy,
        get_disk_space,
        check_ip,
        download_file,
        check_file_url,
        build_android_ytdlp_options,
        check_for_update,
        download_and_install_update,
        deps::check_ytdlp,
        deps::install_ytdlp,
        deps::uninstall_ytdlp,
        deps::get_ytdlp_releases,
        deps::check_ffmpeg,
        deps::install_ffmpeg,
        deps::uninstall_ffmpeg,
        deps::check_aria2,
        deps::install_aria2,
        deps::uninstall_aria2,
        deps::check_deno,
        deps::install_deno,
        deps::uninstall_deno,
        deps::check_quickjs,
        deps::install_quickjs,
        deps::uninstall_quickjs,
        deps::check_lux,
        deps::install_lux,
        deps::uninstall_lux,
        clear_cache,
        clear_cookies,
        get_cache_stats,
        clear_memory_caches,
        autostart_enable,
        autostart_disable,
        autostart_is_enabled
    ]);

    #[cfg(not(target_os = "android"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        download_video,
        job_engine::jobs_start,
        job_engine::jobs_cancel,
        get_video_info,
        get_video_formats,
        get_playlist_info,
        get_media_duration,
        extract_video_thumbnail,
        extract_thumbnail_color,
        get_cached_thumbnail_color,
        set_window_effect,
        set_acrylic,
        notifications::show_notification_window,
        notifications::reveal_notification_window,
        notifications::close_notification_window,
        notifications::close_all_notifications,
        notifications::notification_action,
        logs::get_log_file_path,
        logs::append_log,
        logs::cleanup_old_logs,
        logs::open_logs_folder,
        logs::get_logs_folder_path,
        logs::read_session_logs,
        logs::get_session_log_count,
        resolve_proxy_config,
        validate_proxy_url,
        detect_system_proxy,
        get_disk_space,
        check_ip,
        download_file,
        check_file_url,
        check_for_update,
        download_and_install_update,
        deps::check_ytdlp,
        deps::install_ytdlp,
        deps::uninstall_ytdlp,
        deps::get_ytdlp_releases,
        deps::check_ffmpeg,
        deps::install_ffmpeg,
        deps::uninstall_ffmpeg,
        deps::check_aria2,
        deps::install_aria2,
        deps::uninstall_aria2,
        deps::check_deno,
        deps::install_deno,
        deps::uninstall_deno,
        deps::check_quickjs,
        deps::install_quickjs,
        deps::uninstall_quickjs,
        deps::check_lux,
        deps::install_lux,
        deps::uninstall_lux,
        clear_cache,
        clear_cookies,
        get_cache_stats,
        clear_memory_caches,
        autostart_enable,
        autostart_disable,
        autostart_is_enabled,
        server_start,
        server_stop,
        server_is_running,
        push_queue_status,
        push_history_status
    ]);

    #[cfg(not(target_os = "android"))]
    let builder = builder
        .setup(|app| {
            tray::setup(app.handle())?;

            let start_minimized = std::env::args().any(|arg| arg == "--minimized");
            if start_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    use tauri_plugin_store::StoreExt;
                    let should_minimize = app
                        .store("settings.json")
                        .ok()
                        .and_then(|store| store.get("startMinimized"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);

                    if should_minimize {
                        let _ = window.hide();
                    }
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            let label = window.label();
            if label != "main" {
                return;
            }

            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    let _ = window.emit("close-requested", ());
                    api.prevent_close();
                }
                tauri::WindowEvent::Focused(false) => {
                    if let Ok(visible) = window.is_visible() {
                        if !visible {
                            let _ = window.emit("window-hidden", ());
                        }
                    }
                }
                _ => {}
            }
        });

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn lock_or_recover_handles_poisoned_mutex() {
        let mutex = Arc::new(Mutex::new(vec![1, 2, 3]));
        let mutex_clone = Arc::clone(&mutex);

        let handle = std::thread::spawn(move || {
            let _guard = mutex_clone.lock().unwrap();
            panic!("intentional panic to poison mutex");
        });

        let _ = handle.join();
        assert!(mutex.is_poisoned());

        let guard = lock_or_recover(&mutex);
        assert_eq!(*guard, vec![1, 2, 3]);
    }

    #[test]
    fn lock_or_recover_normal_case() {
        let mutex = Mutex::new(42);
        let guard = lock_or_recover(&mutex);
        assert_eq!(*guard, 42);
    }
}
