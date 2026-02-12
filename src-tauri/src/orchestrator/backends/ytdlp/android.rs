use async_trait::async_trait;
use jni::objects::{JClass, JValue};
use tauri::AppHandle;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::common::{
    apply_metadata_event, normalize_url_for_ytdlp, ProgressTracker, YtDlpArgsBuilder,
};
use super::json::{parse_ytdlp_output, PaginationContext};
use super::shared;
use crate::orchestrator::backends::android_jni::{
    cancel_download_jni, cancel_resolve_jni, get_jni_env, get_ytdlp_class, pause_download_jni,
    register_listener, remove_listener, start_android_job_jni, start_resolve_jni,
    wait_for_jni_ready, AndroidEvent,
};
use crate::orchestrator::backends::{
    resolve_effective_proxy, Backend, BackendCapabilities, SpawnContext, StreamingResolveHandle,
};
use crate::orchestrator::types::*;

pub struct YtdlpBackend {
    app: AppHandle,
}

impl YtdlpBackend {
    pub fn new(app: AppHandle, _binary_path: std::path::PathBuf) -> Self {
        Self { app }
    }

    pub fn new_android(app: AppHandle) -> Self {
        Self { app }
    }

    async fn resolve_impl(
        &self,
        url: &str,
        settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        let output = self.resolve_jni_raw(url, settings).await?;

        let pagination_ctx = PaginationContext::from_settings(settings);

        parse_ytdlp_output(&output, url, pagination_ctx.as_ref())
    }

