use std::path::PathBuf;

use async_trait::async_trait;
use tauri::AppHandle;
use tracing::info;

use crate::orchestrator::backends::{
    extract_filename_from_url, extract_magnet_name, guess_mime_type, has_file_extension,
    is_torrent_url, parse_size_str, Backend, BackendCapabilities, SpawnContext,
    DIRECT_FILE_EXTENSIONS,
};
use crate::orchestrator::types::*;

struct Aria2Config {
    connections: u32,
    splits: u32,
    min_split_size: String,
    disable_ipv6: bool,
    custom_args: Vec<String>,
    speed_limit: Option<u64>,
    proxy: Option<String>,
}

impl Aria2Config {
    fn from_options(opts: &DownloadOptions, effective_speed_limit: Option<u64>) -> Self {
        let custom_args = opts
            .aria2_custom_args
            .as_ref()
            .map(|s| shell_words::split(s).unwrap_or_default())
            .unwrap_or_default();

        Self {
            connections: opts
                .aria2_connections
                .unwrap_or(constants::DEFAULT_ARIA2_CONNECTIONS),
            splits: opts.aria2_splits.unwrap_or(constants::DEFAULT_ARIA2_SPLITS),
            min_split_size: opts
                .aria2_min_split_size
                .clone()
                .unwrap_or_else(|| "1M".to_string()),
            disable_ipv6: opts.aria2_disable_ipv6.unwrap_or(false),
            custom_args,
            speed_limit: effective_speed_limit,
            proxy: resolve_effective_proxy(&opts.proxy),
        }
    }
}

use crate::orchestrator::backends::common::resolve_effective_proxy;

fn build_aria2_args(req: &DownloadRequest, config: &Aria2Config) -> Vec<Vec<String>> {
    let mut args: Vec<Vec<String>> = Vec::with_capacity(32);

    args.push(vec!["-d".to_string(), req.output.directory.clone()]);

    if let Some(ref filename) = req.output.filename {
        args.push(vec!["-o".to_string(), filename.clone()]);
    }

    args.push(vec!["-x".to_string(), config.connections.to_string()]);
    args.push(vec!["-s".to_string(), config.splits.to_string()]);
    args.push(vec!["-k".to_string(), config.min_split_size.clone()]);
    args.push(vec!["--continue=true".to_string()]);
    args.push(vec!["--file-allocation=none".to_string()]);
    args.push(vec!["--auto-file-renaming=false".to_string()]);
    args.push(vec!["--allow-overwrite=true".to_string()]);

    if let Some(limit) = config.speed_limit {
        if limit > 0 {
            args.push(vec![
                "--max-download-limit".to_string(),
                format!("{}K", limit / 1024),
            ]);
        }
    }

    args.push(vec![
        "--user-agent".to_string(),
        constants::USER_AGENT.to_string(),
    ]);

    if let Ok(parsed) = url::Url::parse(&req.url) {
        if let Some(host) = parsed.host_str() {
            let referer = format!("{}://{}/", parsed.scheme(), host);
            args.push(vec!["--referer".to_string(), referer]);
        }
    }

    if let Some(ref proxy_url) = config.proxy {
        args.push(vec!["--all-proxy".to_string(), proxy_url.clone()]);
    }

    if config.disable_ipv6 {
        args.push(vec!["--disable-ipv6=true".to_string()]);
    }

    for arg in &config.custom_args {
        args.push(vec![arg.clone()]);
    }

    args.push(vec!["--show-console-readout=true".to_string()]);
    args.push(vec!["--summary-interval=1".to_string()]);

    if is_torrent_url(&req.url) {
        args.push(vec!["--listen-port".to_string(), "6881-6999".to_string()]);
        args.push(vec![
            "--dht-listen-port".to_string(),
            "6881-6999".to_string(),
        ]);
        args.push(vec!["--enable-dht=true".to_string()]);
        args.push(vec!["--bt-enable-lpd=true".to_string()]);
        args.push(vec!["--seed-ratio".to_string(), "0.0".to_string()]);

        // Prioritize the beginning and end of each file so that media players
        // can determine codec/duration information quickly (streaming playback).
        args.push(vec![
            "--bt-prioritize-piece=head=100M,tail=1M".to_string(),
        ]);

        // Selective file download — only fetch the chosen indices (1-based).
        if let Some(ref selected) = req.options.torrent_selected_files {
            if !selected.is_empty() {
                let indices = selected
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                args.push(vec![format!("--select-file={}", indices)]);
            }
        }
    }

    args.push(vec![req.url.clone()]);

    args
}

