use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as AsyncMutex;

async fn read_lines_crlf<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    mut on_line: impl FnMut(String),
) {
    use tokio::io::AsyncReadExt;

    let mut buf = [0u8; 4096];
    let mut acc: Vec<u8> = Vec::new();

    loop {
        let n = match reader.read(&mut buf).await {
            Ok(n) => n,
            Err(_) => break,
        };
        if n == 0 {
            break;
        }

        acc.extend_from_slice(&buf[..n]);

        // Split on '\n' or '\r' to handle single-line progress output via carriage returns.
        while let Some(pos) = acc.iter().position(|&b| b == b'\n' || b == b'\r') {
            let mut line_bytes: Vec<u8> = acc.drain(..pos).collect();

            // Drain delimiter.
            let delim = acc.drain(..1).next().unwrap_or(b'\n');
            if delim == b'\r' {
                // Consume a following '\n' if this was CRLF.
                if acc.first() == Some(&b'\n') {
                    let _ = acc.drain(..1).next();
                }
            }

            // Strip a trailing '\r' that might still be present.
            while matches!(line_bytes.last(), Some(b'\r' | b'\n')) {
                line_bytes.pop();
            }

            let line = String::from_utf8_lossy(&line_bytes).to_string();
            on_line(line);
        }

        // Avoid unbounded growth if a process prints huge chunks without delimiters.
        if acc.len() > 1024 * 1024 {
            let line = String::from_utf8_lossy(&acc).to_string();
            acc.clear();
            on_line(line);
        }
    }

    if !acc.is_empty() {
        let line = String::from_utf8_lossy(&acc).to_string();
        on_line(line);
    }
}

#[derive(Clone, Default)]
pub struct JobRegistry {
    jobs: Arc<AsyncMutex<HashMap<String, u32>>>,
}

impl JobRegistry {
    pub async fn register(&self, job_id: &str, pid: u32) {
        self.jobs.lock().await.insert(job_id.to_string(), pid);
    }

    pub async fn unregister(&self, job_id: &str) {
        self.jobs.lock().await.remove(job_id);
    }

    pub async fn pid(&self, job_id: &str) -> Option<u32> {
        self.jobs.lock().await.get(job_id).copied()
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub fn new_job_id() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn parse_percent_fraction(s: &str) -> Option<f32> {
    let trimmed = s.trim().trim_end_matches('%');
    let p = trimmed.parse::<f32>().ok()?;
    Some((p / 100.0).clamp(0.0, 1.0))
}

fn parse_hms_to_ms(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s == "NA" || s == "--:--" {
        return None;
    }

    // Accept MM:SS or HH:MM:SS
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec) = match parts.len() {
        2 => (
            0u64,
            parts[0].parse::<u64>().ok()?,
            parts[1].parse::<u64>().ok()?,
        ),
        3 => (
            parts[0].parse::<u64>().ok()?,
            parts[1].parse::<u64>().ok()?,
            parts[2].parse::<u64>().ok()?,
        ),
        _ => return None,
    };

    Some(((h * 3600u64) + (m * 60u64) + sec) * 1000u64)
}

fn parse_hms_ms_to_ms(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s == "NA" || s == "--:--" {
        return None;
    }

    // Accept MM:SS[.ms] or HH:MM:SS[.ms]
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec_str) = match parts.len() {
        2 => (0u64, parts[0].parse::<u64>().ok()?, parts[1]),
        3 => (
            parts[0].parse::<u64>().ok()?,
            parts[1].parse::<u64>().ok()?,
            parts[2],
        ),
        _ => return None,
    };

    let (sec_whole, sec_frac) = sec_str.split_once('.').unwrap_or((sec_str, "0"));
    let sec: u64 = sec_whole.parse::<u64>().ok()?;
    let ms: u64 = {
        let frac = sec_frac.trim();
        if frac.is_empty() {
            0
        } else if frac.len() >= 3 {
            frac[..3].parse::<u64>().ok().unwrap_or(0)
        } else {
            // e.g. ".5" -> 500ms, ".12" -> 120ms
            let parsed = frac.parse::<u64>().ok().unwrap_or(0);
            parsed * 10u64.pow((3 - frac.len()) as u32)
        }
    };

    Some((((h * 3600u64) + (m * 60u64) + sec) * 1000u64) + ms)
}

