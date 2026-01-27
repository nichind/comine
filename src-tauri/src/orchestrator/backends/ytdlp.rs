//! yt-dlp backend (desktop child process / Android JNI).

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use log::{debug, info, warn};

use crate::orchestrator::backends::{
    is_known_video_site, Backend, BackendCapabilities, SpawnContext,
};
use crate::orchestrator::types::*;

#[cfg(not(target_os = "android"))]
use std::path::PathBuf;
#[cfg(not(target_os = "android"))]
use std::process::Stdio;
#[cfg(not(target_os = "android"))]
use tauri::AppHandle;
#[cfg(not(target_os = "android"))]
use tokio::io::{AsyncBufReadExt, BufReader};
#[cfg(not(target_os = "android"))]
use tokio::process::{Child, Command};

#[cfg(not(target_os = "android"))]
fn resolve_effective_proxy(proxy: &Option<ProxyConfig>) -> Option<String> {
    if let Some(p) = proxy {
        if p.enabled {
            if let Some(ref url) = p.url {
                if !url.is_empty() {
                    return Some(url.clone());
                }
            }
            let system_proxy = crate::proxy::detect_system_proxy();
            if !system_proxy.url.is_empty() {
                info!(
                    "Using detected system proxy for yt-dlp: {}",
                    system_proxy.url
                );
                return Some(system_proxy.url);
            }
        }
    }
    None
}

#[cfg(not(target_os = "android"))]
fn apply_site_headers(url: &str, cmd: &mut Command) {
    cmd.args([
        "--user-agent",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    ]);

    let is_bilibili =
        url.contains("bilibili.com") || url.contains("b23.tv") || url.contains("bilivideo.com");
    if is_bilibili {
        cmd.args([
            "--referer",
            "https://www.bilibili.com/",
            "--add-header",
            "Origin:https://www.bilibili.com",
            "--add-header",
            "Accept-Language:zh-CN,zh;q=0.9,en-US;q=0.8,en;q=0.7",
        ]);
    }
}

#[cfg(not(target_os = "android"))]
fn normalize_url_for_ytdlp(url: &str) -> String {
    use url::Url;

    let Ok(mut parsed) = Url::parse(url) else {
        return url.to_string();
    };

    let host = parsed.host_str().unwrap_or("").to_lowercase();

    // Vimeo unlisted links: prefer the player endpoint with the `h=` token.
    let is_vimeo = host == "vimeo.com" || host.ends_with(".vimeo.com");
    if is_vimeo {
        let has_h = parsed
            .query_pairs()
            .any(|(k, _)| k.eq_ignore_ascii_case("h"));

        if !has_h {
            let segments: Vec<String> = parsed
                .path_segments()
                .map(|s| s.map(|p| p.to_string()).collect())
                .unwrap_or_default();

            // https://vimeo.com/<id>/<hash>
            if host == "vimeo.com"
                && segments.len() >= 2
                && segments[0].chars().all(|c| c.is_ascii_digit())
                && !segments[1].is_empty()
                && segments[1].chars().all(|c| c.is_ascii_hexdigit())
            {
                let id = &segments[0];
                let hash = &segments[1];
                let Ok(mut player) = Url::parse(&format!("https://player.vimeo.com/video/{}", id))
                else {
                    return url.to_string();
                };

                let mut qp: Vec<(String, String)> = parsed
                    .query_pairs()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                qp.push(("h".to_string(), hash.to_string()));

                {
                    let mut pairs = player.query_pairs_mut();
                    for (k, v) in qp {
                        pairs.append_pair(&k, &v);
                    }
                }

                return player.to_string();
            }

            // https://player.vimeo.com/video/<id>/<hash>
            if host == "player.vimeo.com"
                && segments.len() >= 3
                && segments[0] == "video"
                && segments[1].chars().all(|c| c.is_ascii_digit())
                && !segments[2].is_empty()
                && segments[2].chars().all(|c| c.is_ascii_hexdigit())
            {
                let id = &segments[1];
                let hash = &segments[2];
                parsed.set_path(&format!("/video/{}", id));
                let mut qp: Vec<(String, String)> = parsed
                    .query_pairs()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                qp.push(("h".to_string(), hash.to_string()));
                parsed.set_query(None);
                {
                    let mut pairs = parsed.query_pairs_mut();
                    for (k, v) in qp {
                        pairs.append_pair(&k, &v);
                    }
                }
                return parsed.to_string();
            }
        }
    }

    url.to_string()
}

#[cfg(target_os = "android")]
pub enum AndroidJobResult {
    Completed {
        output_path: String,
        title: Option<String>,
    },
    Failed(String), // error message
    Cancelled,
}

#[cfg(target_os = "android")]
lazy_static::lazy_static! {
    pub static ref PENDING_ANDROID_JOBS: Mutex<HashMap<String, tokio::sync::oneshot::Sender<AndroidJobResult>>> =
        Mutex::new(HashMap::new());
}

#[cfg(target_os = "android")]
pub fn register_pending_job(job_id: &str) -> tokio::sync::oneshot::Receiver<AndroidJobResult> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    if let Ok(mut pending) = PENDING_ANDROID_JOBS.lock() {
        pending.insert(job_id.to_string(), tx);
    }
    rx
}

#[cfg(target_os = "android")]
pub fn signal_job_completed(job_id: &str, output_path: String, title: Option<String>) {
    if let Ok(mut pending) = PENDING_ANDROID_JOBS.lock() {
        if let Some(tx) = pending.remove(job_id) {
            let _ = tx.send(AndroidJobResult::Completed { output_path, title });
        }
    }
}

#[cfg(target_os = "android")]
pub fn signal_job_failed(job_id: &str, error: String) {
    if let Ok(mut pending) = PENDING_ANDROID_JOBS.lock() {
        if let Some(tx) = pending.remove(job_id) {
            let _ = tx.send(AndroidJobResult::Failed(error));
        }
    }
}

#[cfg(target_os = "android")]
pub fn signal_job_cancelled(job_id: &str) {
    if let Ok(mut pending) = PENDING_ANDROID_JOBS.lock() {
        if let Some(tx) = pending.remove(job_id) {
            let _ = tx.send(AndroidJobResult::Cancelled);
        }
    }
}

#[cfg(target_os = "android")]
use std::sync::{Arc, OnceLock};

