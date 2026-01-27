//! aria2 backend (desktop process / Android JNI).

use std::path::PathBuf;

use async_trait::async_trait;
use log::info;

#[cfg(not(target_os = "android"))]
use log::{debug, warn};
#[cfg(not(target_os = "android"))]
use std::process::Stdio;
#[cfg(not(target_os = "android"))]
use tokio::io::{AsyncBufReadExt, BufReader};
#[cfg(not(target_os = "android"))]
use tokio::process::Command;

#[cfg(target_os = "android")]
use jni::objects::JValue;

use crate::orchestrator::backends::{
    extract_filename_from_response, extract_filename_from_url, extract_magnet_name,
    guess_mime_type, has_file_extension, is_torrent_url, Backend, BackendCapabilities,
    SpawnContext, DIRECT_FILE_EXTENSIONS,
};
use crate::orchestrator::types::*;

// Import JNI helpers from ytdlp (shared across backends)
#[cfg(target_os = "android")]
use crate::orchestrator::backends::{
    get_activity, get_jni_env, register_pending_job, AndroidJobResult, PENDING_ANDROID_JOBS,
};

struct Aria2Config {
    connections: u32,       // -x
    splits: u32,            // -s
    min_split_size: String, // -k
    speed_limit: Option<u64>,
    proxy: Option<String>,
}

impl Aria2Config {
    fn from_options(opts: &DownloadOptions, effective_speed_limit: Option<u64>) -> Self {
        let proxy = opts
            .proxy
            .as_ref()
            .and_then(|p| if p.enabled { p.url.clone() } else { None });
        Self {
            connections: opts
                .aria2_connections
                .unwrap_or(constants::DEFAULT_ARIA2_CONNECTIONS),
            splits: opts.aria2_splits.unwrap_or(constants::DEFAULT_ARIA2_SPLITS),
            min_split_size: "1M".to_string(),
            speed_limit: effective_speed_limit,
            proxy,
        }
    }
}

pub struct Aria2Backend {
    #[cfg(not(target_os = "android"))]
    binary_path: PathBuf,
}

impl Aria2Backend {
    #[cfg(not(target_os = "android"))]
    pub fn new() -> Option<Self> {
        let binary_path = find_aria2_binary()?;
        info!("aria2 backend initialized with binary: {:?}", binary_path);
        Some(Self { binary_path })
    }

    #[cfg(target_os = "android")]
    pub fn new() -> Option<Self> {
        info!("aria2 backend initialized for Android (JNI)");
        Some(Self {})
    }

    #[cfg(not(target_os = "android"))]
    #[allow(dead_code)] // Used for testing and custom setups
    pub fn with_path(binary_path: PathBuf) -> Self {
        Self { binary_path }
    }

