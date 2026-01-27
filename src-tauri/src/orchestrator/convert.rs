//! Local file conversion using FFmpeg.
//! Converts existing files to different formats without downloading.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

struct ActiveConversion {
    cancel_token: CancellationToken,
    #[allow(dead_code)]
    output_path: PathBuf,
}

lazy_static::lazy_static! {
    static ref ACTIVE_CONVERSIONS: Mutex<HashMap<String, ActiveConversion>> = Mutex::new(HashMap::new());
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename_all = "camelCase"))]
pub struct FfmpegConvertSettings {
    /// Hardware acceleration: auto, none, nvenc, qsv, amf, videotoolbox
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

async fn get_media_duration(ffprobe_path: &Path, file_path: &str) -> Option<f64> {
    let mut cmd = tokio::process::Command::new(ffprobe_path);
    cmd.args([
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-of",
        "default=noprint_wrappers=1:nokey=1",
        file_path,
    ]);

    #[cfg(target_os = "windows")]
    {
        use crate::utils::CommandHideConsole;
        cmd.hide_console();
    }

    let output = cmd.output().await.ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse().ok()
}

fn get_file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

#[tauri::command]
pub async fn convert_local_file(
    app: AppHandle,
    request: ConvertRequest,
) -> Result<ConvertResult, String> {
    #[cfg(target_os = "android")]
    {
        let _ = CONVERT_APP_HANDLE.set(app.clone());
        return convert_local_file_android(app, request).await;
    }

    #[cfg(not(target_os = "android"))]
    {
        convert_local_file_desktop(app, request).await
    }
}

#[cfg(not(target_os = "android"))]
async fn convert_local_file_desktop(
    app: AppHandle,
    request: ConvertRequest,
) -> Result<ConvertResult, String> {
    let job_id = request
        .job_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    info!(
        job_id = %job_id,
        source = %request.source_path,
        target = %request.target_format,
        "Starting local file conversion"
    );

    let ffmpeg_path = crate::deps::get_ffmpeg_path(&app)?;

    if !ffmpeg_path.exists() {
        return Err(
            "FFmpeg not installed. Please install it from Settings → Dependencies.".to_string(),
        );
    }

    // Determine ffprobe path (same directory as ffmpeg)
    let ffprobe_path = ffmpeg_path
        .parent()
        .map(|p| {
            #[cfg(target_os = "windows")]
            {
                p.join("ffprobe.exe")
            }
            #[cfg(not(target_os = "windows"))]
            {
                p.join("ffprobe")
            }
        })
        .unwrap_or_else(|| PathBuf::from("ffprobe"));

    // Verify source file exists
    let source_path = Path::new(&request.source_path);
    if !source_path.exists() {
        return Err(format!("Source file not found: {}", request.source_path));
    }

    // Get source duration for progress calculation
    let total_duration = if ffprobe_path.exists() {
        get_media_duration(&ffprobe_path, &request.source_path).await
    } else {
        None
    };

    // Determine output path
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

    // Avoid overwriting the source file
    let output_path = if output_path == source_path {
        output_dir.join(format!(
            "{}_converted.{}",
            output_filename, request.target_format
        ))
    } else {
        // If file exists, add a suffix
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

    // Build FFmpeg command
    let mut cmd = tokio::process::Command::new(&ffmpeg_path);

    // Get FFmpeg settings with defaults
    let ffmpeg_settings = request.ffmpeg.clone().unwrap_or_default();

    // Add hardware acceleration if specified
    match ffmpeg_settings.hw_accel.as_str() {
        "nvenc" => {
            cmd.args(["-hwaccel", "cuda"]);
        }
        "qsv" => {
            cmd.args(["-hwaccel", "qsv"]);
        }
        "amf" => {
            cmd.args(["-hwaccel", "d3d11va"]);
        }
        "videotoolbox" => {
            cmd.args(["-hwaccel", "videotoolbox"]);
        }
        "auto" => {
            cmd.args(["-hwaccel", "auto"]);
        }
        _ => {} // none or unknown - no hw accel
    }

    cmd.arg("-i").arg(&request.source_path);
    cmd.arg("-y"); // Overwrite output
    cmd.arg("-progress").arg("pipe:1"); // Output progress to stdout
    cmd.arg("-stats_period").arg("0.5"); // Update every 0.5 seconds

    // Add format-specific options with sensible defaults
    if request.audio_only {
        cmd.arg("-vn"); // No video
        match request.target_format.as_str() {
            "mp3" => {
                cmd.args(["-c:a", "libmp3lame", "-q:a", "2"]);
            }
            "m4a" | "aac" => {
                cmd.args(["-c:a", "aac", "-b:a", "192k"]);
            }
            "opus" => {
                cmd.args(["-c:a", "libopus", "-b:a", "128k"]);
            }
            "flac" => {
                cmd.args(["-c:a", "flac"]);
            }
            "wav" => {
                cmd.args(["-c:a", "pcm_s16le"]);
            }
            _ => {
                cmd.args(["-c:a", "copy"]);
            }
        }
    } else {
        // Determine video codec based on hw accel and target format
        let (video_codec, video_args): (&str, Vec<&str>) = match (
            ffmpeg_settings.hw_accel.as_str(),
            request.target_format.as_str(),
        ) {
            // NVENC
            ("nvenc", "mp4" | "mov") => ("h264_nvenc", vec!["-rc", "constqp", "-qp", "23"]),
            ("nvenc", "mkv") => ("h264_nvenc", vec!["-rc", "constqp", "-qp", "23"]),
            // QSV
            ("qsv", "mp4" | "mov") => ("h264_qsv", vec!["-global_quality", "23"]),
            ("qsv", "mkv") => ("h264_qsv", vec!["-global_quality", "23"]),
            // AMF
            ("amf", "mp4" | "mov") => {
                ("h264_amf", vec!["-rc", "cqp", "-qp_i", "23", "-qp_p", "23"])
            }
            ("amf", "mkv") => ("h264_amf", vec!["-rc", "cqp", "-qp_i", "23", "-qp_p", "23"]),
            // VideoToolbox
            ("videotoolbox", "mp4" | "mov") => ("h264_videotoolbox", vec!["-q:v", "65"]),
            ("videotoolbox", "mkv") => ("h264_videotoolbox", vec!["-q:v", "65"]),
            // Software encoding based on target format
            (_, "mp4" | "mov") => ("libx264", vec!["-preset", "medium", "-crf", "23"]),
            (_, "webm") => ("libvpx-vp9", vec!["-crf", "30", "-b:v", "0"]),
            (_, "mkv") => ("copy", vec![]),
            (_, "avi") => ("libxvid", vec!["-q:v", "5"]),
            (_, "gif") => ("gif", vec!["-vf", "fps=15,scale=480:-1:flags=lanczos"]),
            _ => ("copy", vec![]),
        };

        cmd.args(["-c:v", video_codec]);
        for arg in video_args {
            cmd.arg(arg);
        }

        // Audio codec based on target format
        match request.target_format.as_str() {
            "mp4" | "mov" | "m4a" => {
                cmd.args(["-c:a", "aac", "-b:a", "192k"]);
            }
            "webm" => {
                cmd.args(["-c:a", "libopus", "-b:a", "128k"]);
            }
            "mkv" => {
                cmd.args(["-c:a", "copy"]);
            }
            "avi" => {
                cmd.args(["-c:a", "libmp3lame", "-b:a", "192k"]);
            }
            "gif" => {
                // GIF has no audio
            }
            _ => {
                cmd.args(["-c:a", "copy"]);
            }
        }

        // Fast start for MP4/MOV
        if request.target_format == "mp4" || request.target_format == "mov" {
            cmd.args(["-movflags", "+faststart"]);
        }
    }

    // Add extra arguments if provided
    if let Some(extra) = &request.extra_args {
        for arg in extra {
            cmd.arg(arg);
        }
    }

    cmd.arg(&output_path);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use crate::utils::CommandHideConsole;
        cmd.hide_console();
    }

    // Create cancellation token and register the conversion
    let cancel_token = CancellationToken::new();
    {
        let mut conversions = ACTIVE_CONVERSIONS
            .lock()
            .map_err(|e| format!("Failed to lock conversions: {}", e))?;
        conversions.insert(
            job_id.clone(),
            ActiveConversion {
                cancel_token: cancel_token.clone(),
                output_path: output_path.clone(),
            },
        );
    }

    // Helper to clean up on exit
    let cleanup = |job_id: &str| {
        let mut conversions = ACTIVE_CONVERSIONS.lock().ok();
        if let Some(ref mut c) = conversions {
            c.remove(job_id);
        }
    };

    // Spawn the process
    let mut child = cmd.spawn().map_err(|e| {
        cleanup(&job_id);
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

    // Emit initial progress
    let _ = app.emit(
        "convert-progress",
        ConvertProgress {
            job_id: job_id.clone(),
            progress: 0.0,
            time_processed: 0.0,
            total_duration,
            speed: None,
        },
    );

    // Parse FFmpeg progress output
    let job_id_clone = job_id.clone();
    let app_clone = app.clone();
    let progress_task = tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        // Regex to parse time from progress output
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

            // Emit progress when we have time info
            if current_time_ms > 0 {
                let time_secs = current_time_ms as f64 / 1_000_000.0;
                let progress = if let Some(total) = total_duration {
                    ((time_secs / total) * 100.0).min(100.0)
                } else {
                    0.0
                };

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

    // Capture stderr for error messages
    let stderr_task = tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        let mut stderr_output = String::new();

        while let Ok(Some(line)) = lines.next_line().await {
            stderr_output.push_str(&line);
            stderr_output.push('\n');
        }

        stderr_output
    });

    // Wait for FFmpeg to complete or cancellation
    let result = tokio::select! {
        status = child.wait() => {
            status.map_err(|e| format!("FFmpeg process error: {}", e))
        }
        _ = cancel_token.cancelled() => {
            warn!(job_id = %job_id, "Conversion cancelled, killing FFmpeg process");
            let _ = child.kill().await;
            // Clean up partial output file
            if output_path.exists() {
                let _ = std::fs::remove_file(&output_path);
            }
            cleanup(&job_id);
            return Err("Conversion cancelled".to_string());
        }
    };

    // Clean up tracking
    cleanup(&job_id);

    // Wait for tasks
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

    // Verify output file exists
    if !output_path.exists() {
        return Err("Conversion completed but output file not found".to_string());
    }

    let filesize = get_file_size(&output_path);
    let output_path_str = output_path.to_string_lossy().to_string();

    // Emit completion progress
    let _ = app.emit(
        "convert-progress",
        ConvertProgress {
            job_id: job_id.clone(),
            progress: 100.0,
            time_processed: total_duration.unwrap_or(0.0),
            total_duration,
            speed: None,
        },
    );

    info!(
        job_id = %job_id,
        output = %output_path_str,
        size = %filesize,
        "Conversion completed successfully"
    );

    Ok(ConvertResult {
        job_id,
        output_path: output_path_str,
        filesize,
        duration: total_duration.map(|d| d as u64),
        extension: request.target_format,
        metadata: request.metadata,
    })
}

#[allow(dead_code)]
pub fn get_conversion_formats(source_extension: &str) -> Vec<&'static str> {
    let source_ext = source_extension.to_lowercase();

    let video_exts = ["mp4", "webm", "mkv", "avi", "mov", "flv", "wmv", "m4v"];
    let audio_exts = ["mp3", "m4a", "aac", "opus", "ogg", "flac", "wav", "wma"];

    let is_video = video_exts.contains(&source_ext.as_str());
    let is_audio = audio_exts.contains(&source_ext.as_str());

    if is_video {
        vec![
            "mp4", "webm", "mkv", "mov", "avi", "gif", "mp3", "m4a", "aac", "opus", "flac", "wav",
        ]
    } else if is_audio {
        vec!["mp3", "m4a", "aac", "opus", "flac", "wav", "ogg"]
    } else {
        vec!["mp4", "mp3", "m4a"]
    }
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

#[cfg(target_os = "android")]
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
static CONVERT_APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

#[cfg(target_os = "android")]
lazy_static::lazy_static! {
    static ref PENDING_CONVERT_JOBS: Mutex<HashMap<String, oneshot::Sender<AndroidConvertResult>>> =
        Mutex::new(HashMap::new());
}

#[cfg(target_os = "android")]
fn register_pending_convert(job_id: &str) -> oneshot::Receiver<AndroidConvertResult> {
    let (tx, rx) = oneshot::channel();
    let mut pending = PENDING_CONVERT_JOBS.lock().unwrap();
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
    let mut pending = PENDING_CONVERT_JOBS.lock().unwrap();
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
    let mut pending = PENDING_CONVERT_JOBS.lock().unwrap();
    if let Some(tx) = pending.remove(job_id) {
        let _ = tx.send(AndroidConvertResult::Failed(error));
    }
}

// JNI CALLBACKS FROM KOTLIN

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_nichind_comine_RustBridge_nativeOnConvertProgress<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    job_id: JString<'local>,
    progress: f32,
    speed: JString<'local>,
) {
    let job_id: String = match env.get_string(&job_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    let speed: Option<String> = if speed.is_null() {
        None
    } else {
        env.get_string(&speed).ok().map(|s| s.into())
    };

    log::debug!(
        "JNI callback: convert progress for job {} - {}%",
        job_id,
        progress
    );

    // Emit progress event using stored AppHandle
    if let Some(app) = CONVERT_APP_HANDLE.get() {
        let _ = app.emit(
            "convert-progress",
            ConvertProgress {
                job_id,
                progress: progress as f64,
                time_processed: 0.0, // Not tracked on Android
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
    let job_id: String = match env.get_string(&job_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let output_path: String = match env.get_string(&output_path) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let extension: String = match env.get_string(&extension) {
        Ok(s) => s.into(),
        Err(_) => "".to_string(),
    };
    let duration = if duration > 0 {
        Some(duration as u64)
    } else {
        None
    };

    log::info!(
        "JNI callback: convert completed for job {} -> {}",
        job_id,
        output_path
    );

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
    let job_id: String = match env.get_string(&job_id) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let error: String = match env.get_string(&error) {
        Ok(s) => s.into(),
        Err(_) => "Unknown error".to_string(),
    };

    log::error!(
        "JNI callback: convert failed for job {} - {}",
        job_id,
        error
    );

    signal_convert_failed(&job_id, error);
}

#[cfg(target_os = "android")]
pub async fn convert_local_file_android(
    _app: AppHandle,
    request: ConvertRequest,
) -> Result<ConvertResult, String> {
    use crate::orchestrator::backends::{get_activity, get_jni_env, wait_for_jni_ready};
    use jni::objects::JValue;

    if !wait_for_jni_ready(10000).await {
        return Err(
            "Android JNI bridge not ready. Please wait for the app to fully initialize."
                .to_string(),
        );
    }

    let job_id = request
        .job_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    info!(
        job_id = %job_id,
        source = %request.source_path,
        target = %request.target_format,
        "Starting Android local file conversion"
    );

    // Build request JSON for Kotlin
    let request_json = serde_json::json!({
        "source_path": request.source_path,
        "target_format": request.target_format,
        "output_directory": request.output_directory,
        "output_filename": request.output_filename,
        "audio_only": request.audio_only,
    })
    .to_string();

    // Register to receive completion callback
    let rx = register_pending_convert(&job_id);

    // Make JNI call in blocking context
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
            "convertFileWithFFmpeg",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Object(&j_job_id), JValue::Object(&j_request_json)],
        )
        .map_err(|e| format!("JNI call failed: {}", e))?;

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("JNI task panicked: {}", e))?;

    if let Err(e) = jni_result {
        let mut pending = PENDING_CONVERT_JOBS.lock().unwrap();
        pending.remove(&job_id);
        return Err(e);
    }

    info!(
        "Started Android FFmpeg conversion via JNI for job {}, waiting for completion...",
        job_id
    );

    // Wait for JNI callback
    match rx.await {
        Ok(AndroidConvertResult::Completed {
            output_path,
            filesize,
            extension,
            duration,
        }) => {
            info!(
                "Android conversion completed for job {}: {}",
                job_id, output_path
            );
            Ok(ConvertResult {
                job_id,
                output_path,
                filesize,
                duration,
                extension,
                metadata: request.metadata,
            })
        }
        Ok(AndroidConvertResult::Failed(error)) => {
            error!("Android conversion failed for job {}: {}", job_id, error);
            Err(error)
        }
        Err(_) => {
            error!(
                "Android conversion channel closed unexpectedly for job {}",
                job_id
            );
            Err("Conversion completion channel closed unexpectedly".to_string())
        }
    }
}
