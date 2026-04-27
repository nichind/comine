use std::sync::{LazyLock, Mutex};
use tauri::AppHandle;
use tokio::io::AsyncBufReadExt;
use tracing::{debug, info, warn};

struct ActiveStream {
    temp_file: std::path::PathBuf,
    child: Option<tokio::process::Child>,
}

static ACTIVE_STREAM: LazyLock<Mutex<Option<ActiveStream>>> =
    LazyLock::new(|| Mutex::new(None));

/// Tear down the current active stream (kill FFmpeg, delete temp file).
fn kill_active_stream() {
    let old = {
        let mut guard = crate::utils::lock_or_recover(&ACTIVE_STREAM);
        guard.take()
    };

    if let Some(mut stream) = old {
        if let Some(ref mut child) = stream.child {
            let _ = child.start_kill();
        }
        if let Err(e) = std::fs::remove_file(&stream.temp_file) {
            debug!("Temp file cleanup: {}", e);
        }
        info!("Media stream torn down");
    }
}

/// Probe the video codec of the first video stream using ffprobe.
#[cfg(desktop)]
async fn probe_video_codec(app: &AppHandle, file_path: &str) -> Option<String> {
    use std::process::Stdio;

    let ffprobe_path = crate::deps::resolve_ffprobe_path(app)?;
    let mut cmd = crate::utils::new_command(&ffprobe_path);
    cmd.args([
        "-v", "error",
        "-select_streams", "v:0",
        "-show_entries", "stream=codec_name",
        "-of", "csv=p=0",
    ])
    .arg(file_path)
    .stdout(Stdio::piped())
    .stderr(Stdio::null());

    let output = cmd.output().await.ok()?;
    let codec = String::from_utf8_lossy(&output.stdout).trim().to_lowercase().to_string();
    if codec.is_empty() { None } else { Some(codec) }
}

