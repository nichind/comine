use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::{AppHandle, Listener, Manager};
use tracing::{debug, error, info};

use crate::orchestrator::manager::JobManager;
use crate::orchestrator::types::JobStatus;

const APP_ID: &str = "1472219940370256035";
const THROTTLE: Duration = Duration::from_secs(5);
const IDLE_INTERVAL: Duration = Duration::from_secs(60);
const IPC_TIMEOUT: Duration = Duration::from_secs(10);

pub struct DiscordRpcState {
    enabled: AtomicBool,
    notify: tokio::sync::Notify,
}

impl DiscordRpcState {
    pub fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            notify: tokio::sync::Notify::new(),
        }
    }

    pub fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
        self.notify.notify_one();
        info!("Discord RPC enabled");
    }

    pub fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        self.notify.notify_one();
        info!("Discord RPC disabled");
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn trigger(&self) {
        self.notify.notify_one();
    }
}

struct DiscordIpc {
    #[cfg(windows)]
    pipe: std::fs::File,
    #[cfg(not(windows))]
    pipe: std::os::unix::net::UnixStream,
}

impl DiscordIpc {
    fn connect() -> Option<Self> {
        (0..10).find_map(Self::try_connect)
    }

    #[cfg(windows)]
    fn try_connect(i: u32) -> Option<Self> {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!(r"\\.\pipe\discord-ipc-{i}"))
            .ok()
            .map(|pipe| Self { pipe })
    }

    #[cfg(not(windows))]
    fn try_connect(i: u32) -> Option<Self> {
        [
            std::env::var("XDG_RUNTIME_DIR").ok(),
            std::env::var("TMPDIR").ok(),
            Some("/tmp".into()),
        ]
        .into_iter()
        .flatten()
        .find_map(|dir| {
            std::os::unix::net::UnixStream::connect(format!("{dir}/discord-ipc-{i}"))
                .ok()
                .map(|pipe| Self { pipe })
        })
    }

    fn send(&mut self, opcode: u32, payload: &Value) -> std::io::Result<()> {
        let data = serde_json::to_vec(payload)?;
        let header = [opcode.to_le_bytes(), (data.len() as u32).to_le_bytes()].concat();
        self.pipe.write_all(&header)?;
        self.pipe.write_all(&data)?;
        self.pipe.flush()
    }

    fn recv(&mut self) -> std::io::Result<(u32, Value)> {
        let mut header = [0u8; 8];
        self.pipe.read_exact(&mut header)?;
        let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
        let len = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
        if len > 1_048_576 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "payload too large",
            ));
        }
        let mut buf = vec![0u8; len];
        self.pipe.read_exact(&mut buf)?;
        serde_json::from_slice(&buf)
            .map(|v| (opcode, v))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    fn handshake(&mut self) -> std::io::Result<Value> {
        self.send(0, &json!({ "v": 1, "client_id": APP_ID }))?;
        let (op, payload) = self.recv()?;
        match op {
            1 => Ok(payload),
            2 => Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                format!(
                    "handshake rejected: {}",
                    payload
                        .pointer("/data/message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown")
                ),
            )),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("unexpected opcode {op}"),
            )),
        }
    }

    fn set_activity(&mut self, activity: Value, nonce: &str) -> std::io::Result<Value> {
        self.send(
            1,
            &json!({
                "cmd": "SET_ACTIVITY",
                "args": { "pid": std::process::id(), "activity": activity },
                "nonce": nonce
            }),
        )?;
        self.recv_response()
    }

    fn clear_activity(&mut self, nonce: &str) -> std::io::Result<Value> {
        self.send(
            1,
            &json!({
                "cmd": "SET_ACTIVITY",
                "args": { "pid": std::process::id() },
                "nonce": nonce
            }),
        )?;
        self.recv_response()
    }

    fn recv_response(&mut self) -> std::io::Result<Value> {
        let (op, response) = self.recv()?;
        if op == 2 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "connection closed",
            ));
        }
        Ok(response)
    }
}

pub fn start_polling(app: &AppHandle) {
    let state = Arc::new(DiscordRpcState::new());
    app.manage(state);

    if crate::store_utils::get_bool(app, "discordRpc", false) {
        if let Some(s) = app.try_state::<Arc<DiscordRpcState>>() {
            s.enable();
        }
    }

    let app_for_events = app.clone();
    app.listen("job-event", move |_| {
        if let Some(s) = app_for_events.try_state::<Arc<DiscordRpcState>>() {
            if s.is_enabled() {
                s.trigger();
            }
        }
    });

    let version = app
        .config()
        .version
        .clone()
        .unwrap_or_else(|| "0.0.0".into());
    let handle = app.clone();
    tauri::async_runtime::spawn(event_loop(handle, version));
}

