mod backends;
mod cache;
mod deps;
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
use backends::{Backend, InfoRequest, PlaylistRequest};

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

#[cfg(not(target_os = "android"))]
static ACTIVE_DOWNLOADS: std::sync::LazyLock<Mutex<HashMap<String, u32>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

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
            // Ensure loaded so we don't overwrite with empty map.
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
#[cfg(not(target_os = "android"))]
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

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
    no_playlist: Option<bool>,
    cookies_from_browser: Option<String>,
    custom_cookies: Option<String>,
    download_path: Option<String>,
    embed_thumbnail: Option<bool>,
    thumbnail_url_for_embed: Option<String>,
    playlist_title: Option<String>,
    proxy_config: Option<proxy::ProxyConfig>,
    sponsor_block: Option<bool>,
    chapters: Option<bool>,
    embed_subtitles: Option<bool>,
    subtitle_languages: Option<String>,
    download_speed_limit: Option<u64>,
    youtube_player_client: Option<String>,
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
        return Err("On Android, use window.AndroidYtDlp.download() from JavaScript".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let resolved_proxy = proxy_config.as_ref().map(proxy::resolve_proxy);
        let proxy_url = resolved_proxy.as_ref().and_then(|r| {
            if r.url.is_empty() {
                None
            } else {
                Some(r.url.as_str())
            }
        });

        if let Some(ref proxy) = resolved_proxy {
            if !proxy.url.is_empty() {
                info!("Using proxy for download: {} ({})", proxy.url, proxy.source);
            }
        }

        let config = backends::get_command(&app, proxy_url)?;

        debug!(
            "Using command: {} with prefix args: {:?}",
            config.ytdlp_path, config.prefix_args
        );

        let mut downloads_dir = if let Some(ref custom_path) = download_path {
            if !custom_path.is_empty() {
                std::path::PathBuf::from(custom_path)
            } else {
                dirs::download_dir().ok_or("Could not find Downloads folder")?
            }
        } else {
            dirs::download_dir().ok_or("Could not find Downloads folder")?
        };

        if let Some(ref title) = playlist_title {
            if !title.is_empty() {
                let safe_folder_name: String = title
                    .chars()
                    .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(100)
                    .collect();
                if !safe_folder_name.is_empty() {
                    downloads_dir = downloads_dir.join(&safe_folder_name);
                    info!("Using playlist subfolder: {:?}", downloads_dir);
                }
            }
        }

        if !downloads_dir.exists() {
            std::fs::create_dir_all(&downloads_dir)
                .map_err(|e| format!("Failed to create download directory: {}", e))?;
            info!("Created download directory: {:?}", downloads_dir);
        }

        let download_mode = download_mode.unwrap_or_else(|| "auto".to_string());
        let ext = "%(ext)s";

        let output_template = downloads_dir
            .join(format!("%(title)s.{}", ext))
            .to_str()
            .ok_or("Invalid path")?
            .to_string();

        info!("Output template: {}", output_template);
        info!(
            "Download mode: {}, Video quality: {:?}, Audio quality: {:?}",
            download_mode, video_quality, audio_quality
        );

        let download_start_time = std::time::SystemTime::now();

        let mut args: Vec<String> = config.prefix_args;
        args.extend([
            "--encoding".to_string(),
            "utf-8".to_string(),
            "-o".to_string(),
            output_template.clone(),
            "--newline".to_string(),
            "--progress".to_string(),
            "--progress-template".to_string(),
            "%(progress._percent_str)s %(progress._speed_str)s %(progress._eta_str)s".to_string(),
            "--print".to_string(),
            "after_move:>>>FILEPATH:%(filepath)s".to_string(),
            "--verbose".to_string(),
        ]);

        if let Some(proxy) = proxy_url {
            args.extend(["--proxy".to_string(), proxy.to_string()]);
            info!("Using --proxy argument: {}", proxy);
        }

        if let Some(ref qjs_path) = config.quickjs_path {
            args.extend(["--js-runtimes".to_string(), format!("quickjs:{}", qjs_path)]);
            info!("Using QuickJS runtime: {}", qjs_path);
        }

        let video_quality = video_quality.unwrap_or_else(|| "max".to_string());
        let audio_quality = audio_quality.unwrap_or_else(|| "best".to_string());

        // Check if video_quality is a raw format ID (e.g., "251", "140+251", "bestvideo+bestaudio")
        // Raw format IDs contain digits, plus signs, or start with "best"
        let is_raw_format = video_quality
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
            || video_quality.contains('+')
            || video_quality.starts_with("best");

        let format_string =
            if is_raw_format {
                info!("Using raw format ID: {}", video_quality);
                video_quality.clone()
            } else {
                match download_mode.as_str() {
                    "audio" => {
                        match audio_quality.as_str() {
                            "320" => "bestaudio[abr<=320]/bestaudio/best".to_string(),
                            "256" => "bestaudio[abr<=256]/bestaudio/best".to_string(),
                            "192" => "bestaudio[abr<=192]/bestaudio/best".to_string(),
                            "128" => "bestaudio[abr<=128]/bestaudio/best".to_string(),
                            "96" => "bestaudio[abr<=96]/bestaudio/best".to_string(),
                            _ => "bestaudio/best".to_string(), // "best"
                        }
                    }
                    "mute" => {
                        match video_quality.as_str() {
                            "4k" => "bestvideo[height<=2160]/bestvideo/best".to_string(),
                            "1440p" => "bestvideo[height<=1440]/bestvideo/best".to_string(),
                            "1080p" => "bestvideo[height<=1080]/bestvideo/best".to_string(),
                            "720p" => "bestvideo[height<=720]/bestvideo/best".to_string(),
                            "480p" => "bestvideo[height<=480]/bestvideo/best".to_string(),
                            "360p" => "bestvideo[height<=360]/bestvideo/best".to_string(),
                            "240p" => "bestvideo[height<=240]/bestvideo/best".to_string(),
                            _ => "bestvideo/best".to_string(), // "max"
                        }
                    }
                    _ => {
                        match video_quality.as_str() {
                            "4k" => "bestvideo[height<=2160]+bestaudio/best[height<=2160]/best"
                                .to_string(),
                            "1440p" => "bestvideo[height<=1440]+bestaudio/best[height<=1440]/best"
                                .to_string(),
                            "1080p" => "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best"
                                .to_string(),
                            "720p" => "bestvideo[height<=720]+bestaudio/best[height<=720]/best"
                                .to_string(),
                            "480p" => "bestvideo[height<=480]+bestaudio/best[height<=480]/best"
                                .to_string(),
                            "360p" => "bestvideo[height<=360]+bestaudio/best[height<=360]/best"
                                .to_string(),
                            "240p" => "bestvideo[height<=240]+bestaudio/best[height<=240]/best"
                                .to_string(),
                            _ => "bestvideo+bestaudio/best".to_string(), // "max"
                        }
                    }
                }
            };

        args.extend(["-f".to_string(), format_string.clone()]);
        info!("Using format: {}", format_string);

        if download_mode == "audio" {
            args.extend([
                "-x".to_string(),
                "--audio-format".to_string(),
                "m4a".to_string(),
            ]);

            let has_thumb_url_for_embed = thumbnail_url_for_embed
                .as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false);

            // For YouTube Music, we want a square cover (cropped if letterboxed).
            // We embed manually from URL after download to ensure proper cropping.
            let should_manual_embed_ytm = embed_thumbnail.unwrap_or(false)
                && url.to_lowercase().contains("music.youtube.com")
                && has_thumb_url_for_embed;

            if embed_thumbnail.unwrap_or(false) && !should_manual_embed_ytm {
                args.push("--embed-thumbnail".to_string());
                info!("Embedding thumbnail as cover art (via yt-dlp)");
            } else if should_manual_embed_ytm {
                info!("Will embed YTM thumbnail manually after download");
            }

            info!("Audio-only download with extraction (ffprobe available)");
        }

        if download_mode != "audio" {
            if convert_to_mp4.unwrap_or(false) {
                args.extend([
                    "--format-sort".to_string(),
                    "vcodec:h264,acodec:aac".to_string(),
                ]);
                args.extend(["--recode-video".to_string(), "mp4".to_string()]);
                info!("Converting to MP4 (with h264/aac preference to minimize re-encoding)");
            } else if remux.unwrap_or(true) {
                args.extend(["--remux-video".to_string(), "mp4".to_string()]);
                info!("Remuxing to MP4 (copy, no re-encode)");
            }
        }

        if clear_metadata.unwrap_or(false) {
            args.push("--no-embed-metadata".to_string());
            info!("Clearing metadata");
        }

        if no_playlist.unwrap_or(true) {
            args.push("--no-playlist".to_string());
            info!("Using --no-playlist (single video only)");
        }

        // Only use custom cookies if cookiesFromBrowser is explicitly set to "custom"
        let use_custom_cookies = cookies_from_browser
            .as_ref()
            .map(|s| s == "custom")
            .unwrap_or(false)
            && custom_cookies
                .as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false);

        if use_custom_cookies {
            if let Some(cookies_text) = custom_cookies.as_deref() {
                let cache_dir = app
                    .path()
                    .app_cache_dir()
                    .map_err(|e| format!("Failed to get cache dir: {}", e))?;
                let cookies_file = cache_dir.join("custom_cookies.txt");

                tokio::fs::create_dir_all(&cache_dir)
                    .await
                    .map_err(|e| format!("Failed to create cache dir: {}", e))?;

                tokio::fs::write(&cookies_file, cookies_text)
                    .await
                    .map_err(|e| format!("Failed to write cookies file: {}", e))?;

                args.push("--cookies".to_string());
                args.push(cookies_file.to_string_lossy().to_string());
                info!("Using custom cookies file: {:?}", cookies_file);
            }
        } else if let Some(ref browser) = cookies_from_browser {
            if !browser.is_empty() && browser != "custom" {
                args.push("--cookies-from-browser".to_string());
                args.push(browser.clone());
                info!("Using cookies from browser: {}", browser);
            }
        }

        let is_youtube = url.contains("youtube.com") || url.contains("youtu.be");
        if is_youtube {
            // When using cookies, avoid android_sdkless as it doesn't support cookies
            let has_cookies = use_custom_cookies
                || cookies_from_browser
                    .as_ref()
                    .map(|s| !s.is_empty() && s != "custom")
                    .unwrap_or(false);

            let default_client = if has_cookies {
                "tv,web"
            } else {
                "tv,android_sdkless"
            };
            let player_client = youtube_player_client
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(default_client);

            let final_client = if has_cookies && player_client.contains("android_sdkless") {
                player_client.replace("android_sdkless", "tv")
            } else {
                player_client.to_string()
            };

            args.extend([
                "--extractor-args".to_string(),
                format!("youtube:player_client={}", final_client),
            ]);
            info!("Using player client chain for YouTube: {}", final_client);
        }

        if sponsor_block.unwrap_or(false) {
            args.extend(["--sponsorblock-remove".to_string(), "default".to_string()]);
            info!("SponsorBlock enabled - removing sponsored segments");
        }

        if chapters.unwrap_or(true) {
            args.push("--embed-chapters".to_string());
            info!("Embedding chapters");
        }

        if embed_subtitles.unwrap_or(false) {
            let langs = subtitle_languages.as_deref().unwrap_or("en.*,ru.*");
            args.extend([
                "--embed-subs".to_string(),
                "--sub-langs".to_string(),
                langs.to_string(),
            ]);
            info!("Embedding subtitles ({})", langs);
        }

        if let Some(limit) = download_speed_limit {
            if limit > 0 {
                args.extend(["--limit-rate".to_string(), format!("{}M", limit)]);
                info!("Download speed limit: {} MB/s", limit);
            }
        }

        let should_use_aria2 = use_aria2.unwrap_or(true);
        if should_use_aria2 {
            let aria2_path = deps::get_aria2_path(&app)?;
            if aria2_path.exists() {
                let connections = aria2_connections.unwrap_or(8).min(16).max(1);
                let splits = aria2_splits.unwrap_or(8).min(16).max(1);
                let min_split = aria2_min_split_size.as_deref().unwrap_or("1M");
                info!("Using aria2 as external downloader: {:?} (connections: {}, splits: {}, min-split: {})", aria2_path, connections, splits, min_split);
                args.extend([
                    "--downloader".to_string(),
                    aria2_path.to_string_lossy().to_string(),
                    "--downloader-args".to_string(),
                    format!(
                        "aria2c:-x {} -s {} -k {} --file-allocation=none",
                        connections, splits, min_split
                    ),
                ]);
            } else {
                info!("aria2 requested but not installed, using default downloader");
            }
        } else {
            info!("aria2 disabled by user, using yt-dlp's native downloader");
        }

        args.push(url.clone());

        let mut cmd = tokio::process::Command::new(&config.ytdlp_path);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().map_err(|e| {
            error!("Failed to start yt-dlp: {}", e);
            format!("Failed to start yt-dlp: {}", e)
        })?;

        let pid = child.id().unwrap_or(0);
        {
            let mut downloads = lock_or_recover(&ACTIVE_DOWNLOADS);
            downloads.insert(url.clone(), pid);
            info!("Registered download process {} for URL: {}", pid, url);
        }

        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        let mut stdout_reader = BufReader::new(stdout);
        let mut stdout_buffer = Vec::new();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut final_file_path: Option<String> = None;
        let mut stdout_done = false;
        let mut stderr_done = false;
        let mut error_messages: Vec<String> = Vec::new();

        let mut last_progress_emit =
            std::time::Instant::now() - std::time::Duration::from_millis(200);
        const PROGRESS_THROTTLE_MS: u64 = 100;

        while !stdout_done || !stderr_done {
            tokio::select! {
                result = stdout_reader.read_u8(), if !stdout_done => {
                    match result {
                        Ok(byte) => {
                            if byte == b'\n' || byte == b'\r' {
                                if !stdout_buffer.is_empty() {
                                    if let Ok(line) = String::from_utf8(stdout_buffer.clone()) {
                                        let line = line.trim().to_string();
                                        stdout_buffer.clear();

                                        if line.is_empty() {
                                            continue;
                                        }

                                        debug!("yt-dlp stdout: {}", line);

                                        if line.starts_with(">>>FILEPATH:") {
                                            let path = line.trim_start_matches(">>>FILEPATH:");
                                            let path_lower = path.to_lowercase();
                                            let is_image = path_lower.ends_with(".png") ||
                                                           path_lower.ends_with(".jpg") ||
                                                           path_lower.ends_with(".jpeg") ||
                                                           path_lower.ends_with(".webp");
                                            if !is_image {
                                                final_file_path = Some(path.to_string());
                                                info!("Captured file path from --print: {:?}", final_file_path);
                                            } else {
                                                debug!("Skipping image file path from --print: {}", path);
                                            }
                                            continue;
                                        }

                                        let now = std::time::Instant::now();
                                        if now.duration_since(last_progress_emit).as_millis() >= PROGRESS_THROTTLE_MS as u128 {
                                            last_progress_emit = now;
                                            let _ = window.emit("download-progress", DownloadProgress {
                                                url: url.clone(),
                                                message: line,
                                            });
                                        }
                                    } else {
                                        stdout_buffer.clear();
                                    }
                                }
                            } else {
                                stdout_buffer.push(byte);
                            }
                        }
                        Err(_) => stdout_done = true,
                    }
                }
                result = stderr_reader.next_line(), if !stderr_done => {
                    match result {
                        Ok(Some(line)) => {
                            if line.contains("ERROR:") || line.contains("error:") {
                                warn!("yt-dlp stderr: {}", line);
                            } else if line.starts_with("[debug]") || line.starts_with("  File \"") || line.starts_with("Traceback") {
                                debug!("yt-dlp output: {}", line);
                            } else {
                                debug!("yt-dlp output: {}", line);
                            }

                            if line.contains("[Merger] Merging formats into") {
                                if let Some(start) = line.find('"') {
                                    if let Some(end) = line.rfind('"') {
                                        if end > start {
                                            let path = &line[start+1..end];
                                            final_file_path = Some(path.to_string());
                                            info!("Captured file path from Merger: {:?}", final_file_path);
                                        }
                                    }
                                }
                            }

                            if line.contains("[download] Destination:") {
                                if let Some(path) = line.split("Destination:").nth(1) {
                                    let path = path.trim();
                                    let path_lower = path.to_lowercase();
                                    let is_image = path_lower.ends_with(".png") ||
                                                   path_lower.ends_with(".jpg") ||
                                                   path_lower.ends_with(".jpeg") ||
                                                   path_lower.ends_with(".webp");
                                    if final_file_path.is_none() && !path.is_empty() && !is_image {
                                        final_file_path = Some(path.to_string());
                                        info!("Captured file path from Destination: {:?}", final_file_path);
                                    }
                                }
                            }

                            if line.contains("ERROR:") || line.contains("Sign in to confirm") ||
                               line.contains("LOGIN_REQUIRED") || line.contains("age-restricted") ||
                               line.contains("Private video") || line.contains("Video unavailable") ||
                               line.contains("members-only") || line.contains("requires payment") ||
                               line.contains("is not a valid URL") || line.contains("Unsupported URL") {
                                error_messages.push(line.clone());
                            }

                            let is_important = line.contains("[Merger]") ||
                                               line.contains("[ExtractAudio]") ||
                                               line.contains("[EmbedThumbnail]") ||
                                               line.contains("[Metadata]") ||
                                               line.contains("[ffmpeg]") ||
                                               line.contains("[download] Destination:");
                            let now = std::time::Instant::now();
                            if is_important || now.duration_since(last_progress_emit).as_millis() >= PROGRESS_THROTTLE_MS as u128 {
                                last_progress_emit = now;
                                let _ = window.emit("download-progress", DownloadProgress {
                                    url: url.clone(),
                                    message: line,
                                });
                            }
                        }
                        Ok(None) => stderr_done = true,
                        Err(_) => stderr_done = true,
                    }
                }
            }
        }

        info!("Waiting for yt-dlp process to exit...");
        let status = child.wait().await.map_err(|e| {
            error!("Process error: {}", e);
            format!("Process error: {}", e)
        })?;

        info!("yt-dlp process exited with status: {:?}", status);

        if status.success() {
            info!("Download completed successfully for: {}", url);
            info!("Captured file path: {:?}", final_file_path);

            let needs_scan = final_file_path
                .as_ref()
                .map(|p| !std::path::Path::new(p).exists())
                .unwrap_or(true);

            if needs_scan {
                info!("Scanning downloads folder for newly created file...");
                if let Ok(entries) = std::fs::read_dir(&downloads_dir) {
                    let mut newest: Option<(std::path::PathBuf, std::time::SystemTime)> = None;

                    for entry in entries.flatten() {
                        let path = entry.path();
                        if !path.is_file() {
                            continue;
                        }

                        if let Some(ext) = path.extension() {
                            let ext_str = ext.to_string_lossy().to_lowercase();
                            if ext_str == "part"
                                || ext_str == "ytdl"
                                || ext_str == "png"
                                || ext_str == "jpg"
                                || ext_str == "jpeg"
                                || ext_str == "webp"
                            {
                                continue;
                            }
                        }

                        if let Ok(metadata) = entry.metadata() {
                            if let Ok(created) = metadata.created() {
                                if created >= download_start_time {
                                    let is_newer =
                                        newest.as_ref().map(|(_, t)| created > *t).unwrap_or(true);
                                    if is_newer {
                                        newest = Some((path, created));
                                    }
                                }
                            }
                        }
                    }

                    if let Some((path, _)) = newest {
                        info!("Found newly created file: {:?}", path);
                        final_file_path = Some(path.to_string_lossy().to_string());
                    }
                }
            }

            info!("Removing from active downloads map...");
            {
                let mut downloads = lock_or_recover(&ACTIVE_DOWNLOADS);
                downloads.remove(&url);
            }

            if use_custom_cookies {
                if let Ok(cache_dir) = app.path().app_cache_dir() {
                    let _ = tokio::fs::remove_file(cache_dir.join("custom_cookies.txt")).await;
                }
            }

            // Embed thumbnail from URL for YTM audio downloads (with auto-cropping)
            if let Some(ref path) = final_file_path {
                let has_thumb_url = thumbnail_url_for_embed
                    .as_ref()
                    .map(|s| !s.is_empty())
                    .unwrap_or(false);

                if embed_thumbnail.unwrap_or(false)
                    && has_thumb_url
                    && url.to_lowercase().contains("music.youtube.com")
                    && download_mode == "audio"
                {
                    if let Some(thumb_url) = thumbnail_url_for_embed.as_ref() {
                        info!("Embedding YTM thumbnail from URL into: {}", path);
                        if let Err(e) = embed_thumbnail_from_url(&app, path, thumb_url).await {
                            warn!("Failed to embed thumbnail from URL: {}", e);
                        } else {
                            info!("Successfully embedded thumbnail from URL");
                        }
                    }
                }
            }

            if let Some(ref path) = final_file_path {
                info!("Emitting download-file-path event: {}", path);
                let _ = window.emit(
                    "download-file-path",
                    DownloadFilePath {
                        url: url.clone(),
                        file_path: path.clone(),
                    },
                );
            } else {
                warn!("No file path captured - download-file-path event not emitted");
            }

            let result =
                final_file_path.unwrap_or_else(|| "Download completed successfully!".to_string());
            info!("Returning download result: {}", result);
            Ok(result)
        } else {
            warn!("yt-dlp exited with non-zero status: {:?}", status);
            warn!("Captured file path before failure: {:?}", final_file_path);

            if let Some(ref path) = final_file_path {
                let file_exists = std::path::Path::new(path).exists();
                info!("Checking if file exists at {:?}: {}", path, file_exists);
                if file_exists {
                    info!(
                        "Download completed (with warnings) for: {} - file exists at {:?}",
                        url, path
                    );

                    {
                        let mut downloads = lock_or_recover(&ACTIVE_DOWNLOADS);
                        downloads.remove(&url);
                    }

                    if use_custom_cookies {
                        if let Ok(cache_dir) = app.path().app_cache_dir() {
                            let _ =
                                tokio::fs::remove_file(cache_dir.join("custom_cookies.txt")).await;
                        }
                    }

                    let _ = window.emit(
                        "download-file-path",
                        DownloadFilePath {
                            url: url.clone(),
                            file_path: path.clone(),
                        },
                    );

                    return Ok(path.clone());
                }
            }

            {
                let mut downloads = lock_or_recover(&ACTIVE_DOWNLOADS);
                downloads.remove(&url);
            }

            if use_custom_cookies {
                if let Ok(cache_dir) = app.path().app_cache_dir() {
                    let _ = tokio::fs::remove_file(cache_dir.join("custom_cookies.txt")).await;
                }
            }

            error!("Download failed for: {}", url);

            let error_msg = if !error_messages.is_empty() {
                let combined = error_messages.join(" ");

                if combined.contains("LOGIN_REQUIRED") || combined.contains("Sign in to confirm") {
                    "LOGIN_REQUIRED: This video requires authentication. Please go to Settings and select a browser in the Cookies option to use your logged-in session.".to_string()
                } else if combined.contains("age-restricted") {
                    "AGE_RESTRICTED: This video is age-restricted. Please go to Settings and select a browser in the Cookies option to use your logged-in session.".to_string()
                } else if combined.contains("Private video") {
                    "PRIVATE_VIDEO: This video is private. Make sure you have access and select a browser in the Cookies option in Settings.".to_string()
                } else if combined.contains("members-only") {
                    "MEMBERS_ONLY: This video is for channel members only. Make sure you're a member and select a browser in the Cookies option in Settings.".to_string()
                } else if combined.contains("requires payment") {
                    "PAYMENT_REQUIRED: This video requires payment to watch.".to_string()
                } else if combined.contains("Video unavailable") {
                    "VIDEO_UNAVAILABLE: This video is not available. It may have been removed or restricted in your region.".to_string()
                } else if combined.contains("is not a valid URL")
                    || combined.contains("Unsupported URL")
                {
                    "INVALID_URL: The URL is not valid or not supported.".to_string()
                } else {
                    format!(
                        "Download failed: {}",
                        error_messages
                            .first()
                            .unwrap_or(&"Unknown error".to_string())
                    )
                }
            } else {
                "Download failed".to_string()
            };

            Err(error_msg)
        }
    }
}