#[cfg(target_os = "android")]
use jni::{
    objects::{GlobalRef, JClass, JObject, JString, JValue},
    sys::jlong,
    JNIEnv, JavaVM,
};

#[cfg(target_os = "android")]
static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

#[cfg(target_os = "android")]
static MAIN_ACTIVITY: OnceLock<GlobalRef> = OnceLock::new();

// Cached on the main thread to avoid ClassLoader issues from background threads.
#[cfg(target_os = "android")]
static YTDLP_CLASS: OnceLock<GlobalRef> = OnceLock::new();

#[cfg(target_os = "android")]
static JOB_MANAGER: OnceLock<Arc<crate::orchestrator::manager::JobManager>> = OnceLock::new();

#[cfg(target_os = "android")]
pub fn init_android(_app: tauri::AppHandle) {
    log::info!("yt-dlp Android backend init - waiting for Kotlin to call initRustJni");
}

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
pub fn init_android(_app: tauri::AppHandle) {
    // No-op on non-Android
}

#[cfg(target_os = "android")]
pub fn set_job_manager(manager: Arc<crate::orchestrator::manager::JobManager>) {
    let _ = JOB_MANAGER.set(manager);
}

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
pub fn set_job_manager(_manager: std::sync::Arc<crate::orchestrator::manager::JobManager>) {
    // No-op on non-Android
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_initRustJniWithActivity<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    activity: JObject<'local>,
) {
    // Store JavaVM for thread attachment
    if let Ok(vm) = env.get_java_vm() {
        if JAVA_VM.set(vm).is_ok() {
            log::info!("JavaVM stored for JNI calls");
        }
    } else {
        log::error!("Failed to get JavaVM");
        return;
    }

    // Cache class refs on the main thread; background find_class can use the wrong ClassLoader.
    match env.find_class("com/nichind/comine/YtDlp") {
        Ok(ytdlp_class) => match env.new_global_ref(ytdlp_class) {
            Ok(global_ref) => {
                if YTDLP_CLASS.set(global_ref).is_ok() {
                    log::info!("YtDlp class cached for JNI calls");
                }
            }
            Err(e) => {
                log::error!("Failed to create global reference for YtDlp class: {}", e);
            }
        },
        Err(e) => {
            log::error!("Failed to find YtDlp class: {}", e);
        }
    }

    match env.new_global_ref(activity) {
        Ok(global_ref) => {
            if MAIN_ACTIVITY.set(global_ref).is_ok() {
                log::info!("MainActivity global reference stored - JNI bridge ready");
            }
        }
        Err(e) => {
            log::error!("Failed to create global reference for MainActivity: {}", e);
        }
    }
}

// Try to terminate in a way that preserves partial downloads.
#[cfg(not(target_os = "android"))]
async fn graceful_shutdown(child: &mut Child) {
    use std::time::Duration;

    let pid = match child.id() {
        Some(id) => id,
        None => {
            return;
        }
    };

    #[cfg(unix)]
    {
        let sigint_result = unsafe { libc::kill(pid as i32, libc::SIGINT) };

        if sigint_result == 0 {
            info!(target: "ytdlp", "Sent SIGINT to yt-dlp process {}, waiting for graceful exit...", pid);

            for _ in 0..30 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if child.try_wait().ok().flatten().is_some() {
                    info!(target: "ytdlp", "yt-dlp process {} exited gracefully", pid);
                    return;
                }
            }

            warn!(target: "ytdlp", "yt-dlp process {} didn't exit after SIGINT, sending SIGTERM", pid);
            unsafe { libc::kill(pid as i32, libc::SIGTERM) };

            for _ in 0..10 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if child.try_wait().ok().flatten().is_some() {
                    info!(target: "ytdlp", "yt-dlp process {} exited after SIGTERM", pid);
                    return;
                }
            }
        }

        warn!(target: "ytdlp", "Force killing yt-dlp process {} after graceful shutdown timeout", pid);
        let _ = child.kill().await;
    }

    #[cfg(windows)]
    {
        let taskkill = tokio::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T"])
            .output()
            .await;

        if taskkill.is_ok() {
            info!(target: "ytdlp", "Sent taskkill to yt-dlp process {}, waiting for exit...", pid);

            for _ in 0..20 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if child.try_wait().ok().flatten().is_some() {
                    info!(target: "ytdlp", "yt-dlp process {} exited gracefully", pid);
                    return;
                }
            }
        }

        warn!(target: "ytdlp", "Force killing yt-dlp process {} after graceful shutdown timeout", pid);
        let _ = child.kill().await;
    }
}

pub struct YtdlpBackend {
    #[cfg(not(target_os = "android"))]
    app: AppHandle,
    #[cfg(not(target_os = "android"))]
    binary_path: PathBuf,
}

impl YtdlpBackend {
    #[cfg(not(target_os = "android"))]
    pub fn new(app: AppHandle, binary_path: PathBuf) -> Self {
        Self { app, binary_path }
    }

    #[cfg(target_os = "android")]
    pub fn new_android() -> Self {
        Self {}
    }

    #[cfg(not(target_os = "android"))]
    fn build_resolve_command(&self, url: &str, settings: &ResolveSettings) -> Command {
        let mut cmd = Command::new(&self.binary_path);

        #[cfg(target_os = "windows")]
        cmd.env("PYTHONIOENCODING", "utf-8");
        cmd.arg("--encoding").arg("utf-8");

        cmd.args([
            "--dump-json",
            "--no-download",
            "--no-warnings",
            "--ignore-errors",
        ]);

        if settings.flat_playlist {
            cmd.arg("--flat-playlist");
        } else {
            cmd.arg("--no-playlist");
        }

        if let Some(ref cookies) = settings.custom_cookies {
            if !cookies.is_empty() {
                cmd.args(["--cookies", cookies]);
            }
        }

        // Resolve proxy (handles both explicit and system proxy)
        if let Some(proxy_url) = resolve_effective_proxy(&settings.proxy) {
            cmd.args(["--proxy", &proxy_url]);
        }

        if let Some(ref client) = settings.youtube_player_client {
            cmd.args([
                "--extractor-args",
                &format!("youtube:player_client={}", client),
            ]);
        }

        // Apply site-specific headers for anti-detection
        let normalized_url = normalize_url_for_ytdlp(url);
        apply_site_headers(&normalized_url, &mut cmd);

        cmd.arg(&normalized_url);
        cmd
    }

