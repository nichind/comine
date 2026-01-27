#[cfg(not(target_os = "android"))]
use futures_util::StreamExt;
use log::info;
#[cfg(not(target_os = "android"))]
use log::{debug, error, warn};
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri::{Emitter, Manager};
#[cfg(not(target_os = "android"))]
use tokio::io::AsyncWriteExt;

#[cfg(target_os = "windows")]
use crate::utils::CommandHideConsole;

#[cfg(not(target_os = "android"))]
use std::io::Read;

#[cfg(not(target_os = "android"))]
use crate::proxy::proxy_strategies;
use crate::proxy::ProxyConfig;
use crate::types::{DependencyStatus, InstallProgress, ReleaseInfo};

#[cfg(not(target_os = "android"))]
const HTTP_TIMEOUT_SECS: u64 = 600;
#[cfg(not(target_os = "android"))]
const HTTP_CONNECT_TIMEOUT_SECS: u64 = 60;
#[cfg(not(target_os = "android"))]
const TCP_KEEPALIVE_SECS: u64 = 30;
#[cfg(not(target_os = "android"))]
const MAX_REDIRECTS: usize = 10;
#[cfg(not(target_os = "android"))]
const MAX_RETRY_ATTEMPTS: u32 = 5;
#[cfg(not(target_os = "android"))]
const ARIA2_FALLBACK_VERSION: &str = "1.37.0";
#[cfg(not(target_os = "android"))]
const PROGRESS_EMIT_INTERVAL_MS: u128 = 100;
#[cfg(not(target_os = "android"))]
const HTTP_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
#[cfg(not(target_os = "android"))]
const RATELIMIT_DELAY_SECS: u64 = 10;
#[cfg(not(target_os = "android"))]
const RATELIMIT_MAX_DELAY_SECS: u64 = 60;

#[cfg(not(target_os = "android"))]
const ARIA2_CONNECTIONS: &str = "16";
#[cfg(not(target_os = "android"))]
const ARIA2_SPLIT: &str = "16";
#[cfg(not(target_os = "android"))]
const ARIA2_MIN_SPLIT_SIZE: &str = "1M";

const EVENT_YTDLP_PROGRESS: &str = "ytdlp-install-progress";
const EVENT_FFMPEG_PROGRESS: &str = "ffmpeg-install-progress";
const EVENT_ARIA2_PROGRESS: &str = "aria2-install-progress";
const EVENT_DENO_PROGRESS: &str = "deno-install-progress";
const EVENT_QUICKJS_PROGRESS: &str = "quickjs-install-progress";
const EVENT_LUX_PROGRESS: &str = "lux-install-progress";

#[cfg(target_os = "windows")]
const YTDLP_BINARY: &str = "yt-dlp.exe";
#[cfg(target_os = "macos")]
const YTDLP_BINARY: &str = "yt-dlp_macos";
#[cfg(target_os = "linux")]
const YTDLP_BINARY: &str = "yt-dlp_linux";

fn get_bin_dir(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "android")]
    {
        let cache_dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| format!("Failed to get app cache dir: {}", e))?;
        info!("Using Android cache dir: {:?}", cache_dir);
        Ok(cache_dir.join("bin"))
    }

    #[cfg(not(target_os = "android"))]
    {
        let app_data = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {}", e))?;
        Ok(app_data.join("bin"))
    }
}

pub fn get_ytdlp_path(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let binary_name = "yt-dlp.exe";
    #[cfg(target_os = "android")]
    let binary_name = "libytdlp.so";
    #[cfg(all(not(target_os = "windows"), not(target_os = "android")))]
    let binary_name = "yt-dlp";

    Ok(get_bin_dir(app)?.join(binary_name))
}

pub fn get_ffmpeg_path(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let binary_name = "ffmpeg.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "ffmpeg";

    Ok(get_bin_dir(app)?.join(binary_name))
}

pub fn get_aria2_path(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let binary_name = "aria2c.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "aria2c";

    Ok(get_bin_dir(app)?.join(binary_name))
}

pub fn get_deno_path(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let binary_name = "deno.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "deno";

    Ok(get_bin_dir(app)?.join(binary_name))
}

pub fn get_quickjs_path(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let binary_name = "qjs.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "qjs";

    Ok(get_bin_dir(app)?.join(binary_name))
}

pub fn get_lux_path(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let binary_name = "lux.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "lux";

    Ok(get_bin_dir(app)?.join(binary_name))
}

#[cfg(not(target_os = "android"))]
fn http_client(proxy_url: Option<&str>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(HTTP_USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .connect_timeout(std::time::Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS))
        .pool_max_idle_per_host(4)
        .tcp_keepalive(std::time::Duration::from_secs(TCP_KEEPALIVE_SECS))
        .tcp_nodelay(true)
        .gzip(true)
        .brotli(true)
        .danger_accept_invalid_certs(false);

    match proxy_url {
        Some(url) if !url.is_empty() => {
            let proxy =
                reqwest::Proxy::all(url).map_err(|e| format!("Invalid proxy URL: {}", e))?;
            builder = builder.proxy(proxy);
            debug!("HTTP client using proxy: {}", url);
        }
        _ => {
            builder = builder.no_proxy();
            debug!("HTTP client with no proxy");
        }
    }

    builder
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

#[cfg(not(target_os = "android"))]
async fn fetch(url: &str, proxy_config: &ProxyConfig) -> Result<reqwest::Response, String> {
    let strategies = proxy_strategies(proxy_config);

    let mut last_error = String::new();

    for (strategy_name, proxy_url) in &strategies {
        info!("Trying download {} - {}", strategy_name, url);

        let client = match http_client(proxy_url.as_deref()) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to create client {}: {}", strategy_name, e);
                last_error = e;
                continue;
            }
        };

        for attempt in 0..MAX_RETRY_ATTEMPTS {
            if attempt > 0 {
                let delay = std::time::Duration::from_millis(1000 * 2u64.pow(attempt));
                info!("Retry {} after {:?}...", attempt, delay);
                tokio::time::sleep(delay).await;
            }

            match client.get(url).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        info!("Download started successfully {}", strategy_name);
                        return Ok(response);
                    } else if status.as_u16() == 429 || status.as_u16() == 503 {
                        let delay = RATELIMIT_DELAY_SECS * 2u64.pow(attempt.min(3));
                        let delay = delay.min(RATELIMIT_MAX_DELAY_SECS);
                        warn!("Rate limited ({}), waiting {}s...", status, delay);
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                        last_error = format!("Rate limited ({})", status);
                    } else if status.as_u16() == 403 {
                        last_error = format!("Access denied ({})", status);
                        break;
                    } else {
                        last_error = format!("HTTP error {}", status);
                    }
                }
                Err(e) => {
                    let err_str = e.to_string();
                    warn!("Request error: {}", err_str);

                    if err_str.contains("connection")
                        || err_str.contains("timeout")
                        || err_str.contains("proxy")
                    {
                        last_error = err_str;
                        if attempt >= 2 {
                            break;
                        }
                    } else {
                        last_error = err_str;
                    }
                }
            }
        }
    }

    Err(format!("All download attempts failed: {}", last_error))
}

#[cfg(not(target_os = "android"))]
async fn fetch_github_release(
    repo: &str,
    proxy_config: &ProxyConfig,
) -> Result<GitHubRelease, String> {
    let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo);

    let response = fetch(&api_url, proxy_config).await?;

    response
        .json::<GitHubRelease>()
        .await
        .map_err(|e| format!("Failed to parse release for {}: {}", repo, e))
}