fn extract_ffmpeg_time_ms(line: &str) -> Option<u64> {
    // Typical ffmpeg progress lines:
    // "frame= ... time=00:00:12.34 bitrate=... speed=..."
    let idx = line.find("time=")?;
    let after = &line[idx + 5..];
    let end = after
        .find([' ', '\t', '\r', '\n'])
        .unwrap_or(after.len());
    let time_str = after[..end].trim();
    parse_hms_ms_to_ms(time_str)
}

fn parse_download_section_duration_ms(section: &str) -> Option<u64> {
    // Expect: "*HH:MM:SS.mmm-HH:MM:SS.mmm" (yt-dlp --download-sections)
    let s = section.trim();
    let s = s.strip_prefix('*').unwrap_or(s);
    let (start, end) = s.split_once('-')?;
    let start_ms = parse_hms_ms_to_ms(start)?;
    let end_ms = parse_hms_ms_to_ms(end)?;
    end_ms.checked_sub(start_ms)
}

fn parse_download_sections_from_args(args: &[String]) -> Vec<u64> {
    let mut durations: Vec<u64> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--download-sections" {
            if let Some(section) = args.get(i + 1) {
                if let Some(dur) = parse_download_section_duration_ms(section) {
                    if dur > 0 {
                        durations.push(dur);
                    }
                }
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    durations
}

fn parse_optional_u64_token(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s == "NA" {
        return None;
    }
    s.parse::<u64>().ok()
}

fn parse_size_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s == "NA" || s == "N/A" {
        return None;
    }

    let (num_str, unit_str) = s.split_at(
        s.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(s.len()),
    );
    let value = num_str.parse::<f64>().ok()?;
    let unit = unit_str.trim();

    let mult: f64 = match unit {
        "B" => 1.0,
        "KB" => 1_000.0,
        "MB" => 1_000_000.0,
        "GB" => 1_000_000_000.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };

    Some((value * mult).round() as u64)
}

fn parse_lux_eta_to_ms(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s == "NA" || s == "N/A" {
        return None;
    }

    // Common Lux format: "3m2s" or "12s"
    let mut seconds: u64 = 0;

    if let Some(m_pos) = s.find('m') {
        let minutes: u64 = s[..m_pos].parse().ok()?;
        let rest = &s[m_pos + 1..];
        if let Some(s_pos) = rest.find('s') {
            seconds = rest[..s_pos].parse().ok().unwrap_or(0);
        }
        return Some(((minutes * 60) + seconds) * 1000);
    }

    if let Some(s_pos) = s.find('s') {
        seconds = s[..s_pos].parse().ok()?;
        return Some(seconds * 1000);
    }

    None
}

fn parse_lux_progress_line(
    line: &str,
) -> Option<(Option<f32>, Option<u64>, Option<u64>, Option<u64>, Option<u64>)> {
    // Example-ish: "12.3MB / 45.6MB [=====>] 27% 1.2MB/s 3m2s"
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 4 {
        return None;
    }

    // Find percent token (e.g. "27%")
    let mut percent_idx: Option<usize> = None;
    for (i, t) in tokens.iter().enumerate() {
        if t.ends_with('%') {
            percent_idx = Some(i);
            break;
        }
    }
    let percent_idx = percent_idx?;
    let fraction = parse_percent_fraction(tokens[percent_idx]);

    // Speed token: often "1.2MB/s" or "1.2" "MB/s"
    let speed_bps = {
        let t = tokens.get(percent_idx + 1).copied();
        let next = tokens.get(percent_idx + 2).copied();
        match (t, next) {
            (Some(a), Some(b)) if !a.contains("/s") && b.contains("/s") => {
                parse_speed_bps(&format!("{}{}", a, b))
            }
            (Some(a), _) => parse_speed_bps(&a.replace(' ', "")),
            _ => None,
        }
    };

    // ETA token: often after speed, like "3m2s"
    let eta_ms = tokens
        .iter()
        .skip(percent_idx + 1)
        .find_map(|t| parse_lux_eta_to_ms(t));

    // Try to parse "<downloaded> / <total>" at the beginning.
    let downloaded_bytes = tokens.get(0).and_then(|s| parse_size_bytes(s));
    let total_bytes = tokens
        .get(2)
        .and_then(|s| parse_size_bytes(s.trim_start_matches('/')));

    Some((fraction, eta_ms, speed_bps, downloaded_bytes, total_bytes))
}

