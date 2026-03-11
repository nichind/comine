use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, LazyLock, Mutex,
};
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

struct ActiveStream {
    port: u16,
    temp_file: std::path::PathBuf,
    cancel: CancellationToken,
}

static ACTIVE_STREAM: LazyLock<Mutex<Option<ActiveStream>>> =
    LazyLock::new(|| Mutex::new(None));

/// Tear down the current active stream (kill FFmpeg, delete temp file, stop tasks).
fn kill_active_stream() {
    let old = {
        let mut guard = crate::utils::lock_or_recover(&ACTIVE_STREAM);
        guard.take()
    };

    if let Some(stream) = old {
        // Signal all tasks (child watcher, accept loop, client handlers) to stop.
        stream.cancel.cancel();
        // Best-effort temp file cleanup (on Unix, unlink succeeds even if open).
        if let Err(e) = std::fs::remove_file(&stream.temp_file) {
            debug!("Temp file cleanup: {}", e);
        }
        info!("Media stream on port {} torn down", stream.port);
    }
}

/// Start a real-time FFmpeg remux to a temp file and serve it via HTTP.
///
/// Returns a URL of the form `http://127.0.0.1:<port>/stream` that the
/// HTML5 video player can open directly.
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

    // Tear down any previous stream first.
    kill_active_stream();

    // Bind to an OS-assigned port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to bind TCP listener: {}", e))?;

    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local address: {}", e))?
        .port();

    info!(port, "Media stream TCP listener bound");

    // Temp file for FFmpeg's fragmented MP4 output.
    let temp_file = std::env::temp_dir().join(format!("comine_stream_{}.mp4", port));

    // Spawn FFmpeg: remux into fragmented MP4 written to temp file.
    let mut cmd = crate::utils::new_command(&ffmpeg_path);
    cmd.arg("-i")
        .arg(&file_path)
        .args([
            "-c",
            "copy",
            "-movflags",
            "frag_keyframe+empty_moov+default_base_moof",
            "-f",
            "mp4",
            "-y", // overwrite temp file if it exists
        ])
        .arg(&temp_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg: {}", e))?;

    // Capture stderr for debugging.
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(log_ffmpeg_stderr(stderr));
    }

    let cancel = CancellationToken::new();
    let ffmpeg_done = Arc::new(AtomicBool::new(false));

    // Spawn child watcher: waits for FFmpeg to exit or cancel signal.
    {
        let ffmpeg_done = ffmpeg_done.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            tokio::select! {
                status = child.wait() => {
                    match status {
                        Ok(s) if s.success() => info!("FFmpeg exited successfully"),
                        Ok(s) => warn!("FFmpeg exited with status: {}", s),
                        Err(e) => warn!("FFmpeg wait error: {}", e),
                    }
                }
                _ = cancel.cancelled() => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    debug!("FFmpeg killed via cancel signal");
                }
            }
            ffmpeg_done.store(true, Ordering::Release);
        });
    }

    // Wait for FFmpeg to produce initial output (up to 5 seconds).
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(5) {
            kill_active_stream();
            return Err(
                "FFmpeg did not produce output within 5 seconds. \
                 The file may use an unsupported codec."
                    .to_string(),
            );
        }
        if ffmpeg_done.load(Ordering::Acquire) {
            // FFmpeg already exited — check if it produced any data at all.
            match std::fs::metadata(&temp_file) {
                Ok(m) if m.len() > 0 => break, // some output produced, proceed
                _ => {
                    kill_active_stream();
                    return Err(
                        "FFmpeg failed to process this file. Check logs for details.".to_string(),
                    );
                }
            }
        }
        match std::fs::metadata(&temp_file) {
            Ok(m) if m.len() > 0 => break,
            _ => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }

    info!(port, "FFmpeg producing output, starting HTTP server");

    // Store active stream state.
    {
        let mut guard = crate::utils::lock_or_recover(&ACTIVE_STREAM);
        *guard = Some(ActiveStream {
            port,
            temp_file: temp_file.clone(),
            cancel: cancel.clone(),
        });
    }

    // Spawn accept loop (handles multiple connections from WebKit).
    tokio::spawn(accept_loop(
        listener, temp_file, ffmpeg_done, cancel, port,
    ));

    let url = format!("http://127.0.0.1:{}/stream", port);
    info!(url = %url, "Media stream ready");
    Ok(url)
}