    #[cfg(not(target_os = "android"))]
    fn build_command(&self, job: &Job, config: &Aria2Config) -> Command {
        let mut cmd = Command::new(&self.binary_path);
        let req = &job.request;

        // Output directory
        cmd.args(["-d", &req.output.directory]);

        // Output filename (if specified)
        if let Some(ref filename) = req.output.filename {
            cmd.args(["-o", filename]);
        }

        // Multi-connection settings
        cmd.args(["-x", &config.connections.to_string()]);
        cmd.args(["-s", &config.splits.to_string()]);
        cmd.args(["-k", &config.min_split_size]);

        // Resume support
        cmd.arg("--continue=true");

        // File allocation (none = fastest, but less efficient disk usage)
        cmd.arg("--file-allocation=none");

        // Don't auto-rename on conflict
        cmd.arg("--auto-file-renaming=false");

        // Allow overwriting
        cmd.arg("--allow-overwrite=true");

        // Speed limit
        if let Some(limit) = config.speed_limit {
            if limit > 0 {
                cmd.args(["--max-download-limit", &format!("{}K", limit / 1024)]);
            }
        }

        // User agent (browser-like)
        cmd.args(["--user-agent", constants::USER_AGENT]);

        // Referer header (use origin of the URL to help with hotlink protection)
        if let Ok(parsed) = url::Url::parse(&req.url) {
            if let Some(host) = parsed.host_str() {
                let referer = format!("{}://{}/", parsed.scheme(), host);
                cmd.args(["--referer", &referer]);
            }
        }

        // Proxy
        if let Some(ref proxy_url) = config.proxy {
            cmd.args(["--all-proxy", proxy_url]);
        }

        // Progress output (human readable with ETA)
        cmd.arg("--show-console-readout=true");
        cmd.arg("--summary-interval=1");

        // For torrents: listen port (randomize for NAT traversal)
        if is_torrent_url(&req.url) {
            cmd.args(["--listen-port", "6881-6999"]);
            cmd.args(["--dht-listen-port", "6881-6999"]);
            cmd.arg("--enable-dht=true");
            cmd.arg("--bt-enable-lpd=true");
            // Seed ratio 0 = stop after download (no seeding)
            cmd.args(["--seed-ratio", "0.0"]);
        }

        // The URL/magnet/torrent file
        cmd.arg(&req.url);

        cmd
    }
}

impl Default for Aria2Backend {
    fn default() -> Self {
        Self::new().expect("aria2 binary not found")
    }
}

#[async_trait]
impl Backend for Aria2Backend {
    fn name(&self) -> &str {
        "aria2"
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
        if url.starts_with("magnet:") {
            return Priority::Absolute;
        }
        if url.ends_with(".torrent") || url.contains(".torrent?") {
            return Priority::Absolute;
        }

        // High priority for FTP
        if url.starts_with("ftp://") || url.starts_with("sftp://") {
            return Priority::High;
        }

        // High priority for direct file downloads (archives, installers, ISOs, images, documents)
        if has_file_extension(url, DIRECT_FILE_EXTENSIONS) {
            return Priority::High;
        }

        // Don't compete with yt-dlp for general HTTP (streaming sites, etc.)
        Priority::None
    }

    async fn resolve(
        &self,
        url: &str,
        _settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        // For magnet links, parse the display name from dn= parameter
        if url.starts_with("magnet:") {
            return Ok(UrlInfo::simple(url, extract_magnet_name(url), "aria2"));
        }

        // For torrent files and FTP, extract filename from URL (no network request needed)
        if url.ends_with(".torrent") || url.starts_with("ftp://") || url.starts_with("sftp://") {
            return Ok(UrlInfo::with_file_info(
                url,
                Some(extract_filename_from_url(url)),
                "aria2",
                None,
                guess_mime_type(url),
            ));
        }

        // For HTTP direct file URLs, do a HEAD request to get file info
        // This runs in parallel with download start (doesn't block it)
        let client = reqwest::Client::builder()
            .user_agent(constants::USER_AGENT)
            // TODO: Add per-site headers config in settings
            // .default_headers(site_specific_headers(url))
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| BackendError::Other(e.to_string()))?;

        let resp = client
            .head(url)
            .header("Accept", "*/*")
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await
            .map_err(|e| BackendError::NetworkError(e.to_string()))?;

        if !resp.status().is_success() {
            // HEAD failed - fall back to just extracting filename from URL
            // The actual download may still work (some servers block HEAD but allow GET)
            return Ok(UrlInfo::with_file_info(
                url,
                Some(extract_filename_from_url(url)),
                "aria2",
                None,
                guess_mime_type(url),
            ));
        }

        let content_length = resp
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let filename = extract_filename_from_response(url, &resp);