#[allow(clippy::too_many_arguments)]
#[cfg(not(target_os = "android"))]
async fn download_with_aria2(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    window: &tauri::Window,
    event_name: &str,
    dep_name: &str,
    version: &str,
    proxy_url: Option<&str>,
) -> Result<(), String> {
    let aria2_path = get_aria2_path(app)?;

    if !aria2_path.exists() {
        return Err("aria2 not installed".to_string());
    }

    info!("Using aria2 to download: {} -> {:?}", url, dest);

    if dest.exists() {
        if let Err(e) = tokio::fs::remove_file(&dest).await {
            warn!("Failed to remove existing file before download: {}", e);
        }
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let dest_dir = dest
        .parent()
        .ok_or("Invalid destination path")?
        .to_string_lossy()
        .to_string();
    let dest_filename = dest
        .file_name()
        .ok_or("Invalid destination filename")?
        .to_string_lossy()
        .to_string();

    let mut cmd = tokio::process::Command::new(&aria2_path);

    let x_arg = format!("-x{}", ARIA2_CONNECTIONS);
    let s_arg = format!("-s{}", ARIA2_SPLIT);
    let k_arg = format!("-k{}", ARIA2_MIN_SPLIT_SIZE);

    let mut args = vec![
        x_arg.as_str(),
        s_arg.as_str(),
        k_arg.as_str(),
        "--file-allocation=none",
        "--max-tries=10",
        "--retry-wait=3",
        "--max-file-not-found=5",
        "--connect-timeout=30",
        "--timeout=600",
        "--summary-interval=1",
        "--console-log-level=notice",
        "--download-result=hide",
        "--allow-overwrite=true",
        "--auto-file-renaming=false",
        "--disable-ipv6=true",
        "--enable-http-pipelining=true",
        "--stream-piece-selector=inorder",
    ];

    let proxy_arg;
    if let Some(proxy) = proxy_url {
        if !proxy.is_empty() {
            proxy_arg = format!("--all-proxy={}", proxy);
            args.push(&proxy_arg);
            info!("aria2 using proxy: {}", proxy);
        }
    }

    args.extend(["-d", &dest_dir, "-o", &dest_filename, url]);
    cmd.args(&args);

    #[cfg(target_os = "windows")]
    cmd.hide_console();

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn aria2c: {}", e))?;

    let _ = window.emit(
        event_name,
        InstallProgress {
            stage: "downloading".to_string(),
            progress: 0,
            downloaded: 0,
            total: 0,
            speed: 0.0,
            message: format!(
                "Downloading {} {} (aria2 x{})...",
                dep_name, version, ARIA2_CONNECTIONS
            ),
        },
    );

    let stderr = child
        .stderr
        .take()
        .ok_or("Failed to capture aria2 stderr")?;

    let event_name = event_name.to_string();
    let dep_name = dep_name.to_string();
    let version = version.to_string();
    let window = window.clone();
    let connections = ARIA2_CONNECTIONS;

    let progress_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut reader = tokio::io::BufReader::new(stderr);
        let mut buffer = [0u8; 1024];
        let mut line_buffer = String::new();
        let mut last_emit = std::time::Instant::now();

        loop {
            match reader.read(&mut buffer).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buffer[..n]);
                    line_buffer.push_str(&chunk);

                    while let Some(cr_pos) = line_buffer.find('\r') {
                        let line = &line_buffer[..cr_pos];

                        if last_emit.elapsed().as_millis() >= PROGRESS_EMIT_INTERVAL_MS {
                            if let Some(progress_info) = parse_aria2_progress(line) {
                                let _ = window.emit(
                                    &event_name,
                                    InstallProgress {
                                        stage: "downloading".to_string(),
                                        progress: progress_info.percent,
                                        downloaded: progress_info.downloaded,
                                        total: progress_info.total,
                                        speed: progress_info.speed,
                                        message: format!(
                                            "Downloading {} {} (aria2 x{})...",
                                            dep_name, version, connections
                                        ),
                                    },
                                );
                                last_emit = std::time::Instant::now();
                            }
                        }

                        line_buffer = line_buffer[cr_pos + 1..].to_string();
                    }

                    while let Some(nl_pos) = line_buffer.find('\n') {
                        line_buffer = line_buffer[nl_pos + 1..].to_string();
                    }
                }
                Err(_) => break,
            }
        }
    });

    let status = child
        .wait()
        .await
        .map_err(|e| format!("aria2c failed: {}", e))?;

    let _ = progress_task.await;

    if !status.success() {
        return Err(format!("aria2c exited with code: {:?}", status.code()));
    }

    let metadata = tokio::fs::metadata(&dest)
        .await
        .map_err(|e| format!("Downloaded file not found: {}", e))?;

    if metadata.len() == 0 {
        let _ = tokio::fs::remove_file(&dest).await;
        return Err("Downloaded file is empty".to_string());
    }

    info!("aria2 download complete: {} bytes", metadata.len());
    Ok(())
}

#[cfg(not(target_os = "android"))]
struct Aria2Progress {
    percent: u8,
    downloaded: u64,
    total: u64,
    speed: f64,
}

#[cfg(not(target_os = "android"))]
fn parse_aria2_progress(line: &str) -> Option<Aria2Progress> {
    let mut percent: u8 = 0;
    let mut downloaded: u64 = 0;
    let mut total: u64 = 0;
    let mut speed: f64 = 0.0;

    if let Some(pct_start) = line.find('(') {
        if let Some(pct_end) = line[pct_start..].find('%') {
            if let Ok(p) = line[pct_start + 1..pct_start + pct_end].parse::<u8>() {
                percent = p;
            }
        }
    }

    if let Some(slash_idx) = line.find("iB/") {
        let before_slash = &line[..slash_idx + 2];

        let size_start = before_slash
            .rfind([' ', '[', '#'])
            .map(|i| i + 1)
            .unwrap_or(0);

        let dl_part = &before_slash[size_start..];
        downloaded = parse_size(dl_part);

        let after_slash = &line[slash_idx + 3..];
        if let Some(end) = after_slash.find(['(', ' ', ']']) {
            let total_part = &after_slash[..end];
            total = parse_size(total_part);
        }
    }

    if let Some(dl_idx) = line.find("DL:") {
        let speed_part = &line[dl_idx + 3..];
        if let Some(end) = speed_part.find([' ', ']', '[']) {
            let speed_str = &speed_part[..end];
            speed = parse_size(speed_str) as f64;
        } else {
            speed = parse_size(speed_part) as f64;
        }
    }

    if percent > 0 || downloaded > 0 || total > 0 || speed > 0.0 {
        Some(Aria2Progress {
            percent,
            downloaded,
            total,
            speed,
        })
    } else {
        None
    }
}

#[cfg(not(target_os = "android"))]
fn parse_size(s: &str) -> u64 {
    let s = s.trim();

    if let Some(pos) = s.find("GiB") {
        if let Ok(n) = s[..pos].parse::<f64>() {
            return (n * 1024.0 * 1024.0 * 1024.0) as u64;
        }
    }
    if let Some(pos) = s.find("MiB") {
        if let Ok(n) = s[..pos].parse::<f64>() {
            return (n * 1024.0 * 1024.0) as u64;
        }
    }
    if let Some(pos) = s.find("KiB") {
        if let Ok(n) = s[..pos].parse::<f64>() {
            return (n * 1024.0) as u64;
        }
    }
    if let Some(pos) = s.find('B') {
        if let Ok(n) = s[..pos].parse::<f64>() {
            return n as u64;
        }
    }

    0
}

#[cfg(not(target_os = "android"))]
type FileMatcher = fn(&str) -> bool;

#[cfg(not(target_os = "android"))]
struct ZipExtractConfig {
    matcher: FileMatcher,
    dest_name: &'static str,
    extract_all: bool,
}

