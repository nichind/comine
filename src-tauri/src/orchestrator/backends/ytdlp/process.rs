use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use super::common::{
    apply_metadata_event, normalize_url_for_ytdlp, parse_ytdlp_error, parse_ytdlp_line_event,
    PreparedCookieProxy, ProgressTracker, YtDlpArgsBuilder, YtdlpEvent,
};
use super::shared;
use crate::orchestrator::backends::apply_args_to_command;
use crate::orchestrator::backends::aria2::parse_aria2_progress_line;
use crate::orchestrator::backends::ytdlp::json::{parse_ytdlp_output, PaginationContext};
use crate::orchestrator::backends::{
    Backend, BackendCapabilities, SpawnContext, StreamingResolveHandle,
};
use crate::orchestrator::types::*;

pub struct YtdlpBackend {
    app: AppHandle,
    binary_path: PathBuf,
}

impl YtdlpBackend {
    pub fn new(app: AppHandle, binary_path: PathBuf) -> Self {
        Self { app, binary_path }
    }

    fn js_runtimes(&self) -> Vec<(String, String)> {
        crate::deps::specs::js_runtime::resolve_available_js_runtimes(&self.app)
            .into_iter()
            .map(|(name, path)| (name, path.to_string_lossy().to_string()))
            .collect()
    }

    async fn resolve_impl(
        &self,
        url: &str,
        settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        let mut cmd = crate::utils::new_command(&self.binary_path);
        let prepared = PreparedCookieProxy::from_resolve_settings(settings);

        cmd.env("PYTHONUNBUFFERED", "1");
        #[cfg(target_os = "windows")]
        cmd.env("PYTHONIOENCODING", "utf-8");

        let option_groups = YtDlpArgsBuilder::new(url)
            .with_proxy(prepared.proxy_url)
            .with_cookies(prepared.cookies_from_browser, prepared.cookie_arg)
            .with_js_runtimes(self.js_runtimes())
            .build_resolve(settings);

        apply_args_to_command(&mut cmd, option_groups);

        let normalized_url = normalize_url_for_ytdlp(url);
        cmd.arg(&normalized_url);

        let output = cmd
            .output()
            .await
            .map_err(|e| BackendError::ProcessError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(parse_ytdlp_error(&stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        let pagination_ctx = PaginationContext::from_settings(settings);

        parse_ytdlp_output(&stdout, url, pagination_ctx.as_ref())
    }

    async fn resolve_stream_impl(
        &self,
        url: &str,
        settings: &ResolveSettings,
        cancel_token: CancellationToken,
        tx: mpsc::UnboundedSender<ResolveEvent>,
    ) {
        let mut cmd = crate::utils::new_command(&self.binary_path);
        let prepared = PreparedCookieProxy::from_resolve_settings(settings);

        cmd.env("PYTHONUNBUFFERED", "1");
        #[cfg(target_os = "windows")]
        cmd.env("PYTHONIOENCODING", "utf-8");

        let option_groups = YtDlpArgsBuilder::new(url)
            .with_proxy(prepared.proxy_url)
            .with_cookies(prepared.cookies_from_browser, prepared.cookie_arg)
            .with_js_runtimes(self.js_runtimes())
            .build_resolve(settings);

        apply_args_to_command(&mut cmd, option_groups);

        let normalized_url = normalize_url_for_ytdlp(url);
        cmd.arg(&normalized_url);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(ResolveEvent::Error {
                    message: format!("Failed to spawn yt-dlp: {}", e),
                });
                return;
            }
        };

        let Some(stdout) = child.stdout.take() else {
            let _ = tx.send(ResolveEvent::Error {
                message: "yt-dlp: stdout not captured".to_string(),
            });
            return;
        };
        let Some(stderr) = child.stderr.take() else {
            let _ = tx.send(ResolveEvent::Error {
                message: "yt-dlp: stderr not captured".to_string(),
            });
            return;
        };
        let mut reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr);
        let url_owned = url.to_string();

        let mut entries: Vec<PlaylistEntry> = Vec::new();
        let mut first_entry_json: Option<serde_json::Value> = None;
        let mut entry_index: u32 = 0;
        let mut is_multi_line = false;

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    let _ = tx.send(ResolveEvent::Cancelled);
                    let _ = child.kill().await;
                    return;
                }
                line = reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            let line = line.trim().to_string();
                            if line.is_empty() {
                                continue;
                            }