fn extract_user_status(line: &str) -> Option<(String, String, String)> {
    let s = line.trim();
    if s.is_empty() {
        return None;
    }

    
    if s.contains("Merging formats") || s.starts_with("[Merger]") {
        return Some((
            "processing".to_string(),
            "downloads.status.merging".to_string(),
            "Merging…".to_string(),
        ));
    }
    if s.starts_with("[ExtractAudio]") {
        return Some((
            "processing".to_string(),
            "downloads.status.extractingAudio".to_string(),
            "Extracting audio…".to_string(),
        ));
    }
    if s.starts_with("[EmbedThumbnail]") {
        return Some((
            "processing".to_string(),
            "downloads.status.embeddingThumbnail".to_string(),
            "Embedding thumbnail…".to_string(),
        ));
    }
    if s.starts_with("[Metadata]") {
        return Some((
            "processing".to_string(),
            "downloads.status.writingMetadata".to_string(),
            "Writing metadata…".to_string(),
        ));
    }
    if s.starts_with("[SponsorBlock]") {
        return Some((
            "processing".to_string(),
            "downloads.status.processing".to_string(),
            "Processing…".to_string(),
        ));
    }
    if s.contains("Deleting original file") || s.starts_with("[Delete]") {
        return Some((
            "processing".to_string(),
            "downloads.status.processing".to_string(),
            "Processing…".to_string(),
        ));
    }

    if s.starts_with("[info]") || s.starts_with("[Info]") {
        return Some((
            "fetching-info".to_string(),
            "downloads.status.fetchingInfo".to_string(),
            "Fetching info…".to_string(),
        ));
    }

    // yt-dlp sometimes prints a destination line before progress starts.
    if s.starts_with("[download] Destination:") || s.starts_with("[download] Resuming download") {
        return Some((
            "download".to_string(),
            "downloads.status.downloading".to_string(),
            "Downloading…".to_string(),
        ));
    }

    if s.starts_with("ERROR:") {
        return Some(("error".to_string(), "".to_string(), "Error".to_string()));
    }

    None
}

fn parse_speed_bps(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s == "NA" || s == "N/A" || s == "~" || s.eq_ignore_ascii_case("unknown") {
        return None;
    }

    // Expect e.g. "1.23MiB/s", "850KiB/s", "123B/s", "3.2MB/s"
    let s = s.trim_end_matches("/s");
    let (num_str, unit_str) = s.split_at(
        s.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(s.len()),
    );
    let value = num_str.parse::<f64>().ok()?;
    let unit = unit_str.trim();

    let mult: f64 = match unit {
        "B" => 1.0,
        "KB" => 1_000.0,
        "MB" => 1_000_000.0,
        "GB" => 1_000_000_000.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };

    Some((value * mult).round() as u64)
}

fn parse_aria2_progress_line(
    line: &str,
) -> Option<(Option<f32>, Option<u64>, Option<u64>, Option<u64>, Option<u64>)> {
    // Common aria2c progress format contains a single updating line, e.g.:
    // "[#1 12MiB/45MiB(26%) CN:8 DL:1.2MiB ETA:1m2s]"
    // Note: this often uses carriage returns instead of newlines.
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // Percent in parentheses, e.g. "(26%)".
    let fraction = line
        .find('(')
        .and_then(|pct_start| line[pct_start + 1..].find('%').map(|e| (pct_start, e)))
        .and_then(|(pct_start, pct_end_rel)| {
            let pct_end = pct_start + 1 + pct_end_rel;
            parse_percent_fraction(&line[pct_start + 1..pct_end])
        });

    // Downloaded/total, e.g. "12MiB/45MiB".
    let (downloaded_bytes, total_bytes) = {
        let mut downloaded: Option<u64> = None;
        let mut total: Option<u64> = None;

        // Prefer iB units (KiB/MiB/GiB) but also allow plain B.
        let slash_idx = line.find("iB/").or_else(|| line.find("B/"));
        if let Some(slash_idx) = slash_idx {
            // Include the unit suffix for the downloaded part (.."iB" or .."B").
            let unit_len = if line[slash_idx..].starts_with("iB/") { 2 } else { 1 };
            let before_slash = &line[..slash_idx + unit_len];

            let size_start = before_slash
                .rfind([' ', '[', '#'])
                .map(|i| i + 1)
                .unwrap_or(0);

            let dl_part = before_slash[size_start..].trim();
            downloaded = parse_size_bytes(dl_part);

            let after_slash = &line[slash_idx + unit_len + 1..];
            if let Some(end) = after_slash.find(['(', ' ', ']']) {
                let total_part = after_slash[..end].trim();
                total = parse_size_bytes(total_part);
            }
        }

        (downloaded, total)
    };

    // Speed, e.g. "DL:1.2MiB" or "DL:1.2MiB/s".
    let speed_bps = line.find("DL:").and_then(|dl_idx| {
        let speed_part = &line[dl_idx + 3..];
        let end = speed_part.find([' ', ']', '[']).unwrap_or(speed_part.len());
        let speed_str = speed_part[..end].trim();
        parse_speed_bps(speed_str)
    });

    // ETA, e.g. "ETA:1m2s" or "ETA:00:12".
    let eta_ms = line.find("ETA:").and_then(|eta_idx| {
        let eta_part = &line[eta_idx + 4..];
        let end = eta_part.find([' ', ']', '[']).unwrap_or(eta_part.len());
        let eta_str = eta_part[..end].trim();
        parse_hms_to_ms(eta_str).or_else(|| parse_lux_eta_to_ms(eta_str))
    });

    if fraction.is_some() || downloaded_bytes.is_some() || total_bytes.is_some() || speed_bps.is_some() {
        Some((fraction, eta_ms, speed_bps, downloaded_bytes, total_bytes))
    } else {
        None
    }
}