    #[cfg(not(target_os = "android"))]
    fn build_download_command(&self, job: &Job, effective_speed_limit: Option<u64>) -> Command {
        let mut cmd = Command::new(&self.binary_path);

        #[cfg(target_os = "windows")]
        cmd.env("PYTHONIOENCODING", "utf-8");
        cmd.arg("--encoding").arg("utf-8");

        let req = &job.request;
        let opts = &req.options;

        let template = req
            .output
            .filename_template
            .as_deref()
            .unwrap_or("%(title)s.%(ext)s");
        let output_path = format!("{}/{}", req.output.directory, template);
        cmd.args(["-o", &output_path]);

        if req.quality.audio_only {
            cmd.arg("-x");
            cmd.args(["-f", &req.quality.format]); // Use the format filter for audio codec preference
            if let Some(ref fmt) = req.quality.audio_format {
                cmd.args(["--audio-format", fmt]);
            }
        } else {
            cmd.args(["-f", &req.quality.format]);
            if let Some(height) = req.quality.max_height {
                cmd.args(["-S", &format!("res:{}", height)]);
            }
        }

        // For audio-only downloads, we handle thumbnail embedding ourselves (with letterbox cropping)
        // so don't let yt-dlp embed the uncropped thumbnail
        if opts.embed_thumbnail && !req.quality.audio_only {
            cmd.arg("--embed-thumbnail");
        }
        if opts.embed_metadata {
            cmd.arg("--embed-metadata");
        }
        if opts.embed_subtitles {
            cmd.arg("--embed-subs");
            if let Some(ref langs) = opts.subtitle_langs {
                cmd.args(["--sub-langs", langs]);
            }
        }

        if let Some(ref categories) = opts.sponsorblock_remove {
            cmd.args(["--sponsorblock-remove", categories]);
        }

        if let Some(ref cookies) = opts.custom_cookies {
            if !cookies.is_empty() {
                cmd.args(["--cookies", cookies]);
            }
        }

        if let Some(ref ranges) = opts.clip_ranges {
            for range in ranges {
                cmd.args([
                    "--download-sections",
                    &format!("*{:.2}-{:.2}", range.start, range.end),
                ]);
            }
            cmd.arg("--force-keyframes-at-cuts");
        }

        // Resolve proxy (handles both explicit and system proxy)
        if let Some(proxy_url) = resolve_effective_proxy(&opts.proxy) {
            cmd.args(["--proxy", &proxy_url]);
        }

        if let Some(limit) = effective_speed_limit {
            if limit > 0 {
                cmd.args(["--limit-rate", &format!("{}K", limit / 1024)]);
            }
        }

        cmd.arg("--continue");
        cmd.arg("--progress");
        cmd.arg("--newline");

        cmd.args([
            "--progress-template",
            r#"download:__COMINE_PROGRESS__{downloaded:%(progress.downloaded_bytes)s,total:%(progress.total_bytes)s,total_estimate:%(progress.total_bytes_estimate)s,speed:%(progress.speed)s,eta:%(progress.eta)s,filename:%(progress.filename)s}__COMINE_PROGRESS__"#,
            // Print title and thumbnail early (before download starts)
            "--print",
            "pre_process:>>>TITLE:%(title)s",
            "--print",
            "pre_process:>>>THUMBNAIL:%(thumbnail)s",
            "--print",
            "after_move:>>>FILEPATH:%(filepath)s",
        ]);

        // Apply site-specific headers for anti-detection
        let normalized_url = normalize_url_for_ytdlp(&req.url);
        apply_site_headers(&normalized_url, &mut cmd);

        cmd.arg(&normalized_url);
        cmd
    }

    #[cfg(not(target_os = "android"))]
    async fn resolve_desktop(
        &self,
        url: &str,
        settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        let output = self
            .build_resolve_command(url, settings)
            .output()
            .await
            .map_err(|e| BackendError::ProcessError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(parse_ytdlp_error(&stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_ytdlp_output(&stdout, url)
    }

    #[cfg(not(target_os = "android"))]
    async fn spawn_desktop(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        info!(target: "ytdlp", "Starting download job {} for URL: {}", ctx.job.id, ctx.job.request.url);
        info!(target: "ytdlp", "Quality settings: format={}, max_height={:?}, audio_only={}", 
              ctx.job.request.quality.format, ctx.job.request.quality.max_height, ctx.job.request.quality.audio_only);

        let resolved_thumb =
            if ctx.job.request.quality.audio_only && ctx.job.request.options.embed_thumbnail {
                let settings = ResolveSettings::default();
                self.resolve_desktop(&ctx.job.request.url, &settings)
                    .await
                    .ok()
                    .and_then(|info| info.thumbnail)
            } else {
                None
            };

        let mut cmd = self.build_download_command(&ctx.job, ctx.effective_speed_limit);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| BackendError::ProcessError(e.to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BackendError::ProcessError("Failed to capture stdout".to_string()))?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| BackendError::ProcessError("Failed to capture stderr".to_string()))?;

        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                warn!(target: "ytdlp", "ERR: {}", line);
            }
        });