                            match shared::process_ndjson_line(
                                &line,
                                &url_owned,
                                &mut entry_index,
                                &mut is_multi_line,
                                &mut first_entry_json,
                                &mut entries,
                                &tx,
                            ) {
                                Some(true) => {
                                    let _ = child.wait().await;
                                    return;
                                }
                                Some(false) => {}
                                None => {
                                    debug!("Failed to parse NDJSON line");
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            let _ = tx.send(ResolveEvent::Error {
                                message: format!("Read error: {}", e),
                            });
                            let _ = child.kill().await;
                            return;
                        }
                    }
                }
            }
        }

        let status = child.wait().await;

        if entries.is_empty() {
            let mut stderr_buf = String::new();
            let _ =
                tokio::io::AsyncReadExt::read_to_string(&mut stderr_reader, &mut stderr_buf).await;
            if !stderr_buf.is_empty() {
                let err = parse_ytdlp_error(&stderr_buf);
                let _ = tx.send(ResolveEvent::Error {
                    message: err.to_string(),
                });
                return;
            }
            if let Ok(s) = status {
                if !s.success() {
                    let _ = tx.send(ResolveEvent::Error {
                        message: "yt-dlp exited with error".to_string(),
                    });
                    return;
                }
            }
        }

        shared::finalize_streaming_resolve(
            &url_owned,
            settings,
            first_entry_json.as_ref(),
            entries,
            &tx,
        );
    }

    async fn spawn_impl(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        info!(target: "ytdlp", "Starting download job {} for URL: {}", ctx.job.id, ctx.job.request.url);
        info!(target: "ytdlp", "Quality settings: format={}, max_height={:?}, audio_only={}", 
              ctx.job.request.quality.format, ctx.job.request.quality.max_height, ctx.job.request.quality.audio_only);

        let mut cmd = crate::utils::new_command(&self.binary_path);

        let req = &ctx.job.request;
        let opts = &req.options;
        let prepared = PreparedCookieProxy::from_download_options(opts);

        cmd.env("PYTHONUNBUFFERED", "1");
        #[cfg(target_os = "windows")]
        cmd.env("PYTHONIOENCODING", "utf-8");

        // Use parent dir so yt-dlp finds ffprobe too
        let ffmpeg_location = crate::deps::resolve_ffmpeg_path(&self.app).map(|p| {
            p.parent()
                .map(|d| d.to_string_lossy().to_string())
                .unwrap_or_else(|| p.to_string_lossy().to_string())
        });

        let option_groups = YtDlpArgsBuilder::new(&req.url)
            .with_proxy(prepared.proxy_url)
            .with_cookies(prepared.cookies_from_browser, prepared.cookie_arg)
            .with_js_runtimes(self.js_runtimes())
            .build_download(req, ctx.effective_speed_limit, ffmpeg_location);

        let all_args: Vec<String> = option_groups.iter().flatten().cloned().collect();
        info!(target: "ytdlp", "yt-dlp args: {:?}", all_args);

        apply_args_to_command(&mut cmd, option_groups);

        let normalized_url = normalize_url_for_ytdlp(&req.url);
        cmd.arg(&normalized_url);

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

        let stderr_tail: Arc<Mutex<VecDeque<String>>> =
            Arc::new(Mutex::new(VecDeque::with_capacity(80)));
        let stderr_tail_clone = stderr_tail.clone();
        let stderr_progress_tx = ctx.progress_tx.clone();
        let stderr_job_id = ctx.job.id.clone();
        let stderr_tracker: Arc<Mutex<ProgressTracker>> =
            Arc::new(Mutex::new(ProgressTracker::new(ctx.job.id.clone())));
        let stderr_tracker_clone = stderr_tracker.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buf = Vec::with_capacity(4096);
            let mut last_aria2_emit = std::time::Instant::now();

            loop {
                buf.clear();
                // Split on both \r and \n (aria2c uses \r)
                let line = loop {
                    match reader.read_u8().await {
                        Ok(b'\n') => break Some(String::from_utf8_lossy(&buf).trim().to_string()),
                        Ok(b'\r') => {
                            let line = String::from_utf8_lossy(&buf).trim().to_string();
                            buf.clear();
                            if !line.is_empty() {
                                break Some(line);
                            }
                        }
                        Ok(b) => buf.push(b),
                        Err(_) => {
                            if !buf.is_empty() {
                                break Some(String::from_utf8_lossy(&buf).trim().to_string());
                            }
                            break None;
                        }
                    }
                };

                let Some(line) = line else { break };
                if line.is_empty() {
                    continue;
                }

                if line.contains("__COMINE_PROGRESS__") {
                    debug!(target: "ytdlp", "STDERR progress line: {}", line);
                    let update = {
                        let mut tracker = match stderr_tracker_clone.lock() {
                            Ok(g) => g,
                            Err(poisoned) => {
                                warn!(target: "ytdlp", "stderr progress tracker mutex poisoned, recovering");
                                poisoned.into_inner()
                            }
                        };
                        tracker.parse_progress_line(&line)
                    };
                    if let Some(update) = update {
                        let _ = stderr_progress_tx.send(update);
                    }
                } else if let Some(raw_update) = parse_aria2_progress_line(&line, &stderr_job_id) {
                    if last_aria2_emit.elapsed().as_millis() >= 250 {
                        let update = {
                            let mut tracker = match stderr_tracker_clone.lock() {
                                Ok(g) => g,
                                Err(poisoned) => {
                                    warn!(target: "ytdlp", "stderr progress tracker mutex poisoned, recovering");
                                    poisoned.into_inner()
                                }
                            };
                            tracker.feed_external_progress(
                                raw_update.downloaded_bytes,
                                raw_update.total_bytes,
                                raw_update.speed,
                                raw_update.eta,
                            )
                        };
                        if let Some(update) = update {
                            debug!(target: "ytdlp", "STDERR aria2 progress: {}", line);
                            let _ = stderr_progress_tx.send(update);
                        }
                        last_aria2_emit = std::time::Instant::now();
                    }
                } else {
                    warn!(target: "ytdlp", "ERR: {}", line);
                }

                let mut guard = match stderr_tail_clone.lock() {
                    Ok(g) => g,
                    Err(poisoned) => {
                        warn!(target: "ytdlp", "stderr tail mutex poisoned, recovering");
                        poisoned.into_inner()
                    }
                };
                if guard.len() >= crate::orchestrator::types::constants::STDERR_TAIL_SIZE {
                    guard.pop_front();
                }
                guard.push_back(line);
            }
        });

        let mut buffer = Vec::with_capacity(4096);
        let mut reader = BufReader::new(stdout);
        let mut captured_output_paths: Vec<String> = Vec::new();
        let mut _captured_title: Option<String> = None;
        let mut captured_thumbnail_url: Option<String> = None;
        let mut last_stdout_aria2_emit = std::time::Instant::now();

        if let Some(ref ranges) = ctx.job.request.options.clip_ranges {
            if ranges.len() > 1 {
                let durations: Vec<f64> = ranges.iter().map(|r| r.end - r.start).collect();
                let starts: Vec<f64> = ranges.iter().map(|r| r.start).collect();
                let mut tracker = match stderr_tracker.lock() {
                    Ok(g) => g,
                    Err(poisoned) => {
                        warn!(target: "ytdlp", "progress tracker mutex poisoned, recovering");
                        poisoned.into_inner()
                    }
                };
                tracker.set_section_info(durations, starts);
            }
        }

        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    graceful_shutdown(&mut child).await;
                    return Err(BackendError::Cancelled);
                }
                result = async {
                    buffer.clear();
                    loop {
                        match reader.read_u8().await {
                            Ok(b'\n') => return Ok(buffer.len()),
                            Ok(b'\r') => {
                                if !buffer.is_empty() {
                                    return Ok(buffer.len());
                                }
                            }
                            Ok(b) => buffer.push(b),
                            Err(e) if buffer.is_empty() => return Err(e),
                            Err(_) => return Ok(buffer.len()),
                        }
                    }
                } => {
                    match result {
                        Ok(0) => break,
                        Ok(_) => {
                            let line = String::from_utf8_lossy(&buffer).trim().to_string();
                            if line.is_empty() {
                                continue;
                            }

                            if line.contains("__COMINE_PROGRESS__") {
                                debug!(target: "ytdlp", "STD progress (fallback): {}", line);
                                let update = {
                                    let mut tracker = match stderr_tracker.lock() {
                                        Ok(g) => g,
                                        Err(poisoned) => {
                                            warn!(target: "ytdlp", "progress tracker mutex poisoned, recovering");
                                            poisoned.into_inner()
                                        }
                                    };
                                    tracker.parse_progress_line(&line)
                                };
                                if let Some(u) = update {
                                    let _ = ctx.progress_tx.send(u);
                                }
                            } else if let Some(raw_update) = parse_aria2_progress_line(&line, &ctx.job.id) {
                                if last_stdout_aria2_emit.elapsed().as_millis() >= 250 {
                                    let update = {
                                        let mut tracker = match stderr_tracker.lock() {
                                            Ok(g) => g,
                                            Err(poisoned) => {
                                                warn!(target: "ytdlp", "progress tracker mutex poisoned, recovering");
                                                poisoned.into_inner()
                                            }
                                        };
                                        tracker.feed_external_progress(
                                            raw_update.downloaded_bytes,
                                            raw_update.total_bytes,
                                            raw_update.speed,
                                            raw_update.eta,
                                        )
                                    };
                                    if let Some(u) = update {
                                        debug!(target: "ytdlp", "STD aria2 progress: {}", line);
                                        let _ = ctx.progress_tx.send(u);
                                    }
                                    last_stdout_aria2_emit = std::time::Instant::now();
                                }
                            } else {
                                info!(target: "ytdlp", "STD: {}", line);
                            }

                            if let Some(event) = parse_ytdlp_line_event(&line) {
                                match &event {
                                    YtdlpEvent::Filepath(p) => {
                                        if !captured_output_paths.contains(p) {
                                            captured_output_paths.push(p.clone());
                                        }
                                    }
                                    YtdlpEvent::Title(t) => _captured_title = Some(t.clone()),
                                    YtdlpEvent::Thumbnail(t) => captured_thumbnail_url = Some(t.clone()),
                                    _ => {}
                                }
                                apply_metadata_event(&event, &ctx.metadata_tx);
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

        let status = child
            .wait()
            .await
            .map_err(|e| BackendError::ProcessError(e.to_string()))?;

        if !status.success() {
            let tail = {
                let guard = match stderr_tail.lock() {
                    Ok(g) => g,
                    Err(poisoned) => {
                        warn!(target: "ytdlp", "stderr tail mutex poisoned, recovering");
                        poisoned.into_inner()
                    }
                };
                guard.iter().cloned().collect::<Vec<_>>().join("\n")
            };
            let tail_msg = if tail.trim().is_empty() {
                String::new()
            } else {
                format!("\n\nyt-dlp stderr tail:\n{}", tail)
            };

            // Partial success: proceed with captured sections instead of failing entirely
            let is_multi_section = ctx
                .job
                .request
                .options
                .clip_ranges
                .as_ref()
                .is_some_and(|r| r.len() > 1);

            if is_multi_section && !captured_output_paths.is_empty() {
                let total_sections = ctx
                    .job
                    .request
                    .options
                    .clip_ranges
                    .as_ref()
                    .map_or(0, |r| r.len());
                warn!(target: "ytdlp",
                    "yt-dlp exited with code {:?} during multi-section download, but {} of {} section files were captured. Completing with partial output.",
                    status.code(), captured_output_paths.len(), total_sections);
            } else {
                return Err(BackendError::ProcessError(format!(
                    "yt-dlp exited with code {:?}{}",
                    status.code(),
                    tail_msg
                )));
            }
        }

        let output_path = if let Some(merged) = shared::concat_section_files(
            &self.app,
            &ctx.job.id,
            captured_output_paths.clone(),
            ctx.progress_tx.clone(),
        )
        .await
        {
            merged
        } else if let Some(path) = captured_output_paths.first() {
            path.clone()
        } else {
            // Fallback: most recently modified file in output dir
            let dir = &ctx.job.request.output.directory;
            let fallback = std::fs::read_dir(dir)
                .ok()
                .and_then(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().is_file())
                        .max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
                        .map(|e| e.path().to_string_lossy().to_string())
                })
                .unwrap_or_else(|| format!("{}/download", dir));
            fallback
        };

        if ctx.job.request.options.embed_thumbnail {
            let app = self.app.clone();
            let output_path_clone = output_path.clone();
            let job_id = ctx.job.id.clone();
            let url = ctx.job.request.url.clone();
            let thumbnail_url = captured_thumbnail_url.clone();
            let cookies = ctx.job.request.options.custom_cookies.clone();
            let cookies_browser = ctx.job.request.options.cookies_from_browser.clone();
            let proxy = ctx.job.request.options.proxy.clone();
            let player_client = ctx.job.request.options.youtube_player_client.clone();
            let binary_path = self.binary_path.clone();

            tokio::spawn(async move {
                let final_thumbnail_url = if thumbnail_url.is_some() {
                    thumbnail_url
                } else {
                    let backend = YtdlpBackend::new(app.clone(), binary_path);
                    let mut settings = ResolveSettings::default();
                    settings.custom_cookies = cookies;
                    settings.cookies_from_browser = cookies_browser;
                    settings.proxy = proxy;
                    settings.youtube_player_client = player_client;
                    backend
                        .resolve_impl(&url, &settings)
                        .await
                        .ok()
                        .and_then(|info| info.thumbnail)
                };

                if let Some(thumb_url) = final_thumbnail_url.as_deref() {
                    match crate::orchestrator::thumbnail::embed_thumbnail(
                        &app,
                        &output_path_clone,
                        thumb_url,
                    )
                    .await
                    {
                        Ok(_) => {
                            info!(target: "ytdlp", "Thumbnail embedded successfully");
                            match crate::thumbnail_color::extract_thumbnail_color(
                                app.clone(),
                                thumb_url.to_string(),
                            )
                            .await
                            {
                                Ok(color) => {
                                    let _ = app.emit(
                                        "job-event",
                                        &crate::orchestrator::types::JobEvent::ThumbnailColorExtracted {
                                            job_id,
                                            color: (color[0], color[1], color[2]),
                                        },
                                    );
                                }
                                Err(e) => {
                                    debug!(target: "ytdlp", "Color extraction after embed failed: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Thumbnail embedding failed: {}", e);
                            let _ = app.emit(
                                "job-event",
                                &crate::orchestrator::types::JobEvent::ThumbnailEmbedFailed {
                                    job_id,
                                    error: e.to_string(),
                                },
                            );
                        }
                    }
                }
            });
        }

        Ok(output_path)
    }
}

#[async_trait]
impl Backend for YtdlpBackend {
    fn name(&self) -> &str {
        "ytdlp"
    }

    fn capabilities(&self) -> BackendCapabilities {
        shared::ytdlp_capabilities()
    }

    fn priority(&self, url: &str) -> Priority {
        shared::ytdlp_priority(url)
    }

    async fn resolve(
        &self,
        url: &str,
        settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        self.resolve_impl(url, settings).await
    }

    async fn spawn(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        self.spawn_impl(ctx).await
    }

    async fn resolve_stream(
        &self,
        url: &str,
        settings: &ResolveSettings,
        cancel_token: CancellationToken,
    ) -> StreamingResolveHandle {
        let backend = Self {
            app: self.app.clone(),
            binary_path: self.binary_path.clone(),
        };
        let url = url.to_string();
        let settings = settings.clone();

        shared::make_streaming_handle(move |tx| async move {
            backend
                .resolve_stream_impl(&url, &settings, cancel_token, tx)
                .await;
        })
    }
}

async fn graceful_shutdown(child: &mut Child) {
    crate::orchestrator::backends::graceful_shutdown(child, "ytdlp").await;
}