/// Accept HTTP connections in a loop and serve the media file to each.
///
/// WebKit often makes a probe request, disconnects, then reconnects for the
/// real stream. Accepting multiple connections handles this pattern.
#[cfg(desktop)]
async fn accept_loop(
    listener: tokio::net::TcpListener,
    temp_file: std::path::PathBuf,
    ffmpeg_done: Arc<AtomicBool>,
    cancel: CancellationToken,
    port: u16,
) {
    loop {
        let client_cancel = cancel.clone();
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        debug!(port, client = %addr, "Client connected");
                        tokio::spawn(handle_client(
                            stream,
                            temp_file.clone(),
                            ffmpeg_done.clone(),
                            client_cancel,
                            port,
                        ));
                    }
                    Err(e) => {
                        warn!(port, "Accept failed: {}", e);
                        break;
                    }
                }
            }
            _ = cancel.cancelled() => {
                debug!(port, "Accept loop shutdown");
                break;
            }
        }
    }
}

/// Serve the growing temp file to a single HTTP client.
///
/// Opens the temp file from the beginning and streams its contents.
/// While FFmpeg is still running, polls for new data when EOF is reached.
#[cfg(desktop)]
async fn handle_client(
    tcp_stream: tokio::net::TcpStream,
    temp_file: std::path::PathBuf,
    ffmpeg_done: Arc<AtomicBool>,
    cancel: CancellationToken,
    port: u16,
) {
    let (tcp_reader, mut tcp_writer) = tcp_stream.into_split();

    // Drain the HTTP request (we don't parse it — just consume it).
    let _ = drain_request(tcp_reader).await;

    // Send HTTP response header.
    let header = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: video/mp4\r\n",
        "Access-Control-Allow-Origin: *\r\n",
        "Cache-Control: no-cache, no-store\r\n",
        "Accept-Ranges: none\r\n",
        "Connection: close\r\n",
        "\r\n"
    );

    if let Err(e) = tcp_writer.write_all(header.as_bytes()).await {
        warn!(port, "Failed to write HTTP header: {}", e);
        return;
    }

    // Open the temp file and stream its contents.
    let mut file = match tokio::fs::File::open(&temp_file).await {
        Ok(f) => f,
        Err(e) => {
            warn!(port, "Failed to open temp file: {}", e);
            return;
        }
    };

    let mut buf = vec![0u8; 65536];
    let mut total_sent: u64 = 0;
    let mut consecutive_eofs: u32 = 0;

    loop {
        if cancel.is_cancelled() {
            debug!(port, "Client handler cancelled");
            break;
        }

        match file.read(&mut buf).await {
            Ok(0) => {
                // EOF — either FFmpeg is done or still writing.
                if ffmpeg_done.load(Ordering::Acquire) {
                    debug!(port, bytes = total_sent, "Stream complete (FFmpeg done)");
                    break;
                }
                // FFmpeg still running — wait for more data.
                consecutive_eofs += 1;
                if consecutive_eofs > 100 {
                    // 10 seconds with no new data — give up.
                    warn!(port, "No new data for 10 seconds, closing client");
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Ok(n) => {
                consecutive_eofs = 0;
                if let Err(e) = tcp_writer.write_all(&buf[..n]).await {
                    debug!(port, bytes = total_sent, "Client disconnected: {}", e);
                    break;
                }
                total_sent += n as u64;
            }
            Err(e) => {
                warn!(port, "File read error: {}", e);
                break;
            }
        }
    }

    let _ = tcp_writer.flush().await;
    let _ = tcp_writer.shutdown().await;
}

/// Log FFmpeg stderr output for debugging.
#[cfg(desktop)]
async fn log_ffmpeg_stderr(stderr: tokio::process::ChildStderr) {
    let reader = tokio::io::BufReader::new(stderr);
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        // FFmpeg progress lines (frame=, size=, etc.) -> debug level.
        if line.starts_with("frame=") || line.starts_with("size=") || line.starts_with("speed=") {
            debug!(target: "ffmpeg", "{}", line);
        } else {
            warn!(target: "ffmpeg", "{}", line);
        }
    }
}

/// Drain the incoming HTTP request from the TCP read half.
/// Also logs the first line (method + path) for debugging.
#[cfg(desktop)]
async fn drain_request(mut reader: tokio::net::tcp::OwnedReadHalf) {
    let mut buf = [0u8; 4096];
    let mut accumulated = Vec::with_capacity(512);
    loop {
        match tokio::time::timeout(
            std::time::Duration::from_millis(500),
            reader.read(&mut buf),
        )
        .await
        {
            Ok(Ok(0)) | Err(_) => break, // EOF or timeout
            Ok(Ok(n)) => {
                accumulated.extend_from_slice(&buf[..n]);
                if accumulated.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
                if accumulated.len() > 16 * 1024 {
                    break;
                }
            }
            Ok(Err(_)) => break,
        }
    }
    // Log request line for debugging.
    if let Ok(text) = std::str::from_utf8(&accumulated) {
        if let Some(first_line) = text.lines().next() {
            debug!(target: "media_stream", "HTTP request: {}", first_line);
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
