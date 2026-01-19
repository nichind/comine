mod lux;
mod ytdlp;

use crate::deps;
use crate::job_engine::JobRegistry;
use crate::proxy::ProxyConfig;
use crate::types::{PlaylistInfo, VideoFormats, VideoInfo};
use async_trait::async_trait;
use tauri::AppHandle;
use tauri::Window;

pub use lux::LuxBackend;
pub use ytdlp::YtDlpBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendHint {
    #[default]
    Auto,
    YtDlp,
    Lux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum BackendKind {
    #[default]
    YtDlp,
    Lux,
}

impl BackendKind {
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "lux" => BackendKind::Lux,
            _ => BackendKind::YtDlp,
        }
    }
}

fn is_ytdlp_installed(app: &AppHandle) -> bool {
    deps::get_ytdlp_path(app)
        .map(|p| p.exists())
        .unwrap_or(false)
}

fn is_lux_installed(app: &AppHandle) -> bool {
    deps::get_lux_path(app)
        .map(|p| p.exists())
        .unwrap_or(false)
}

pub fn resolve_backend_kind(app: &AppHandle, hint: Option<BackendHint>) -> Result<BackendKind, String> {
    let hint = hint.unwrap_or(BackendHint::Auto);
    let ytdlp_ok = is_ytdlp_installed(app);
    let lux_ok = is_lux_installed(app);

    match hint {
        BackendHint::YtDlp => {
            if ytdlp_ok {
                Ok(BackendKind::YtDlp)
            } else if lux_ok {
                Ok(BackendKind::Lux)
            } else {
                Err("No backend installed (yt-dlp or lux)".to_string())
            }
        }
        BackendHint::Lux => {
            if lux_ok {
                Ok(BackendKind::Lux)
            } else if ytdlp_ok {
                Ok(BackendKind::YtDlp)
            } else {
                Err("No backend installed (yt-dlp or lux)".to_string())
            }
        }
        BackendHint::Auto => {
            // Prefer yt-dlp when available; it is the most feature-complete.
            if ytdlp_ok {
                Ok(BackendKind::YtDlp)
            } else if lux_ok {
                Ok(BackendKind::Lux)
            } else {
                Err("No backend installed (yt-dlp or lux)".to_string())
            }
        }
    }
}

fn backend_name(kind: BackendKind) -> &'static str {
    match kind {
        BackendKind::YtDlp => "yt-dlp",
        BackendKind::Lux => "lux",
    }
}

fn resolve_backend_candidates(
    app: &AppHandle,
    hint: Option<BackendHint>,
) -> Result<Vec<BackendKind>, String> {
    let hint = hint.unwrap_or(BackendHint::Auto);
    let ytdlp_ok = is_ytdlp_installed(app);
    let lux_ok = is_lux_installed(app);

    let ordered: [BackendKind; 2] = match hint {
        // Auto and "prefer yt-dlp" both try yt-dlp first, then lux.
        BackendHint::Auto | BackendHint::YtDlp => [BackendKind::YtDlp, BackendKind::Lux],
        BackendHint::Lux => [BackendKind::Lux, BackendKind::YtDlp],
    };

    let mut out: Vec<BackendKind> = Vec::with_capacity(2);
    for k in ordered {
        match k {
            BackendKind::YtDlp if ytdlp_ok => out.push(k),
            BackendKind::Lux if lux_ok => out.push(k),
            _ => {}
        }
    }

    if out.is_empty() {
        Err("No backend installed (yt-dlp or lux)".to_string())
    } else {
        Ok(out)
    }
}

fn format_backend_attempt_errors(errors: Vec<(BackendKind, String)>) -> String {
    let mut msg = String::from("All backends failed.\n");
    for (kind, err) in errors {
        msg.push_str(&format!("- {}: {}\n", backend_name(kind), err.trim()));
    }
    msg.trim_end().to_string()
}

fn backend_for_kind(kind: BackendKind) -> Box<dyn Backend> {
    match kind {
        BackendKind::YtDlp => Box::new(YtDlpBackend),
        BackendKind::Lux => Box::new(LuxBackend),
    }
}

fn download_backend_for_kind(kind: BackendKind) -> Box<dyn DownloadBackend> {
    match kind {
        BackendKind::YtDlp => Box::new(YtDlpBackend),
        BackendKind::Lux => Box::new(LuxBackend),
    }
}