    async fn resolve_stream_impl(
        &self,
        url: &str,
        settings: &ResolveSettings,
        cancel_token: CancellationToken,
        tx: mpsc::UnboundedSender<ResolveEvent>,
    ) {
        if cancel_token.is_cancelled() {
            let _ = tx.send(ResolveEvent::Cancelled);
            return;
        }

        if !wait_for_jni_ready(10000).await {
            let _ = tx.send(ResolveEvent::Error {
                message: "Android JNI bridge not ready.".to_string(),
            });
            return;
        }

        let resolve_id = format!("resolve_{}", uuid::Uuid::new_v4());
        let normalized_url = normalize_url_for_ytdlp(url);
        let proxy_url = resolve_effective_proxy(&settings.proxy);

        let option_groups = YtDlpArgsBuilder::new(&normalized_url)
            .with_proxy(proxy_url)
            .with_cookies(
                settings.cookies_from_browser.clone(),
                settings.custom_cookies.clone(),
            )
            .build_resolve(settings);

        let args_json = match serde_json::to_string(&option_groups) {
            Ok(j) => j,
            Err(e) => {
                let _ = tx.send(ResolveEvent::Error {
                    message: format!("Failed to serialize args: {}", e),
                });
                return;
            }
        };

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        register_listener(&resolve_id, event_tx);

        if let Err(e) = start_resolve_jni(&resolve_id, &normalized_url, &args_json).await {
            remove_listener(&resolve_id);
            let _ = tx.send(ResolveEvent::Error {
                message: e.to_string(),
            });
            return;
        }

        let mut entries: Vec<PlaylistEntry> = Vec::new();
        let mut first_entry_json: Option<serde_json::Value> = None;
        let mut entry_index: u32 = 0;
        let mut is_multi_line = false;
        let mut got_single = false;

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    remove_listener(&resolve_id);
                    let _ = cancel_resolve_jni(&resolve_id);
                    let _ = tx.send(ResolveEvent::Cancelled);
                    return;
                }
                event = event_rx.recv() => {
                    match event {
                        Some(AndroidEvent::ResolveOutput(line)) => {
                            if line.trim().is_empty() {
                                continue;
                            }
                            match shared::process_ndjson_line(
                                &line,
                                url,
                                &mut entry_index,
                                &mut is_multi_line,
                                &mut first_entry_json,
                                &mut entries,
                                &tx,
                            ) {
                                Some(true) => {
                                    got_single = true;
                                }
                                Some(false) => {}
                                None => {}
                            }
                        }
                        Some(AndroidEvent::Completed { .. }) => {
                            break;
                        }
                        Some(AndroidEvent::Failed(e)) => {
                            remove_listener(&resolve_id);
                            if entries.is_empty() && !got_single {
                                let _ = tx.send(ResolveEvent::Error { message: e });
                            } else {
                                break;
                            }
                            return;
                        }
                        Some(AndroidEvent::Cancelled) => {
                            remove_listener(&resolve_id);
                            let _ = tx.send(ResolveEvent::Cancelled);
                            return;
                        }
                        None => {
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        remove_listener(&resolve_id);

        if !got_single {
            shared::finalize_streaming_resolve(
                url,
                settings,
                first_entry_json.as_ref(),
                entries,
                &tx,
            );
        }
    }

    async fn resolve_jni_raw(
        &self,
        url: &str,
        settings: &ResolveSettings,
    ) -> Result<String, BackendError> {
        if !wait_for_jni_ready(10000).await {
            return Err(BackendError::Other(
                "Android JNI bridge not ready.".to_string(),
            ));
        }

        let normalized_url = normalize_url_for_ytdlp(url);
        let proxy_url = resolve_effective_proxy(&settings.proxy);

        let option_groups = YtDlpArgsBuilder::new(&normalized_url)
            .with_proxy(proxy_url)
            .with_cookies(
                settings.cookies_from_browser.clone(),
                settings.custom_cookies.clone(),
            )
            .build_resolve(settings);

        let args_json = serde_json::to_string(&option_groups)
            .map_err(|e| BackendError::Other(format!("Failed to serialize args: {}", e)))?;

        tokio::task::spawn_blocking({
            let url = normalized_url;
            move || -> Result<String, BackendError> {
                let mut env = get_jni_env().map_err(BackendError::Other)?;
                let ytdlp_class = get_ytdlp_class().map_err(BackendError::Other)?;

                let j_url = env.new_string(&url)
                    .map_err(|e| BackendError::Other(format!("Failed to create url string: {}", e)))?;
                let j_args = env.new_string(&args_json)
                    .map_err(|e| BackendError::Other(format!("Failed to create args string: {}", e)))?;

                let result = env.call_static_method(
                    <&JClass>::from(ytdlp_class.as_obj()),
                    "resolveJson",
                    "(Ljava/lang/String;Ljava/lang/String;)Lcom/nichind/comine/YtDlp$ResolveResult;",
                    &[
                        JValue::Object(&j_url),
                        JValue::Object(&j_args),
                    ],
                ).map_err(|e| BackendError::Other(format!("JNI call failed: {}", e)))?;

                let result_obj = result.l()
                    .map_err(|e| BackendError::Other(format!("Failed to get result object: {}", e)))?;

                let has_output = env.call_method(&result_obj, "getOutput", "()Ljava/lang/String;", &[]);
                if let Ok(output_val) = has_output {
                    if let Ok(output_obj) = output_val.l() {
                        if !output_obj.is_null() {
                            let output_str: String = env.get_string((&output_obj).into())
                                .map_err(|e| BackendError::Other(format!("Failed to convert output: {}", e)))?
                                .into();
                            return Ok(output_str);
                        }
                    }
                }

                let _ = env.exception_clear();

                let error = env.call_method(&result_obj, "getError", "()Ljava/lang/String;", &[])
                    .map_err(|e| BackendError::Other(format!("Failed to get error: {}", e)))?
                    .l()
                    .map_err(|e| BackendError::Other(format!("Failed to get error string: {}", e)))?;
                let error_str: String = env.get_string((&error).into())
                    .map_err(|e| BackendError::Other(format!("Failed to convert error: {}", e)))?
                    .into();
                Err(BackendError::Other(error_str))
            }
        })
        .await
        .map_err(|e| BackendError::Other(format!("JNI task failed: {}", e)))?
    }

    async fn spawn_impl(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        if !wait_for_jni_ready(10000).await {
            return Err(BackendError::Other(
                "Android JNI bridge not ready.".to_string(),
            ));
        }

        let job_id = ctx.job.id.clone();
        let req = &ctx.job.request;
        let url = normalize_url_for_ytdlp(&req.url);
        let output_dir = req.output.directory.clone();
        let title = ctx.job.title.clone().unwrap_or_default();

        let proxy_url = resolve_effective_proxy(&req.options.proxy);
        let cookies_from_browser = req.options.cookies_from_browser.clone();
        let cookie_arg =
            req.options
                .custom_cookies
                .clone()
                .and_then(|c| if c.is_empty() { None } else { Some(c) });

        let option_groups = YtDlpArgsBuilder::new(&url)
            .with_proxy(proxy_url)
            .with_cookies(cookies_from_browser, cookie_arg)
            .build_download(req, ctx.effective_speed_limit, None);

        let args_json = serde_json::to_string(&option_groups)
            .map_err(|e| BackendError::Other(format!("Failed to serialize args: {}", e)))?;

        let payload = serde_json::json!({
            "url": url,
            "args": args_json,
            "output_directory": output_dir
        });
        let payload_str = serde_json::to_string(&payload)
            .map_err(|e| BackendError::Other(format!("Failed to serialize payload: {}", e)))?;

        let (tx, mut rx) = mpsc::unbounded_channel();
        register_listener(&job_id, tx);

        let mut tracker = ProgressTracker::new(job_id.clone());

        if let Some(ref ranges) = req.options.clip_ranges {
            if ranges.len() > 1 {
                let durations: Vec<f64> = ranges.iter().map(|r| r.end - r.start).collect();
                let starts: Vec<f64> = ranges.iter().map(|r| r.start).collect();
                tracker.set_section_info(durations, starts);
            }
        }

        start_android_job_jni(&job_id, "ytdlp", &payload_str, &title).await?;

        let mut final_path = None;
        let mut captured_output_paths: Vec<String> = Vec::new();

        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    remove_listener(&job_id);
                    let _ = cancel_ytdlp(&job_id);
                    return Err(BackendError::Cancelled);
                }
                event = rx.recv() => {
                    match event {
                        Some(AndroidEvent::RawProgressLine(line)) => {
                            if let Some(update) = tracker.parse_progress_line(&line) {
                                let _ = ctx.progress_tx.send(update);
                            }
                        }
                        Some(AndroidEvent::Metadata(event)) => {
                            apply_metadata_event(&event, &ctx.metadata_tx);
                        }
                        Some(AndroidEvent::FileOutput(path)) => {
                            if !captured_output_paths.contains(&path) {
                                info!(target: "ytdlp", "[Android] Captured output file: {}", path);
                                captured_output_paths.push(path);
                            }
                        }
                        Some(AndroidEvent::Completed { output_path, title: _ }) => {
                            final_path = Some(output_path);
                            break;
                        }
                        Some(AndroidEvent::Failed(e)) => {
                            return Err(BackendError::Other(e));
                        }
                        Some(AndroidEvent::Cancelled) => {
                            return Err(BackendError::Cancelled);
                        }
                        Some(AndroidEvent::Paused) => {
                            return Err(BackendError::Paused);
                        }
                        Some(AndroidEvent::ResolveOutput(_)) => {}
                        None => {
                            return Err(BackendError::Other("Android event channel closed".to_string()));
                        }
                    }
                }
            }
        }

        remove_listener(&job_id);

        let files_from_progress = tracker.captured_files();
        let files_to_use = if captured_output_paths.is_empty() && !files_from_progress.is_empty() {
            info!(target: "ytdlp", "[Android] Using files from progress tracking: {:?}", files_from_progress);
            files_from_progress
        } else {
            captured_output_paths.clone()
        };

        let output_path = if let Some(merged) = shared::concat_section_files(
            &self.app,
            &job_id,
            files_to_use.clone(),
            ctx.progress_tx.clone(),
        )
        .await
        {
            merged
        } else if let Some(path) = files_to_use.first() {
            path.clone()
        } else {
            final_path.unwrap_or_default()
        };

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

pub fn init_android(_app: AppHandle) {
    info!("yt-dlp Android backend init - waiting for Kotlin to call initRustJni");
}

pub fn cancel_ytdlp(job_id: &str) -> Result<(), String> {
    cancel_download_jni(job_id)
}

pub fn pause_android_download(job_id: &str) -> Result<(), String> {
    pause_download_jni(job_id)
}