#[cfg(not(target_os = "android"))]
async fn extract_from_zip(
    archive_path: &Path,
    bin_dir: &Path,
    config: ZipExtractConfig,
) -> Result<(), String> {
    if !archive_path.exists() {
        error!("Archive file does not exist: {:?}", archive_path);
        return Err(format!("Archive file does not exist: {:?}", archive_path));
    }

    let metadata = tokio::fs::metadata(&archive_path)
        .await
        .map_err(|e| format!("Failed to get archive metadata: {}", e))?;

    info!(
        "Archive size: {} bytes at {:?}",
        metadata.len(),
        archive_path
    );

    if metadata.len() == 0 {
        return Err("Archive file is empty".to_string());
    }

    let archive_path = archive_path.to_path_buf();
    let bin_dir = bin_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&archive_path)
            .map_err(|e| format!("Failed to open archive: {}", e))?;

        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("Failed to open zip: {}", e))?;

        let mut found = false;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read zip entry: {}", e))?;

            let name = file.name().to_string();

            if (config.matcher)(&name) && !file.is_dir() {
                let dest_path = bin_dir.join(config.dest_name);

                let mut contents = Vec::new();
                file.read_to_end(&mut contents)
                    .map_err(|e| format!("Failed to read file from zip: {}", e))?;

                std::fs::write(&dest_path, &contents)
                    .map_err(|e| format!("Failed to write file: {}", e))?;

                info!(
                    "Extracted {} from {} to {:?}",
                    config.dest_name, name, dest_path
                );
                found = true;

                if !config.extract_all {
                    break;
                }
            }
        }

        if !found {
            warn!(
                "No matching files found in archive for {}",
                config.dest_name
            );
        }

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

#[allow(dead_code)]
#[cfg(not(target_os = "android"))]
async fn extract_multiple_from_zip(
    archive_path: &Path,
    bin_dir: &Path,
    matchers: &[(FileMatcher, &'static str)],
) -> Result<(), String> {
    if !archive_path.exists() {
        error!("Archive file does not exist: {:?}", archive_path);
        return Err(format!("Archive file does not exist: {:?}", archive_path));
    }

    let metadata = tokio::fs::metadata(&archive_path)
        .await
        .map_err(|e| format!("Failed to get archive metadata: {}", e))?;

    info!(
        "Archive size: {} bytes at {:?}",
        metadata.len(),
        archive_path
    );

    if metadata.len() == 0 {
        return Err("Archive file is empty".to_string());
    }

    let archive_path = archive_path.to_path_buf();
    let bin_dir = bin_dir.to_path_buf();
    let matchers: Vec<_> = matchers.to_vec();

    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&archive_path)
            .map_err(|e| format!("Failed to open archive: {}", e))?;

        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("Failed to open zip: {}", e))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Failed to read zip entry: {}", e))?;

            let name = file.name().to_string();

            for (matcher, dest_name) in &matchers {
                if matcher(&name) && !file.is_dir() {
                    let dest_path = bin_dir.join(dest_name);

                    let mut contents = Vec::new();
                    file.read_to_end(&mut contents)
                        .map_err(|e| format!("Failed to read file from zip: {}", e))?;

                    std::fs::write(&dest_path, &contents)
                        .map_err(|e| format!("Failed to write file: {}", e))?;

                    info!("Extracted {} to {:?}", dest_name, dest_path);
                    break; // Move to next file in archive
                }
            }
        }

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

#[allow(dead_code)]
#[cfg(all(not(target_os = "android"), target_os = "windows"))]
fn is_ffmpeg_file(name: &str) -> bool {
    name.ends_with("/ffmpeg.exe") || name == "ffmpeg.exe"
}

#[allow(dead_code)]
#[cfg(all(not(target_os = "android"), target_os = "windows"))]
fn is_ffprobe_file(name: &str) -> bool {
    name.ends_with("/ffprobe.exe") || name == "ffprobe.exe"
}

#[allow(dead_code)]
#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
fn is_ffmpeg_file(name: &str) -> bool {
    (name.ends_with("/ffmpeg") || name == "ffmpeg") && !name.ends_with(".exe")
}

#[allow(dead_code)]
#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
fn is_ffprobe_file(name: &str) -> bool {
    (name.ends_with("/ffprobe") || name == "ffprobe") && !name.ends_with(".exe")
}

#[cfg(all(not(target_os = "android"), target_os = "windows"))]
fn is_aria2_file(name: &str) -> bool {
    name.ends_with("/aria2c.exe") || name == "aria2c.exe"
}

#[cfg(all(not(target_os = "android"), target_os = "linux"))]
fn is_aria2_file(name: &str) -> bool {
    (name.ends_with("/aria2c") || name == "aria2c") && !name.ends_with(".exe")
}

#[cfg(all(not(target_os = "android"), target_os = "windows"))]
fn is_deno_file(name: &str) -> bool {
    name == "deno.exe" || name.ends_with("/deno.exe")
}

#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
fn is_deno_file(name: &str) -> bool {
    name == "deno" || name.ends_with("/deno")
}

#[cfg(all(not(target_os = "android"), target_os = "windows"))]
fn is_quickjs_file(name: &str) -> bool {
    name == "qjs"
        || name == "qjs.com"
        || name == "qjs.exe"
        || name.ends_with("/qjs")
        || name.ends_with("/qjs.com")
        || name.ends_with("/qjs.exe")
}

#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
fn is_quickjs_file(name: &str) -> bool {
    (name == "qjs" || name.ends_with("/qjs") || name == "qjs.com" || name.ends_with("/qjs.com"))
        && !name.ends_with(".exe")
}

#[cfg(all(not(target_os = "android"), target_os = "windows"))]
fn is_lux_file(name: &str) -> bool {
    name == "lux.exe" || name.ends_with("/lux.exe")
}

#[allow(clippy::too_many_arguments)]
#[cfg(not(target_os = "android"))]
async fn download_file(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    window: &tauri::Window,
    event_name: &str,
    dep_name: &str,
    version: &str,
    proxy_config: Option<&ProxyConfig>,
) -> Result<(), String> {
    let config = proxy_config.cloned().unwrap_or_default();
    let strategies = proxy_strategies(&config);

    let aria2_path = get_aria2_path(app)?;
    if aria2_path.exists() {
        for (strategy_name, proxy_url) in &strategies {
            info!("Trying aria2 download with {} strategy", strategy_name);
            match download_with_aria2(
                app,
                url,
                dest,
                window,
                event_name,
                dep_name,
                version,
                proxy_url.as_deref(),
            )
            .await
            {
                Ok(()) => return Ok(()),
                Err(e) => {
                    warn!("aria2 download failed with {}: {}", strategy_name, e);
                }
            }
        }
        warn!("All aria2 strategies failed, falling back to reqwest");
    }

    download_with_progress(url, dest, window, event_name, dep_name, version, &config).await
}

#[cfg(not(target_os = "android"))]
async fn download_with_progress(
    url: &str,
    dest: &Path,
    window: &tauri::Window,
    event_name: &str,
    dep_name: &str,
    version: &str,
    proxy_config: &ProxyConfig,
) -> Result<(), String> {
    if dest.exists() {
        info!("Removing existing file: {:?}", dest);
        if let Err(e) = tokio::fs::remove_file(&dest).await {
            warn!("Failed to remove existing file: {}", e);
        }
    }

    info!("Starting download: {} -> {:?}", url, dest);

    let response = fetch(url, proxy_config).await.map_err(|e| {
        error!("Failed to download {}: {}", dep_name, e);
        format!("Failed to download {}: {}", dep_name, e)
    })?;

    let total_size = response.content_length().unwrap_or(0);
    info!("Download response OK, expected size: {} bytes", total_size);

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let mut file = tokio::fs::File::create(&dest)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    let start_time = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write error: {}", e))?;

        downloaded += chunk.len() as u64;

        if last_emit.elapsed().as_millis() >= PROGRESS_EMIT_INTERVAL_MS {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                downloaded as f64 / elapsed
            } else {
                0.0
            };

            let progress = if total_size > 0 {
                ((downloaded as f64 / total_size as f64) * 100.0) as u8
            } else {
                0
            };

            let _ = window.emit(
                event_name,
                InstallProgress {
                    stage: "downloading".to_string(),
                    progress,
                    downloaded,
                    total: total_size,
                    speed,
                    message: format!("Downloading {} {}...", dep_name, version),
                },
            );

            last_emit = std::time::Instant::now();
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;
    drop(file);

    let metadata = tokio::fs::metadata(&dest)
        .await
        .map_err(|e| format!("Downloaded file not found: {}", e))?;

    let actual_size = metadata.len();
    info!(
        "Download complete: {} bytes written to {:?}",
        actual_size, dest
    );

    if total_size > 0 && actual_size != total_size {
        error!(
            "Size mismatch! Expected {} bytes, got {} bytes",
            total_size, actual_size
        );
        if let Err(e) = tokio::fs::remove_file(&dest).await {
            warn!("Failed to clean up incomplete download: {}", e);
        }
        return Err(format!(
            "Download incomplete: expected {} bytes, got {}",
            total_size, actual_size
        ));
    }

    if actual_size == 0 {
        error!("Downloaded file is empty!");
        if let Err(e) = tokio::fs::remove_file(&dest).await {
            warn!("Failed to clean up empty download: {}", e);
        }
        return Err("Downloaded file is empty".to_string());
    }

    Ok(())
}