/// Remux a media file to a fragmented MP4 temp file using FFmpeg.
///
/// Returns the path to the temp file. The frontend should use `convertFileSrc()`
/// on this path to get an asset:// URL that WebKit can play with full Range
/// request support.
#[cfg(desktop)]
#[tauri::command]
pub async fn start_media_stream(app: AppHandle, file_path: String) -> Result<String, String> {
    use std::process::Stdio;
    use std::time::Duration;

    info!(path = %file_path, "start_media_stream called");

    let ffmpeg_path = crate::deps::resolve_ffmpeg_path(&app).ok_or_else(|| {
        "FFmpeg not installed. Please install it from Settings \u{2192} Dependencies.".to_string()
    })?;

    if !std::path::Path::new(&file_path).exists() {
        return Err(format!("File not found: {}", file_path));
    }

    // Probe video codec to determine if transcoding is needed.
    let video_codec = probe_video_codec(&app, &file_path).await;
    let needs_transcode = matches!(video_codec.as_deref(), Some("hevc" | "h265"));
    if needs_transcode {
        info!(codec = ?video_codec, "HEVC detected — will transcode to H.264");
    }
    // Transcoding needs more time than remuxing.
    let ffmpeg_timeout = if needs_transcode {
        Duration::from_secs(600)
    } else {
        Duration::from_secs(120)
    };

    // Tear down any previous stream first.
    kill_active_stream();

    // Temp file for FFmpeg's fragmented MP4 output.
    let temp_file = std::env::temp_dir().join(format!(
        "comine_stream_{}.mp4",
        std::process::id()
    ));

    // Retry loop: if FFmpeg fails quickly (e.g. file still downloading, headers
    // incomplete), wait for more data and retry.
    const MAX_ATTEMPTS: u32 = 6;
    const RETRY_DELAY: Duration = Duration::from_secs(3);
    let mut last_error = String::new();
    let mut prev_file_size: u64 = std::fs::metadata(&file_path)
        .map(|m| m.len())
        .unwrap_or(0);

    for attempt in 0..MAX_ATTEMPTS {
        if attempt > 0 {
            let current_size = std::fs::metadata(&file_path)
                .map(|m| m.len())
                .unwrap_or(0);
            if current_size == prev_file_size {
                info!(attempt, "Source file not growing, stopping retries");
                break;
            }
            prev_file_size = current_size;
            info!(attempt, size = current_size, "Retrying FFmpeg (file still growing)");
            tokio::time::sleep(RETRY_DELAY).await;
        }

        let _ = std::fs::remove_file(&temp_file);

        // Spawn FFmpeg: remux (or transcode) into fragmented MP4.
        let mut cmd = crate::utils::new_command(&ffmpeg_path);
        cmd.arg("-i").arg(&file_path);

        if needs_transcode {
            // HEVC → transcode video to H.264 for WebKit compatibility.
            // ultrafast preset minimizes encoding time; CRF 23 balances quality/size.
            cmd.args(["-c:v", "libx264", "-preset", "ultrafast", "-crf", "23"]);
            // Copy audio if possible (AAC/MP3/Opus work in MP4).
            cmd.args(["-c:a", "copy"]);
        } else {
            cmd.args(["-c", "copy"]);
        }

        cmd.args([
            "-movflags",
            "frag_keyframe+empty_moov+default_base_moof",
            "-f",
            "mp4",
            "-y",
        ])
        .arg(&temp_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                last_error = format!("Failed to spawn FFmpeg: {}", e);
                continue;
            }
        };

        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(log_ffmpeg_stderr(stderr));
        }

        // Wait for FFmpeg to finish. Timeout varies: 120s for remux, 600s for transcode.
        let result = tokio::time::timeout(ffmpeg_timeout, child.wait()).await;

        match result {
            Ok(Ok(status)) if status.success() => {
                // Verify the temp file has content.
                match std::fs::metadata(&temp_file) {
                    Ok(m) if m.len() > 0 => {
                        let path_str = temp_file.to_string_lossy().to_string();
                        info!(path = %path_str, size = m.len(), "Remux complete");

                        // Store for cleanup on close.
                        {
                            let mut guard = crate::utils::lock_or_recover(&ACTIVE_STREAM);
                            *guard = Some(ActiveStream {
                                temp_file,
                                child: None,
                            });
                        }

                        return Ok(path_str);
                    }
                    _ => {
                        last_error = "FFmpeg produced empty output.".to_string();
                    }
                }
            }
            Ok(Ok(status)) => {
                last_error = format!("FFmpeg exited with status: {}", status);
            }
            Ok(Err(e)) => {
                last_error = format!("FFmpeg wait error: {}", e);
            }
            Err(_) => {
                // Timeout — kill the child. This may happen for in-progress downloads
                // where FFmpeg keeps reading as the source file grows.
                // Return what we have so far if the temp file has content.
                let _ = child.start_kill();
                let _ = child.wait().await;

                match std::fs::metadata(&temp_file) {
                    Ok(m) if m.len() > 0 => {
                        let path_str = temp_file.to_string_lossy().to_string();
                        info!(
                            path = %path_str,
                            size = m.len(),
                            "FFmpeg timed out but produced partial output, using it"
                        );

                        {
                            let mut guard = crate::utils::lock_or_recover(&ACTIVE_STREAM);
                            *guard = Some(ActiveStream {
                                temp_file,
                                child: None,
                            });
                        }

                        return Ok(path_str);
                    }
                    _ => {
                        last_error =
                            "FFmpeg timed out without producing output.".to_string();
                    }
                }
            }
        }
    }

    // All retries exhausted.
    let _ = std::fs::remove_file(&temp_file);
    Err(last_error)
}

/// Log FFmpeg stderr output for debugging.
#[cfg(desktop)]
async fn log_ffmpeg_stderr(stderr: tokio::process::ChildStderr) {
    let reader = tokio::io::BufReader::new(stderr);
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.starts_with("frame=") || line.starts_with("size=") || line.starts_with("speed=") {
            debug!(target: "ffmpeg", "{}", line);
        } else {
            warn!(target: "ffmpeg", "{}", line);
        }
    }
}

/// Stop the currently active media stream, killing FFmpeg and freeing resources.
#[cfg(desktop)]
#[tauri::command]
pub async fn stop_media_stream() -> Result<(), String> {
    info!("stop_media_stream called");
    kill_active_stream();
    Ok(())
}

// Mobile stubs — FFmpeg is not available on mobile targets.

#[cfg(mobile)]
#[tauri::command]
pub async fn start_media_stream(_app: AppHandle, _file_path: String) -> Result<String, String> {
    Err("Media streaming is not supported on mobile".to_string())
}

#[cfg(mobile)]
#[tauri::command]
pub async fn stop_media_stream() -> Result<(), String> {
    Err("Media streaming is not supported on mobile".to_string())
}