        Ok(UrlInfo::with_file_info(
            url,
            Some(filename),
            "aria2",
            content_length,
            content_type,
        ))
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

#[cfg(not(target_os = "android"))]
async fn graceful_shutdown_aria2(child: &mut tokio::process::Child) {
    use std::time::Duration;

    let pid = match child.id() {
        Some(id) => id,
        None => return,
    };

    #[cfg(unix)]
    {
        let sigint_result = unsafe { libc::kill(pid as i32, libc::SIGINT) };

        if sigint_result == 0 {
            info!(target: "aria2", "Sent SIGINT to aria2 process {}, waiting for graceful exit...", pid);

            for _ in 0..30 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if child.try_wait().ok().flatten().is_some() {
                    info!(target: "aria2", "aria2 process {} exited gracefully", pid);
                    return;
                }
            }

            warn!(target: "aria2", "aria2 process {} didn't exit after SIGINT, sending SIGTERM", pid);
            unsafe { libc::kill(pid as i32, libc::SIGTERM) };

            for _ in 0..10 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
            }
        }

        let _ = child.kill().await;
    }

    #[cfg(windows)]
    {
        let taskkill = tokio::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T"])
            .output()
            .await;

        if taskkill.is_ok() {
            for _ in 0..20 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
            }
        }

        let _ = child.kill().await;
    }
}

#[cfg(not(target_os = "android"))]
impl Aria2Backend {
    async fn spawn_desktop(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        info!(target: "aria2", "Starting download job {} for URL: {}", ctx.job.id, ctx.job.request.url);

        let config = Aria2Config::from_options(&ctx.job.request.options, ctx.effective_speed_limit);
        let mut cmd = self.build_command(&ctx.job, &config);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| BackendError::ProcessError(format!("Failed to spawn aria2: {}", e)))?;

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
                if !line.trim().is_empty() {
                    warn!(target: "aria2", "ERR: {}", line);
                }
            }
        });

        let mut reader = BufReader::new(stdout).lines();
        let mut output_path: Option<String> = None;
        let mut last_update = std::time::Instant::now();

        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    graceful_shutdown_aria2(&mut child).await;
                    return Err(BackendError::Cancelled);
                }
                line_result = reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            if line.trim().is_empty() {
                                continue;
                            }
                            debug!(target: "aria2", "OUT: {}", line);

                            // Parse progress from aria2 output
                            if let Some(update) = parse_aria2_progress(&line, &ctx.job.id) {
                                // Throttle progress updates
                                if last_update.elapsed().as_millis() >= 250 {
                                    let _ = ctx.progress_tx.send(update);
                                    last_update = std::time::Instant::now();
                                }
                            }

                            // Detect output file path
                            if let Some(path) = extract_output_path(&line) {
                                output_path = Some(path);
                            }
                        }
                        Ok(None) => break, // EOF
                        Err(e) => {
                            warn!(target: "aria2", "Error reading output: {}", e);
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
            return Err(BackendError::ProcessError(format!(
                "aria2 exited with code {:?}",
                status.code()
            )));
        }

        // If we didn't capture the output path, construct it from the request
        let final_path = output_path.unwrap_or_else(|| {
            let dir = &ctx.job.request.output.directory;
            let filename = ctx
                .job
                .request
                .output
                .filename
                .as_deref()
                .or(ctx.job.title.as_deref())
                .unwrap_or("download");
            format!("{}/{}", dir, filename)
        });

        Ok(final_path)
    }
}