#[cfg(all(unix, not(target_os = "android")))]
async fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    info!("Setting executable permissions on: {:?}", path);

    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("Failed to get metadata for {:?}: {}", path, e))?;

    let mut perms = metadata.permissions();
    let old_mode = perms.mode();
    perms.set_mode(0o755);

    tokio::fs::set_permissions(&path, perms)
        .await
        .map_err(|e| format!("Failed to set permissions on {:?}: {}", path, e))?;

    let new_metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("Failed to verify permissions: {}", e))?;
    let new_mode = new_metadata.permissions().mode();

    info!("Permissions updated ({:o} -> {:o})", old_mode, new_mode);

    if new_mode & 0o111 == 0 {
        warn!("Execute permission not set! Mode is {:o}", new_mode);
        return Err(format!(
            "Failed to set execute permission. Current mode: {:o}",
            new_mode
        ));
    }

    Ok(())
}

#[cfg(not(target_os = "android"))]
async fn get_latest_release(proxy_config: &ProxyConfig) -> Result<GitHubRelease, String> {
    info!("Fetching latest release info from GitHub API...");
    fetch_github_release("yt-dlp/yt-dlp", proxy_config).await
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub async fn get_ytdlp_releases(
    proxy_config: Option<ProxyConfig>,
) -> Result<Vec<ReleaseInfo>, String> {
    let config = proxy_config.unwrap_or_default();
    let response = fetch(
        "https://api.github.com/repos/yt-dlp/yt-dlp/releases?per_page=10",
        &config,
    )
    .await
    .map_err(|e| format!("Failed to fetch releases: {}", e))?;

    let releases: Vec<GitHubRelease> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse releases: {}", e))?;

    Ok(releases
        .into_iter()
        .map(|r| ReleaseInfo {
            tag: r.tag_name,
            name: r.name,
            published_at: r.published_at,
        })
        .collect())
}

#[cfg(target_os = "android")]
#[tauri::command]
pub async fn get_ytdlp_releases() -> Result<Vec<ReleaseInfo>, String> {
    Ok(vec![ReleaseInfo {
        tag: "embedded".to_string(),
        name: "youtubedl-android (embedded)".to_string(),
        published_at: "".to_string(),
    }])
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn check_ytdlp(
    app: AppHandle,
    check_updates: Option<bool>,
) -> Result<DependencyStatus, String> {
    #[cfg(target_os = "android")]
    {
        return Ok(DependencyStatus::embedded("youtubedl-android library"));
    }

    #[cfg(not(target_os = "android"))]
    {
        let ytdlp_path = get_ytdlp_path(&app)?;
        debug!("Checking yt-dlp at: {:?}", ytdlp_path);

        if ytdlp_path.exists() {
            info!("yt-dlp binary exists at {:?}", ytdlp_path);

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = std::fs::metadata(&ytdlp_path) {
                    let mode = metadata.permissions().mode();
                    let size = metadata.len();
                    info!("yt-dlp file size: {} bytes, permissions: {:o}", size, mode);
                }
            }

            let mut cmd = tokio::process::Command::new(&ytdlp_path);
            cmd.arg("--version");

            #[cfg(target_os = "windows")]
            cmd.hide_console();

            let output = cmd.output().await;

            match output {
                Ok(output) if output.status.success() => {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    info!("yt-dlp version: {}", version);

                    let update_available = if check_updates.unwrap_or(false) {
                        match get_latest_release(&ProxyConfig::default()).await {
                            Ok(release) => {
                                if release.tag_name != version {
                                    Some(release.tag_name)
                                } else {
                                    None
                                }
                            }
                            Err(_) => None,
                        }
                    } else {
                        None
                    };

                    Ok(DependencyStatus::installed(
                        version,
                        ytdlp_path.to_string_lossy().to_string(),
                    )
                    .with_update(update_available))
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    warn!("yt-dlp exists but failed to run: {}", stderr);
                    Ok(DependencyStatus::not_installed())
                }
                Err(e) => {
                    error!("Failed to execute yt-dlp at {:?}: {}", ytdlp_path, e);
                    Ok(DependencyStatus {
                        path: Some(ytdlp_path.to_string_lossy().to_string()),
                        ..DependencyStatus::not_installed()
                    })
                }
            }
        } else {
            debug!("yt-dlp not found at {:?}", ytdlp_path);
            Ok(DependencyStatus::not_installed())
        }
    }
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn install_ytdlp(
    app: AppHandle,
    window: tauri::Window,
    version: Option<String>,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    info!("Starting yt-dlp installation");

    #[cfg(target_os = "android")]
    {
        info!("Android: yt-dlp is bundled with the app via youtubedl-android");

        let _ = window.emit(
            EVENT_YTDLP_PROGRESS,
            InstallProgress {
                stage: "complete".to_string(),
                progress: 100,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "yt-dlp is ready! (bundled with app)".to_string(),
            },
        );

        return Ok("embedded".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let ytdlp_path = get_ytdlp_path(&app)?;
        info!("Target path: {:?}", ytdlp_path);

        if let Some(parent) = ytdlp_path.parent() {
            info!("Creating bin directory: {:?}", parent);
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                error!("Failed to create bin directory: {}", e);
                format!("Failed to create bin directory: {}", e)
            })?;
        }

        let _ = window.emit(
            EVENT_YTDLP_PROGRESS,
            InstallProgress {
                stage: "fetching".to_string(),
                progress: 0,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Fetching latest release info...".to_string(),
            },
        );

        let config = proxy_config.clone().unwrap_or_default();

        let target_version = match version {
            Some(v) => v,
            None => {
                info!("Fetching latest release info");
                let release = get_latest_release(&config).await?;
                release.tag_name
            }
        };

        info!("Target version: {}", target_version);

        let download_url = format!(
            "https://github.com/yt-dlp/yt-dlp/releases/download/{}/{}",
            target_version, YTDLP_BINARY
        );

        info!("Download URL: {}", download_url);

        if ytdlp_path.exists() {
            info!("Removing existing file: {:?}", ytdlp_path);
            let _ = tokio::fs::remove_file(&ytdlp_path).await;
        }

        let _ = window.emit(
            EVENT_YTDLP_PROGRESS,
            InstallProgress {
                stage: "downloading".to_string(),
                progress: 0,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: format!("Downloading yt-dlp {}...", target_version),
            },
        );

        download_file(
            &app,
            &download_url,
            &ytdlp_path,
            &window,
            EVENT_YTDLP_PROGRESS,
            "yt-dlp",
            &target_version,
            proxy_config.as_ref(),
        )
        .await
        .map_err(|e| {
            error!("Failed to download yt-dlp: {}", e);
            format!("Failed to download yt-dlp: {}", e)
        })?;

        let metadata = tokio::fs::metadata(&ytdlp_path)
            .await
            .map_err(|e| format!("Downloaded file not found: {}", e))?;
        let total_size = metadata.len();

        #[cfg(unix)]
        {
            let _ = window.emit(
                EVENT_YTDLP_PROGRESS,
                InstallProgress {
                    stage: "permissions".to_string(),
                    progress: 95,
                    downloaded: total_size,
                    total: total_size,
                    speed: 0.0,
                    message: "Setting executable permissions...".to_string(),
                },
            );

            make_executable(&ytdlp_path).await.map_err(|e| {
                error!("Failed to make yt-dlp executable: {}", e);
                format!("Failed to make yt-dlp executable: {}", e)
            })?;
        }

        let file_exists = ytdlp_path.exists();
        info!("Installation complete. File exists: {}", file_exists);

        if !file_exists {
            error!("File not found after installation!");
            return Err("Installation failed: file not found after download".to_string());
        }

        let _ = window.emit(
            EVENT_YTDLP_PROGRESS,
            InstallProgress {
                stage: "verifying".to_string(),
                progress: 98,
                downloaded: total_size,
                total: total_size,
                speed: 0.0,
                message: "Verifying installation...".to_string(),
            },
        );

        let mut test_cmd = tokio::process::Command::new(&ytdlp_path);
        test_cmd.arg("--version");

        #[cfg(target_os = "windows")]
        test_cmd.hide_console();

        let test_result = test_cmd.output().await;

        match test_result {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                info!("yt-dlp verification successful, version: {}", version);
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!("yt-dlp test run failed: {}", stderr);
                return Err(format!("yt-dlp verification failed: {}", stderr));
            }
            Err(e) => {
                error!("Failed to execute yt-dlp for verification: {}", e);
                return Err(format!("yt-dlp verification failed: {}", e));
            }
        }

        let _ = window.emit(
            EVENT_YTDLP_PROGRESS,
            InstallProgress {
                stage: "complete".to_string(),
                progress: 100,
                downloaded: total_size,
                total: total_size,
                speed: 0.0,
                message: format!("yt-dlp {} installed successfully!", target_version),
            },
        );

        info!(
            "yt-dlp {} installed successfully at {:?}",
            target_version, ytdlp_path
        );
        Ok(ytdlp_path.to_string_lossy().to_string())
    }
}

