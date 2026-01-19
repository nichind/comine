use crate::backends::{Backend, DownloadBackend, DownloadRequest, InfoRequest, PlaylistRequest};
use crate::proxy;
use crate::types::{PlaylistEntry, PlaylistInfo, VideoFormat, VideoFormats, VideoInfo};
use async_trait::async_trait;
use log::{debug, info, warn};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tauri::{AppHandle, Manager, Window};

use crate::job_engine::{self, JobRegistry};

#[cfg(target_os = "windows")]
use crate::utils::CommandHideConsole;

pub struct LuxBackend;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct LuxVideoResponse {
    pub site: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub media_type: Option<String>,
    pub streams: Option<HashMap<String, LuxStream>>,
    // Playlist fields
    #[serde(default)]
    pub err: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LuxStream {
    pub urls: Option<Vec<LuxUrl>>,
    pub quality: Option<String>,
    pub size: Option<u64>,
    #[serde(default)]
    pub ext: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct LuxUrl {
    pub url: Option<String>,
    pub size: Option<u64>,
    pub ext: Option<String>,
}

pub struct LuxCommandConfig {
    pub lux_path: String,
    pub env_vars: Vec<(String, String)>,
}

pub fn get_lux_path(app: &AppHandle) -> Result<PathBuf, String> {
    let bin_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?
        .join("bin");

    #[cfg(target_os = "windows")]
    let lux_path = bin_dir.join("lux.exe");
    #[cfg(not(target_os = "windows"))]
    let lux_path = bin_dir.join("lux");

    Ok(lux_path)
}

pub fn get_command(app: &AppHandle, proxy_url: Option<&str>) -> Result<LuxCommandConfig, String> {
    let lux_path = get_lux_path(app)?;
    if !lux_path.exists() {
        return Err("Lux is not installed. Please install it first.".to_string());
    }

    let bin_dir = lux_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut env_vars = vec![];

    if !bin_dir.is_empty() {
        let current_path = std::env::var("PATH").unwrap_or_default();
        #[cfg(target_os = "windows")]
        let new_path = format!("{};{}", bin_dir, current_path);
        #[cfg(not(target_os = "windows"))]
        let new_path = format!("{}:{}", bin_dir, current_path);
        env_vars.push(("PATH".to_string(), new_path));
    }

    if let Some(p) = proxy_url {
        if !p.is_empty() {
            info!("Setting proxy environment variables for lux: {}", p);
            env_vars.push(("HTTP_PROXY".to_string(), p.to_string()));
            env_vars.push(("HTTPS_PROXY".to_string(), p.to_string()));
            env_vars.push(("http_proxy".to_string(), p.to_string()));
            env_vars.push(("https_proxy".to_string(), p.to_string()));
        }
    }

    Ok(LuxCommandConfig {
        lux_path: lux_path.to_string_lossy().to_string(),
        env_vars,
    })
}

pub async fn setup_cookies(
    app: &AppHandle,
    args: &mut Vec<String>,
    custom_cookies: &Option<String>,
) -> Result<bool, String> {
    let use_custom_cookies = custom_cookies
        .as_ref()
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    if use_custom_cookies {
        if let Some(cookies_text) = custom_cookies.as_deref() {
            let cache_dir = app
                .path()
                .app_cache_dir()
                .map_err(|e| format!("Failed to get cache dir: {}", e))?;
            let cookies_file = cache_dir.join("lux_cookies.txt");

            tokio::fs::create_dir_all(&cache_dir)
                .await
                .map_err(|e| format!("Failed to create cache dir: {}", e))?;

            tokio::fs::write(&cookies_file, cookies_text)
                .await
                .map_err(|e| format!("Failed to write cookies file: {}", e))?;

            args.push("-c".to_string());
            args.push(cookies_file.to_string_lossy().to_string());
            info!("Using custom cookies file for lux");
            return Ok(true);
        }
    }

    Ok(false)
}

fn parse_resolution(quality: &str) -> Option<String> {
    let patterns = [
        "2160p", "1440p", "1080p", "720p", "480p", "360p", "240p", "144p",
    ];
    for pattern in patterns {
        if quality.contains(pattern) {
            return Some(pattern.to_string());
        }
    }

    if quality.contains("1080") {
        return Some("1080p".to_string());
    } else if quality.contains("720") {
        return Some("720p".to_string());
    } else if quality.contains("480") {
        return Some("480p".to_string());
    } else if quality.contains("360") {
        return Some("360p".to_string());
    }

    None
}

fn parse_codec(quality: &str) -> Option<String> {
    if quality.contains("vp9") || quality.contains("VP9") {
        Some("vp9".to_string())
    } else if quality.contains("avc1") || quality.contains("h264") || quality.contains("H264") {
        Some("h264".to_string())
    } else if quality.contains("av01") || quality.contains("av1") || quality.contains("AV1") {
        Some("av1".to_string())
    } else if quality.contains("hevc") || quality.contains("h265") || quality.contains("H265") {
        Some("hevc".to_string())
    } else {
        None
    }
}

fn has_video(quality: &str) -> bool {
    quality.contains("video")
        || quality.contains("p ")
        || quality.contains("1080")
        || quality.contains("720")
        || quality.contains("480")
        || quality.contains("360")
        || quality.contains("高清")
        || quality.contains("清晰")
        || quality.contains("流畅")
}

fn has_audio(quality: &str) -> bool {
    quality.contains("audio") || quality.contains("音频")
}

fn extract_site_id(site: &str) -> String {
    let lower = site.to_lowercase();
    if lower.contains("youtube") {
        "youtube".to_string()
    } else if lower.contains("bilibili") {
        "bilibili".to_string()
    } else if lower.contains("douyin") || lower.contains("抖音") {
        "douyin".to_string()
    } else if lower.contains("tiktok") {
        "tiktok".to_string()
    } else if lower.contains("twitter") {
        "twitter".to_string()
    } else if lower.contains("instagram") {
        "instagram".to_string()
    } else if lower.contains("vimeo") {
        "vimeo".to_string()
    } else if lower.contains("weibo") || lower.contains("微博") {
        "weibo".to_string()
    } else {
        // Extract domain from site string
        site.split_whitespace()
            .last()
            .unwrap_or("unknown")
            .split('.')
            .next()
            .unwrap_or("unknown")
            .to_string()
    }
}

#[async_trait]
impl Backend for LuxBackend {
    async fn get_video_info(
        &self,
        app: &AppHandle,
        request: InfoRequest,
    ) -> Result<VideoInfo, String> {
        let resolved_proxy = request
            .proxy_config
            .as_ref()
            .map(|c| proxy::resolve_proxy(c));
        let has_proxy = resolved_proxy
            .as_ref()
            .map(|r| !r.url.is_empty())
            .unwrap_or(false);

        let first_result = self
            .execute_get_video_info(
                app,
                &request,
                resolved_proxy.as_ref().and_then(|r| {
                    if r.url.is_empty() {
                        None
                    } else {
                        Some(r.url.as_str())
                    }
                }),
            )
            .await;

        match first_result {
            Ok(info) => Ok(info),
            Err(first_error) => {
                let retry_enabled = request
                    .proxy_config
                    .as_ref()
                    .map(|c| c.retry_without_proxy)
                    .unwrap_or(true);
                if !retry_enabled {
                    return Err(first_error);
                }

                let is_proxy_error = first_error.contains("412")
                    || first_error.contains("HTTP")
                    || first_error.contains("request error")
                    || first_error.contains("connection")
                    || first_error.contains("timeout");

                if has_proxy && is_proxy_error {
                    warn!(
                        "Lux get_video_info failed with proxy, retrying without proxy: {}",
                        first_error.lines().next().unwrap_or(&first_error)
                    );
                    self.execute_get_video_info(app, &request, None).await
                } else if !has_proxy && is_proxy_error {
                    let system_proxy = proxy::resolve_proxy(&proxy::ProxyConfig {
                        mode: "system".to_string(),
                        custom_url: String::new(),
                        retry_without_proxy: false,
                    });
                    if !system_proxy.url.is_empty() {
                        warn!(
                            "Lux get_video_info failed without proxy, retrying with system proxy: {}",
                            first_error.lines().next().unwrap_or(&first_error)
                        );
                        self.execute_get_video_info(app, &request, Some(&system_proxy.url))
                            .await
                    } else {
                        Err(first_error)
                    }
                } else {
                    Err(first_error)
                }
            }
        }
    }

    async fn get_playlist_info(
        &self,
        app: &AppHandle,
        request: PlaylistRequest,
    ) -> Result<PlaylistInfo, String> {
        let resolved_proxy = request
            .proxy_config
            .as_ref()
            .map(|c| proxy::resolve_proxy(c));
        let proxy_url = resolved_proxy.as_ref().and_then(|r| {
            if r.url.is_empty() {
                None
            } else {
                Some(r.url.as_str())
            }
        });

        let config = get_command(app, proxy_url)?;

        let mut args = vec!["-j".to_string(), "-i".to_string(), "-p".to_string()];

        let start = request.offset + 1;
        let end = request.offset + request.limit;
        args.push("-start".to_string());
        args.push(start.to_string());
        args.push("-end".to_string());
        args.push(end.to_string());

        setup_cookies(app, &mut args, &request.custom_cookies).await?;
        args.push(request.url.clone());

        debug!(
            "Running lux playlist command: {} {:?}",
            config.lux_path, args
        );

        let mut cmd = tokio::process::Command::new(&config.lux_path);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to execute lux: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        debug!("Lux playlist stdout length: {} bytes", stdout.len());

        let mut entries: Vec<PlaylistEntry> = Vec::new();
        let playlist_title = String::new();
        let mut site_name = String::new();

        for line in stdout.lines() {
            if !line.trim().starts_with('{') {
                continue;
            }

            match serde_json::from_str::<LuxVideoResponse>(line) {
                Ok(response) => {
                    if let Some(err) = response.err {
                        warn!("Lux playlist item error: {}", err);
                        continue;
                    }

                    let title = response.title.unwrap_or_else(|| "Unknown".to_string());

                    if playlist_title.is_empty() {
                        if let Some(site) = &response.site {
                            site_name = site.clone();
                        }
                    }

                    let id = format!("lux_{}", entries.len());

                    entries.push(PlaylistEntry {
                        id,
                        url: request.url.clone(),
                        title,
                        duration: None,
                        thumbnail: None,
                        uploader: response.site,
                        is_music: false,
                    });
                }
                Err(e) => {
                    debug!("Failed to parse playlist item: {}", e);
                }
            }
        }

        if entries.is_empty() && !stderr.is_empty() {
            return Err(format!(
                "Failed to get playlist info: {}",
                stderr.lines().take(5).collect::<Vec<_>>().join("\n")
            ));
        }

        let has_more = entries.len() >= request.limit;

        Ok(PlaylistInfo {
            is_playlist: entries.len() > 1,
            id: Some(extract_site_id(&site_name)),
            title: if playlist_title.is_empty() {
                format!("Playlist from {}", site_name)
            } else {
                playlist_title
            },
            uploader: Some(site_name),
            thumbnail: None,
            total_count: entries.len(),
            entries,
            has_more,
        })
    }

    async fn get_video_formats(
        &self,
        app: &AppHandle,
        request: InfoRequest,
    ) -> Result<VideoFormats, String> {
        let resolved_proxy = request
            .proxy_config
            .as_ref()
            .map(|c| proxy::resolve_proxy(c));
        let proxy_url = resolved_proxy.as_ref().and_then(|r| {
            if r.url.is_empty() {
                None
            } else {
                Some(r.url.as_str())
            }
        });

        let config = get_command(app, proxy_url)?;

        let mut args = vec!["-j".to_string(), "-i".to_string()];

        setup_cookies(app, &mut args, &request.custom_cookies).await?;
        args.push(request.url.clone());

        debug!(
            "Running lux formats command: {} {:?}",
            config.lux_path, args
        );

        let mut cmd = tokio::process::Command::new(&config.lux_path);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        let output = tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output())
            .await
            .map_err(|_| "Lux formats request timed out after 15 seconds".to_string())?
            .map_err(|e| format!("Failed to execute lux: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let trimmed = stdout.trim();

        let response: LuxVideoResponse = if trimmed.starts_with('[') {
            let responses: Vec<LuxVideoResponse> = serde_json::from_str(trimmed).map_err(|e| {
                format!(
                    "Failed to parse lux JSON array: {}. Output starts with: {}",
                    e,
                    &trimmed[..trimmed.len().min(100)]
                )
            })?;
            responses
                .into_iter()
                .next()
                .ok_or_else(|| "Lux returned empty array".to_string())?
        } else if trimmed.starts_with('{') {
            serde_json::from_str(trimmed).map_err(|e| format!("Failed to parse lux JSON: {}", e))?
        } else {
            return Err(format!(
                "No JSON output from lux. stderr: {}",
                stderr.lines().take(5).collect::<Vec<_>>().join("\n")
            ));
        };

        if let Some(err) = response.err {
            return Err(format!("Lux error: {}", err));
        }

        let title = response.title.unwrap_or_else(|| "Unknown".to_string());
        let author = response.site.clone();

        let mut formats: Vec<VideoFormat> = Vec::new();

        if let Some(streams) = response.streams {
            for (format_id, stream) in streams {
                let quality = stream.quality.clone().unwrap_or_default();
                let resolution = parse_resolution(&quality);
                let vcodec = parse_codec(&quality);

                let ext = stream
                    .urls
                    .as_ref()
                    .and_then(|urls| urls.first())
                    .and_then(|u| u.ext.clone())
                    .or_else(|| stream.ext.clone())
                    .unwrap_or_else(|| "mp4".to_string());

                let has_vid = has_video(&quality);
                let has_aud = has_audio(&quality);

                formats.push(VideoFormat {
                    format_id,
                    ext,
                    resolution,
                    fps: None,
                    vcodec,
                    acodec: None,
                    filesize: stream.size,
                    filesize_approx: None,
                    tbr: None,
                    vbr: None,
                    abr: None,
                    asr: None,
                    format_note: stream.quality,
                    has_video: has_vid,
                    has_audio: has_aud,
                    quality: None,
                });
            }
        }

        formats.sort_by(|a, b| {
            let res_order = |r: &Option<String>| -> i32 {
                match r.as_deref() {
                    Some("2160p") => 8,
                    Some("1440p") => 7,
                    Some("1080p") => 6,
                    Some("720p") => 5,
                    Some("480p") => 4,
                    Some("360p") => 3,
                    Some("240p") => 2,
                    Some("144p") => 1,
                    _ => 0,
                }
            };
            res_order(&b.resolution).cmp(&res_order(&a.resolution))
        });

        Ok(VideoFormats {
            title,
            author,
            thumbnail: None,
            duration: None,
            formats,
            view_count: None,
            like_count: None,
            description: None,
            upload_date: None,
            channel_url: None,
            channel_id: None,
            storyboards: None,
            chapters: None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct LuxDownloadRequest {
    pub url: String,
    pub format_id: Option<String>,
    pub output_dir: std::path::PathBuf,
    pub custom_cookies: Option<String>,
    pub proxy_config: Option<crate::proxy::ProxyConfig>,
    pub multi_thread: bool,
    pub thread_count: Option<u32>,
}

impl LuxBackend {
    pub async fn download_job(
        &self,
        app: &AppHandle,
        window: tauri::Window,
        registry: JobRegistry,
        request: LuxDownloadRequest,
    ) -> Result<String, String> {
        let resolved_proxy = request
            .proxy_config
            .as_ref()
            .map(|c| proxy::resolve_proxy(c));

        let proxy_url = resolved_proxy.as_ref().and_then(|r| {
            if r.url.is_empty() {
                None
            } else {
                Some(r.url.as_str())
            }
        });

        let config = get_command(app, proxy_url)?;

        let mut args = vec![
            "-o".to_string(),
            request.output_dir.to_string_lossy().to_string(),
        ];

        if let Some(ref format_id) = request.format_id {
            let is_generic = format_id.is_empty()
                || format_id == "default"
                || format_id == "max"
                || format_id == "best"
                || format_id == "bestvideo+bestaudio"
                || format_id == "bestvideo"
                || format_id == "bestaudio";

            if !is_generic {
                args.push("-f".to_string());
                args.push(format_id.clone());
            }
        }

        if request.multi_thread {
            args.push("-m".to_string());
            if let Some(threads) = request.thread_count {
                args.push("-n".to_string());
                args.push(threads.to_string());
            }
        }

        let used_cookies = setup_cookies(app, &mut args, &request.custom_cookies).await?;
        args.push(request.url.clone());

        let env: HashMap<String, String> = config.env_vars.into_iter().collect();
        let env_opt = if env.is_empty() { None } else { Some(env) };

        let mut cleanup_files: Vec<String> = vec![];
        if used_cookies {
            if let Ok(cache_dir) = app.path().app_cache_dir() {
                cleanup_files.push(
                    cache_dir
                        .join("lux_cookies.txt")
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }

        job_engine::spawn_process_job(
            window,
            registry,
            format!("lux_download_video: {}", request.url),
            config.lux_path,
            args,
            None,
            env_opt,
            cleanup_files,
            Some(request.output_dir.to_string_lossy().to_string()),
        )
        .await
    }

    pub async fn download(
        &self,
        app: &AppHandle,
        request: LuxDownloadRequest,
        progress_callback: impl Fn(String) + Send + Sync + 'static,
    ) -> Result<String, String> {
        let callback = std::sync::Arc::new(progress_callback);

        let resolved_proxy = request
            .proxy_config
            .as_ref()
            .map(|c| proxy::resolve_proxy(c));
        let has_proxy = resolved_proxy
            .as_ref()
            .map(|r| !r.url.is_empty())
            .unwrap_or(false);

        let first_result = self
            .execute_download(
                app,
                &request,
                resolved_proxy.as_ref().and_then(|r| {
                    if r.url.is_empty() {
                        None
                    } else {
                        Some(r.url.as_str())
                    }
                }),
                callback.clone(),
            )
            .await;

        match first_result {
            Ok(path) => Ok(path),
            Err(first_error) => {
                let retry_enabled = request
                    .proxy_config
                    .as_ref()
                    .map(|c| c.retry_without_proxy)
                    .unwrap_or(true);
                if !retry_enabled {
                    return Err(first_error);
                }

                let is_proxy_error = first_error.contains("412")
                    || first_error.contains("HTTP")
                    || first_error.contains("request error")
                    || first_error.contains("connection")
                    || first_error.contains("timeout");

                if has_proxy && is_proxy_error {
                    warn!(
                        "Lux download failed with proxy ({}), retrying without proxy...",
                        first_error.lines().next().unwrap_or(&first_error)
                    );

                    self.execute_download(app, &request, None, callback.clone())
                        .await
                        .map_err(|second_error| {
                            format!(
                                "Lux download failed both with and without proxy.\nWith proxy: {}\nWithout proxy: {}",
                                first_error.lines().next().unwrap_or(&first_error),
                                second_error.lines().next().unwrap_or(&second_error)
                            )
                        })
                } else if !has_proxy {
                    let system_proxy = proxy::resolve_proxy(&proxy::ProxyConfig {
                        mode: "system".to_string(),
                        custom_url: String::new(),
                        retry_without_proxy: false,
                    });

                    if !system_proxy.url.is_empty() {
                        warn!(
                            "Lux download failed without proxy ({}), retrying with system proxy...",
                            first_error.lines().next().unwrap_or(&first_error)
                        );

                        self.execute_download(app, &request, Some(&system_proxy.url), callback.clone())
                            .await
                            .map_err(|second_error| {
                                format!(
                                    "Lux download failed both without and with proxy.\nWithout proxy: {}\nWith proxy: {}",
                                    first_error.lines().next().unwrap_or(&first_error),
                                    second_error.lines().next().unwrap_or(&second_error)
                                )
                            })
                    } else {
                        Err(first_error)
                    }
                } else {
                    Err(first_error)
                }
            }
        }
    }

    async fn execute_get_video_info(
        &self,
        app: &AppHandle,
        request: &InfoRequest,
        proxy_url: Option<&str>,
    ) -> Result<VideoInfo, String> {
        let config = get_command(app, proxy_url)?;

        let mut args = vec!["-j".to_string(), "-i".to_string()];

        setup_cookies(app, &mut args, &request.custom_cookies).await?;
        args.push(request.url.clone());

        debug!("Running lux info command: {} {:?}", config.lux_path, args);

        let mut cmd = tokio::process::Command::new(&config.lux_path);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        let output = tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output())
            .await
            .map_err(|_| "Lux info request timed out".to_string())?
            .map_err(|e| format!("Failed to execute lux: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        debug!("Lux stdout: {}", stdout);
        if !stderr.is_empty() {
            debug!("Lux stderr: {}", stderr);
        }

        if stdout.contains("HTTP 412") || stdout.contains("request error") {
            return Err(format!(
                "Lux request failed: {}",
                stdout.lines().take(3).collect::<Vec<_>>().join("\n")
            ));
        }

        let trimmed = stdout.trim();

        let response: LuxVideoResponse = if trimmed.starts_with('[') {
            let responses: Vec<LuxVideoResponse> = serde_json::from_str(trimmed).map_err(|e| {
                format!(
                    "Failed to parse lux JSON array: {}. Output starts with: {}",
                    e,
                    &trimmed[..trimmed.len().min(100)]
                )
            })?;
            responses
                .into_iter()
                .next()
                .ok_or_else(|| "Lux returned empty array".to_string())?
        } else if trimmed.starts_with('{') {
            serde_json::from_str(trimmed).map_err(|e| {
                format!(
                    "Failed to parse lux JSON: {}. Output starts with: {}",
                    e,
                    &trimmed[..trimmed.len().min(100)]
                )
            })?
        } else {
            return Err(format!(
                "No JSON output from lux. stderr: {}",
                stderr.lines().take(5).collect::<Vec<_>>().join("\n")
            ));
        };

        if let Some(err) = response.err {
            return Err(format!("Lux error: {}", err));
        }

        let title = response.title.unwrap_or_else(|| "Unknown".to_string());

        let (filesize, ext) = response
            .streams
            .as_ref()
            .and_then(|s| s.values().next())
            .map(|stream| {
                let ext = stream
                    .urls
                    .as_ref()
                    .and_then(|urls| urls.first())
                    .and_then(|u| u.ext.clone())
                    .or_else(|| stream.ext.clone());
                (stream.size, ext)
            })
            .unwrap_or((None, None));

        let uploader = response.site.clone();

        Ok(VideoInfo {
            title,
            uploader,
            channel: None,
            creator: None,
            uploader_id: None,
            thumbnail: None,
            duration: None,
            filesize,
            ext,
        })
    }

    async fn execute_download<F>(
        &self,
        app: &AppHandle,
        request: &LuxDownloadRequest,
        proxy_url: Option<&str>,
        progress_callback: std::sync::Arc<F>,
    ) -> Result<String, String>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        let config = get_command(app, proxy_url)?;

        let mut args = vec![
            "-o".to_string(),
            request.output_dir.to_string_lossy().to_string(),
        ];

        if let Some(ref format_id) = request.format_id {
            let is_generic = format_id.is_empty()
                || format_id == "default"
                || format_id == "max"
                || format_id == "best"
                || format_id == "bestvideo+bestaudio"
                || format_id == "bestvideo"
                || format_id == "bestaudio";

            if !is_generic {
                args.push("-f".to_string());
                args.push(format_id.clone());
            }
        }

        if request.multi_thread {
            args.push("-m".to_string());
            if let Some(threads) = request.thread_count {
                args.push("-n".to_string());
                args.push(threads.to_string());
            }
        }

        setup_cookies(app, &mut args, &request.custom_cookies).await?;
        args.push(request.url.clone());

        info!("Running lux download: {} {:?}", config.lux_path, args);

        let mut cmd = tokio::process::Command::new(&config.lux_path);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn lux: {}", e))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let downloaded_file = std::sync::Arc::new(std::sync::Mutex::new(Option::<String>::None));
        let downloaded_file_clone = downloaded_file.clone();

        let error_capture = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let error_capture_stdout = error_capture.clone();
        let error_capture_stderr = error_capture.clone();

        let stdout_handle = if let Some(stdout) = stdout {
            let callback = progress_callback.clone();
            let file_tracker = downloaded_file.clone();
            Some(tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    if line.contains("File saved:") || line.contains("Saved to:") {
                        if let Some(path_part) = line.split(':').nth(1) {
                            let filepath = path_part.trim();
                            if let Ok(mut file) = file_tracker.lock() {
                                *file = Some(filepath.to_string());
                                info!("Extracted lux output file: {}", filepath);
                            }
                        }
                    } else if line.contains(": file already exists") {
                        if let Some(path_part) = line.split(": file already exists").next() {
                            let filepath = path_part.trim();
                            if let Ok(mut file) = file_tracker.lock() {
                                *file = Some(filepath.to_string());
                                info!("Extracted existing file path: {}", filepath);
                            }
                        }
                    }

                    if line.contains("error") || line.contains("HTTP") || line.contains("failed") {
                        if let Ok(mut capture) = error_capture_stdout.lock() {
                            if capture.len() < 50 {
                                capture.push(line.clone());
                            }
                        }
                    }
                    callback(line);
                }
            }))
        } else {
            None
        };

        let stderr_handle = if let Some(stderr) = stderr {
            let callback = progress_callback.clone();
            Some(tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    if line.contains("error") || line.contains("failed") {
                        if let Ok(mut capture) = error_capture_stderr.lock() {
                            if capture.len() < 50 {
                                capture.push(line.clone());
                            }
                        }
                    }
                    callback(line);
                }
            }))
        } else {
            None
        };

        let status = child
            .wait()
            .await
            .map_err(|e| format!("Failed to wait for lux: {}", e))?;

        if let Some(handle) = stdout_handle {
            let _ = handle.await;
        }
        if let Some(handle) = stderr_handle {
            let _ = handle.await;
        }

        if !status.success() {
            let error_messages = error_capture
                .lock()
                .map(|v| v.iter().take(10).cloned().collect::<Vec<_>>().join("\n"))
                .unwrap_or_default();

            let error_output = if !error_messages.is_empty() {
                error_messages
                    .lines()
                    .take(5)
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                "Unknown error".to_string()
            };

            return Err(format!("Lux download failed: {}", error_output));
        }

        if let Ok(captured_file) = downloaded_file_clone.lock() {
            if let Some(ref filepath) = *captured_file {
                info!("Lux download complete (from output): {}", filepath);
                return Ok(filepath.clone());
            }
        }

        warn!("Could not extract filename from lux output, scanning directory...");
        let mut latest_file: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
        let scan_start = std::time::SystemTime::now();

        if let Ok(entries) = std::fs::read_dir(&request.output_dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        if let Ok(modified) = metadata.modified() {
                            if scan_start
                                .duration_since(modified)
                                .map(|d| d.as_secs() < 60)
                                .unwrap_or(false)
                            {
                                if latest_file.as_ref().map_or(true, |(_, t)| modified > *t) {
                                    latest_file = Some((entry.path(), modified));
                                }
                            }
                        }
                    }
                }
            }
        }

        match latest_file {
            Some((path, _)) => {
                info!("Lux download complete (from directory scan): {:?}", path);
                Ok(path.to_string_lossy().to_string())
            }
            None => Err("Download completed but could not find output file".to_string()),
        }
    }
}