#[cfg(not(target_os = "android"))]
#[derive(serde::Serialize, Clone)]
struct DownloadProgress {
    url: String,
    message: String,
}

#[cfg(not(target_os = "android"))]
#[derive(serde::Serialize, Clone)]
struct DownloadFilePath {
    url: String,
    file_path: String,
}

/// Cancel an active download by URL
#[tauri::command]
#[allow(unused_variables)]
async fn cancel_download(url: String) -> Result<(), String> {
    info!("Cancelling download for URL: {}", url);

    #[cfg(target_os = "android")]
    {
        return Err("Cancel not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let pid = {
            let downloads = lock_or_recover(&ACTIVE_DOWNLOADS);
            downloads.get(&url).copied()
        };

        if let Some(pid) = pid {
            info!("Killing process {} for URL: {}", pid, url);

            #[cfg(target_os = "windows")]
            {
                use crate::utils::StdCommandHideConsole;
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .hide_console()
                    .output();
            }

            #[cfg(not(target_os = "windows"))]
            {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
            }

            {
                let mut downloads = lock_or_recover(&ACTIVE_DOWNLOADS);
                downloads.remove(&url);
            }

            Ok(())
        } else {
            Err("Download not found or already completed".to_string())
        }
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
        let backend = backends::YtDlpBackend;
        backend
            .get_playlist_info(
                &app,
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
        let backend = backends::YtDlpBackend;
        backend
            .get_video_info(
                &app,
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
        let backend = backends::YtDlpBackend;
        backend
            .get_video_formats(
                &app,
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

// ==================== Lux Backend Commands ====================

/// Get video info using Lux backend
#[tauri::command]
#[allow(unused_variables)]
async fn lux_get_video_info(
    app: AppHandle,
    url: String,
    custom_cookies: Option<String>,
    proxy_config: Option<proxy::ProxyConfig>,
) -> Result<VideoInfo, String> {
    info!("Getting video info via Lux for URL: {}", url);

    #[cfg(target_os = "android")]
    {
        return Err("Lux is not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let backend = backends::LuxBackend;
        backend
            .get_video_info(
                &app,
                InfoRequest {
                    url,
                    cookies_from_browser: None,
                    custom_cookies,
                    proxy_config,
                    youtube_player_client: None,
                },
            )
            .await
    }
}

/// Get video formats using Lux backend
#[tauri::command]
#[allow(unused_variables)]
async fn lux_get_video_formats(
    app: AppHandle,
    url: String,
    custom_cookies: Option<String>,
    proxy_config: Option<proxy::ProxyConfig>,
) -> Result<VideoFormats, String> {
    info!("Getting video formats via Lux for URL: {}", url);

    #[cfg(target_os = "android")]
    {
        return Err("Lux is not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let backend = backends::LuxBackend;
        backend
            .get_video_formats(
                &app,
                InfoRequest {
                    url,
                    cookies_from_browser: None,
                    custom_cookies,
                    proxy_config,
                    youtube_player_client: None,
                },
            )
            .await
    }
}

/// Get playlist info using Lux backend
#[tauri::command]
#[allow(unused_variables)]
async fn lux_get_playlist_info(
    app: AppHandle,
    url: String,
    offset: Option<usize>,
    limit: Option<usize>,
    custom_cookies: Option<String>,
    proxy_config: Option<proxy::ProxyConfig>,
) -> Result<PlaylistInfo, String> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(50);

    info!(
        "Getting playlist info via Lux for URL: {} (offset={}, limit={})",
        url, offset, limit
    );

    #[cfg(target_os = "android")]
    {
        return Err("Lux is not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let backend = backends::LuxBackend;
        backend
            .get_playlist_info(
                &app,
                PlaylistRequest {
                    url,
                    offset,
                    limit,
                    cookies_from_browser: None,
                    custom_cookies,
                    proxy_config,
                    youtube_player_client: None,
                },
            )
            .await
    }
}

/// Download video using Lux backend
#[tauri::command]
#[allow(unused_variables)]
async fn lux_download_video(
    app: AppHandle,
    window: tauri::Window,
    url: String,
    format_id: Option<String>,
    download_path: Option<String>,
    custom_cookies: Option<String>,
    proxy_config: Option<proxy::ProxyConfig>,
    multi_thread: Option<bool>,
    thread_count: Option<u32>,
) -> Result<String, String> {
    info!("Starting Lux download for URL: {}", url);

    #[cfg(target_os = "android")]
    {
        return Err("Lux is not supported on Android".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let downloads_dir = if let Some(ref custom_path) = download_path {
            if !custom_path.is_empty() {
                std::path::PathBuf::from(custom_path)
            } else {
                dirs::download_dir().ok_or("Could not find Downloads folder")?
            }
        } else {
            dirs::download_dir().ok_or("Could not find Downloads folder")?
        };

        if !downloads_dir.exists() {
            std::fs::create_dir_all(&downloads_dir)
                .map_err(|e| format!("Failed to create download directory: {}", e))?;
        }

        let window_clone = window.clone();
        let url_clone = url.clone();

        let backend = backends::LuxBackend;
        backend
            .download(
                &app,
                backends::LuxDownloadRequest {
                    url,
                    format_id,
                    output_dir: downloads_dir,
                    custom_cookies,
                    proxy_config,
                    multi_thread: multi_thread.unwrap_or(false),
                    thread_count,
                },
                move |progress| {
                    let _ = window_clone.emit(
                        "download-progress",
                        types::DownloadProgress {
                            url: url_clone.clone(),
                            message: progress,
                        },
                    );
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
            .arg("--continue=true") // resume support
            .arg("--auto-file-renaming=false")
            .arg("--allow-overwrite=true")
            .arg("--summary-interval=1") // Output progress every second
            .arg("--console-log-level=notice");

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

        // Register the process for cancellation support
        if let Some(pid) = child.id() {
            let mut downloads = lock_or_recover(&ACTIVE_DOWNLOADS);
            downloads.insert(url.clone(), pid);
            info!("Registered aria2c process {} for URL: {}", pid, url);
        }

        info!("aria2c process spawned, reading stdout...");

        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let mut stdout_reader = BufReader::new(stdout).lines();

        // Ensure stderr is also consumed to prevent blocking, though we primarily watch stdout
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(_)) = reader.next_line().await {
                // Consume stderr
            }
        });

        let mut last_progress_emit =
            std::time::Instant::now() - std::time::Duration::from_millis(200);
        const PROGRESS_THROTTLE_MS: u64 = 250;
        let mut line_count = 0;

        // Read stdout for progress
        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            line_count += 1;
                            if !line.is_empty() {
                                debug!("aria2c: {}", line);

                                let now = std::time::Instant::now();
                                if now.duration_since(last_progress_emit).as_millis() >= PROGRESS_THROTTLE_MS as u128 {
                                    last_progress_emit = now;

                                    let _ = window.emit("download-progress", serde_json::json!({
                                        "url": url,
                                        "message": line,
                                    }));
                                }
                            }
                        }
                        Ok(None) => {
                            info!("aria2c stdout closed after {} lines", line_count);
                            break;
                        }
                        Err(e) => {
                            warn!("aria2c stdout read error: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        info!("Waiting for aria2c to exit...");
        let status = child
            .wait()
            .await
            .map_err(|e| format!("aria2c failed: {}", e))?;
        info!("aria2c exited with status: {:?}", status);

        // Remove from active downloads after completion
        {
            let mut downloads = lock_or_recover(&ACTIVE_DOWNLOADS);
            downloads.remove(&url);
        }

        if !status.success() {
            error!("aria2c exited with non-zero status");
            return Err("Download failed".to_string());
        }

        info!("aria2c download complete: {:?}", dest_path);
    } else {
        info!("aria2c not available, using reqwest fallback");

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
                let speed = if elapsed > 0.0 {
                    downloaded as f64 / elapsed
                } else {
                    0.0
                };

                let progress = if total_size > 0 {
                    (downloaded as f64 / total_size as f64) * 100.0
                } else {
                    0.0
                };

                let _ = window.emit(
                    "file-download-progress",
                    serde_json::json!({
                        "url": url,
                        "progress": progress,
                        "downloaded": downloaded,
                        "total": total_size,
                        "speed": speed,
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
        cancel_download,
        get_video_info,
        get_video_formats,
        get_playlist_info,
        get_media_duration,
        extract_video_thumbnail,
        extract_thumbnail_color,
        get_cached_thumbnail_color,
        lux_get_video_info,
        lux_get_video_formats,
        lux_get_playlist_info,
        lux_download_video,
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
        autostart_is_enabled
    ]);

    #[cfg(not(target_os = "android"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        download_video,
        cancel_download,
        get_video_info,
        get_video_formats,
        get_playlist_info,
        get_media_duration,
        extract_video_thumbnail,
        extract_thumbnail_color,
        get_cached_thumbnail_color,
        lux_get_video_info,
        lux_get_video_formats,
        lux_get_playlist_info,
        lux_download_video,
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