pub fn parse_comine_progress_line(line: &str) -> Option<(Option<f32>, Option<u64>, Option<u64>, Option<u64>, Option<u64>)> {
    // Format: "__COMINE_PROGRESS__ <percent> <speed> <eta> <downloaded_bytes> <total_bytes>"
    let rest = line.trim().strip_prefix("__COMINE_PROGRESS__")?.trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }

    let fraction = parse_percent_fraction(parts[0]);
    let speed_bps = parse_speed_bps(parts[1]);

    // yt-dlp sometimes prefixes ETA with "ETA" depending on template choice.
    let eta_token = parts[2].trim_start_matches("ETA").trim();
    let eta_ms = parse_hms_to_ms(eta_token);

    let downloaded_bytes = parts.get(3).and_then(|s| parse_optional_u64_token(s));
    let total_bytes = parts.get(4).and_then(|s| parse_optional_u64_token(s));

    Some((fraction, eta_ms, speed_bps, downloaded_bytes, total_bytes))
}

pub async fn spawn_process_job(
    window: tauri::Window,
    registry: JobRegistry,
    title: String,
    command: String,
    args: Vec<String>,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    cleanup_files: Vec<String>,
    artifact_scan_dir: Option<String>,
) -> Result<String, String> {


    let job_id = new_job_id();
    let step_id = "run".to_string();

    #[derive(Clone, Default)]
    struct ClipSectionState {
        durations_ms: Vec<u64>,
        completed: usize,
        last_fraction: Option<f32>,
    }

    let clip_section_durations = parse_download_sections_from_args(&args);
    let clip_state: Arc<std::sync::Mutex<ClipSectionState>> = Arc::new(std::sync::Mutex::new(
        ClipSectionState {
            durations_ms: clip_section_durations,
            completed: 0,
            last_fraction: None,
        },
    ));

    #[derive(Clone, Default)]
    struct StatusState {
        last_at: u128,
        last_message: Option<String>,
    }

    let status_state: Arc<std::sync::Mutex<StatusState>> = Arc::new(std::sync::Mutex::new(
        StatusState {
            last_at: 0,
            last_message: None,
        },
    ));

    window
        .emit(
            "job-event",
            JobEvent::Started {
                job_id: job_id.clone(),
                step_id: step_id.clone(),
                at_ms: now_ms(),
                title: title.clone(),
                command: command.clone(),
                args: args.clone(),
            },
        )
        .map_err(|e| e.to_string())?;

    let mut cmd = tokio::process::Command::new(&command);
    cmd.args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    if let Some(cwd) = &cwd {
        cmd.current_dir(cwd);
    }
    if let Some(env) = &env {
        cmd.envs(env);
    }

    #[cfg(target_os = "windows")]
    {
        use crate::utils::CommandHideConsole;
        cmd.hide_console();
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", command, e))?;

    if let Some(pid) = child.id() {
        registry.register(&job_id, pid).await;
    }

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    let window_task = window.clone();
    let registry_task = registry.clone();
    let job_id_task = job_id.clone();
    let step_id_task = step_id.clone();
    let clip_state_task = clip_state.clone();
    let status_state_task = status_state.clone();

    tokio::spawn(async move {
        let stderr_task = {
            let window_err = window_task.clone();
            let job_id_err = job_id_task.clone();
            let step_id_err = step_id_task.clone();
            let clip_state_err = clip_state_task.clone();
            let status_state_err = status_state_task.clone();
            tokio::spawn(async move {
                read_lines_crlf(stderr, |line| {
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        return;
                    }

                    if let Some((phase, key, message)) = extract_user_status(&line) {
                        if let Ok(mut s) = status_state_err.lock() {
                            let now = now_ms();
                            let should_emit = s
                                .last_message
                                .as_ref()
                                .map(|m| m != &message)
                                .unwrap_or(true)
                                && (now.saturating_sub(s.last_at) >= 400);

                            if should_emit {
                                s.last_at = now;
                                s.last_message = Some(message.clone());
                                let _ = window_err.emit(
                                    "job-event",
                                    JobEvent::Status {
                                        job_id: job_id_err.clone(),
                                        step_id: step_id_err.clone(),
                                        at_ms: now,
                                        phase,
                                        key: if key.is_empty() { None } else { Some(key) },
                                        message,
                                    },
                                );
                            }
                        }
                    }

                    // If yt-dlp is doing sectioned downloads via ffmpeg, progress fields can be missing.
                    // Use ffmpeg's "time=" as a proxy for section completion.
                    if let Some(time_ms) = extract_ffmpeg_time_ms(&line) {
                        if let Ok(mut s) = clip_state_err.lock() {
                            let total = s.durations_ms.len();
                            if total > 0 && s.completed < total {
                                let dur_ms = s.durations_ms[s.completed];
                                if dur_ms > 0 {
                                    let section_frac = (time_ms as f32 / dur_ms as f32).clamp(0.0, 1.0);
                                    let overall = ((s.completed as f32) + section_frac) / (total as f32);
                                    s.last_fraction = Some(overall);

                                    let _ = window_err.emit(
                                        "job-event",
                                        JobEvent::Progress {
                                            job_id: job_id_err.clone(),
                                            step_id: step_id_err.clone(),
                                            at_ms: now_ms(),
                                            phase: "download".to_string(),
                                            fraction: Some(overall),
                                            eta_ms: None,
                                            speed_bps: None,
                                            downloaded_bytes: None,
                                            total_bytes: None,
                                        },
                                    );
                                    return;
                                }
                            }
                        }
                    }

                    // yt-dlp progress is often written to stderr. Parse progress from stderr too.
                    if let Some((fraction, eta_ms, speed_bps, downloaded_bytes, total_bytes)) =
                        parse_comine_progress_line(&line)
                    {
                        let fraction = if fraction.is_some() {
                            fraction
                        } else if let (Some(d), Some(t)) = (downloaded_bytes, total_bytes) {
                            if t > 0 {
                                Some(((d as f32) / (t as f32)).clamp(0.0, 1.0))
                            } else {
                                None
                            }
                        } else {
                            clip_state_err
                                .lock()
                                .ok()
                                .and_then(|s| s.last_fraction)
                        };
                        let _ = window_err.emit(
                            "job-event",
                            JobEvent::Progress {
                                job_id: job_id_err.clone(),
                                step_id: step_id_err.clone(),
                                at_ms: now_ms(),
                                phase: "download".to_string(),
                                fraction,
                                eta_ms,
                                speed_bps,
                                downloaded_bytes,
                                total_bytes,
                            },
                        );
                        return;
                    }

                    if let Some((fraction, eta_ms, speed_bps, downloaded_bytes, total_bytes)) =
                        parse_lux_progress_line(&line)
                    {
                        let _ = window_err.emit(
                            "job-event",
                            JobEvent::Progress {
                                job_id: job_id_err.clone(),
                                step_id: step_id_err.clone(),
                                at_ms: now_ms(),
                                phase: "download".to_string(),
                                fraction,
                                eta_ms,
                                speed_bps,
                                downloaded_bytes,
                                total_bytes,
                            },
                        );
                        return;
                    }

                    if let Some((fraction, eta_ms, speed_bps, downloaded_bytes, total_bytes)) =
                        parse_aria2_progress_line(&line)
                    {
                        let _ = window_err.emit(
                            "job-event",
                            JobEvent::Progress {
                                job_id: job_id_err.clone(),
                                step_id: step_id_err.clone(),
                                at_ms: now_ms(),
                                phase: "download".to_string(),
                                fraction,
                                eta_ms,
                                speed_bps,
                                downloaded_bytes,
                                total_bytes,
                            },
                        );
                        return;
                    }

                    // Convention: tools can emit JSON lines for progress.
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                        if v.get("type")
                            == Some(&serde_json::Value::String("progress".to_string()))
                        {
                            let _ = window_err.emit(
                                "job-event",
                                JobEvent::Progress {
                                    job_id: job_id_err.clone(),
                                    step_id: step_id_err.clone(),
                                    at_ms: now_ms(),
                                    phase: v
                                        .get("phase")
                                        .and_then(|x| x.as_str())
                                        .unwrap_or("running")
                                        .to_string(),
                                    fraction: v
                                        .get("fraction")
                                        .and_then(|x| x.as_f64())
                                        .map(|x| x as f32),
                                    eta_ms: v.get("eta_ms").and_then(|x| x.as_u64()),
                                    speed_bps: v.get("speed_bps").and_then(|x| x.as_u64()),
                                    downloaded_bytes: v
                                        .get("downloaded_bytes")
                                        .and_then(|x| x.as_u64()),
                                    total_bytes: v
                                        .get("total_bytes")
                                        .and_then(|x| x.as_u64()),
                                },
                            );
                            return;
                        }
                    }

                    let _ = window_err.emit(
                        "job-event",
                        JobEvent::Log {
                            job_id: job_id_err.clone(),
                            step_id: step_id_err.clone(),
                            at_ms: now_ms(),
                            level: "debug".to_string(),
                            message: line,
                        },
                    );
                }).await;
            })
        };

        let mut saw_artifact = false;
        read_lines_crlf(stdout, |line| {
            let line = line.trim().to_string();
            if line.is_empty() {
                return;
            }

            if let Some((phase, key, message)) = extract_user_status(&line) {
                if let Ok(mut s) = status_state_task.lock() {
                    let now = now_ms();
                    let should_emit = s
                        .last_message
                        .as_ref()
                        .map(|m| m != &message)
                        .unwrap_or(true)
                        && (now.saturating_sub(s.last_at) >= 400);
                    if should_emit {
                        s.last_at = now;
                        s.last_message = Some(message.clone());
                        let _ = window_task.emit(
                            "job-event",
                            JobEvent::Status {
                                job_id: job_id_task.clone(),
                                step_id: step_id_task.clone(),
                                at_ms: now,
                                phase,
                                key: if key.is_empty() { None } else { Some(key) },
                                message,
                            },
                        );
                    }
                }
            }

            if line.starts_with(">>>FILEPATH:") {
                let path = line.trim_start_matches(">>>FILEPATH:").trim().to_string();
                let _ = window_task.emit(
                    "job-event",
                    JobEvent::Artifact {
                        job_id: job_id_task.clone(),
                        step_id: step_id_task.clone(),
                        at_ms: now_ms(),
                        kind: "output".to_string(),
                        path,
                    },
                );
                saw_artifact = true;

                if let Ok(mut s) = clip_state_task.lock() {
                    let total = s.durations_ms.len();
                    if total > 0 {
                        s.completed = (s.completed + 1).min(total);
                        let overall = (s.completed as f32 / total as f32).clamp(0.0, 1.0);
                        s.last_fraction = Some(overall);
                        let _ = window_task.emit(
                            "job-event",
                            JobEvent::Progress {
                                job_id: job_id_task.clone(),
                                step_id: step_id_task.clone(),
                                at_ms: now_ms(),
                                phase: "download".to_string(),
                                fraction: Some(overall),
                                eta_ms: None,
                                speed_bps: None,
                                downloaded_bytes: None,
                                total_bytes: None,
                            },
                        );
                    }
                }
                return;
            }

            // Lux output conventions
            if line.contains("File saved:") || line.contains("Saved to:") {
                if let Some(path_part) = line.split(':').nth(1) {
                    let path = path_part.trim().to_string();
                    if !path.is_empty() {
                        let _ = window_task.emit(
                            "job-event",
                            JobEvent::Artifact {
                                job_id: job_id_task.clone(),
                                step_id: step_id_task.clone(),
                                at_ms: now_ms(),
                                kind: "output".to_string(),
                                path,
                            },
                        );
                        saw_artifact = true;
                        return;
                    }
                }
            }

            if line.contains(": file already exists") {
                if let Some(path_part) = line.split(": file already exists").next() {
                    let path = path_part.trim().to_string();
                    if !path.is_empty() {
                        let _ = window_task.emit(
                            "job-event",
                            JobEvent::Artifact {
                                job_id: job_id_task.clone(),
                                step_id: step_id_task.clone(),
                                at_ms: now_ms(),
                                kind: "output".to_string(),
                                path,
                            },
                        );
                        saw_artifact = true;
                        return;
                    }
                }
            }

            if let Some((fraction, eta_ms, speed_bps, downloaded_bytes, total_bytes)) =
                parse_comine_progress_line(&line)
            {
                let _ = window_task.emit(
                    "job-event",
                    JobEvent::Progress {
                        job_id: job_id_task.clone(),
                        step_id: step_id_task.clone(),
                        at_ms: now_ms(),
                        phase: "download".to_string(),
                        fraction,
                        eta_ms,
                        speed_bps,
                        downloaded_bytes,
                        total_bytes,
                    },
                );
                return;
            }

            if let Some((fraction, eta_ms, speed_bps, downloaded_bytes, total_bytes)) =
                parse_lux_progress_line(&line)
            {
                let _ = window_task.emit(
                    "job-event",
                    JobEvent::Progress {
                        job_id: job_id_task.clone(),
                        step_id: step_id_task.clone(),
                        at_ms: now_ms(),
                        phase: "download".to_string(),
                        fraction,
                        eta_ms,
                        speed_bps,
                        downloaded_bytes,
                        total_bytes,
                    },
                );
                return;
            }

            if let Some((fraction, eta_ms, speed_bps, downloaded_bytes, total_bytes)) =
                parse_aria2_progress_line(&line)
            {
                let _ = window_task.emit(
                    "job-event",
                    JobEvent::Progress {
                        job_id: job_id_task.clone(),
                        step_id: step_id_task.clone(),
                        at_ms: now_ms(),
                        phase: "download".to_string(),
                        fraction,
                        eta_ms,
                        speed_bps,
                        downloaded_bytes,
                        total_bytes,
                    },
                );
                return;
            }

            // Convention: tools can emit JSON lines for progress.
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                if v.get("type") == Some(&serde_json::Value::String("progress".to_string())) {
                    let _ = window_task.emit(
                        "job-event",
                        JobEvent::Progress {
                            job_id: job_id_task.clone(),
                            step_id: step_id_task.clone(),
                            at_ms: now_ms(),
                            phase: v
                                .get("phase")
                                .and_then(|x| x.as_str())
                                .unwrap_or("running")
                                .to_string(),
                            fraction: v.get("fraction").and_then(|x| x.as_f64()).map(|x| x as f32),
                            eta_ms: v.get("eta_ms").and_then(|x| x.as_u64()),
                            speed_bps: v.get("speed_bps").and_then(|x| x.as_u64()),
                            downloaded_bytes: v.get("downloaded_bytes").and_then(|x| x.as_u64()),
                            total_bytes: v.get("total_bytes").and_then(|x| x.as_u64()),
                        },
                    );
                    return;
                }
            }

            let _ = window_task.emit(
                "job-event",
                JobEvent::Log {
                    job_id: job_id_task.clone(),
                    step_id: step_id_task.clone(),
                    at_ms: now_ms(),
                    level: "info".to_string(),
                    message: line,
                },
            );
        }).await;

        let status = child.wait().await;
        let _ = stderr_task.await;
        registry_task.unregister(&job_id_task).await;

        for p in cleanup_files {
            let _ = tokio::fs::remove_file(p).await;
        }

        match status {
            Ok(status) => {
                let exit_code = status.code().unwrap_or(-1);

                if exit_code == 0 {
                    if let Some(scan_dir) = &artifact_scan_dir {
                        if !saw_artifact {
                            let scan_dir_path = std::path::PathBuf::from(scan_dir);
                            let mut latest: Option<(std::path::PathBuf, std::time::SystemTime)> =
                                None;
                            let scan_start = std::time::SystemTime::now();
                            if let Ok(entries) = std::fs::read_dir(&scan_dir_path) {
                                for entry in entries.flatten() {
                                    let path = entry.path();
                                    if !path.is_file() {
                                        continue;
                                    }

                                    let ext = path
                                        .extension()
                                        .and_then(|e| e.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    if ext == "part"
                                        || ext == "tmp"
                                        || ext == "webp"
                                        || ext == "jpg"
                                        || ext == "jpeg"
                                        || ext == "png"
                                    {
                                        continue;
                                    }

                                    let Ok(metadata) = entry.metadata() else {
                                        continue;
                                    };
                                    let Ok(modified) = metadata.modified() else {
                                        continue;
                                    };

                                    // Prefer files written "recently".
                                    let is_recent = scan_start
                                        .duration_since(modified)
                                        .map(|d| d.as_secs() <= 120)
                                        .unwrap_or(false);
                                    if !is_recent {
                                        continue;
                                    }

                                    if latest.as_ref().map_or(true, |(_, t)| modified > *t) {
                                        latest = Some((path, modified));
                                    }
                                }
                            }

                            if let Some((path, _)) = latest {
                                let _ = window_task.emit(
                                    "job-event",
                                    JobEvent::Artifact {
                                        job_id: job_id_task.clone(),
                                        step_id: step_id_task.clone(),
                                        at_ms: now_ms(),
                                        kind: "output".to_string(),
                                        path: path.to_string_lossy().to_string(),
                                    },
                                );
                            }
                        }
                    }
                }

                let _ = window_task.emit(
                    "job-event",
                    JobEvent::Finished {
                        job_id: job_id_task.clone(),
                        step_id: step_id_task.clone(),
                        at_ms: now_ms(),
                        exit_code,
                    },
                );
            }
            Err(e) => {
                let _ = window_task.emit(
                    "job-event",
                    JobEvent::Failed {
                        job_id: job_id_task.clone(),
                        step_id: step_id_task.clone(),
                        at_ms: now_ms(),
                        error: e.to_string(),
                    },
                );
            }
        }
    });

    Ok(job_id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum JobEvent {
    Started {
        job_id: String,
        step_id: String,
        at_ms: u128,
        title: String,
        command: String,
        args: Vec<String>,
    },
    Log {
        job_id: String,
        step_id: String,
        at_ms: u128,
        level: String,
        message: String,
    },
    Status {
        job_id: String,
        step_id: String,
        at_ms: u128,
        phase: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        message: String,
    },
    Progress {
        job_id: String,
        step_id: String,
        at_ms: u128,
        phase: String,
        fraction: Option<f32>,
        eta_ms: Option<u64>,
        speed_bps: Option<u64>,
        downloaded_bytes: Option<u64>,
        total_bytes: Option<u64>,
    },
    Artifact {
        job_id: String,
        step_id: String,
        at_ms: u128,
        kind: String,
        path: String,
    },
    Finished {
        job_id: String,
        step_id: String,
        at_ms: u128,
        exit_code: i32,
    },
    Failed {
        job_id: String,
        step_id: String,
        at_ms: u128,
        error: String,
    },
    Cancelled {
        job_id: String,
        at_ms: u128,
        reason: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct JobStartRequest {
    pub title: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
}

#[tauri::command]
pub async fn jobs_start(
    app: AppHandle,
    window: tauri::Window,
    registry: tauri::State<'_, JobRegistry>,
    req: JobStartRequest,
) -> Result<String, String> {
    // Keep app used to avoid warnings in some cfg combos.
    let _ = app;
    spawn_process_job(
        window,
        registry.inner().clone(),
        req.title,
        req.command,
        req.args,
        req.cwd,
        req.env,
        vec![],
        None,
    )
    .await
}

#[tauri::command]
pub async fn jobs_cancel(
    registry: tauri::State<'_, JobRegistry>,
    window: tauri::Window,
    job_id: String,
) -> Result<(), String> {
    let pid = registry
        .pid(&job_id)
        .await
        .ok_or_else(|| "Job not found or already completed".to_string())?;

    #[cfg(target_os = "windows")]
    {
        use crate::utils::StdCommandHideConsole;
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .hide_console()
            .output();
    }

    #[cfg(all(unix, not(target_os = "windows")))]
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }

    registry.unregister(&job_id).await;

    window
        .emit(
            "job-event",
            JobEvent::Cancelled {
                job_id,
                at_ms: now_ms(),
                reason: "user_cancel".to_string(),
            },
        )
        .map_err(|e| e.to_string())?;

    Ok(())
}