pub fn parse_aria2_progress_line(line: &str, job_id: &str) -> Option<ProgressUpdate> {
    let clean_line = line.trim();
    if !clean_line.contains('[') || !clean_line.contains(']') {
        return None;
    }

    let bracket_start = clean_line.find('[')?;
    let bracket_end = clean_line.rfind(']')?;
    if bracket_start >= bracket_end {
        return None;
    }

    let content = &clean_line[bracket_start + 1..bracket_end];
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

fn parse_size_progress(content: &str) -> (u64, Option<u64>) {
    for part in content.split_whitespace() {
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

fn parse_speed(content: &str) -> Option<u64> {
    for part in content.split_whitespace() {
        if let Some(speed_str) = part.strip_prefix("DL:") {
            let clean = speed_str.trim_end_matches("/s");
            return parse_size_str(clean);
        }
    }
    None
}

fn parse_eta(content: &str) -> Option<u64> {
    for part in content.split_whitespace() {
        if let Some(eta_str) = part.strip_prefix("ETA:") {
            return parse_eta_str(eta_str);
        }
    }
    None
}

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

pub struct Aria2Backend {
    #[cfg(desktop)]
    binary_path: PathBuf,
}

impl Aria2Backend {
    pub fn new(app: &AppHandle) -> Option<Self> {
        exec::create_backend(app)
    }
}

#[async_trait]
impl Backend for Aria2Backend {
    fn name(&self) -> &str {
        "aria2"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            name: "aria2".into(),
            streaming_resolve: false,
            playlists: false,
            pause_resume: true,
            multi_connection: true,
            format_selection: false,
            subtitles: false,
            speed_limit: true,
            proxy: true,
            cookies: false,
            torrent_magnet: true,
            post_processing: false,
        }
    }

    fn priority(&self, url: &str) -> Priority {
        if url.starts_with("magnet:") || url.ends_with(".torrent") || url.contains(".torrent?") {
            return Priority::Absolute;
        }

        if url.starts_with("ftp://") || url.starts_with("sftp://") {
            return Priority::High;
        }

        if has_file_extension(url, DIRECT_FILE_EXTENSIONS) {
            return Priority::High;
        }

        Priority::None
    }

    async fn resolve(
        &self,
        url: &str,
        _settings: &ResolveSettings,
    ) -> Result<UrlInfo, BackendError> {
        if url.starts_with("magnet:") {
            let mut info = UrlInfo::simple(url, extract_magnet_name(url), "aria2");
            info.content_type = ContentType::Torrent;
            return Ok(info);
        }

        if url.ends_with(".torrent") || url.starts_with("ftp://") || url.starts_with("sftp://") {
            let mut info = UrlInfo::with_file_info(
                url,
                Some(extract_filename_from_url(url)),
                "aria2",
                None,
                guess_mime_type(url),
            );
            if is_torrent_url(url) {
                info.content_type = ContentType::Torrent;
            }
            return Ok(info);
        }

        // Use shared HTTP HEAD resolve; if it fails (non-200), fall back to
        // URL-based metadata so aria2 can still attempt the download.
        match crate::orchestrator::backends::resolve_http_file(url, "aria2").await {
            Ok(info) => Ok(info),
            Err(_) => Ok(UrlInfo::with_file_info(
                url,
                Some(extract_filename_from_url(url)),
                "aria2",
                None,
                guess_mime_type(url),
            )),
        }
    }

    async fn spawn(&self, ctx: SpawnContext) -> Result<String, BackendError> {
        info!(target: "aria2", "Starting download job {} for URL: {}", ctx.job.id, ctx.job.request.url);
        let config = Aria2Config::from_options(&ctx.job.request.options, ctx.effective_speed_limit);
        let args = build_aria2_args(&ctx.job.request, &config);
        exec::run_aria2(self, ctx, args).await
    }
}

#[cfg(target_os = "android")]
pub fn cancel_aria2(job_id: &str) -> Result<(), String> {
    exec::cancel_aria2(job_id)
}

mod exec {
    use super::*;

    #[cfg(desktop)]
    pub fn create_backend(app: &AppHandle) -> Option<Aria2Backend> {
        let binary_path = crate::deps::resolve_aria2_path(app)?;
        info!("aria2 backend using binary: {:?}", binary_path);
        Some(Aria2Backend { binary_path })
    }

    #[cfg(desktop)]
    pub async fn run_aria2(
        backend: &Aria2Backend,
        ctx: SpawnContext,
        args: Vec<Vec<String>>,
    ) -> Result<String, BackendError> {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tracing::{debug, warn};

        let mut cmd = crate::utils::new_command(&backend.binary_path);

        crate::orchestrator::backends::apply_args_to_command(&mut cmd, args);

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
        let mut output_path_count: u32 = 0;
        let mut last_update = std::time::Instant::now();
        let job_id = ctx.job.id.clone();

        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    graceful_shutdown(&mut child).await;
                    return Err(BackendError::Cancelled);
                }
                line_result = reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            if line.trim().is_empty() {
                                continue;
                            }
                            debug!(target: "aria2", "OUT: {}", line);

                            if let Some(update) = parse_aria2_progress_line(&line, &job_id) {
                                if last_update.elapsed().as_millis() >= 250 {
                                    let _ = ctx.progress_tx.send(update);
                                    last_update = std::time::Instant::now();
                                }
                            }

                            if let Some(path) = extract_output_path(&line) {
                                output_path = Some(path);
                                output_path_count += 1;
                            }
                        }
                        Ok(None) => break,
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

        let final_path = if output_path_count > 1 {
            // Multi-file download (torrent): use parent directory
            output_path
                .as_ref()
                .and_then(|p| std::path::Path::new(p).parent())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ctx.job.request.output.directory.clone())
        } else {
            output_path.unwrap_or_else(|| {
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
            })
        };

        Ok(final_path)
    }

    #[cfg(desktop)]
    pub(super) fn extract_output_path(line: &str) -> Option<String> {
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

    #[cfg(desktop)]
    async fn graceful_shutdown(child: &mut tokio::process::Child) {
        crate::orchestrator::backends::graceful_shutdown(child, "aria2").await;
    }

    #[cfg(mobile)]
    pub fn create_backend(_app: &AppHandle) -> Option<Aria2Backend> {
        info!("aria2 backend initialized for mobile");
        Some(Aria2Backend {})
    }

    #[cfg(target_os = "ios")]
    pub async fn run_aria2(
        _backend: &Aria2Backend,
        _ctx: SpawnContext,
        _args: Vec<Vec<String>>,
    ) -> Result<String, BackendError> {
        Err(BackendError::Other(
            "aria2 downloads are not supported on iOS".to_string(),
        ))
    }

    #[cfg(target_os = "android")]
    pub async fn run_aria2(
        _backend: &Aria2Backend,
        ctx: SpawnContext,
        args: Vec<Vec<String>>,
    ) -> Result<String, BackendError> {
        use crate::orchestrator::backends::android_jni::{
            register_listener, remove_listener, start_android_job_jni, wait_for_jni_ready,
            AndroidEvent,
        };
        use tokio::sync::mpsc;

        if !wait_for_jni_ready(10000).await {
            return Err(BackendError::Other(
                "Android JNI bridge not ready".to_string(),
            ));
        }

        let job_id = ctx.job.id.clone();
        let args_json = serde_json::to_string(&args)
            .map_err(|e| BackendError::Other(format!("Failed to serialize args: {}", e)))?;

        let payload = serde_json::json!({ "args": args_json });
        let payload_str = serde_json::to_string(&payload)
            .map_err(|e| BackendError::Other(format!("Failed to serialize payload: {}", e)))?;

        let (tx, mut rx) = mpsc::unbounded_channel();
        register_listener(&job_id, tx);

        start_android_job_jni(&job_id, "aria2", &payload_str, "Aria2 Download").await?;

        let mut final_path = None;
        let job_id_for_parse = job_id.clone();
        loop {
            tokio::select! {
                _ = ctx.cancel_token.cancelled() => {
                    remove_listener(&job_id);
                    let _ = cancel_aria2(&job_id);
                    return Err(BackendError::Cancelled);
                }
                event = rx.recv() => {
                    match event {
                        Some(AndroidEvent::RawProgressLine(line)) => {
                            if let Some(update) = parse_aria2_progress_line(&line, &job_id_for_parse) {
                                let _ = ctx.progress_tx.send(update);
                            }
                        }
                        Some(AndroidEvent::Completed { output_path, .. }) => {
                            final_path = Some(output_path);
                            break;
                        }
                        Some(AndroidEvent::Failed(e)) => {
                            remove_listener(&job_id);
                            return Err(BackendError::Other(e));
                        }
                        Some(AndroidEvent::Cancelled) => {
                            remove_listener(&job_id);
                            return Err(BackendError::Cancelled);
                        }
                        Some(_) => {}
                        None => {
                            remove_listener(&job_id);
                            return Err(BackendError::Other("Android event channel closed".to_string()));
                        }
                    }
                }
            }
        }

        remove_listener(&job_id);
        final_path.ok_or_else(|| BackendError::Other("No output path received".to_string()))
    }

    #[cfg(target_os = "android")]
    pub fn cancel_aria2(job_id: &str) -> Result<(), String> {
        crate::orchestrator::backends::android_jni::cancel_download_jni(job_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn test_request(url: &str) -> DownloadRequest {
        DownloadRequest {
            url: url.to_string(),
            backend: None,
            id: None,
            quality: QualitySettings::default(),
            output: OutputSettings {
                directory: "/tmp/downloads".to_string(),
                filename_template: None,
                filename: None,
            },
            options: DownloadOptions::default(),
            post_process: Vec::new(),
        }
    }

    fn test_config() -> Aria2Config {
        Aria2Config {
            connections: 8,
            splits: 8,
            min_split_size: "1M".to_string(),
            disable_ipv6: false,
            custom_args: Vec::new(),
            speed_limit: None,
            proxy: None,
        }
    }

    #[test]
    fn test_aria2_basic_progress() {
        let line = "[#abc123 5.0MiB/10.0MiB(50%) CN:8 DL:1.2MiB ETA:4s]";
        let result = parse_aria2_progress_line(line, "job1").unwrap();
        assert!(result.downloaded_bytes > 0);
        assert!(result.total_bytes.is_some());
        assert!(result.speed.is_some());
        assert_eq!(result.eta, Some(4));
    }

    #[test]
    fn test_aria2_no_brackets() {
        assert!(parse_aria2_progress_line("no brackets here", "job1").is_none());
    }

    #[test]
    fn test_aria2_bracket_present_no_data() {
        let line = "[some random text in brackets]";
        let result = parse_aria2_progress_line(line, "job1");
        assert!(result.is_none());
    }

    #[test]
    fn test_aria2_kib_units() {
        let line = "[#abc 512KiB/1024KiB CN:1 DL:256KiB ETA:2s]";
        let result = parse_aria2_progress_line(line, "job1").unwrap();
        assert_eq!(result.downloaded_bytes, 512 * 1024);
        assert_eq!(result.total_bytes, Some(1024 * 1024));
    }

    #[test]
    fn test_aria2_gib_progress() {
        let line = "[#abc 1.5GiB/4.0GiB(37%) CN:8 DL:10.0MiB ETA:4m15s]";
        let result = parse_aria2_progress_line(line, "job1").unwrap();
        assert_eq!(result.downloaded_bytes, (1.5 * 1_073_741_824.0) as u64);
        assert_eq!(result.total_bytes, Some((4.0 * 1_073_741_824.0) as u64));
        assert_eq!(result.speed, Some((10.0 * 1_048_576.0) as u64));
        assert_eq!(result.eta, Some(4 * 60 + 15));
    }

    #[test]
    fn test_aria2_no_speed_no_eta() {
        let line = "[#abc 100KiB/500KiB(20%) CN:1]";
        let result = parse_aria2_progress_line(line, "job1").unwrap();
        assert_eq!(result.downloaded_bytes, 100 * 1024);
        assert_eq!(result.total_bytes, Some(500 * 1024));
        assert_eq!(result.speed, None);
        assert_eq!(result.eta, None);
    }

    #[test]
    fn test_aria2_job_id_propagated() {
        let line = "[#abc 1MiB/2MiB CN:1 DL:1MiB ETA:1s]";
        let result = parse_aria2_progress_line(line, "my-job-42").unwrap();
        assert_eq!(result.job_id, "my-job-42");
    }

    #[test]
    fn test_eta_seconds() {
        assert_eq!(parse_eta_str("30s"), Some(30));
    }

    #[test]
    fn test_eta_minutes_seconds() {
        assert_eq!(parse_eta_str("2m30s"), Some(150));
    }

    #[test]
    fn test_eta_hours_minutes_seconds() {
        assert_eq!(parse_eta_str("1h30m15s"), Some(5415));
    }

    #[test]
    fn test_eta_empty() {
        assert_eq!(parse_eta_str(""), None);
    }

    #[test]
    fn test_eta_hours_only() {
        assert_eq!(parse_eta_str("2h"), Some(7200));
    }

    #[test]
    fn test_eta_minutes_only() {
        assert_eq!(parse_eta_str("45m"), Some(2700));
    }

    #[test]
    fn test_parse_speed_mib() {
        let speed = parse_speed("#abc 5MiB/10MiB DL:1.2MiB ETA:4s");
        assert_eq!(speed, Some((1.2 * 1_048_576.0) as u64));
    }

    #[test]
    fn test_parse_speed_kib() {
        let speed = parse_speed("#abc 5MiB/10MiB DL:256KiB ETA:4s");
        assert_eq!(speed, Some(256 * 1024));
    }

    #[test]
    fn test_parse_speed_none() {
        assert_eq!(parse_speed("#abc 1MiB/2MiB CN:1"), None);
    }

    #[test]
    fn test_parse_size_progress_basic() {
        let (dl, total) = parse_size_progress("#abc 5MiB/10MiB(50%) CN:8 DL:1MiB");
        assert_eq!(dl, (5.0 * 1_048_576.0) as u64);
        assert_eq!(total, Some((10.0 * 1_048_576.0) as u64));
    }

    #[test]
    fn test_parse_size_progress_no_match() {
        let (dl, total) = parse_size_progress("no size data here");
        assert_eq!(dl, 0);
        assert_eq!(total, None);
    }

    fn args_contain(args: &[Vec<String>], flag: &str) -> bool {
        args.iter().any(|group| group.iter().any(|a| a == flag))
    }

    fn args_get_pair(args: &[Vec<String>], key: &str) -> Option<String> {
        args.iter().find_map(|group| {
            if group.len() == 2 && group[0] == key {
                Some(group[1].clone())
            } else {
                None
            }
        })
    }

    #[test]
    fn test_build_aria2_args_basic_http() {
        let req = test_request("https://example.com/file.zip");
        let config = test_config();
        let args = build_aria2_args(&req, &config);

        assert_eq!(
            args_get_pair(&args, "-d"),
            Some("/tmp/downloads".to_string())
        );
        assert_eq!(args_get_pair(&args, "-x"), Some("8".to_string()));
        assert_eq!(args_get_pair(&args, "-s"), Some("8".to_string()));
        assert_eq!(args_get_pair(&args, "-k"), Some("1M".to_string()));
        assert!(args_contain(&args, "--continue=true"));
        assert!(args_contain(&args, "--file-allocation=none"));
        assert!(args_contain(&args, "--show-console-readout=true"));
        assert!(args_contain(&args, "--summary-interval=1"));
        let last = args.last().unwrap();
        assert_eq!(last[0], "https://example.com/file.zip");
        assert!(!args_contain(&args, "--enable-dht=true"));
    }

    #[test]
    fn test_build_aria2_args_with_filename() {
        let mut req = test_request("https://example.com/file.zip");
        req.output.filename = Some("custom_name.zip".to_string());
        let config = test_config();
        let args = build_aria2_args(&req, &config);

        assert_eq!(
            args_get_pair(&args, "-o"),
            Some("custom_name.zip".to_string())
        );
    }

    #[test]
    fn test_build_aria2_args_torrent_url() {
        let req = test_request("magnet:?xt=urn:btih:abc123");
        let config = test_config();
        let args = build_aria2_args(&req, &config);

        assert!(args_contain(&args, "--enable-dht=true"));
        assert!(args_contain(&args, "--bt-enable-lpd=true"));
        assert_eq!(
            args_get_pair(&args, "--listen-port"),
            Some("6881-6999".to_string())
        );
        assert_eq!(
            args_get_pair(&args, "--dht-listen-port"),
            Some("6881-6999".to_string())
        );
        assert_eq!(
            args_get_pair(&args, "--seed-ratio"),
            Some("0.0".to_string())
        );
    }

    #[test]
    fn test_build_aria2_args_dottorent_url() {
        let req = test_request("https://example.com/file.torrent");
        let config = test_config();
        let args = build_aria2_args(&req, &config);

        assert!(args_contain(&args, "--enable-dht=true"));
    }

    #[test]
    fn test_build_aria2_args_with_proxy() {
        let req = test_request("https://example.com/file.zip");
        let config = Aria2Config {
            proxy: Some("socks5://127.0.0.1:1080".to_string()),
            ..test_config()
        };
        let args = build_aria2_args(&req, &config);

        assert_eq!(
            args_get_pair(&args, "--all-proxy"),
            Some("socks5://127.0.0.1:1080".to_string())
        );
    }

    #[test]
    fn test_build_aria2_args_with_speed_limit() {
        let req = test_request("https://example.com/file.zip");
        let config = Aria2Config {
            speed_limit: Some(1_048_576), // 1 MiB/s
            ..test_config()
        };
        let args = build_aria2_args(&req, &config);

        assert_eq!(
            args_get_pair(&args, "--max-download-limit"),
            Some("1024K".to_string())
        );
    }

    #[test]
    fn test_build_aria2_args_speed_limit_zero() {
        let req = test_request("https://example.com/file.zip");
        let config = Aria2Config {
            speed_limit: Some(0),
            ..test_config()
        };
        let args = build_aria2_args(&req, &config);

        assert!(args_get_pair(&args, "--max-download-limit").is_none());
    }

    #[test]
    fn test_build_aria2_args_disable_ipv6() {
        let req = test_request("https://example.com/file.zip");
        let config = Aria2Config {
            disable_ipv6: true,
            ..test_config()
        };
        let args = build_aria2_args(&req, &config);

        assert!(args_contain(&args, "--disable-ipv6=true"));
    }

    #[test]
    fn test_build_aria2_args_custom_args() {
        let req = test_request("https://example.com/file.zip");
        let config = Aria2Config {
            custom_args: vec![
                "--header=Cookie: abc".to_string(),
                "--timeout=60".to_string(),
            ],
            ..test_config()
        };
        let args = build_aria2_args(&req, &config);

        assert!(args_contain(&args, "--header=Cookie: abc"));
        assert!(args_contain(&args, "--timeout=60"));
    }

    #[test]
    fn test_build_aria2_args_referer_from_url() {
        let req = test_request("https://cdn.example.com/path/file.zip");
        let config = test_config();
        let args = build_aria2_args(&req, &config);

        assert_eq!(
            args_get_pair(&args, "--referer"),
            Some("https://cdn.example.com/".to_string())
        );
    }

    #[test]
    fn test_build_aria2_args_user_agent() {
        let req = test_request("https://example.com/file.zip");
        let config = test_config();
        let args = build_aria2_args(&req, &config);

        let ua = args_get_pair(&args, "--user-agent");
        assert!(ua.is_some());
        assert!(ua.unwrap().contains("Mozilla"));
    }

    #[test]
    fn test_aria2_config_defaults() {
        let opts = DownloadOptions::default();
        let config = Aria2Config::from_options(&opts, None);

        assert_eq!(config.connections, constants::DEFAULT_ARIA2_CONNECTIONS);
        assert_eq!(config.splits, constants::DEFAULT_ARIA2_SPLITS);
        assert_eq!(config.min_split_size, "1M");
        assert!(!config.disable_ipv6);
        assert!(config.custom_args.is_empty());
        assert!(config.speed_limit.is_none());
        assert!(config.proxy.is_none());
    }

    #[test]
    fn test_aria2_config_custom_values() {
        let opts = DownloadOptions {
            aria2_connections: Some(16),
            aria2_splits: Some(4),
            aria2_min_split_size: Some("4M".to_string()),
            aria2_disable_ipv6: Some(true),
            aria2_custom_args: Some("--timeout=60 --header=\"Custom: val\"".to_string()),
            ..Default::default()
        };
        let config = Aria2Config::from_options(&opts, Some(512_000));

        assert_eq!(config.connections, 16);
        assert_eq!(config.splits, 4);
        assert_eq!(config.min_split_size, "4M");
        assert!(config.disable_ipv6);
        assert_eq!(config.custom_args.len(), 2);
        assert_eq!(config.speed_limit, Some(512_000));
    }

    #[cfg(desktop)]
    #[test]
    fn test_extract_output_path_basic() {
        assert_eq!(
            exec::extract_output_path("Download complete: /tmp/file.zip"),
            Some("/tmp/file.zip".to_string())
        );
    }

    #[cfg(desktop)]
    #[test]
    fn test_extract_output_path_with_notice() {
        assert_eq!(
            exec::extract_output_path("[NOTICE] Download complete: /home/user/video.mp4"),
            Some("/home/user/video.mp4".to_string())
        );
    }

    #[cfg(desktop)]
    #[test]
    fn test_extract_output_path_no_match() {
        assert_eq!(exec::extract_output_path("Some random output line"), None);
        assert_eq!(exec::extract_output_path("Downloading file..."), None);
    }

    #[test]
    fn test_aria2_priority_magnet() {
        let backend = create_dummy_backend_for_priority();
        assert_eq!(
            backend.priority("magnet:?xt=urn:btih:abc"),
            Priority::Absolute
        );
    }

    #[test]
    fn test_aria2_priority_torrent_file() {
        let backend = create_dummy_backend_for_priority();
        assert_eq!(
            backend.priority("https://example.com/file.torrent"),
            Priority::Absolute
        );
    }

    #[test]
    fn test_aria2_priority_ftp() {
        let backend = create_dummy_backend_for_priority();
        assert_eq!(
            backend.priority("ftp://files.example.com/data.bin"),
            Priority::High
        );
    }

    #[test]
    fn test_aria2_priority_direct_file() {
        let backend = create_dummy_backend_for_priority();
        assert_eq!(
            backend.priority("https://example.com/archive.zip"),
            Priority::High
        );
        assert_eq!(
            backend.priority("https://example.com/video.mp4"),
            Priority::High
        );
    }

    #[test]
    fn test_aria2_priority_unsupported() {
        let backend = create_dummy_backend_for_priority();
        assert_eq!(
            backend.priority("https://youtube.com/watch?v=abc"),
            Priority::None
        );
        assert_eq!(backend.priority("https://example.com/page"), Priority::None);
    }

    struct PriorityTestBackend;

    impl PriorityTestBackend {
        fn priority(&self, url: &str) -> Priority {
            if url.starts_with("magnet:") || url.ends_with(".torrent") || url.contains(".torrent?")
            {
                return Priority::Absolute;
            }
            if url.starts_with("ftp://") || url.starts_with("sftp://") {
                return Priority::High;
            }
            if has_file_extension(url, DIRECT_FILE_EXTENSIONS) {
                return Priority::High;
            }
            Priority::None
        }
    }

    fn create_dummy_backend_for_priority() -> PriorityTestBackend {
        PriorityTestBackend
    }
}
