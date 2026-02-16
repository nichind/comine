use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{LazyLock, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

use crate::orchestrator::types::ProgressUpdate;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct FfmpegConvertSettings {
    #[serde(default)]
    pub hw_accel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct ConvertRequest {
    pub job_id: Option<String>,
    pub source_path: String,
    pub target_format: String,
    pub output_directory: Option<String>,
    pub output_filename: Option<String>,
    #[serde(default)]
    pub audio_only: bool,
    pub extra_args: Option<Vec<String>>,
    #[serde(default)]
    pub ffmpeg: Option<FfmpegConvertSettings>,
    pub metadata: Option<ConvertMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct ConvertMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: Option<u64>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct ConvertResult {
    pub job_id: String,
    pub output_path: String,
    pub filesize: u64,
    pub duration: Option<u64>,
    pub extension: String,
    pub metadata: Option<ConvertMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct ConvertProgress {
    pub job_id: String,
    pub progress: f64,
    pub time_processed: f64,
    pub total_duration: Option<f64>,
    pub speed: Option<String>,
}

struct ActiveConversion {
    cancel_token: CancellationToken,
}

static ACTIVE_CONVERSIONS: LazyLock<Mutex<HashMap<String, ActiveConversion>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn get_file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn build_ffmpeg_components(
    request: &ConvertRequest,
    force_software: bool,
) -> (Vec<String>, Vec<String>) {
    let mut pre_args = Vec::new();
    let mut post_args = Vec::new();

    let ffmpeg_settings = request.ffmpeg.clone().unwrap_or_default();
    let hw_accel = if force_software {
        "none"
    } else {
        ffmpeg_settings.hw_accel.as_str()
    };

    match hw_accel {
        "nvenc" => pre_args.extend_from_slice(&["-hwaccel".to_string(), "cuda".to_string()]),
        "qsv" => pre_args.extend_from_slice(&["-hwaccel".to_string(), "qsv".to_string()]),
        "amf" => pre_args.extend_from_slice(&["-hwaccel".to_string(), "d3d11va".to_string()]),
        "videotoolbox" => {
            pre_args.extend_from_slice(&["-hwaccel".to_string(), "videotoolbox".to_string()])
        }
        "auto" => pre_args.extend_from_slice(&["-hwaccel".to_string(), "auto".to_string()]),
        _ => {}
    }

    if request.audio_only {
        post_args.push("-vn".to_string());
        match request.target_format.as_str() {
            "mp3" => post_args.extend_from_slice(&[
                "-c:a".to_string(),
                "libmp3lame".to_string(),
                "-q:a".to_string(),
                "2".to_string(),
            ]),
            "m4a" | "aac" => post_args.extend_from_slice(&[
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                "192k".to_string(),
            ]),
            "opus" => post_args.extend_from_slice(&[
                "-c:a".to_string(),
                "libopus".to_string(),
                "-b:a".to_string(),
                "128k".to_string(),
            ]),
            "flac" => post_args.extend_from_slice(&["-c:a".to_string(), "flac".to_string()]),
            "wav" => post_args.extend_from_slice(&["-c:a".to_string(), "pcm_s16le".to_string()]),
            _ => post_args.extend_from_slice(&["-c:a".to_string(), "copy".to_string()]),
        }
    } else {
        let (video_codec, video_args): (&str, Vec<&str>) =
            match (hw_accel, request.target_format.as_str()) {
                ("nvenc", "mp4" | "mov" | "mkv") => {
                    ("h264_nvenc", vec!["-rc", "constqp", "-qp", "23"])
                }
                ("qsv", "mp4" | "mov" | "mkv") => ("h264_qsv", vec!["-global_quality", "23"]),
                ("amf", "mp4" | "mov" | "mkv") => {
                    ("h264_amf", vec!["-rc", "cqp", "-qp_i", "23", "-qp_p", "23"])
                }
                ("videotoolbox", "mp4" | "mov" | "mkv") => {
                    ("h264_videotoolbox", vec!["-q:v", "65"])
                }
                (_, "mp4" | "mov") => ("libx264", vec!["-preset", "medium", "-crf", "23"]),
                (_, "webm") => ("libvpx-vp9", vec!["-crf", "30", "-b:v", "0"]),
                (_, "mkv") => ("copy", vec![]),
                (_, "avi") => ("libxvid", vec!["-q:v", "5"]),
                (_, "gif") => ("gif", vec!["-vf", "fps=15,scale=480:-1:flags=lanczos"]),
                _ => ("copy", vec![]),
            };

        post_args.extend_from_slice(&["-c:v".to_string(), video_codec.to_string()]);
        for arg in video_args {
            post_args.push(arg.to_string());
        }

        match request.target_format.as_str() {
            "mp4" | "mov" | "m4a" => post_args.extend_from_slice(&[
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                "192k".to_string(),
            ]),
            "webm" => post_args.extend_from_slice(&[
                "-c:a".to_string(),
                "libopus".to_string(),
                "-b:a".to_string(),
                "128k".to_string(),
            ]),
            "mkv" => post_args.extend_from_slice(&["-c:a".to_string(), "copy".to_string()]),
            "avi" => post_args.extend_from_slice(&[
                "-c:a".to_string(),
                "libmp3lame".to_string(),
                "-b:a".to_string(),
                "192k".to_string(),
            ]),
            "gif" => {}
            _ => post_args.extend_from_slice(&["-c:a".to_string(), "copy".to_string()]),
        }

        if request.target_format == "mp4" || request.target_format == "mov" {
            post_args.extend_from_slice(&["-movflags".to_string(), "+faststart".to_string()]);
        }
    }

    if let Some(extra) = &request.extra_args {
        post_args.extend(extra.clone());
    }

    (pre_args, post_args)
}

fn create_concat_list(files: &[String], output_dir: &Path) -> Result<(PathBuf, String), String> {
    let concat_list_path = output_dir.join(format!(".concat_{}.txt", uuid::Uuid::new_v4()));
    let concat_content: String = files
        .iter()
        .map(|p| format!("file '{}'", p.replace('\\', "/").replace("'", "'\\''")))
        .collect::<Vec<_>>()
        .join("\n");

    std::fs::write(&concat_list_path, &concat_content)
        .map_err(|e| format!("Failed to write concat list: {}", e))?;

    Ok((concat_list_path, concat_content))
}

fn cleanup_source_files(files: &[String]) {
    for file in files {
        if let Err(e) = std::fs::remove_file(file) {
            warn!("Failed to delete source file {}: {}", file, e);
        }
    }
}

pub async fn concat_files(
    app: &AppHandle,
    _job_id: &str,
    files: Vec<String>,
    output_path: &str,
    delete_sources: bool,
    _progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
) -> Result<String, String> {
    if files.is_empty() {
        return Err("No files to concatenate".to_string());
    }

    if files.len() == 1 {
        if files[0] != output_path {
            std::fs::rename(&files[0], output_path)
                .map_err(|e| format!("Failed to rename file: {}", e))?;
        }
        return Ok(output_path.to_string());
    }

    info!("Concatenating {} files into {}", files.len(), output_path);

    let output_dir = std::path::Path::new(output_path)
        .parent()
        .unwrap_or(std::path::Path::new("."));

    let (concat_list_path, concat_content) = create_concat_list(&files, output_dir)?;
    info!("Concat list:\n{}", concat_content);

    let result = exec::run_ffmpeg_concat(app, &concat_list_path, output_path).await;

    let _ = std::fs::remove_file(&concat_list_path);

    result?;

    info!("Concatenation successful: {}", output_path);

    if delete_sources {
        cleanup_source_files(&files);
    }

    Ok(output_path.to_string())
}

#[tauri::command]
pub async fn convert_local_file(
    app: AppHandle,
    request: ConvertRequest,
) -> Result<ConvertResult, String> {
    let job_id = request
        .job_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    info!(job_id = %job_id, source = %request.source_path, target = %request.target_format, "Starting conversion");

    let source_path = Path::new(&request.source_path);
    if !source_path.exists() {
        return Err(format!("Source file not found: {}", request.source_path));
    }

    let source_stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let output_filename = request.output_filename.as_deref().unwrap_or(source_stem);
    let output_dir = request
        .output_directory
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| source_path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let output_path = output_dir.join(format!("{}.{}", output_filename, request.target_format));

    let output_path = if output_path == source_path {
        output_dir.join(format!(
            "{}_converted.{}",
            output_filename, request.target_format
        ))
    } else {
        let mut final_path = output_path.clone();
        let mut counter = 1;
        while final_path.exists() {
            final_path = output_dir.join(format!(
                "{}_{}.{}",
                output_filename, counter, request.target_format
            ));
            counter += 1;
        }
        final_path
    };

    let force_software = cfg!(target_os = "android");
    let (pre_args, post_args) = build_ffmpeg_components(&request, force_software);

    let result = exec::run_ffmpeg_convert(
        &app,
        &job_id,
        &request.source_path,
        &output_path,
        pre_args,
        post_args,
    )
    .await?;

    let filesize = get_file_size(&output_path);
    let output_path_str = output_path.to_string_lossy().to_string();

    let _ = app.emit(
        "convert-progress",
        ConvertProgress {
            job_id: job_id.clone(),
            progress: 100.0,
            time_processed: result.duration.unwrap_or(0.0),
            total_duration: result.duration,
            speed: None,
        },
    );

    info!(job_id = %job_id, output = %output_path_str, size = %filesize, "Conversion completed");

    Ok(ConvertResult {
        job_id,
        output_path: output_path_str,
        filesize,
        duration: result.duration.map(|d| d as u64),
        extension: request.target_format,
        metadata: request.metadata,
    })
}

#[tauri::command]
pub async fn cancel_conversion(job_id: String) -> Result<(), String> {
    info!(job_id = %job_id, "Cancelling conversion");

    let cancel_token = {
        let conversions = ACTIVE_CONVERSIONS
            .lock()
            .map_err(|e| format!("Failed to lock conversions: {}", e))?;
        conversions.get(&job_id).map(|c| c.cancel_token.clone())
    };

    if let Some(token) = cancel_token {
        token.cancel();
        info!(job_id = %job_id, "Conversion cancellation triggered");
        Ok(())
    } else {
        warn!(job_id = %job_id, "No active conversion found to cancel");
        Err("Conversion not found or already completed".to_string())
    }
}

mod exec {
    use super::*;
    use std::path::Path;

    pub struct ConvertExecResult {
        pub duration: Option<f64>,
    }

    pub async fn run_ffmpeg_concat(
        app: &AppHandle,
        concat_list_path: &Path,
        output_path: &str,
    ) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            run_ffmpeg_concat_android(concat_list_path, output_path).await
        }
        #[cfg(not(target_os = "android"))]
        {
            run_ffmpeg_concat_desktop(app, concat_list_path, output_path).await
        }
    }

    pub async fn run_ffmpeg_convert(
        app: &AppHandle,
        job_id: &str,
        source_path: &str,
        output_path: &Path,
        pre_args: Vec<String>,
        post_args: Vec<String>,
    ) -> Result<ConvertExecResult, String> {
        #[cfg(target_os = "android")]
        {
            run_ffmpeg_convert_android(app, job_id, source_path, output_path, post_args).await
        }
        #[cfg(not(target_os = "android"))]
        {
            run_ffmpeg_convert_desktop(app, job_id, source_path, output_path, pre_args, post_args)
                .await
        }
    }

    #[cfg(not(target_os = "android"))]
    use crate::utils::get_media_duration;

    #[cfg(not(target_os = "android"))]
    async fn run_ffmpeg_concat_desktop(
        app: &AppHandle,
        concat_list_path: &Path,
        output_path: &str,
    ) -> Result<(), String> {
        let ffmpeg_path = match crate::deps::resolve_ffmpeg_path(app) {
            Some(path) => path,
            None => return Err("FFmpeg not installed".to_string()),
        };

        let mut cmd = crate::utils::new_command(&ffmpeg_path);
        cmd.args(["-f", "concat", "-safe", "0", "-i"])
            .arg(concat_list_path)
            .args(["-c", "copy", "-y"])
            .arg(output_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to run FFmpeg: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("FFmpeg concat failed: {}", stderr));
        }

        Ok(())
    }

    #[cfg(not(target_os = "android"))]
    async fn run_ffmpeg_convert_desktop(
        app: &AppHandle,
        job_id: &str,
        source_path: &str,
        output_path: &Path,
        pre_args: Vec<String>,
        post_args: Vec<String>,
    ) -> Result<ConvertExecResult, String> {
        let ffmpeg_path = match crate::deps::resolve_ffmpeg_path(app) {
            Some(path) => path,
            None => {
                return Err(
                    "FFmpeg not installed. Please install it from Settings → Dependencies."
                        .to_string(),
                )
            }
        };

        let ffprobe_path = crate::deps::resolve_ffprobe_path(app);

        let total_duration = match ffprobe_path {
            Some(ref p) => get_media_duration(p, source_path).await,
            None => None,
        };

        let mut cmd = crate::utils::new_command(&ffmpeg_path);

        for arg in &pre_args {
            cmd.arg(arg);
        }

        cmd.arg("-i")
            .arg(source_path)
            .arg("-y")
            .arg("-progress")
            .arg("pipe:1")
            .arg("-stats_period")
            .arg("0.5");

        for arg in &post_args {
            cmd.arg(arg);
        }

        cmd.arg(output_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let cancel_token = CancellationToken::new();
        {
            let mut conversions = ACTIVE_CONVERSIONS
                .lock()
                .map_err(|e| format!("Failed to lock conversions: {}", e))?;
            conversions.insert(
                job_id.to_string(),
                ActiveConversion {
                    cancel_token: cancel_token.clone(),
                },
            );
        }

        let cleanup = |job_id: &str| {
            if let Ok(mut c) = ACTIVE_CONVERSIONS.lock() {
                c.remove(job_id);
            }
        };

        let mut child = cmd.spawn().map_err(|e| {
            cleanup(job_id);
            format!("Failed to start FFmpeg: {}", e)
        })?;

        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture FFmpeg stdout")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("Failed to capture FFmpeg stderr")?;

        let _ = app.emit(
            "convert-progress",
            ConvertProgress {
                job_id: job_id.to_string(),
                progress: 0.0,
                time_processed: 0.0,
                total_duration,
                speed: None,
            },
        );

        let job_id_clone = job_id.to_string();
        let app_clone = app.clone();
        let progress_task = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let time_re = Regex::new(r"out_time_ms=(\d+)").ok();
            let speed_re = Regex::new(r"speed=\s*([0-9.]+)x").ok();
            let mut current_time_ms: u64 = 0;
            let mut current_speed: Option<String> = None;

            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(ref re) = time_re {
                    if let Some(caps) = re.captures(&line) {
                        if let Ok(ms) = caps[1].parse::<u64>() {
                            current_time_ms = ms;
                        }
                    }
                }
                if let Some(ref re) = speed_re {
                    if let Some(caps) = re.captures(&line) {
                        current_speed = Some(format!("{}x", &caps[1]));
                    }
                }
                if current_time_ms > 0 {
                    let time_secs = current_time_ms as f64 / 1_000_000.0;
                    let progress = total_duration
                        .map(|t| ((time_secs / t) * 100.0).min(100.0))
                        .unwrap_or(0.0);
                    let _ = app_clone.emit(
                        "convert-progress",
                        ConvertProgress {
                            job_id: job_id_clone.clone(),
                            progress,
                            time_processed: time_secs,
                            total_duration,
                            speed: current_speed.clone(),
                        },
                    );
                }
            }
        });

        let stderr_task = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let mut output = String::new();
            while let Ok(Some(line)) = lines.next_line().await {
                output.push_str(&line);
                output.push('\n');
            }
            output
        });

        let result = tokio::select! {
            status = child.wait() => status.map_err(|e| format!("FFmpeg process error: {}", e)),
            _ = cancel_token.cancelled() => {
                warn!(job_id = %job_id, "Conversion cancelled");
                let _ = child.kill().await;
                if output_path.exists() {
                    let _ = std::fs::remove_file(output_path);
                }
                cleanup(job_id);
                return Err("Conversion cancelled".to_string());
            }
        };

        cleanup(job_id);

        let _ = progress_task.await;
        let stderr_output = stderr_task.await.unwrap_or_default();

        let status = result?;
        if !status.success() {
            error!(job_id = %job_id, stderr = %stderr_output, "FFmpeg conversion failed");
            return Err(format!(
                "Conversion failed: {}",
                stderr_output.lines().last().unwrap_or("Unknown error")
            ));
        }

        if !output_path.exists() {
            return Err("Conversion completed but output file not found".to_string());
        }

        Ok(ConvertExecResult {
            duration: total_duration,
        })
    }

    #[cfg(target_os = "android")]
    use std::sync::OnceLock;

    #[cfg(target_os = "android")]
    use tokio::sync::oneshot;

    #[cfg(target_os = "android")]
    use jni::{
        objects::{JClass, JString},
        sys::jlong,
        JNIEnv,
    };

    #[cfg(target_os = "android")]
    use crate::orchestrator::backends::android_jni::jni_string;

    #[cfg(target_os = "android")]
    static CONVERT_APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

    #[cfg(target_os = "android")]
    pub enum AndroidConvertResult {
        Completed {
            output_path: String,
            filesize: u64,
            extension: String,
            duration: Option<u64>,
        },
        Failed(String),
    }

    #[cfg(target_os = "android")]
    static PENDING_CONVERT_JOBS: LazyLock<
        Mutex<HashMap<String, oneshot::Sender<AndroidConvertResult>>>,
    > = LazyLock::new(|| Mutex::new(HashMap::new()));

    #[cfg(target_os = "android")]
    fn lock_pending_converts(
    ) -> std::sync::MutexGuard<'static, HashMap<String, oneshot::Sender<AndroidConvertResult>>>
    {
        PENDING_CONVERT_JOBS.lock().unwrap_or_else(|e| {
            warn!("PENDING_CONVERT_JOBS mutex was poisoned, recovering");
            e.into_inner()
        })
    }

    #[cfg(target_os = "android")]
    fn register_pending_convert(job_id: &str) -> oneshot::Receiver<AndroidConvertResult> {
        let (tx, rx) = oneshot::channel();
        let mut pending = lock_pending_converts();
        pending.insert(job_id.to_string(), tx);
        rx
    }

    #[cfg(target_os = "android")]
    fn signal_convert_completed(
        job_id: &str,
        output_path: String,
        filesize: u64,
        extension: String,
        duration: Option<u64>,
    ) {
        let mut pending = lock_pending_converts();
        if let Some(tx) = pending.remove(job_id) {
            let _ = tx.send(AndroidConvertResult::Completed {
                output_path,
                filesize,
                extension,
                duration,
            });
        }
    }

    #[cfg(target_os = "android")]
    fn signal_convert_failed(job_id: &str, error: String) {
        let mut pending = lock_pending_converts();
        if let Some(tx) = pending.remove(job_id) {
            let _ = tx.send(AndroidConvertResult::Failed(error));
        }
    }

    #[cfg(target_os = "android")]
    async fn run_ffmpeg_concat_android(
        concat_list_path: &Path,
        output_path: &str,
    ) -> Result<(), String> {
        use crate::orchestrator::backends::android_jni::get_jni_env;
        use jni::objects::{JClass, JValue};

        let concat_list_str = concat_list_path.to_string_lossy().to_string();
        let output = output_path.to_string();

        tokio::task::spawn_blocking(move || {
            let mut env = get_jni_env()?;

            let cls = crate::orchestrator::backends::android_jni::get_rust_bridge_class()?;

            let j_concat = env
                .new_string(&concat_list_str)
                .map_err(|e| format!("JNI string error: {}", e))?;
            let j_output = env
                .new_string(&output)
                .map_err(|e| format!("JNI string error: {}", e))?;

            let result = env
                .call_static_method(
                    <&JClass>::from(cls.as_obj()),
                    "concatFiles",
                    "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(&j_concat), JValue::Object(&j_output)],
                )
                .map_err(|e| format!("JNI concatFiles call failed: {}", e))?;

            let jstr = result
                .l()
                .map_err(|e| format!("JNI return error: {}", e))?;

            if jstr.is_null() {
                Ok(())
            } else {
                let error: String = env
                    .get_string((&jstr).into())
                    .map_err(|e| format!("JNI string read error: {}", e))?
                    .into();
                Err(format!("FFmpeg concat failed: {}", error))
            }
        })
        .await
        .map_err(|e| format!("JNI task panicked: {}", e))?
    }

    #[cfg(target_os = "android")]
    async fn run_ffmpeg_convert_android(
        app: &AppHandle,
        job_id: &str,
        source_path: &str,
        output_path: &Path,
        post_args: Vec<String>,
    ) -> Result<ConvertExecResult, String> {
        use crate::orchestrator::backends::android_jni::{
            get_activity, get_jni_env, wait_for_jni_ready,
        };
        use jni::objects::JValue;

        let _ = CONVERT_APP_HANDLE.set(app.clone());

        if !wait_for_jni_ready(10000).await {
            return Err("Android JNI bridge not ready".to_string());
        }

        let request_json = serde_json::json!({
            "source_path": source_path,
            "output_path": output_path.to_string_lossy(),
            "args": post_args,
        })
        .to_string();

        let rx = register_pending_convert(job_id);

        let job_id_clone = job_id.to_string();
        let jni_result = tokio::task::spawn_blocking(move || {
            let mut env = get_jni_env()?;
            let activity = get_activity()?;

            let j_job_id = env
                .new_string(&job_id_clone)
                .map_err(|e: jni::errors::Error| e.to_string())?;
            let j_request = env
                .new_string(&request_json)
                .map_err(|e: jni::errors::Error| e.to_string())?;
            let j_backend = env
                .new_string("ffmpeg")
                .map_err(|e: jni::errors::Error| e.to_string())?;
            let j_title = env
                .new_string("Converting...")
                .map_err(|e: jni::errors::Error| e.to_string())?;

            env.call_method(
                activity.as_obj(),
                "startJobFromRust",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
                &[
                    JValue::Object(&j_job_id),
                    JValue::Object(&j_backend),
                    JValue::Object(&j_request),
                    JValue::Object(&j_title),
                ],
            )
            .map_err(|e| format!("JNI call failed: {}", e))?;

            Ok::<(), String>(())
        })
        .await
        .map_err(|e| format!("JNI task panicked: {}", e))?;

        if let Err(e) = jni_result {
            let mut pending = lock_pending_converts();
            pending.remove(job_id);
            return Err(e);
        }

        match rx.await {
            Ok(AndroidConvertResult::Completed { duration, .. }) => Ok(ConvertExecResult {
                duration: duration.map(|d| d as f64),
            }),
            Ok(AndroidConvertResult::Failed(e)) => Err(e),
            Err(_) => Err("Conversion channel closed".to_string()),
        }
    }

    #[cfg(target_os = "android")]
    #[no_mangle]
    pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnConvertProgress<'local>(
        mut env: JNIEnv<'local>,
        _class: JClass<'local>,
        job_id: JString<'local>,
        progress: f32,
        speed: JString<'local>,
    ) {
        let job_id = jni_string!(env, job_id);
        let speed: Option<String> = if speed.is_null() {
            None
        } else {
            env.get_string(&speed).ok().map(|s| String::from(s))
        };

        if let Some(app) = CONVERT_APP_HANDLE.get() {
            let _ = app.emit(
                "convert-progress",
                ConvertProgress {
                    job_id,
                    progress: progress as f64,
                    time_processed: 0.0,
                    total_duration: None,
                    speed,
                },
            );
        }
    }

    #[cfg(target_os = "android")]
    #[no_mangle]
    pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnConvertCompleted<'local>(
        mut env: JNIEnv<'local>,
        _class: JClass<'local>,
        job_id: JString<'local>,
        output_path: JString<'local>,
        filesize: jlong,
        extension: JString<'local>,
        duration: jlong,
    ) {
        let job_id = jni_string!(env, job_id);
        let output_path = jni_string!(env, output_path);
        let extension: String = env
            .get_string(&extension)
            .ok()
            .map(|s| String::from(s))
            .unwrap_or_default();
        let duration = if duration > 0 {
            Some(duration as u64)
        } else {
            None
        };

        tracing::info!("JNI: convert completed for {} -> {}", job_id, output_path);
        signal_convert_completed(&job_id, output_path, filesize as u64, extension, duration);
    }

    #[cfg(target_os = "android")]
    #[no_mangle]
    pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnConvertFailed<'local>(
        mut env: JNIEnv<'local>,
        _class: JClass<'local>,
        job_id: JString<'local>,
        error: JString<'local>,
    ) {
        let job_id = jni_string!(env, job_id);
        let error = jni_string!(env, error, "Unknown error".to_string());

        tracing::error!("JNI: convert failed for {} - {}", job_id, error);
        signal_convert_failed(&job_id, error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn test_convert_request(format: &str) -> ConvertRequest {
        ConvertRequest {
            job_id: Some("test".into()),
            source_path: "/tmp/input.mp4".into(),
            target_format: format.into(),
            output_directory: None,
            output_filename: None,
            audio_only: false,
            extra_args: None,
            ffmpeg: None,
            metadata: None,
        }
    }

    #[test]
    fn test_ffmpeg_no_hw_accel() {
        let req = test_convert_request("mp4");
        let (pre, post) = build_ffmpeg_components(&req, false);
        assert!(pre.is_empty());
        assert!(post.contains(&"-c:v".to_string()));
        assert!(post.contains(&"libx264".to_string()));
    }

    #[test]
    fn test_ffmpeg_nvenc() {
        let mut req = test_convert_request("mp4");
        req.ffmpeg = Some(FfmpegConvertSettings {
            hw_accel: "nvenc".into(),
        });
        let (pre, post) = build_ffmpeg_components(&req, false);
        assert!(pre.contains(&"-hwaccel".to_string()));
        assert!(pre.contains(&"cuda".to_string()));
        assert!(post.contains(&"h264_nvenc".to_string()));
    }

    #[test]
    fn test_ffmpeg_force_software() {
        let mut req = test_convert_request("mp4");
        req.ffmpeg = Some(FfmpegConvertSettings {
            hw_accel: "nvenc".into(),
        });
        let (pre, post) = build_ffmpeg_components(&req, true);
        assert!(!pre.contains(&"cuda".to_string()));
        assert!(post.contains(&"libx264".to_string()));
    }

    #[test]
    fn test_ffmpeg_qsv() {
        let mut req = test_convert_request("mp4");
        req.ffmpeg = Some(FfmpegConvertSettings {
            hw_accel: "qsv".into(),
        });
        let (pre, post) = build_ffmpeg_components(&req, false);
        assert!(pre.contains(&"qsv".to_string()));
        assert!(post.contains(&"h264_qsv".to_string()));
    }

    #[test]
    fn test_ffmpeg_amf() {
        let mut req = test_convert_request("mp4");
        req.ffmpeg = Some(FfmpegConvertSettings {
            hw_accel: "amf".into(),
        });
        let (pre, post) = build_ffmpeg_components(&req, false);
        assert!(pre.contains(&"d3d11va".to_string()));
        assert!(post.contains(&"h264_amf".to_string()));
    }

    #[test]
    fn test_ffmpeg_videotoolbox() {
        let mut req = test_convert_request("mp4");
        req.ffmpeg = Some(FfmpegConvertSettings {
            hw_accel: "videotoolbox".into(),
        });
        let (pre, post) = build_ffmpeg_components(&req, false);
        assert!(pre.contains(&"videotoolbox".to_string()));
        assert!(post.contains(&"h264_videotoolbox".to_string()));
    }

    #[test]
    fn test_ffmpeg_auto_accel() {
        let mut req = test_convert_request("mp4");
        req.ffmpeg = Some(FfmpegConvertSettings {
            hw_accel: "auto".into(),
        });
        let (pre, _) = build_ffmpeg_components(&req, false);
        assert!(pre.contains(&"auto".to_string()));
    }

    #[test]
    fn test_ffmpeg_audio_only_mp3() {
        let mut req = test_convert_request("mp3");
        req.audio_only = true;
        let (_, post) = build_ffmpeg_components(&req, false);
        assert!(post.contains(&"-vn".to_string()));
        assert!(post.contains(&"libmp3lame".to_string()));
    }

    #[test]
    fn test_ffmpeg_audio_only_opus() {
        let mut req = test_convert_request("opus");
        req.audio_only = true;
        let (_, post) = build_ffmpeg_components(&req, false);
        assert!(post.contains(&"libopus".to_string()));
    }

    #[test]
    fn test_ffmpeg_audio_only_flac() {
        let mut req = test_convert_request("flac");
        req.audio_only = true;
        let (_, post) = build_ffmpeg_components(&req, false);
        assert!(post.contains(&"flac".to_string()));
    }

    #[test]
    fn test_ffmpeg_audio_only_wav() {
        let mut req = test_convert_request("wav");
        req.audio_only = true;
        let (_, post) = build_ffmpeg_components(&req, false);
        assert!(post.contains(&"pcm_s16le".to_string()));
    }

    #[test]
    fn test_ffmpeg_audio_only_aac() {
        let mut req = test_convert_request("m4a");
        req.audio_only = true;
        let (_, post) = build_ffmpeg_components(&req, false);
        assert!(post.contains(&"aac".to_string()));
        assert!(post.contains(&"192k".to_string()));
    }

    #[test]
    fn test_ffmpeg_mp4_faststart() {
        let req = test_convert_request("mp4");
        let (_, post) = build_ffmpeg_components(&req, false);
        assert!(post.contains(&"+faststart".to_string()));
        assert!(post.contains(&"aac".to_string()));
    }

    #[test]
    fn test_ffmpeg_webm() {
        let req = test_convert_request("webm");
        let (_, post) = build_ffmpeg_components(&req, false);
        assert!(post.contains(&"libvpx-vp9".to_string()));
        assert!(post.contains(&"libopus".to_string()));
    }

    #[test]
    fn test_ffmpeg_mkv_copy() {
        let req = test_convert_request("mkv");
        let (_, post) = build_ffmpeg_components(&req, false);
        let copy_count = post.iter().filter(|a| *a == "copy").count();
        assert_eq!(copy_count, 2);
    }

    #[test]
    fn test_ffmpeg_gif() {
        let req = test_convert_request("gif");
        let (_, post) = build_ffmpeg_components(&req, false);
        assert!(post.contains(&"gif".to_string()));
        assert!(post.iter().any(|a| a.contains("fps=15")));
    }

    #[test]
    fn test_ffmpeg_avi() {
        let req = test_convert_request("avi");
        let (_, post) = build_ffmpeg_components(&req, false);
        assert!(post.contains(&"libxvid".to_string()));
        assert!(post.contains(&"libmp3lame".to_string()));
    }

    #[test]
    fn test_ffmpeg_extra_args() {
        let mut req = test_convert_request("mp4");
        req.extra_args = Some(vec!["-threads".to_string(), "4".to_string()]);
        let (_, post) = build_ffmpeg_components(&req, false);
        assert!(post.contains(&"-threads".to_string()));
        assert!(post.contains(&"4".to_string()));
    }
}