        let mut buffer = Vec::new();
        let mut reader = BufReader::new(stdout);
        let mut captured_output_path: Option<String> = None;
        let mut _captured_title: Option<String> = None;
        let mut _captured_thumbnail: Option<String> = None;

        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    graceful_shutdown(&mut child).await;
                    cleanup_progress_tracker(&ctx.job.id);
                    return Err(BackendError::Cancelled);
                }
                result = async {
                    use tokio::io::AsyncBufReadExt;
                    buffer.clear();
                    reader.read_until(b'\n', &mut buffer).await
                } => {
                    match result {
                        Ok(0) => break,
                        Ok(_) => {
                            let line = String::from_utf8_lossy(&buffer).trim().to_string();
                            if line.is_empty() {
                                continue;
                            }

                            if !line.contains("__COMINE_PROGRESS__") {
                                info!(target: "ytdlp", "STD: {}", line);
                            } else {
                                debug!(target: "ytdlp", "RAW progress line: {}", line);
                                if parse_progress_line(&line, &ctx.job.id).is_none() {
                                    warn!(target: "ytdlp", "Failed to parse progress: {}", line);
                                }
                            }

                            if let Some(update) = parse_progress_line(&line, &ctx.job.id) {
                                let _ = ctx.progress_tx.send(update);
                            }
                            if let Some(path) = parse_filepath_line(&line) {
                                captured_output_path = Some(path);
                            }
                            // Parse title and thumbnail from yt-dlp --print output
                            if let Some(title) = parse_title_line(&line) {
                                if !title.is_empty() && title != "NA" {
                                    _captured_title = Some(title.clone());
                                    // Update job title immediately so UI can show it
                                    use tauri::Manager;
                                    let manager = self.app.state::<std::sync::Arc<crate::orchestrator::manager::JobManager>>();
                                    manager.update_job_metadata(&ctx.job.id, Some(&title), None);
                                }
                            }
                            if let Some(thumb) = parse_thumbnail_line(&line) {
                                if !thumb.is_empty() && thumb != "NA" {
                                    _captured_thumbnail = Some(thumb.clone());
                                    use tauri::Manager;
                                    let manager = self.app.state::<std::sync::Arc<crate::orchestrator::manager::JobManager>>();
                                    manager.update_job_metadata(&ctx.job.id, None, Some(&thumb));
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Error reading yt-dlp output: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        cleanup_progress_tracker(&ctx.job.id);

        let status = child
            .wait()
            .await
            .map_err(|e| BackendError::ProcessError(e.to_string()))?;

        if !status.success() {
            cleanup_progress_tracker(&ctx.job.id);
            return Err(BackendError::ProcessError(format!(
                "yt-dlp exited with code {:?}",
                status.code()
            )));
        }

        let output_path = captured_output_path.unwrap_or_else(|| {
            format!(
                "{}/{}",
                ctx.job.request.output.directory,
                ctx.job
                    .request
                    .output
                    .filename_template
                    .as_deref()
                    .unwrap_or("%(title)s.%(ext)s")
            )
        });

        // For audio-only downloads with thumbnail embedding, we handle it ourselves
        // (with letterbox cropping) since yt-dlp doesn't do cropping
        let final_output_path =
            if ctx.job.request.quality.audio_only && ctx.job.request.options.embed_thumbnail {
                if let Some(thumbnail_url) = resolved_thumb.as_deref() {
                    match crate::orchestrator::thumbnail::embed_thumbnail(
                        &self.app,
                        &output_path,
                        thumbnail_url,
                    )
                    .await
                    {
                        Ok(new_path) => new_path,
                        Err(e) => {
                            warn!("Thumbnail embedding failed: {}", e);
                            output_path.clone()
                        }
                    }
                } else {
                    output_path.clone()
                }
            } else {
                output_path.clone()
            };

        Ok(final_output_path)
    }

    #[cfg(target_os = "android")]
    async fn resolve_android(
        &self,
        url: &str,
        settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        use jni::objects::JValue;

        if !wait_for_jni_ready(10000).await {
            return Err(BackendError::Other(
                "Android JNI bridge not ready. Please wait for the app to fully initialize."
                    .to_string(),
            ));
        }

        let output = tokio::task::spawn_blocking({
            let url = url.to_string();
            let flat_playlist = settings.flat_playlist;
            let player_client = settings.youtube_player_client.clone();

            move || -> Result<String, String> {
                let mut env = get_jni_env()?;

                // Use cached YtDlp class reference (find_class doesn't work from background threads)
                let ytdlp_class = get_ytdlp_class()?;

                let j_url = env.new_string(&url)
                    .map_err(|e| format!("Failed to create url string: {}", e))?;
                let j_player_client = match &player_client {
                    Some(pc) => env.new_string(pc)
                        .map_err(|e| format!("Failed to create player_client string: {}", e))?,
                    None => env.new_string("")
                        .map_err(|e| format!("Failed to create empty string: {}", e))?,
                };

                // Call YtDlp.resolve(url, flatPlaylist, youtubePlayerClient)
                let result = env.call_static_method(
                    <&JClass>::from(ytdlp_class.as_obj()),
                    "resolve",
                    "(Ljava/lang/String;ZLjava/lang/String;)Lcom/nichind/comine/YtDlp$ResolveResult;",
                    &[
                        JValue::Object(&j_url),
                        JValue::Bool(flat_playlist as u8),
                        JValue::Object(&j_player_client),
                    ],
                ).map_err(|e| format!("JNI call failed: {}", e))?;

                let result_obj = result.l()
                    .map_err(|e| format!("Failed to get result object: {}", e))?;

                // Detect success by method availability (inner classes not cached).
                let has_output = env.call_method(&result_obj, "getOutput", "()Ljava/lang/String;", &[]);

                if let Ok(output_val) = has_output {
                    if let Ok(output_obj) = output_val.l() {
                        if !output_obj.is_null() {
                            let output_str: String = env.get_string((&output_obj).into())
                                .map_err(|e| format!("Failed to convert output: {}", e))?
                                .into();
                            return Ok(output_str);
                        }
                    }
                }

                // Clear any pending exception from the failed getOutput() call
                // (occurs when result is ResolveResult.Failed which doesn't have getOutput)
                let _ = env.exception_clear();

                // Must be Failed - get the error
                let error = env.call_method(&result_obj, "getError", "()Ljava/lang/String;", &[])
                    .map_err(|e| format!("Failed to get error: {}", e))?
                    .l()
                    .map_err(|e| format!("Failed to get error string: {}", e))?;
                let error_str: String = env.get_string((&error).into())
                    .map_err(|e| format!("Failed to convert error: {}", e))?
                    .into();
                Err(error_str)
            }
        })
        .await
        .map_err(|e| BackendError::Other(format!("JNI task failed: {}", e)))?
        .map_err(|e| BackendError::Other(e))?;

        // Parse the output using shared parsing logic
        parse_ytdlp_output(&output, url)
    }

    #[cfg(target_os = "android")]
    async fn spawn_android(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        // Wait for JNI to be ready (max 10 seconds)
        if !wait_for_jni_ready(10000).await {
            return Err(BackendError::Other(
                "Android JNI bridge not ready. Please wait for the app to fully initialize."
                    .to_string(),
            ));
        }

        let job_id = ctx.job.id.clone();
        let request_json = serde_json::to_string(&ctx.job.request)
            .map_err(|e| BackendError::Other(format!("Failed to serialize request: {}", e)))?;

        // Register this job to receive completion signal from JNI callback
        let rx = register_pending_job(&job_id);

        // Make JNI call in a blocking context to avoid Send issues with JObject
        let job_id_clone = job_id.clone();
        let jni_result = tokio::task::spawn_blocking(move || {
            let mut env = get_jni_env()?;
            let activity = get_activity()?;

            let j_job_id = env
                .new_string(&job_id_clone)
                .map_err(|e| format!("Failed to create job_id string: {}", e))?;
            let j_request_json = env
                .new_string(&request_json)
                .map_err(|e| format!("Failed to create request_json string: {}", e))?;

            env.call_method(
                activity.as_obj(),
                "startDownloadFromRust",
                "(Ljava/lang/String;Ljava/lang/String;)V",
                &[JValue::Object(&j_job_id), JValue::Object(&j_request_json)],
            )
            .map_err(|e| format!("JNI call failed: {}", e))?;

            Ok::<(), String>(())
        })
        .await
        .map_err(|e| BackendError::Other(format!("JNI task panicked: {}", e)))?
        .map_err(BackendError::Other);

        // If JNI call failed, clean up pending job and return error
        if let Err(e) = jni_result {
            let mut pending = PENDING_ANDROID_JOBS.lock().unwrap();
            pending.remove(&job_id);
            return Err(e);
        }

        log::info!(
            "Started Android download via JNI for job {}, waiting for completion...",
            job_id
        );

        // Wait for the JNI callback to signal completion
        match rx.await {
            Ok(AndroidJobResult::Completed { output_path, title }) => {
                log::info!(
                    "Android download completed for job {}: {}, title={:?}",
                    job_id,
                    output_path,
                    title
                );

                // Update job title if we got one from the completion callback
                if let Some(ref t) = title {
                    if let Some(manager) = JOB_MANAGER.get() {
                        manager.update_job_title(&job_id, t);
                    }
                }

                Ok(output_path)
            }
            Ok(AndroidJobResult::Failed(error)) => {
                log::error!("Android download failed for job {}: {}", job_id, error);
                Err(BackendError::Other(error))
            }
            Ok(AndroidJobResult::Cancelled) => {
                log::info!("Android download cancelled for job {}", job_id);
                Err(BackendError::Cancelled)
            }
            Err(_) => {
                log::error!(
                    "Android download channel closed unexpectedly for job {}",
                    job_id
                );
                Err(BackendError::Other(
                    "Download completion channel closed unexpectedly".to_string(),
                ))
            }
        }
    }
}

#[async_trait]
impl Backend for YtdlpBackend {
    fn name(&self) -> &str {
        "ytdlp"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_pause: true,
            supports_resume: true,
            supports_progress: true,
            supports_speed_limit: true,
        }
    }

    fn priority(&self, url: &str) -> Priority {
        if is_known_video_site(url) {
            Priority::High
        } else if url.starts_with("http://") || url.starts_with("https://") {
            Priority::Medium
        } else {
            Priority::None
        }
    }

    async fn resolve(
        &self,
        url: &str,
        settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        #[cfg(not(target_os = "android"))]
        {
            self.resolve_desktop(url, settings).await
        }
        #[cfg(target_os = "android")]
        {
            self.resolve_android(url, settings).await
        }
    }

    async fn spawn(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        #[cfg(not(target_os = "android"))]
        {
            self.spawn_desktop(ctx).await
        }
        #[cfg(target_os = "android")]
        {
            self.spawn_android(ctx).await
        }
    }
}

#[cfg(target_os = "android")]
pub fn is_jni_ready() -> bool {
    JAVA_VM.get().is_some() && MAIN_ACTIVITY.get().is_some() && YTDLP_CLASS.get().is_some()
}

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
pub fn is_jni_ready() -> bool {
    true
}

#[cfg(target_os = "android")]
pub async fn wait_for_jni_ready(timeout_ms: u64) -> bool {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);

    while !is_jni_ready() {
        if start.elapsed() > timeout {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    true
}

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
pub async fn wait_for_jni_ready(_timeout_ms: u64) -> bool {
    true
}

#[cfg(target_os = "android")]
pub fn get_jni_env() -> Result<jni::AttachGuard<'static>, String> {
    let vm = JAVA_VM.get().ok_or_else(|| {
        "JavaVM not initialized - JNI bridge not ready. Try again shortly.".to_string()
    })?;

    vm.attach_current_thread()
        .map_err(|e| format!("Failed to attach thread to JVM: {}", e))
}

#[cfg(target_os = "android")]
pub fn get_activity() -> Result<&'static GlobalRef, String> {
    MAIN_ACTIVITY.get().ok_or_else(|| {
        "MainActivity not initialized - JNI bridge not ready. Try again shortly.".to_string()
    })
}

#[cfg(target_os = "android")]
pub fn get_ytdlp_class() -> Result<&'static GlobalRef, String> {
    YTDLP_CLASS.get().ok_or_else(|| {
        "YtDlp class not cached - JNI bridge not ready. Try again shortly.".to_string()
    })
}