#[derive(Debug, Clone)]
pub struct InfoRequest {
    pub url: String,
    pub cookies_from_browser: Option<String>,
    pub custom_cookies: Option<String>,
    pub proxy_config: Option<ProxyConfig>,
    pub youtube_player_client: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlaylistRequest {
    pub url: String,
    pub offset: usize,
    pub limit: usize,
    pub cookies_from_browser: Option<String>,
    pub custom_cookies: Option<String>,
    pub proxy_config: Option<ProxyConfig>,
    pub youtube_player_client: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub url: String,

    // Common-ish download controls
    pub video_quality: Option<String>,
    pub download_mode: Option<String>,
    pub audio_quality: Option<String>,
    pub convert_to_mp4: Option<bool>,
    pub remux: Option<bool>,
    pub clear_metadata: Option<bool>,
    pub use_aria2: Option<bool>,
    pub aria2_connections: Option<u32>,
    pub aria2_splits: Option<u32>,
    pub aria2_min_split_size: Option<String>,
    pub aria2_disable_ipv6: Option<bool>,
    pub aria2_custom_args: Option<String>,
    pub no_playlist: Option<bool>,
    pub cookies_from_browser: Option<String>,
    pub custom_cookies: Option<String>,
    pub download_path: Option<String>,
    pub embed_thumbnail: Option<bool>,
    pub thumbnail_url_for_embed: Option<String>,
    pub playlist_title: Option<String>,
    pub proxy_config: Option<ProxyConfig>,
    pub sponsor_block: Option<bool>,
    pub sponsor_block_skip_sponsors: Option<bool>,
    pub sponsor_block_skip_intros: Option<bool>,
    pub sponsor_block_skip_self_promo: Option<bool>,
    pub sponsor_block_skip_interaction: Option<bool>,
    pub chapters: Option<bool>,
    pub embed_subtitles: Option<bool>,
    pub subtitle_languages: Option<String>,
    pub download_speed_limit: Option<u64>,
    pub youtube_player_client: Option<String>,
    pub concurrent_fragments: Option<u32>,
    pub retries: Option<u32>,
    pub fragment_retries: Option<u32>,
    pub download_custom_args: Option<String>,
    pub post_process_custom_args: Option<String>,
    pub keep_original: Option<bool>,
    pub output_template: Option<String>,
    pub restrict_filenames: Option<bool>,
    pub windows_filenames: Option<bool>,
    pub clip_ranges: Option<Vec<crate::types::ClipRange>>,

    // Lux-only knobs (ignored by yt-dlp backend)
    pub multi_thread: Option<bool>,
    pub thread_count: Option<u32>,
}

#[async_trait]
pub trait Backend: Send + Sync {
    async fn get_video_info(
        &self,
        app: &AppHandle,
        request: InfoRequest,
    ) -> Result<VideoInfo, String>;

    async fn get_playlist_info(
        &self,
        app: &AppHandle,
        request: PlaylistRequest,
    ) -> Result<PlaylistInfo, String>;

    async fn get_video_formats(
        &self,
        app: &AppHandle,
        request: InfoRequest,
    ) -> Result<VideoFormats, String>;
}

#[async_trait]
pub trait DownloadBackend: Send + Sync {
    async fn download_job(
        &self,
        app: &AppHandle,
        window: Window,
        registry: JobRegistry,
        request: DownloadRequest,
    ) -> Result<String, String>;
}

pub async fn get_video_info_auto(
    app: &AppHandle,
    hint: Option<BackendHint>,
    request: InfoRequest,
) -> Result<VideoInfo, String> {
    let candidates = resolve_backend_candidates(app, hint)?;
    let mut errors: Vec<(BackendKind, String)> = Vec::new();

    for kind in candidates {
        match backend_for_kind(kind)
            .get_video_info(app, request.clone())
            .await
        {
            Ok(info) => return Ok(info),
            Err(e) => errors.push((kind, e)),
        }
    }

    Err(format_backend_attempt_errors(errors))
}

pub async fn get_playlist_info_auto(
    app: &AppHandle,
    hint: Option<BackendHint>,
    request: PlaylistRequest,
) -> Result<PlaylistInfo, String> {
    let candidates = resolve_backend_candidates(app, hint)?;
    let mut errors: Vec<(BackendKind, String)> = Vec::new();

    for kind in candidates {
        match backend_for_kind(kind)
            .get_playlist_info(app, request.clone())
            .await
        {
            Ok(info) => return Ok(info),
            Err(e) => errors.push((kind, e)),
        }
    }

    Err(format_backend_attempt_errors(errors))
}

pub async fn get_video_formats_auto(
    app: &AppHandle,
    hint: Option<BackendHint>,
    request: InfoRequest,
) -> Result<VideoFormats, String> {
    let candidates = resolve_backend_candidates(app, hint)?;
    let mut errors: Vec<(BackendKind, String)> = Vec::new();

    for kind in candidates {
        match backend_for_kind(kind)
            .get_video_formats(app, request.clone())
            .await
        {
            Ok(info) => return Ok(info),
            Err(e) => errors.push((kind, e)),
        }
    }

    Err(format_backend_attempt_errors(errors))
}

fn info_request_from_download_request(req: &DownloadRequest) -> InfoRequest {
    InfoRequest {
        url: req.url.clone(),
        cookies_from_browser: req.cookies_from_browser.clone(),
        custom_cookies: req.custom_cookies.clone(),
        proxy_config: req.proxy_config.clone(),
        youtube_player_client: req.youtube_player_client.clone(),
    }
}

pub async fn download_video_auto(
    app: &AppHandle,
    hint: Option<BackendHint>,
    window: Window,
    registry: JobRegistry,
    request: DownloadRequest,
) -> Result<String, String> {
    let candidates = resolve_backend_candidates(app, hint)?;

    // Fast path: only one backend available.
    if candidates.len() == 1 {
        return download_backend_for_kind(candidates[0])
            .download_job(app, window, registry, request)
            .await;
    }

    // When multiple backends are installed, probe extractability first to avoid spawning a
    // download job that is very likely to fail for an unsupported URL.
    let probe_req = info_request_from_download_request(&request);
    let mut probe_errors: Vec<(BackendKind, String)> = Vec::new();

    for kind in candidates {
        match backend_for_kind(kind)
            .get_video_info(app, probe_req.clone())
            .await
        {
            Ok(_) => {
                return download_backend_for_kind(kind)
                    .download_job(app, window, registry, request)
                    .await;
            }
            Err(e) => probe_errors.push((kind, e)),
        }
    }

    Err(format!(
        "No backend could extract this URL for download.\n{}",
        format_backend_attempt_errors(probe_errors)
    ))
}