#[cfg(target_os = "android")]
impl Aria2Backend {
    async fn spawn_android(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        use crate::orchestrator::backends::wait_for_jni_ready;

        if !wait_for_jni_ready(10000).await {
            return Err(BackendError::Other(
                "Android JNI bridge not ready. Please wait for the app to fully initialize."
                    .to_string(),
            ));
        }

        let job_id = ctx.job.id.clone();
        let config = Aria2Config::from_options(&ctx.job.request.options, ctx.effective_speed_limit);

        let aria2_opts = serde_json::json!({
            "url": ctx.job.request.url,
            "output_dir": ctx.job.request.output.directory,
            "output_file": ctx.job.request.output.filename,
            "connections": config.connections,
            "splits": config.splits,
            "min_split_size": config.min_split_size,
            "speed_limit": config.speed_limit,
            "proxy": config.proxy,
            "is_torrent": is_torrent_url(&ctx.job.request.url),
        });
        let opts_json = serde_json::to_string(&aria2_opts).map_err(|e| {
            BackendError::Other(format!("Failed to serialize aria2 options: {}", e))
        })?;

        let rx = register_pending_job(&job_id);

        // JNI types aren't Send; do the call on a blocking thread.
        let job_id_clone = job_id.clone();
        let jni_result = tokio::task::spawn_blocking(move || {
            let mut env = get_jni_env()?;
            let activity = get_activity()?;

            let j_job_id = env
                .new_string(&job_id_clone)
                .map_err(|e| format!("Failed to create job_id string: {}", e))?;
            let j_opts_json = env
                .new_string(&opts_json)
                .map_err(|e| format!("Failed to create opts_json string: {}", e))?;

            env.call_method(
                activity.as_obj(),
                "startAria2DownloadFromRust",
                "(Ljava/lang/String;Ljava/lang/String;)V",
                &[JValue::Object(&j_job_id), JValue::Object(&j_opts_json)],
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

        info!(
            "Started Android aria2 download via JNI for job {}, waiting for completion...",
            job_id
        );

        // Wait for the JNI callback to signal completion
        match rx.await {
            Ok(AndroidJobResult::Completed {
                output_path,
                title: _,
            }) => {
                info!(
                    "Android aria2 download completed for job {}: {}",
                    job_id, output_path
                );
                Ok(output_path)
            }
            Ok(AndroidJobResult::Failed(error)) => {
                log::error!(
                    "Android aria2 download failed for job {}: {}",
                    job_id,
                    error
                );
                Err(BackendError::Other(error))
            }
            Ok(AndroidJobResult::Cancelled) => {
                info!("Android aria2 download cancelled for job {}", job_id);
                Err(BackendError::Cancelled)
            }
            Err(_) => {
                log::error!(
                    "Android aria2 download channel closed unexpectedly for job {}",
                    job_id
                );
                Err(BackendError::Other(
                    "Download completion channel closed unexpectedly".to_string(),
                ))
            }
        }
    }
}

#[cfg(target_os = "android")]
pub fn cancel_aria2_android(job_id: &str) -> Result<(), String> {
    let mut env = get_jni_env()?;
    let activity = get_activity()?;

    let j_job_id = env
        .new_string(job_id)
        .map_err(|e| format!("Failed to create job_id string: {}", e))?;

    env.call_method(
        activity.as_obj(),
        "cancelAria2DownloadFromRust",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&j_job_id)],
    )
    .map_err(|e| format!("JNI cancel aria2 call failed: {}", e))?;

    log::info!(
        "Cancelled Android aria2 download via JNI for job {}",
        job_id
    );
    Ok(())
}

#[cfg(not(target_os = "android"))]
#[allow(dead_code)]
pub fn cancel_aria2_android(_job_id: &str) -> Result<(), String> {
    Err("Not on Android".to_string())
}

#[cfg(not(target_os = "android"))]
fn find_aria2_binary() -> Option<PathBuf> {
    let candidates = [
        which::which("aria2c").ok(),
        Some(PathBuf::from("/usr/bin/aria2c")),
        Some(PathBuf::from("/usr/local/bin/aria2c")),
        #[cfg(target_os = "windows")]
        Some(PathBuf::from("C:\\Program Files\\aria2\\aria2c.exe")),
        #[cfg(target_os = "windows")]
        Some(PathBuf::from("C:\\aria2\\aria2c.exe")),
        #[cfg(target_os = "macos")]
        Some(PathBuf::from("/opt/homebrew/bin/aria2c")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.exists() {
            return Some(candidate);
        }
    }

    which::which("aria2c").ok()
}

#[cfg(not(target_os = "android"))]
fn parse_aria2_progress(line: &str, job_id: &str) -> Option<ProgressUpdate> {
    if !line.contains('[') || !line.contains(']') {
        return None;
    }

    let bracket_start = line.find('[')?;
    let bracket_end = line.rfind(']')?;
    if bracket_start >= bracket_end {
        return None;
    }

    let content = &line[bracket_start + 1..bracket_end];
    let (downloaded, total) = parse_size_progress(content);
    let speed = parse_speed(content);
    let eta = parse_eta(content);
    if downloaded > 0 || total.is_some() || speed.is_some() {
        Some(ProgressUpdate {
            job_id: job_id.to_string(),
            downloaded_bytes: downloaded,
            total_bytes: total,
            speed,
            eta,
        })
    } else {
        None
    }
}

#[cfg(not(target_os = "android"))]
fn parse_size_progress(content: &str) -> (u64, Option<u64>) {
    let parts: Vec<&str> = content.split_whitespace().collect();

    for part in parts {
        if part.contains('/') && (part.contains("iB") || part.contains("B")) {
            let clean = part.split('(').next().unwrap_or(part);
            let sizes: Vec<&str> = clean.split('/').collect();

            if sizes.len() == 2 {
                let downloaded = parse_size_str(sizes[0]).unwrap_or(0);
                let total = parse_size_str(sizes[1]);
                return (downloaded, total);
            }
        }
    }

    (0, None)
}

#[cfg(not(target_os = "android"))]
fn parse_speed(content: &str) -> Option<u64> {
    for part in content.split_whitespace() {
        if part.starts_with("DL:") {
            let speed_str = &part[3..];
            let clean = speed_str.trim_end_matches("/s");
            return parse_size_str(clean);
        }
    }
    None
}

#[cfg(not(target_os = "android"))]
fn parse_eta(content: &str) -> Option<u64> {
    for part in content.split_whitespace() {
        if part.starts_with("ETA:") {
            let eta_str = &part[4..];
            return parse_eta_str(eta_str);
        }
    }
    None
}

#[cfg(not(target_os = "android"))]
fn parse_size_str(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let mut num_end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() || c == '.' {
            num_end = i + 1;
        } else {
            break;
        }
    }

    if num_end == 0 {
        return None;
    }

    let num: f64 = s[..num_end].parse().ok()?;
    let unit = s[num_end..].trim().to_lowercase();

    let multiplier: u64 = match unit.as_str() {
        "b" => 1,
        "kb" => 1_000,
        "kib" => 1_024,
        "mb" => 1_000_000,
        "mib" => 1_048_576,
        "gb" => 1_000_000_000,
        "gib" => 1_073_741_824,
        "tb" => 1_000_000_000_000,
        "tib" => 1_099_511_627_776,
        _ => 1,
    };

    Some((num * multiplier as f64) as u64)
}

#[cfg(not(target_os = "android"))]
fn parse_eta_str(s: &str) -> Option<u64> {
    let mut total_seconds: u64 = 0;
    let mut current_num = String::new();

    for c in s.chars() {
        if c.is_ascii_digit() {
            current_num.push(c);
        } else {
            if let Ok(num) = current_num.parse::<u64>() {
                match c {
                    'h' => total_seconds += num * 3600,
                    'm' => total_seconds += num * 60,
                    's' => total_seconds += num,
                    _ => {}
                }
            }
            current_num.clear();
        }
    }

    if total_seconds > 0 {
        Some(total_seconds)
    } else {
        None
    }
}

#[cfg(not(target_os = "android"))]
fn extract_output_path(line: &str) -> Option<String> {
    if line.contains("Download complete:") {
        let path = line.split("Download complete:").nth(1)?.trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }

    if line.contains("[NOTICE]") && line.contains("Download complete:") {
        let path = line.split("Download complete:").nth(1)?.trim();
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }

    None
}