#[cfg(target_os = "android")]
pub fn pause_android_download(job_id: &str) -> Result<(), String> {
    let mut env = get_jni_env()?;
    let activity = get_activity()?;

    let j_job_id = env
        .new_string(job_id)
        .map_err(|e| format!("Failed to create job_id string: {}", e))?;

    env.call_method(
        activity.as_obj(),
        "pauseDownloadFromRust",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&j_job_id)],
    )
    .map_err(|e| format!("JNI pause call failed: {}", e))?;

    log::info!("Paused Android download via JNI for job {}", job_id);
    Ok(())
}

#[cfg(target_os = "android")]
pub fn cancel_android_download(job_id: &str) -> Result<(), String> {
    let mut env = get_jni_env()?;
    let activity = get_activity()?;

    let j_job_id = env
        .new_string(job_id)
        .map_err(|e| format!("Failed to create job_id string: {}", e))?;

    env.call_method(
        activity.as_obj(),
        "cancelDownloadFromRust",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&j_job_id)],
    )
    .map_err(|e| format!("JNI cancel call failed: {}", e))?;

    log::info!("Cancelled Android download via JNI for job {}", job_id);
    Ok(())
}

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
pub fn pause_android_download(_job_id: &str) -> Result<(), String> {
    Err("Not on Android".to_string())
}

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
pub fn cancel_android_download(_job_id: &str) -> Result<(), String> {
    Err("Not on Android".to_string())
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnDownloadStarted<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
    title: JString<'local>,
) {
    let job_id: String = match env.get_string(&job_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let title: String = match env.get_string(&title) {
        Ok(s) => s.into(),
        Err(_) => "".to_string(),
    };

    log::debug!(
        "JNI callback: download started for job {} - {}",
        job_id,
        title
    );

    if let Some(manager) = JOB_MANAGER.get() {
        manager.handle_android_started(&job_id, &title);
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnDownloadProgress<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
    progress: f32,
    speed_bps: jlong,
    eta_seconds: jlong,
    downloaded_bytes: jlong,
    total_bytes: jlong,
) {
    let job_id: String = match env.get_string(&job_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    if let Some(manager) = JOB_MANAGER.get() {
        manager.handle_android_progress(
            &job_id,
            progress as f64,
            if speed_bps > 0 {
                Some(speed_bps as u64)
            } else {
                None
            },
            if eta_seconds > 0 {
                Some(eta_seconds as u64)
            } else {
                None
            },
            if downloaded_bytes > 0 {
                Some(downloaded_bytes as u64)
            } else {
                None
            },
            if total_bytes > 0 {
                Some(total_bytes as u64)
            } else {
                None
            },
        );
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnDownloadCompleted<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
    output_path: JString<'local>,
    title: JString<'local>,
    _thumbnail: JString<'local>,
) {
    let job_id: String = match env.get_string(&job_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let output_path: String = match env.get_string(&output_path) {
        Ok(s) => s.into(),
        Err(_) => "".to_string(),
    };
    // Get title - filter out empty strings and "Downloading..." placeholder
    let title: Option<String> = env
        .get_string(&title)
        .ok()
        .map(|s| {
            let t: String = s.into();
            if t.is_empty() || t == "Downloading..." {
                None
            } else {
                Some(t)
            }
        })
        .flatten();

    log::info!(
        "JNI callback: download completed for job {} -> {}, title={:?}",
        job_id,
        output_path,
        title
    );

    // Signal pending spawn() to complete - Manager's spawn_job will handle state update
    signal_job_completed(&job_id, output_path, title);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnDownloadFailed<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
    error: JString<'local>,
) {
    let job_id: String = match env.get_string(&job_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let error: String = match env.get_string(&error) {
        Ok(s) => s.into(),
        Err(_) => "Unknown error".to_string(),
    };

    log::error!(
        "JNI callback: download failed for job {} - {}",
        job_id,
        error
    );

    // Signal pending spawn() to fail - Manager's spawn_job will handle state update
    signal_job_failed(&job_id, error);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnDownloadCancelled<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
) {
    let job_id: String = match env.get_string(&job_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    log::info!("JNI callback: download cancelled for job {}", job_id);

    // Signal pending spawn() as cancelled - Manager's spawn_job will handle state update
    signal_job_cancelled(&job_id);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnDownloadPaused<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
) {
    let job_id: String = match env.get_string(&job_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    log::info!("JNI callback: download paused for job {}", job_id);

    if let Some(manager) = JOB_MANAGER.get() {
        manager.handle_android_paused(&job_id);
    }
}

// yt-dlp uses NDJSON for playlists/channels.
fn parse_ytdlp_output(output: &str, url: &str) -> Result<UrlInfo, BackendError> {
    let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();

    if lines.is_empty() {
        return Err(BackendError::ParseError(
            "No JSON output from yt-dlp".to_string(),
        ));
    }

    // If only one line, parse as single video
    if lines.len() == 1 {
        return parse_ytdlp_single_json(lines[0], url);
    }

    // Multiple lines = playlist/channel entries
    let mut entries: Vec<PlaylistEntry> = Vec::new();
    let mut first_entry_info: Option<serde_json::Value> = None;

    for (idx, line) in lines.iter().enumerate() {
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => {
                if first_entry_info.is_none() {
                    first_entry_info = Some(v.clone());
                }

                let id = v
                    .get("id")
                    .and_then(|s| s.as_str())
                    .unwrap_or_default()
                    .to_string();
                if id.is_empty() {
                    continue;
                }

                let entry_url = v
                    .get("url")
                    .and_then(|s| s.as_str())
                    .map(String::from)
                    .or_else(|| {
                        v.get("webpage_url")
                            .and_then(|s| s.as_str())
                            .map(String::from)
                    })
                    .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={}", id));

                entries.push(PlaylistEntry {
                    id,
                    url: entry_url,
                    title: v.get("title").and_then(|s| s.as_str()).map(String::from),
                    thumbnail: v
                        .get("thumbnails")
                        .and_then(|t| t.as_array())
                        .and_then(|a| a.last())
                        .and_then(|i| i.get("url"))
                        .and_then(|s| s.as_str())
                        .map(String::from)
                        .or_else(|| {
                            v.get("thumbnail")
                                .and_then(|s| s.as_str())
                                .map(String::from)
                        }),
                    duration: v.get("duration").and_then(|d| d.as_f64()).map(|d| d as u64),
                    index: idx as u32,
                    uploader: v
                        .get("uploader")
                        .and_then(|s| s.as_str())
                        .map(String::from)
                        .or_else(|| v.get("channel").and_then(|s| s.as_str()).map(String::from)),
                    is_music: false,
                });
            }
            Err(e) => {
                debug!("Failed to parse line {}: {}", idx, e);
                continue;
            }
        }
    }

    let first = first_entry_info.as_ref();

    Ok(UrlInfo {
        url: url.to_string(),
        title: first
            .and_then(|v| {
                v.get("playlist_title")
                    .or_else(|| v.get("channel"))
                    .and_then(|s| s.as_str())
            })
            .map(String::from),
        thumbnail: first
            .and_then(|v| v.get("thumbnail").and_then(|s| s.as_str()))
            .map(String::from),
        duration: None,
        filesize: None,
        extractor: first
            .and_then(|v| v.get("extractor").and_then(|s| s.as_str()))
            .unwrap_or("youtube")
            .to_string(),
        is_playlist: true,
        playlist_count: Some(entries.len() as u32),
        formats: None,
        mime_type: None,
        entries: Some(entries),
        uploader: first
            .and_then(|v| {
                v.get("uploader")
                    .or_else(|| v.get("channel"))
                    .and_then(|s| s.as_str())
            })
            .map(String::from),
        channel: first
            .and_then(|v| v.get("channel").and_then(|s| s.as_str()))
            .map(String::from),
        view_count: None,
        like_count: None,
        description: first
            .and_then(|v| v.get("description").and_then(|s| s.as_str()))
            .map(String::from),
        upload_date: None,
        channel_url: first
            .and_then(|v| {
                v.get("channel_url")
                    .or_else(|| v.get("uploader_url"))
                    .and_then(|s| s.as_str())
            })
            .map(String::from),
        channel_id: first
            .and_then(|v| {
                v.get("channel_id")
                    .or_else(|| v.get("uploader_id"))
                    .and_then(|s| s.as_str())
            })
            .map(String::from),
        storyboards: None,
        chapters: None,
    })
}

fn parse_ytdlp_single_json(json_str: &str, url: &str) -> Result<UrlInfo, BackendError> {
    let v: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| BackendError::ParseError(e.to_string()))?;

    let formats = v.get("formats").and_then(|f| f.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|f| {
                Some(VideoFormat {
                    format_id: f.get("format_id")?.as_str()?.to_string(),
                    ext: f.get("ext")?.as_str()?.to_string(),
                    resolution: f
                        .get("resolution")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    fps: f.get("fps").and_then(|v| v.as_u64()).map(|n| n as u32),
                    vcodec: f.get("vcodec").and_then(|v| v.as_str()).map(String::from),
                    acodec: f.get("acodec").and_then(|v| v.as_str()).map(String::from),
                    filesize: f.get("filesize").and_then(|v| v.as_u64()),
                    filesize_approx: f.get("filesize_approx").and_then(|v| v.as_u64()),
                    tbr: f.get("tbr").and_then(|v| v.as_f64()),
                    vbr: f.get("vbr").and_then(|v| v.as_f64()),
                    abr: f.get("abr").and_then(|v| v.as_f64()),
                    asr: f.get("asr").and_then(|v| v.as_u64()).map(|n| n as u32),
                    has_video: f
                        .get("vcodec")
                        .and_then(|v| v.as_str())
                        .map(|s| s != "none")
                        .unwrap_or(false),
                    has_audio: f
                        .get("acodec")
                        .and_then(|v| v.as_str())
                        .map(|s| s != "none")
                        .unwrap_or(false),
                    quality: f.get("quality").and_then(|v| v.as_i64()).map(|n| n as i32),
                    format_note: f
                        .get("format_note")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    rows: f.get("rows").and_then(|v| v.as_u64()).map(|n| n as u32),
                    columns: f
                        .get("columns")
                        .or_else(|| f.get("cols"))
                        .and_then(|v| v.as_u64())
                        .map(|n| n as u32),
                    fragments: f
                        .get("fragments")
                        .and_then(|frag| frag.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|fr| {
                                    Some(Fragment {
                                        url: fr
                                            .get("url")
                                            .and_then(|s| s.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        duration: fr
                                            .get("duration")
                                            .and_then(|d| d.as_f64())
                                            .unwrap_or(0.0),
                                    })
                                })
                                .collect()
                        }),
                })
            })
            .collect()
    });

    let entries = v.get("entries").and_then(|e| e.as_array()).map(|arr| {
        arr.iter()
            .enumerate()
            .filter_map(|(idx, e)| {
                let id = e
                    .get("id")
                    .and_then(|s| s.as_str())
                    .unwrap_or_default()
                    .to_string();
                if id.is_empty() {
                    return None;
                }

                let url = e
                    .get("url")
                    .and_then(|s| s.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={}", id));

                Some(PlaylistEntry {
                    id,
                    url,
                    title: e.get("title").and_then(|s| s.as_str()).map(String::from),
                    thumbnail: e
                        .get("thumbnails")
                        .and_then(|t| t.as_array())
                        .and_then(|a| a.last())
                        .and_then(|i| i.get("url"))
                        .and_then(|s| s.as_str())
                        .map(String::from)
                        .or_else(|| {
                            e.get("thumbnail")
                                .and_then(|s| s.as_str())
                                .map(String::from)
                        }),
                    duration: e.get("duration").and_then(|d| d.as_f64()).map(|d| d as u64),
                    index: idx as u32,
                    uploader: e.get("uploader").and_then(|s| s.as_str()).map(String::from),
                    is_music: false,
                })
            })
            .collect()
    });

    let chapters = v.get("chapters").and_then(|c| c.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|c| {
                Some(Chapter {
                    title: c
                        .get("title")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    start_time: c.get("start_time").and_then(|f| f.as_f64()).unwrap_or(0.0),
                    end_time: c.get("end_time").and_then(|f| f.as_f64()).unwrap_or(0.0),
                })
            })
            .collect()
    });

    Ok(UrlInfo {
        url: url.to_string(),
        title: v.get("title").and_then(|v| v.as_str()).map(String::from),
        thumbnail: v
            .get("thumbnail")
            .and_then(|v| v.as_str())
            .map(String::from),
        duration: v.get("duration").and_then(|v| v.as_u64()),
        filesize: v.get("filesize").and_then(|v| v.as_u64()),
        extractor: v
            .get("extractor")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        is_playlist: v.get("_type").and_then(|v| v.as_str()) == Some("playlist"),
        playlist_count: v
            .get("playlist_count")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .or_else(|| {
                entries
                    .as_ref()
                    .map(|e: &Vec<PlaylistEntry>| e.len() as u32)
            }),
        formats,
        mime_type: None,
        entries,
        uploader: v.get("uploader").and_then(|v| v.as_str()).map(String::from),
        channel: v.get("channel").and_then(|v| v.as_str()).map(String::from),
        view_count: v.get("view_count").and_then(|v| v.as_u64()),
        like_count: v.get("like_count").and_then(|v| v.as_u64()),
        description: v
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from),
        upload_date: v
            .get("upload_date")
            .and_then(|v| v.as_str())
            .map(String::from),
        channel_url: v
            .get("channel_url")
            .or_else(|| v.get("uploader_url"))
            .and_then(|v| v.as_str())
            .map(String::from),
        channel_id: v
            .get("channel_id")
            .or_else(|| v.get("uploader_id"))
            .and_then(|v| v.as_str())
            .map(String::from),
        storyboards: None,
        chapters,
    })
}

#[cfg(not(target_os = "android"))]
fn parse_ytdlp_error(stderr: &str) -> BackendError {
    let lower = stderr.to_lowercase();

    if lower.contains("video unavailable") || lower.contains("not available") {
        BackendError::NotFound(truncate(stderr, 200))
    } else if lower.contains("private video") || lower.contains("sign in") {
        BackendError::Unauthorized(truncate(stderr, 200))
    } else if lower.contains("429") || lower.contains("rate limit") {
        BackendError::RateLimited(truncate(stderr, 200))
    } else if lower.contains("403") {
        BackendError::Forbidden(truncate(stderr, 200))
    } else if lower.contains("unsupported url") || lower.contains("no video formats") {
        BackendError::UnsupportedUrl(truncate(stderr, 200))
    } else {
        BackendError::Other(truncate(stderr, 200))
    }
}

// yt-dlp downloads multiple files (video/audio/merge); use filename to detect phase changes.
#[cfg(not(target_os = "android"))]
struct MultiPhaseProgress {
    completed_bytes: u64,
    completed_total: u64,
    current_filename: Option<String>,
    current_file_total: Option<u64>,
    current_phase_max_downloaded: u64,
}

#[cfg(not(target_os = "android"))]
impl MultiPhaseProgress {
    fn new() -> Self {
        Self {
            completed_bytes: 0,
            completed_total: 0,
            current_filename: None,
            current_file_total: None,
            current_phase_max_downloaded: 0,
        }
    }

    fn update(
        &mut self,
        downloaded: u64,
        total: Option<u64>,
        filename: Option<&str>,
    ) -> (u64, Option<u64>) {
        let filename_changed = match (&self.current_filename, filename) {
            (Some(current), Some(new)) => current != new,
            (None, Some(_)) => false,
            _ => false,
        };

        if filename_changed {
            if let Some(prev_total) = self.current_file_total {
                self.completed_bytes += prev_total;
                self.completed_total += prev_total;
                info!(target: "ytdlp", "Phase transition: completed {} bytes, new file: {:?}", 
                      prev_total, filename);
            } else {
                self.completed_bytes += self.current_phase_max_downloaded;
                self.completed_total += self.current_phase_max_downloaded;
                info!(target: "ytdlp", "Phase transition (estimated): completed {} bytes, new file: {:?}", 
                      self.current_phase_max_downloaded, filename);
            }
            self.current_phase_max_downloaded = 0;
            self.current_file_total = None;
        }

        if let Some(f) = filename {
            self.current_filename = Some(f.to_string());
        }

        if let Some(t) = total {
            self.current_file_total = Some(t);
        }

        self.current_phase_max_downloaded = self.current_phase_max_downloaded.max(downloaded);

        let cumulative_downloaded = self.completed_bytes + downloaded;
        let cumulative_total = self.current_file_total.map(|t| self.completed_total + t);

        (cumulative_downloaded, cumulative_total)
    }
}

#[cfg(not(target_os = "android"))]
lazy_static::lazy_static! {
    static ref PROGRESS_TRACKERS: Mutex<HashMap<String, MultiPhaseProgress>> = Mutex::new(HashMap::new());
}

#[cfg(not(target_os = "android"))]
fn parse_progress_line(line: &str, job_id: &str) -> Option<ProgressUpdate> {
    let start = line.find("__COMINE_PROGRESS__")?;
    let end = line.rfind("__COMINE_PROGRESS__")?;
    if start >= end {
        return None;
    }

    let content = &line[start + 19..end];

    let extract_value = |key: &str| -> Option<&str> {
        let pattern = format!("{}:", key);
        let start_idx = content.find(&pattern)?;
        let value_start = start_idx + pattern.len();
        let rest = &content[value_start..];
        let end_idx = rest.find(|c| c == ',' || c == '}').unwrap_or(rest.len());
        Some(rest[..end_idx].trim())
    };

    let parse_u64 = |val: Option<&str>| -> Option<u64> {
        let s = val?;
        if s == "NA" || s.is_empty() {
            None
        } else {
            s.parse::<f64>().ok().map(|n| n as u64)
        }
    };

    let downloaded = parse_u64(extract_value("downloaded")).unwrap_or(0);
    let total =
        parse_u64(extract_value("total")).or_else(|| parse_u64(extract_value("total_estimate")));
    let speed = parse_u64(extract_value("speed"));
    let eta = parse_u64(extract_value("eta"));

    let filename = extract_value("filename").and_then(|f| {
        if f == "NA" || f.is_empty() {
            None
        } else {
            Some(f)
        }
    });

    let (cumulative_downloaded, cumulative_total) = {
        let mut trackers = PROGRESS_TRACKERS.lock().unwrap();
        let tracker = trackers
            .entry(job_id.to_string())
            .or_insert_with(MultiPhaseProgress::new);
        tracker.update(downloaded, total, filename)
    };

    debug!(target: "ytdlp", "Progress: file={:?}, downloaded={}, total={:?}, cumulative={}/{:?}", 
           filename, downloaded, total, cumulative_downloaded, cumulative_total);

    Some(ProgressUpdate {
        job_id: job_id.to_string(),
        downloaded_bytes: cumulative_downloaded,
        total_bytes: cumulative_total,
        speed,
        eta,
    })
}

#[cfg(not(target_os = "android"))]
pub fn cleanup_progress_tracker(job_id: &str) {
    let mut trackers = PROGRESS_TRACKERS.lock().unwrap();
    trackers.remove(job_id);
}

#[cfg(target_os = "android")]
pub fn cleanup_progress_tracker(_job_id: &str) {}

#[cfg(not(target_os = "android"))]
fn parse_filepath_line(line: &str) -> Option<String> {
    let marker = ">>>FILEPATH:";
    let idx = line.find(marker)?;
    Some(line[idx + marker.len()..].trim().to_string())
}

#[cfg(not(target_os = "android"))]
fn parse_title_line(line: &str) -> Option<String> {
    let marker = ">>>TITLE:";
    let idx = line.find(marker)?;
    Some(line[idx + marker.len()..].trim().to_string())
}

#[cfg(not(target_os = "android"))]
fn parse_thumbnail_line(line: &str) -> Option<String> {
    let marker = ">>>THUMBNAIL:";
    let idx = line.find(marker)?;
    Some(line[idx + marker.len()..].trim().to_string())
}

#[cfg(not(target_os = "android"))]
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