async fn event_loop(app: AppHandle, version: String) {
    let ipc: Arc<Mutex<Option<DiscordIpc>>> = Arc::new(Mutex::new(None));
    let mut nonce: u64 = 0;
    let session_start = epoch_secs();
    let mut last_update = Instant::now();
    let mut was_enabled = false;

    loop {
        let discord = match app.try_state::<Arc<DiscordRpcState>>() {
            Some(s) => s.inner().clone(),
            None => {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let interval = if has_active_jobs(&app) {
            THROTTLE
        } else {
            IDLE_INTERVAL
        };
        tokio::select! {
            _ = discord.notify.notified() => {},
            _ = tokio::time::sleep(interval) => {},
        }

        let enabled = discord.is_enabled();
        let toggled = enabled != was_enabled;
        was_enabled = enabled;

        if !toggled && last_update.elapsed() < THROTTLE {
            continue;
        }

        if !enabled {
            disconnect(&ipc, &mut nonce).await;
            last_update = Instant::now();
            continue;
        }

        let activity = match app.try_state::<Arc<JobManager>>() {
            Some(m) => build_activity(&m, session_start, &version).await,
            None => continue,
        };

        send_activity(&ipc, &mut nonce, activity).await;
        last_update = Instant::now();
    }
}

async fn disconnect(ipc: &Arc<Mutex<Option<DiscordIpc>>>, nonce: &mut u64) {
    let ipc = ipc.clone();
    *nonce += 1;
    let n = nonce.to_string();
    tokio::task::spawn_blocking(move || {
        if let Ok(mut guard) = ipc.lock() {
            if let Some(ref mut pipe) = *guard {
                let _ = pipe.clear_activity(&n);
            }
            *guard = None;
        }
    })
    .await
    .ok();
}

async fn send_activity(ipc: &Arc<Mutex<Option<DiscordIpc>>>, nonce: &mut u64, activity: Value) {
    let ipc_ref = ipc.clone();
    *nonce += 1;
    let n = nonce.to_string();

    let result = tokio::time::timeout(
        IPC_TIMEOUT,
        tokio::task::spawn_blocking(move || -> Option<Value> {
            let mut guard = ipc_ref.lock().ok()?;

            if guard.is_none() {
                let mut pipe = DiscordIpc::connect()?;
                match pipe.handshake() {
                    Ok(resp) => {
                        info!("Discord RPC connected: {resp}");
                        *guard = Some(pipe);
                    }
                    Err(e) => {
                        error!("Discord RPC handshake failed: {e}");
                        return None;
                    }
                }
            }

            let pipe = guard.as_mut().unwrap();
            match pipe.set_activity(activity, &n) {
                Ok(resp) => Some(resp),
                Err(e) => {
                    error!("Discord RPC IPC error: {e}");
                    *guard = None;
                    None
                }
            }
        }),
    )
    .await;

    match result {
        Ok(Ok(Some(resp))) => {
            if resp.get("evt").and_then(|v| v.as_str()) == Some("ERROR") {
                error!(
                    "Discord RPC error ({}): {}",
                    resp.pointer("/data/code")
                        .and_then(|c| c.as_i64())
                        .unwrap_or(-1),
                    resp.pointer("/data/message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown"),
                );
            } else {
                debug!("Discord RPC activity updated");
            }
        }
        Err(_) => {
            error!("Discord RPC IPC timed out");
            if let Ok(mut guard) = ipc.lock() {
                *guard = None;
            }
        }
        _ => {}
    }
}

fn has_active_jobs(app: &AppHandle) -> bool {
    app.try_state::<Arc<JobManager>>()
        .map(|m| m.get_all_jobs().iter().any(|j| j.status.is_active()))
        .unwrap_or(false)
}

async fn build_activity(manager: &Arc<JobManager>, session_start: u64, version: &str) -> Value {
    let jobs = manager.get_all_jobs();
    let active: Vec<_> = jobs.iter().filter(|j| j.status.is_active()).collect();

    let by_status = |s: JobStatus| -> Vec<_> {
        active
            .iter()
            .filter(|j| std::mem::discriminant(&j.status) == std::mem::discriminant(&s))
            .copied()
            .collect()
    };

    let downloading = by_status(JobStatus::Downloading);
    let processing = by_status(JobStatus::PostProcessing);
    let resolving = by_status(JobStatus::Resolving);
    let now = epoch_secs();

    let assets = json!({
        "large_image": "comine",
        "large_text": format!("Comine v{version}")
    });

    if !downloading.is_empty() {
        build_downloading(&downloading, now, &assets)
    } else if !processing.is_empty() {
        build_processing(&processing, now, &assets)
    } else if !resolving.is_empty() {
        build_resolving(&resolving, now, &assets)
    } else {
        build_idle(manager, session_start, &assets).await
    }
}

fn build_downloading(jobs: &[&crate::orchestrator::types::Job], now: u64, assets: &Value) -> Value {
    let count = jobs.len();
    let total_speed: u64 = jobs.iter().filter_map(|j| j.speed).sum();
    let (downloaded, total) = jobs.iter().fold((0u64, 0u64), |(d, t), j| {
        (d + j.downloaded_bytes, t + j.total_bytes.unwrap_or(0))
    });

    let details = if count == 1 {
        "Downloading 1 file".into()
    } else {
        format!("Downloading {count} files")
    };

    let state = {
        let speed = if total_speed > 0 {
            format!("↓ {}", fmt_speed(total_speed as f64))
        } else {
            "Downloading…".into()
        };
        if total > 0 {
            format!("{speed} · {}/{}", fmt_bytes(downloaded), fmt_bytes(total))
        } else {
            speed
        }
    };

    let earliest = jobs
        .iter()
        .filter_map(|j| j.started_at)
        .min()
        .map(|ms| ms / 1000)
        .unwrap_or(now);
    let timestamps = if total > 0 && downloaded > 0 {
        let progress = (downloaded as f64 / total as f64).clamp(0.0, 1.0);
        let elapsed = now.saturating_sub(earliest).max(1);
        let est_total = (elapsed as f64 / progress) as u64;
        json!({ "start": now - (progress * est_total as f64) as u64, "end": now - (progress * est_total as f64) as u64 + est_total })
    } else if let Some(eta) = jobs
        .iter()
        .filter_map(|j| j.eta)
        .max()
        .filter(|&e| e > 0 && e < 86400)
    {
        json!({ "start": earliest, "end": now + eta })
    } else {
        json!({ "start": earliest })
    };

    json!({ "details": details, "state": state, "assets": assets, "timestamps": timestamps, "buttons": buttons() })
}

fn build_processing(jobs: &[&crate::orchestrator::types::Job], now: u64, assets: &Value) -> Value {
    let count = jobs.len();
    let details = if count == 1 {
        "Processing 1 file".into()
    } else {
        format!("Processing {count} files")
    };
    let start = jobs
        .iter()
        .filter_map(|j| j.started_at)
        .min()
        .map(|ms| ms / 1000)
        .unwrap_or(now);
    json!({ "details": details, "state": "Post-processing…", "assets": assets, "timestamps": { "start": start }, "buttons": buttons() })
}

fn build_resolving(jobs: &[&crate::orchestrator::types::Job], now: u64, assets: &Value) -> Value {
    let count = jobs.len();
    let details = if count == 1 {
        "Resolving 1 URL".into()
    } else {
        format!("Resolving {count} URLs")
    };
    json!({ "details": details, "state": "Fetching metadata…", "assets": assets, "timestamps": { "start": now }, "buttons": buttons() })
}

async fn build_idle(manager: &Arc<JobManager>, session_start: u64, assets: &Value) -> Value {
    let stats = manager.stats.get().await;
    let gb = stats.total_size_mb / 1024.0;
    let state = if gb >= 1.0 {
        format!("{} downloads · {gb:.1} GB total", stats.total_downloads)
    } else {
        format!(
            "{} downloads · {:.0} MB total",
            stats.total_downloads, stats.total_size_mb
        )
    };
    json!({ "details": "Idling", "state": state, "assets": assets, "timestamps": { "start": session_start }, "buttons": buttons() })
}

fn buttons() -> Value {
    json!([
        { "label": "Website", "url": "https://comine.app" },
        { "label": "GitHub", "url": "https://github.com/nichind/comine" }
    ])
}

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn fmt_bytes(b: u64) -> String {
    match b {
        b if b >= 1_073_741_824 => format!("{:.1} GB", b as f64 / 1_073_741_824.0),
        b if b >= 1_048_576 => format!("{:.1} MB", b as f64 / 1_048_576.0),
        b if b >= 1024 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{b} B"),
    }
}

fn fmt_speed(bps: f64) -> String {
    match bps {
        s if s >= 1_073_741_824.0 => format!("{:.1} GB/s", s / 1_073_741_824.0),
        s if s >= 1_048_576.0 => format!("{:.1} MB/s", s / 1_048_576.0),
        s if s >= 1024.0 => format!("{:.0} KB/s", s / 1024.0),
        s => format!("{s:.0} B/s"),
    }
}