#[tauri::command]
pub async fn uninstall_ytdlp(app: AppHandle) -> Result<(), String> {
    let ytdlp_path = get_ytdlp_path(&app)?;

    if ytdlp_path.exists() {
        tokio::fs::remove_file(&ytdlp_path)
            .await
            .map_err(|e| format!("Failed to remove yt-dlp: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn check_ffmpeg(app: AppHandle) -> Result<DependencyStatus, String> {
    #[cfg(target_os = "android")]
    {
        return Ok(DependencyStatus::embedded("youtubedl-android library"));
    }

    #[cfg(not(target_os = "android"))]
    {
        let ffmpeg_path = get_ffmpeg_path(&app)?;

        if ffmpeg_path.exists() {
            let mut cmd = tokio::process::Command::new(&ffmpeg_path);
            cmd.arg("-version");

            #[cfg(target_os = "windows")]
            cmd.hide_console();

            let output = cmd.output().await;

            match output {
                Ok(output) if output.status.success() => {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    let version = output_str
                        .lines()
                        .next()
                        .and_then(|line| line.strip_prefix("ffmpeg version "))
                        .map(|v| v.split_whitespace().next().unwrap_or("unknown"))
                        .unwrap_or("unknown")
                        .to_string();

                    Ok(DependencyStatus::installed(
                        version,
                        ffmpeg_path.to_string_lossy().to_string(),
                    ))
                }
                _ => Ok(DependencyStatus::not_installed()),
            }
        } else {
            Ok(DependencyStatus::not_installed())
        }
    }
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn install_ffmpeg(
    app: AppHandle,
    window: tauri::Window,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    return Err(
        "ffmpeg installation is not supported on this platform. On Android, install via Termux."
            .to_string(),
    );

    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        let ffmpeg_path = get_ffmpeg_path(&app)?;
        let bin_dir = get_bin_dir(&app)?;

        tokio::fs::create_dir_all(&bin_dir)
            .await
            .map_err(|e| format!("Failed to create bin directory: {}", e))?;

        let _ = window.emit(
            EVENT_FFMPEG_PROGRESS,
            InstallProgress {
                stage: "downloading".to_string(),
                progress: 0,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Downloading ffmpeg...".to_string(),
            },
        );

        #[cfg(target_os = "windows")]
        let download_url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
        #[cfg(target_os = "macos")]
        let download_url = "https://evermeet.cx/ffmpeg/getrelease/zip";
        #[cfg(target_os = "linux")]
        let download_url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz";

        let temp_archive = bin_dir.join("ffmpeg_temp.archive");

        download_file(
            &app,
            download_url,
            &temp_archive,
            &window,
            EVENT_FFMPEG_PROGRESS,
            "ffmpeg",
            "latest",
            proxy_config.as_ref(),
        )
        .await?;

        let _ = window.emit(
            EVENT_FFMPEG_PROGRESS,
            InstallProgress {
                stage: "extracting".to_string(),
                progress: 90,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Extracting ffmpeg...".to_string(),
            },
        );

        #[cfg(target_os = "windows")]
        {
            extract_zip_ffmpeg(&temp_archive, &bin_dir).await?;
        }

        #[cfg(target_os = "macos")]
        {
            extract_zip_ffmpeg(&temp_archive, &bin_dir).await?;
        }

        #[cfg(target_os = "linux")]
        {
            extract_tar_xz_ffmpeg(&temp_archive, &bin_dir).await?;
        }

        let _ = tokio::fs::remove_file(&temp_archive).await;

        #[cfg(unix)]
        {
            make_executable(&ffmpeg_path).await?;
            let ffprobe_path = bin_dir.join("ffprobe");
            if ffprobe_path.exists() {
                make_executable(&ffprobe_path).await?;
            }
        }

        let _ = window.emit(
            EVENT_FFMPEG_PROGRESS,
            InstallProgress {
                stage: "complete".to_string(),
                progress: 100,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "ffmpeg installed successfully!".to_string(),
            },
        );

        Ok(ffmpeg_path.to_string_lossy().to_string())
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
async fn extract_zip_ffmpeg(archive_path: &Path, bin_dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let matchers: &[(FileMatcher, &'static str)] = &[
        (is_ffmpeg_file, "ffmpeg.exe"),
        (is_ffprobe_file, "ffprobe.exe"),
    ];
    #[cfg(not(target_os = "windows"))]
    let matchers: &[(FileMatcher, &'static str)] =
        &[(is_ffmpeg_file, "ffmpeg"), (is_ffprobe_file, "ffprobe")];

    extract_multiple_from_zip(archive_path, bin_dir, matchers).await
}

#[cfg(target_os = "linux")]
async fn extract_tar_xz_ffmpeg(archive_path: &Path, bin_dir: &Path) -> Result<(), String> {
    use std::process::Stdio;

    let output = tokio::process::Command::new("tar")
        .args([
            "-xJf",
            archive_path.to_str().ok_or("Invalid path")?,
            "-C",
            bin_dir.to_str().ok_or("Invalid path")?,
            "--strip-components=2",
            "--wildcards",
            "*/bin/ffmpeg",
            "*/bin/ffprobe",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to run tar: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tar extraction failed: {}", stderr));
    }

    Ok(())
}

#[tauri::command]
pub async fn uninstall_ffmpeg(app: AppHandle) -> Result<(), String> {
    let ffmpeg_path = get_ffmpeg_path(&app)?;
    let bin_dir = get_bin_dir(&app)?;

    if ffmpeg_path.exists() {
        tokio::fs::remove_file(&ffmpeg_path)
            .await
            .map_err(|e| format!("Failed to remove ffmpeg: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    let ffprobe_name = "ffprobe.exe";
    #[cfg(not(target_os = "windows"))]
    let ffprobe_name = "ffprobe";

    let ffprobe_path = bin_dir.join(ffprobe_name);
    if ffprobe_path.exists() {
        let _ = tokio::fs::remove_file(&ffprobe_path).await;
    }

    Ok(())
}

#[cfg(not(target_os = "android"))]
async fn get_latest_aria2_version(proxy_config: &ProxyConfig) -> Result<String, String> {
    let release = fetch_github_release("aria2/aria2", proxy_config).await?;

    let version = release
        .tag_name
        .strip_prefix("release-")
        .unwrap_or(&release.tag_name)
        .to_string();

    Ok(version)
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn check_aria2(app: AppHandle) -> Result<DependencyStatus, String> {
    #[cfg(target_os = "android")]
    {
        return Ok(DependencyStatus::embedded("youtubedl-android library"));
    }

    #[cfg(not(target_os = "android"))]
    {
        let aria2_path = get_aria2_path(&app)?;

        if aria2_path.exists() {
            let mut cmd = tokio::process::Command::new(&aria2_path);
            cmd.arg("--version");

            #[cfg(target_os = "windows")]
            cmd.hide_console();

            let output = cmd.output().await;

            match output {
                Ok(output) if output.status.success() => {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    let version = output_str
                        .lines()
                        .next()
                        .and_then(|line| line.strip_prefix("aria2 version "))
                        .unwrap_or("unknown")
                        .to_string();

                    Ok(DependencyStatus::installed(
                        version,
                        aria2_path.to_string_lossy().to_string(),
                    ))
                }
                _ => Ok(DependencyStatus::not_installed()),
            }
        } else {
            Ok(DependencyStatus::not_installed())
        }
    }
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn install_aria2(
    app: AppHandle,
    window: tauri::Window,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    return Err("aria2 installation on macOS: please install via 'brew install aria2'".to_string());

    #[cfg(target_os = "android")]
    return Err(
        "aria2 installation on Android is not supported. Please install via Termux.".to_string(),
    );

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_os = "android"
    )))]
    return Err("aria2 installation is not supported on this platform.".to_string());

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        let aria2_path = get_aria2_path(&app)?;
        let bin_dir = get_bin_dir(&app)?;

        tokio::fs::create_dir_all(&bin_dir)
            .await
            .map_err(|e| format!("Failed to create bin directory: {}", e))?;

        let config = proxy_config.clone().unwrap_or_default();

        let _ = window.emit(
            EVENT_ARIA2_PROGRESS,
            InstallProgress {
                stage: "fetching".to_string(),
                progress: 0,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Fetching latest aria2 version...".to_string(),
            },
        );

        let version = get_latest_aria2_version(&config).await.unwrap_or_else(|e| {
            warn!(
                "Failed to fetch latest aria2 version, using fallback: {}",
                e
            );
            ARIA2_FALLBACK_VERSION.to_string()
        });
        info!("Installing aria2 version: {}", version);

        let _ = window.emit(
            EVENT_ARIA2_PROGRESS,
            InstallProgress {
                stage: "downloading".to_string(),
                progress: 5,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: format!("Downloading aria2 {}...", version),
            },
        );

        #[cfg(target_os = "windows")]
        let download_url = format!(
            "https://github.com/aria2/aria2/releases/download/release-{}/aria2-{}-win-64bit-build1.zip",
            version, version
        );

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let download_url = format!(
            "https://github.com/abcfy2/aria2-static-build/releases/download/{}/aria2-x86_64-linux-musl_static.zip",
            version
        );
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let download_url = format!(
            "https://github.com/abcfy2/aria2-static-build/releases/download/{}/aria2-aarch64-linux-musl_static.zip",
            version
        );

        let temp_archive = bin_dir.join("aria2_temp.zip");

        download_with_progress(
            &download_url,
            &temp_archive,
            &window,
            EVENT_ARIA2_PROGRESS,
            "aria2",
            &version,
            &config,
        )
        .await?;

        let _ = window.emit(
            EVENT_ARIA2_PROGRESS,
            InstallProgress {
                stage: "extracting".to_string(),
                progress: 90,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Extracting aria2...".to_string(),
            },
        );

        #[cfg(target_os = "windows")]
        extract_zip_aria2(&temp_archive, &bin_dir).await?;

        #[cfg(target_os = "linux")]
        extract_zip_aria2(&temp_archive, &bin_dir).await?;

        let _ = tokio::fs::remove_file(&temp_archive).await;

        #[cfg(target_os = "linux")]
        {
            make_executable(&aria2_path).await?;
        }

        let _ = window.emit(
            EVENT_ARIA2_PROGRESS,
            InstallProgress {
                stage: "complete".to_string(),
                progress: 100,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "aria2 installed successfully!".to_string(),
            },
        );

        Ok(aria2_path.to_string_lossy().to_string())
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
async fn extract_zip_aria2(archive_path: &Path, bin_dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let dest_name = "aria2c.exe";
    #[cfg(target_os = "linux")]
    let dest_name = "aria2c";

    extract_from_zip(
        archive_path,
        bin_dir,
        ZipExtractConfig {
            matcher: is_aria2_file,
            dest_name,
            extract_all: false,
        },
    )
    .await
}

#[tauri::command]
pub async fn uninstall_aria2(app: AppHandle) -> Result<(), String> {
    let aria2_path = get_aria2_path(&app)?;

    if aria2_path.exists() {
        tokio::fs::remove_file(&aria2_path)
            .await
            .map_err(|e| format!("Failed to remove aria2: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn check_deno(app: AppHandle) -> Result<DependencyStatus, String> {
    let deno_path = get_deno_path(&app)?;

    if deno_path.exists() {
        let mut cmd = tokio::process::Command::new(&deno_path);
        cmd.arg("--version");

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        let output = cmd.output().await;

        match output {
            Ok(output) if output.status.success() => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let version = output_str
                    .lines()
                    .next()
                    .and_then(|line| line.strip_prefix("deno "))
                    .map(|v| v.split_whitespace().next().unwrap_or("unknown"))
                    .unwrap_or("unknown")
                    .to_string();

                Ok(DependencyStatus::installed(
                    version,
                    deno_path.to_string_lossy().to_string(),
                ))
            }
            _ => Ok(DependencyStatus::not_installed()),
        }
    } else {
        Ok(DependencyStatus::not_installed())
    }
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn install_deno(
    app: AppHandle,
    window: tauri::Window,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(target_os = "android")]
    return Err("Deno installation on Android is not supported.".to_string());

    #[cfg(not(target_os = "android"))]
    {
        let deno_path = get_deno_path(&app)?;
        let bin_dir = get_bin_dir(&app)?;

        tokio::fs::create_dir_all(&bin_dir)
            .await
            .map_err(|e| format!("Failed to create bin directory: {}", e))?;

        let _ = window.emit(
            EVENT_DENO_PROGRESS,
            InstallProgress {
                stage: "downloading".to_string(),
                progress: 0,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Downloading deno...".to_string(),
            },
        );

        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        let download_url = "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip";
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        let download_url = "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-apple-darwin.zip";
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let download_url = "https://github.com/denoland/deno/releases/latest/download/deno-aarch64-apple-darwin.zip";
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let download_url = "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-unknown-linux-gnu.zip";
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let download_url = "https://github.com/denoland/deno/releases/latest/download/deno-aarch64-unknown-linux-gnu.zip";

        let temp_archive = bin_dir.join("deno_temp.zip");

        download_file(
            &app,
            download_url,
            &temp_archive,
            &window,
            EVENT_DENO_PROGRESS,
            "deno",
            "latest",
            proxy_config.as_ref(),
        )
        .await?;

        let _ = window.emit(
            EVENT_DENO_PROGRESS,
            InstallProgress {
                stage: "extracting".to_string(),
                progress: 90,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Extracting deno...".to_string(),
            },
        );

        extract_zip_deno(&temp_archive, &bin_dir).await?;

        let _ = tokio::fs::remove_file(&temp_archive).await;

        #[cfg(unix)]
        {
            make_executable(&deno_path).await?;
        }

        let _ = window.emit(
            EVENT_DENO_PROGRESS,
            InstallProgress {
                stage: "verifying".to_string(),
                progress: 95,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Verifying installation...".to_string(),
            },
        );

        let mut test_cmd = tokio::process::Command::new(&deno_path);
        test_cmd.arg("--version");

        #[cfg(target_os = "windows")]
        test_cmd.hide_console();

        match test_cmd.output().await {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout);
                info!(
                    "Deno installed successfully: {}",
                    version.lines().next().unwrap_or("unknown")
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("Deno verification failed: {}", stderr));
            }
            Err(e) => {
                return Err(format!("Failed to run deno: {}", e));
            }
        }

        let _ = window.emit(
            EVENT_DENO_PROGRESS,
            InstallProgress {
                stage: "complete".to_string(),
                progress: 100,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Deno installed successfully!".to_string(),
            },
        );

        Ok(deno_path.to_string_lossy().to_string())
    }
}

#[cfg(not(target_os = "android"))]
async fn extract_zip_deno(archive_path: &Path, bin_dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let dest_name = "deno.exe";
    #[cfg(not(target_os = "windows"))]
    let dest_name = "deno";

    extract_from_zip(
        archive_path,
        bin_dir,
        ZipExtractConfig {
            matcher: is_deno_file,
            dest_name,
            extract_all: false,
        },
    )
    .await
}

#[tauri::command]
pub async fn uninstall_deno(app: AppHandle) -> Result<(), String> {
    let deno_path = get_deno_path(&app)?;

    if deno_path.exists() {
        tokio::fs::remove_file(&deno_path)
            .await
            .map_err(|e| format!("Failed to remove deno: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn check_quickjs(app: AppHandle) -> Result<DependencyStatus, String> {
    let quickjs_path = get_quickjs_path(&app)?;

    if quickjs_path.exists() {
        match verify_quickjs(&quickjs_path).await {
            Ok(()) => Ok(DependencyStatus::installed(
                "installed".to_string(),
                quickjs_path.to_string_lossy().to_string(),
            )),
            Err(_) => Ok(DependencyStatus::not_installed()),
        }
    } else {
        Ok(DependencyStatus::not_installed())
    }
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn install_quickjs(
    app: AppHandle,
    window: tauri::Window,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(target_os = "android")]
    return Err("QuickJS installation on Android is not supported.".to_string());

    #[cfg(not(target_os = "android"))]
    {
        let quickjs_path = get_quickjs_path(&app)?;
        let bin_dir = get_bin_dir(&app)?;

        tokio::fs::create_dir_all(&bin_dir)
            .await
            .map_err(|e| format!("Failed to create bin directory: {}", e))?;

        let _ = window.emit(
            EVENT_QUICKJS_PROGRESS,
            InstallProgress {
                stage: "fetching".to_string(),
                progress: 0,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Fetching latest QuickJS version...".to_string(),
            },
        );

        let config = proxy_config.unwrap_or_default();

        // Fetch latest version from bellard.org
        let version = match fetch(
            "https://bellard.org/quickjs/binary_releases/LATEST.json",
            &config,
        )
        .await
        {
            Ok(response) => {
                #[derive(serde::Deserialize)]
                struct LatestVersion {
                    version: String,
                }
                match response.json::<LatestVersion>().await {
                    Ok(latest) => latest.version,
                    Err(e) => {
                        warn!("Failed to parse LATEST.json: {}, using fallback", e);
                        "2025-09-13".to_string()
                    }
                }
            }
            Err(e) => {
                warn!("Failed to fetch LATEST.json: {}, using fallback", e);
                "2025-09-13".to_string()
            }
        };

        info!("QuickJS latest version: {}", version);

        let _ = window.emit(
            EVENT_QUICKJS_PROGRESS,
            InstallProgress {
                stage: "downloading".to_string(),
                progress: 5,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: format!("Downloading QuickJS {}...", version),
            },
        );

        // Build download URL based on platform
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        let download_url = format!(
            "https://bellard.org/quickjs/binary_releases/quickjs-win-x86_64-{}.zip",
            version
        );
        #[cfg(all(target_os = "windows", target_arch = "x86"))]
        let download_url = format!(
            "https://bellard.org/quickjs/binary_releases/quickjs-win-i686-{}.zip",
            version
        );
        #[cfg(target_os = "macos")]
        let download_url = format!(
            "https://bellard.org/quickjs/binary_releases/quickjs-cosmo-{}.zip",
            version
        );
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let download_url = format!(
            "https://bellard.org/quickjs/binary_releases/quickjs-linux-x86_64-{}.zip",
            version
        );
        #[cfg(all(target_os = "linux", target_arch = "x86"))]
        let download_url = format!(
            "https://bellard.org/quickjs/binary_releases/quickjs-linux-i686-{}.zip",
            version
        );
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let download_url = format!(
            "https://bellard.org/quickjs/binary_releases/quickjs-cosmo-{}.zip",
            version
        );

        let temp_archive = bin_dir.join("quickjs_temp.zip");
        download_with_progress(
            &download_url,
            &temp_archive,
            &window,
            EVENT_QUICKJS_PROGRESS,
            "QuickJS",
            &version,
            &config,
        )
        .await?;

        let _ = window.emit(
            EVENT_QUICKJS_PROGRESS,
            InstallProgress {
                stage: "extracting".to_string(),
                progress: 90,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Extracting QuickJS...".to_string(),
            },
        );

        extract_zip_quickjs(&temp_archive, &bin_dir).await?;

        let _ = tokio::fs::remove_file(&temp_archive).await;

        #[cfg(unix)]
        {
            make_executable(&quickjs_path).await?;
        }

        let _ = window.emit(
            EVENT_QUICKJS_PROGRESS,
            InstallProgress {
                stage: "verifying".to_string(),
                progress: 95,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Verifying installation...".to_string(),
            },
        );

        verify_quickjs(&quickjs_path).await.map_err(|e| {
            format!("QuickJS verification failed: {}", e)
        })?;

        info!("QuickJS installed successfully");

        let _ = window.emit(
            EVENT_QUICKJS_PROGRESS,
            InstallProgress {
                stage: "complete".to_string(),
                progress: 100,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "QuickJS installed successfully!".to_string(),
            },
        );

        Ok(quickjs_path.to_string_lossy().to_string())
    }
}

#[cfg(not(target_os = "android"))]
async fn verify_quickjs(quickjs_path: &Path) -> Result<(), String> {
    let attempts: &[(&str, &[&str])] = &[
        ("std_print", &["--std", "-e", "print('ok')"]),
        ("print", &["-e", "print('ok')"]),
    ];

    let mut errors: Vec<String> = Vec::new();

    for (label, args) in attempts {
        let mut cmd = tokio::process::Command::new(quickjs_path);
        cmd.args(*args);

        #[cfg(target_os = "windows")]
        cmd.hide_console();

        match cmd.output().await {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains("ok") {
                    return Ok(());
                }
                errors.push(format!(
                    "attempt={} exit={:?} stdout_missing_ok stdout={} stderr={}",
                    label,
                    output.status.code(),
                    stdout.trim(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            Ok(output) => {
                errors.push(format!(
                    "attempt={} exit={:?} stdout={} stderr={}",
                    label,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout).trim(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            Err(e) => {
                errors.push(format!("attempt={} spawn_error={}", label, e));
            }
        }
    }

    Err(errors.join(" | "))
}

#[cfg(not(target_os = "android"))]
async fn extract_zip_quickjs(archive_path: &Path, bin_dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let dest_name = "qjs.exe";
    #[cfg(not(target_os = "windows"))]
    let dest_name = "qjs";

    extract_from_zip(
        archive_path,
        bin_dir,
        ZipExtractConfig {
            matcher: is_quickjs_file,
            dest_name,
            extract_all: false,
        },
    )
    .await
}

#[tauri::command]
pub async fn uninstall_quickjs(app: AppHandle) -> Result<(), String> {
    let quickjs_path = get_quickjs_path(&app)?;

    if quickjs_path.exists() {
        tokio::fs::remove_file(&quickjs_path)
            .await
            .map_err(|e| format!("Failed to remove QuickJS: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn check_lux(app: AppHandle) -> Result<DependencyStatus, String> {
    #[cfg(target_os = "android")]
    return Ok(DependencyStatus::not_installed());

    #[cfg(not(target_os = "android"))]
    {
        let lux_path = get_lux_path(&app)?;

        if lux_path.exists() {
            let mut cmd = tokio::process::Command::new(&lux_path);
            cmd.arg("-v");

            #[cfg(target_os = "windows")]
            cmd.hide_console();

            let output = cmd.output().await;

            match output {
                Ok(output) if output.status.success() => {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    // Format: "lux: version X.Y.Z, A fast and simple..."
                    let version = output_str
                        .trim()
                        .strip_prefix("lux: version ")
                        .and_then(|s| s.split(',').next())
                        .unwrap_or("installed")
                        .trim()
                        .to_string();
                    Ok(DependencyStatus::installed(
                        if version.is_empty() {
                            "installed".to_string()
                        } else {
                            version
                        },
                        lux_path.to_string_lossy().to_string(),
                    ))
                }
                _ => Ok(DependencyStatus::not_installed()),
            }
        } else {
            Ok(DependencyStatus::not_installed())
        }
    }
}

#[tauri::command]
#[allow(unused_variables)]
pub async fn install_lux(
    app: AppHandle,
    window: tauri::Window,
    proxy_config: Option<ProxyConfig>,
) -> Result<String, String> {
    #[cfg(target_os = "android")]
    return Err("Lux installation on Android is not supported.".to_string());

    #[cfg(not(target_os = "android"))]
    {
        let lux_path = get_lux_path(&app)?;
        let bin_dir = get_bin_dir(&app)?;

        tokio::fs::create_dir_all(&bin_dir)
            .await
            .map_err(|e| format!("Failed to create bin directory: {}", e))?;

        let _ = window.emit(
            EVENT_LUX_PROGRESS,
            InstallProgress {
                stage: "fetching".to_string(),
                progress: 0,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Fetching latest lux release...".to_string(),
            },
        );

        let config = proxy_config.clone().unwrap_or_default();

        let release = fetch_github_release("iawia002/lux", &config).await?;

        let version = release.tag_name.trim_start_matches('v');
        info!("Latest lux version: {}", version);

        let _ = window.emit(
            EVENT_LUX_PROGRESS,
            InstallProgress {
                stage: "downloading".to_string(),
                progress: 5,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: format!("Downloading lux {}...", version),
            },
        );

        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        let asset_name = format!("lux_{}_Windows_x86_64.zip", version);
        #[cfg(all(target_os = "windows", target_arch = "x86"))]
        let asset_name = format!("lux_{}_Windows_i386.zip", version);
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        let asset_name = format!("lux_{}_Darwin_x86_64.tar.gz", version);
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let asset_name = format!("lux_{}_Darwin_arm64.tar.gz", version);
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let asset_name = format!("lux_{}_Linux_x86_64.tar.gz", version);
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let asset_name = format!("lux_{}_Linux_arm64.tar.gz", version);

        let download_url = format!(
            "https://github.com/iawia002/lux/releases/download/{}/{}",
            release.tag_name, asset_name
        );

        info!("Downloading lux from: {}", download_url);

        #[cfg(target_os = "windows")]
        let temp_archive = bin_dir.join("lux_temp.zip");
        #[cfg(not(target_os = "windows"))]
        let temp_archive = bin_dir.join("lux_temp.tar.gz");

        download_with_progress(
            &download_url,
            &temp_archive,
            &window,
            EVENT_LUX_PROGRESS,
            "Lux",
            version,
            &config,
        )
        .await?;

        let _ = window.emit(
            EVENT_LUX_PROGRESS,
            InstallProgress {
                stage: "extracting".to_string(),
                progress: 90,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Extracting lux...".to_string(),
            },
        );

        #[cfg(target_os = "windows")]
        extract_zip_lux(&temp_archive, &bin_dir).await?;
        #[cfg(not(target_os = "windows"))]
        extract_tar_gz_lux(&temp_archive, &bin_dir).await?;

        let _ = tokio::fs::remove_file(&temp_archive).await;

        #[cfg(unix)]
        {
            make_executable(&lux_path).await?;
        }

        let _ = window.emit(
            EVENT_LUX_PROGRESS,
            InstallProgress {
                stage: "verifying".to_string(),
                progress: 95,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Verifying installation...".to_string(),
            },
        );

        let mut test_cmd = tokio::process::Command::new(&lux_path);
        test_cmd.arg("-v");

        #[cfg(target_os = "windows")]
        test_cmd.hide_console();

        match test_cmd.output().await {
            Ok(output) if output.status.success() => {
                info!("Lux installed successfully");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("Lux verification failed: {}", stderr));
            }
            Err(e) => {
                return Err(format!("Failed to run lux: {}", e));
            }
        }

        let _ = window.emit(
            EVENT_LUX_PROGRESS,
            InstallProgress {
                stage: "complete".to_string(),
                progress: 100,
                downloaded: 0,
                total: 0,
                speed: 0.0,
                message: "Lux installed successfully!".to_string(),
            },
        );

        Ok(lux_path.to_string_lossy().to_string())
    }
}

#[cfg(all(not(target_os = "android"), target_os = "windows"))]
async fn extract_zip_lux(archive_path: &Path, bin_dir: &Path) -> Result<(), String> {
    extract_from_zip(
        archive_path,
        bin_dir,
        ZipExtractConfig {
            matcher: is_lux_file,
            dest_name: "lux.exe",
            extract_all: false,
        },
    )
    .await
}

#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
async fn extract_tar_gz_lux(archive_path: &Path, bin_dir: &Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    if !archive_path.exists() {
        error!("Archive file does not exist: {:?}", archive_path);
        return Err(format!("Archive file does not exist: {:?}", archive_path));
    }

    let archive_path = archive_path.to_path_buf();
    let bin_dir = bin_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&archive_path)
            .map_err(|e| format!("Failed to open archive: {}", e))?;

        let tar = GzDecoder::new(file);
        let mut archive = Archive::new(tar);

        for entry in archive
            .entries()
            .map_err(|e| format!("Failed to read tar entries: {}", e))?
        {
            let mut entry = entry.map_err(|e| format!("Failed to read tar entry: {}", e))?;
            let path = entry
                .path()
                .map_err(|e| format!("Failed to get entry path: {}", e))?;

            let name = path.to_string_lossy().to_string();

            if name == "lux" || name.ends_with("/lux") {
                let dest_path = bin_dir.join("lux");
                entry
                    .unpack(&dest_path)
                    .map_err(|e| format!("Failed to extract lux: {}", e))?;
                info!("Extracted lux to {:?}", dest_path);
                break;
            }
        }

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn uninstall_lux(app: AppHandle) -> Result<(), String> {
    let lux_path = get_lux_path(&app)?;

    if lux_path.exists() {
        tokio::fs::remove_file(&lux_path)
            .await
            .map_err(|e| format!("Failed to remove Lux: {}", e))?;
    }

    Ok(())
}

#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: String,
    published_at: String,
}