#[async_trait]
impl DownloadBackend for LuxBackend {
    async fn download_job(
        &self,
        app: &AppHandle,
        window: Window,
        registry: JobRegistry,
        request: DownloadRequest,
    ) -> Result<String, String> {
        let downloads_dir = if let Some(ref custom_path) = request.download_path {
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

        // Lux expects a format id; `video_quality` is reused.
        self.download_job(
            app,
            window,
            registry,
            LuxDownloadRequest {
                url: request.url,
                format_id: request.video_quality.filter(|s| !s.is_empty()),
                output_dir: downloads_dir,
                custom_cookies: request.custom_cookies,
                proxy_config: request.proxy_config,
                multi_thread: request.multi_thread.unwrap_or(false),
                thread_count: request.thread_count,
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_resolution() {
        assert_eq!(
            parse_resolution("1080p video/webm; codecs=\"vp9\""),
            Some("1080p".to_string())
        );
        assert_eq!(parse_resolution("720p video/mp4"), Some("720p".to_string()));
        assert_eq!(parse_resolution("高清 1080P"), Some("1080p".to_string()));
        assert_eq!(parse_resolution("流畅 360P"), Some("360p".to_string()));
        assert_eq!(parse_resolution("unknown"), None);
    }

    #[test]
    fn test_parse_codec() {
        assert_eq!(
            parse_codec("video/webm; codecs=\"vp9\""),
            Some("vp9".to_string())
        );
        assert_eq!(
            parse_codec("video/mp4; codecs=\"avc1.640028\""),
            Some("h264".to_string())
        );
        assert_eq!(
            parse_codec("video/mp4; codecs=\"av01.0.05M.08\""),
            Some("av1".to_string())
        );
    }

    #[test]
    fn test_extract_site_id() {
        assert_eq!(extract_site_id("YouTube youtube.com"), "youtube");
        assert_eq!(extract_site_id("哔哩哔哩 bilibili.com"), "bilibili");
        assert_eq!(extract_site_id("抖音 douyin.com"), "douyin");
    }
}
