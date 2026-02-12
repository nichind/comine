use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::prelude::*;
use futures_util::{SinkExt, StreamExt};
use rand::{thread_rng, Rng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_server_url", alias = "serverUrl")]
    pub server_url: String,
    #[serde(default = "default_host_id", alias = "hostId")]
    pub host_id: String,
    #[serde(default = "default_master_secret", alias = "masterSecret")]
    pub master_secret: String,
}

fn default_server_url() -> String {
    RelayConfig::default().server_url
}

fn default_host_id() -> String {
    generate_host_id()
}

fn default_master_secret() -> String {
    generate_master_secret()
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server_url: "wss://sync.comine.app/ws".to_string(),
            host_id: generate_host_id(),
            master_secret: generate_master_secret(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: String,
    pub device_name: String,
    pub browser: String,
    pub secret: String,
    pub paired_at: i64,
    #[serde(default)]
    pub last_seen: i64,
    #[serde(default)]
    pub last_command_at: i64,
    #[serde(default)]
    pub command_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RelayMessage {
    t: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    host_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    browser: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    encrypted_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "box")]
    box_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "openApp")]
    pub open_app: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingPairing {
    pub device_id: String,
    pub device_name: String,
    pub browser: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelayStatus {
    pub enabled: bool,
    pub server_url: String,
    pub connected: bool,
    pub host_id: String,
    pub device_count: usize,
    pub pairing_code: Option<String>,
}

pub struct RelayState {
    config: RwLock<RelayConfig>,
    devices: RwLock<HashMap<String, PairedDevice>>,
    pending_pairings: RwLock<HashMap<String, PendingPairing>>,
    current_pairing_code: RwLock<Option<String>>,
    sender: RwLock<Option<mpsc::Sender<String>>>,
    connected: RwLock<bool>,
    connect_task_running: AtomicBool,
    data_dir: PathBuf,
}

impl RelayState {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            config: RwLock::new(RelayConfig::default()),
            devices: RwLock::new(HashMap::new()),
            pending_pairings: RwLock::new(HashMap::new()),
            current_pairing_code: RwLock::new(None),
            sender: RwLock::new(None),
            connected: RwLock::new(false),
            connect_task_running: AtomicBool::new(false),
            data_dir,
        }
    }

    pub async fn load(&self) -> Result<(), String> {
        let config_path = self.data_dir.join("relay_config.json");
        let devices_path = self.data_dir.join("relay_devices.json");
        
        tracing::info!("[Relay] Loading config from {:?}", config_path);

        let mut config_dirty = false;

        if config_path.exists() {
            let data = tokio::fs::read_to_string(&config_path)
                .await
                .map_err(|e| e.to_string())?;
            let config: RelayConfig = serde_json::from_str(&data).map_err(|e| e.to_string())?;

            let mut config = config;
            if config.server_url.trim().is_empty() {
                config.server_url = RelayConfig::default().server_url;
                config_dirty = true;
            }
            if !(config.server_url.starts_with("ws://") || config.server_url.starts_with("wss://")) {
                config.server_url = RelayConfig::default().server_url;
                config_dirty = true;
            }
            if config.host_id.trim().is_empty() {
                config.host_id = generate_host_id();
                config_dirty = true;
            }
            if config.master_secret.trim().is_empty() {
                config.master_secret = generate_master_secret();
                config_dirty = true;
            }

            *self.config.write().await = config;
        }

        if !config_path.exists() {
            config_dirty = true;
        }

        if config_dirty {
            let _ = self.save_config().await;
        }

        if devices_path.exists() {
            let data = tokio::fs::read_to_string(&devices_path)
                .await
                .map_err(|e| e.to_string())?;
            let mut devices: HashMap<String, PairedDevice> =
                serde_json::from_str(&data).map_err(|e| e.to_string())?;

            for d in devices.values_mut() {
                if d.last_seen <= 0 {
                    d.last_seen = d.paired_at;
                }
            }
            *self.devices.write().await = devices;
        }

        Ok(())
    }

    pub async fn save_config(&self) -> Result<(), String> {
        let config_path = self.data_dir.join("relay_config.json");
        tracing::info!("[Relay] Saving config to {:?}", config_path);
        let config = self.config.read().await;
        let data = serde_json::to_string_pretty(&*config).map_err(|e| e.to_string())?;
        tokio::fs::write(&config_path, data)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn save_devices(&self) -> Result<(), String> {
        let devices_path = self.data_dir.join("relay_devices.json");
        let devices = self.devices.read().await;
        let data = serde_json::to_string_pretty(&*devices).map_err(|e| e.to_string())?;
        tokio::fs::write(&devices_path, data)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn generate_host_id() -> String {
    let mut bytes = [0u8; 16];
    thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn generate_master_secret() -> String {
    let mut bytes = [0u8; 32];
    thread_rng().fill_bytes(&mut bytes);
    BASE64_STANDARD.encode(bytes)
}

fn generate_device_secret() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    thread_rng().fill_bytes(&mut bytes);
    bytes
}

fn generate_pairing_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = thread_rng();
    (0..6)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

fn encrypt_with_pairing_code(code: &str, secret: &[u8]) -> String {
    // XOR with SHA256(code)
    let mut hasher = Sha256::new();
    hasher.update(b"comine-pairing-v1\0");
    hasher.update(code.as_bytes());
    let key: [u8; 32] = hasher.finalize().into();

    let encrypted: Vec<u8> = secret.iter().enumerate().map(|(i, &b)| b ^ key[i % 32]).collect();
    BASE64_STANDARD.encode(&encrypted)
}

fn encrypt_frame(key: &[u8; 32], plaintext: &[u8]) -> Result<(String, String), String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;

    let mut nonce_bytes = [0u8; 12];
    thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("Encrypt failed: {}", e))?;

    Ok((
        BASE64_STANDARD.encode(nonce_bytes),
        BASE64_STANDARD.encode(ciphertext),
    ))
}

fn decrypt_frame(key: &[u8; 32], nonce_b64: &str, box_b64: &str) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;

    let nonce_bytes = BASE64_STANDARD
        .decode(nonce_b64)
        .map_err(|e| format!("Invalid nonce: {}", e))?;
    let ciphertext = BASE64_STANDARD
        .decode(box_b64)
        .map_err(|e| format!("Invalid ciphertext: {}", e))?;

    if nonce_bytes.len() != 12 {
        return Err("Invalid nonce length".to_string());
    }

    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| format!("Decrypt failed: {}", e))
}

pub async fn connect(app: AppHandle, state: Arc<RelayState>) {
    if state.connect_task_running.swap(true, Ordering::SeqCst) {
        tracing::info!("[Relay] Connect task already running");
        return;
    }

    struct ResetFlag<'a>(&'a AtomicBool);
    impl Drop for ResetFlag<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let _reset = ResetFlag(&state.connect_task_running);

    loop {
        let config = state.config.read().await.clone();

        if !config.enabled {
            tracing::info!("[Relay] Disabled, not connecting");
            return;
        }

        let host_prefix = config.host_id.get(0..8).unwrap_or(config.host_id.as_str());
        tracing::info!("[Relay] Connecting to {} (host_id {}…)", config.server_url, host_prefix);

        let url_str = config.server_url.clone();
        let session_host_id = config.host_id.clone();

        let ws_result = connect_async(&url_str).await;
        let (ws_stream, _) = match ws_result {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("[Relay] Connection failed: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        tracing::info!("[Relay] WebSocket connected");

        let (write, mut read) = ws_stream.split();
        let (tx, mut rx) = mpsc::channel::<String>(256);

        *state.sender.write().await = Some(tx.clone());

        let hello = serde_json::to_string(&RelayMessage {
            t: "host_hello".to_string(),
            host_id: Some(session_host_id.clone()),
            ..Default::default()
        })
        .unwrap_or_default();

        let write = Arc::new(tokio::sync::Mutex::new(write));
        {
            let mut w = write.lock().await;
            if let Err(e) = w.send(WsMessage::Text(hello.into())).await {
                tracing::error!("[Relay] Failed to send host_hello: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        }

        let write_clone = write.clone();
        let writer_handle = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let mut w = write_clone.lock().await;
                if w.send(WsMessage::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        });

        loop {
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(500)) => {
                    let cfg = state.config.read().await;
                    if !cfg.enabled {
                        tracing::info!("[Relay] Disabled, closing connection");
                        break;
                    }

                    if cfg.server_url != url_str || cfg.host_id != session_host_id {
                        tracing::info!("[Relay] Config changed, reconnecting");
                        break;
                    }
                }
                result = read.next() => {
                    let Some(result) = result else { break; };
                    match result {
                        Ok(WsMessage::Text(text)) => {
                            let msg: RelayMessage = match serde_json::from_str(&text) {
                                Ok(m) => m,
                                Err(_) => continue,
                            };
                            handle_message(&app, &state, msg, &tx).await;
                        }
                        Ok(WsMessage::Ping(data)) => {
                            let mut w = write.lock().await;
                            let _ = w.send(WsMessage::Pong(data)).await;
                        }
                        Ok(WsMessage::Close(_)) => {
                            tracing::info!("[Relay] Server closed connection");
                            break;
                        }
                        Err(e) => {
                            tracing::error!("[Relay] Read error: {}", e);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        writer_handle.abort();
        *state.connected.write().await = false;
        *state.sender.write().await = None;
        let latest_config = state.config.read().await;
        let _ = app.emit(
            "relay-status",
            RelayStatus {
                enabled: latest_config.enabled,
                server_url: latest_config.server_url.clone(),
                connected: false,
                host_id: latest_config.host_id.clone(),
                device_count: state.devices.read().await.len(),
                pairing_code: None,
            },
        );

        let config = state.config.read().await;
        if !config.enabled {
            tracing::info!("[Relay] Disabled, stopping reconnect loop");
            return;
        }

        tracing::info!("[Relay] Reconnecting in 5 seconds...");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

async fn handle_message(
    app: &AppHandle,
    state: &Arc<RelayState>,
    msg: RelayMessage,
    tx: &mpsc::Sender<String>,
) {
    match msg.t.as_str() {
        "host_ok" => {
            tracing::info!("[Relay] Host registered successfully");
            *state.connected.write().await = true;
            let config = state.config.read().await;
            let _ = app.emit("relay-status", RelayStatus {
                enabled: config.enabled,
                server_url: config.server_url.clone(),
                connected: true,
                host_id: config.host_id.clone(),
                device_count: state.devices.read().await.len(),
                pairing_code: state.current_pairing_code.read().await.clone(),
            });
        }

        "pairing_request" => {
            let device_id = msg.device_id.unwrap_or_default();
            let code = msg.code.unwrap_or_default();
            let device_name = msg.device_name.unwrap_or_else(|| "Unknown".to_string());
            let browser = msg.browser.unwrap_or_else(|| "Browser".to_string());

            let current_code = state.current_pairing_code.read().await.clone();
            if current_code.as_deref() != Some(&code) {
                tracing::warn!("[Relay] Invalid pairing code from {}", device_id);
                let reject = serde_json::to_string(&RelayMessage {
                    t: "pairing_reject".to_string(),
                    device_id: Some(device_id),
                    ..Default::default()
                })
                .unwrap_or_default();
                let _ = tx.send(reject).await;
                return;
            }

            tracing::info!("[Relay] Pairing request from {} ({})", device_name, browser);

            let pending = PendingPairing {
                device_id: device_id.clone(),
                device_name: device_name.clone(),
                browser: browser.clone(),
                code: code.clone(),
            };
            state
                .pending_pairings
                .write()
                .await
                .insert(device_id.clone(), pending.clone());

            let _ = app.emit("relay-pairing-request", pending);
        }

        "frame" => {
            let device_id = msg.device_id.unwrap_or_default();
            let nonce = msg.nonce.unwrap_or_default();
            let box_data = msg.box_data.unwrap_or_default();

            let devices = state.devices.read().await;
            let device = match devices.get(&device_id) {
                Some(d) => d,
                None => {
                    tracing::warn!("[Relay] Frame from unknown device: {}", device_id);
                    return;
                }
            };

            let secret = match BASE64_STANDARD.decode(&device.secret) {
                Ok(s) if s.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&s);
                    arr
                }
                _ => {
                    tracing::error!("[Relay] Invalid device secret for {}", device_id);
                    return;
                }
            };

            let plaintext = match decrypt_frame(&secret, &nonce, &box_data) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("[Relay] Decrypt failed: {}", e);
                    return;
                }
            };

            let inner: InnerMessage = match serde_json::from_slice(&plaintext) {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("[Relay] Invalid inner message: {}", e);
                    return;
                }
            };

            handle_inner_message(app, state, &device_id, &secret, inner, tx).await;
        }

        "error" => {
            tracing::error!("[Relay] Server error: {}", msg.error.unwrap_or_default());
        }

        "pong" => {}

        _ => {
            tracing::debug!("[Relay] Unknown message type: {}", msg.t);
        }
    }
}

async fn handle_inner_message(
    app: &AppHandle,
    state: &Arc<RelayState>,
    device_id: &str,
    device_secret: &[u8; 32],
    msg: InnerMessage,
    tx: &mpsc::Sender<String>,
) {
    let now = chrono::Utc::now().timestamp();
    {
        let mut devices = state.devices.write().await;
        if let Some(d) = devices.get_mut(device_id) {
            d.last_seen = now;
            match msg.msg_type.as_str() {
                "download" | "cancel" => {
                    d.command_count = d.command_count.saturating_add(1);
                    d.last_command_at = now;
                }
                _ => {}
            }
        }
    }
    let _ = state.save_devices().await;

    match msg.msg_type.as_str() {
        "probe" => {
            if let Some(cmd_id) = msg.cmd_id {
                send_ack(device_id, device_secret, &cmd_id, true, None, tx).await;
            }
        }

        "download" => {
            let url = msg.url.unwrap_or_default();
            let title = msg.title;
            let thumbnail = msg.thumbnail;
            let cmd_id = msg.cmd_id.clone();
            let open_app = msg.open_app.unwrap_or(false);

            tracing::info!("[Relay] Download request from {}: {}", device_id, url);

            let _ = app.emit(
                "extension-download",
                serde_json::json!({
                    "url": url,
                    "title": title,
                    "thumbnail": thumbnail,
                    "id": cmd_id.clone().unwrap_or_else(|| format!("relay-{}", chrono::Utc::now().timestamp_millis())),
                    "openApp": open_app,
                    "deviceId": device_id,
                    "fromRelay": true,
                }),
            );

            if let Some(cmd_id) = cmd_id {
                send_ack(device_id, device_secret, &cmd_id, true, None, tx).await;
            }
        }

        "cancel" => {
            let url = msg.url.unwrap_or_default();
            let cmd_id = msg.cmd_id.clone();

            tracing::info!("[Relay] Cancel request from {}: {}", device_id, url);

            let _ = app.emit(
                "extension-cancel",
                serde_json::json!({
                    "url": url,
                    "id": cmd_id.clone().unwrap_or_else(|| format!("relay-{}", chrono::Utc::now().timestamp_millis())),
                    "deviceId": device_id,
                    "fromRelay": true,
                }),
            );

            if let Some(cmd_id) = cmd_id {
                send_ack(device_id, device_secret, &cmd_id, true, None, tx).await;
            }
        }

        _ => {
            tracing::debug!("[Relay] Unknown inner message type: {}", msg.msg_type);
        }
    }
}

async fn send_ack(
    device_id: &str,
    secret: &[u8; 32],
    cmd_id: &str,
    success: bool,
    error: Option<&str>,
    tx: &mpsc::Sender<String>,
) {
    let inner = InnerMessage {
        msg_type: "ack".to_string(),
        cmd_id: Some(cmd_id.to_string()),
        open_app: None,
        success: Some(success),
        error: error.map(String::from),
        url: None,
        title: None,
        thumbnail: None,
    };

    let plaintext = match serde_json::to_vec(&inner) {
        Ok(p) => p,
        Err(_) => return,
    };

    let (nonce, box_data) = match encrypt_frame(secret, &plaintext) {
        Ok(f) => f,
        Err(_) => return,
    };

    let msg = RelayMessage {
        t: "frame".to_string(),
        device_id: Some(device_id.to_string()),
        nonce: Some(nonce),
        box_data: Some(box_data),
        ..Default::default()
    };

    if let Ok(json) = serde_json::to_string(&msg) {
        let _ = tx.send(json).await;
    }
}

pub async fn start_pairing(state: &Arc<RelayState>) -> Result<String, String> {
    let code = generate_pairing_code();
    *state.current_pairing_code.write().await = Some(code.clone());
    tracing::info!("[Relay] Pairing code generated: {}", code);
    Ok(code)
}

pub async fn stop_pairing(state: &Arc<RelayState>) {
    *state.current_pairing_code.write().await = None;
    state.pending_pairings.write().await.clear();
}

pub async fn accept_pairing(state: &Arc<RelayState>, device_id: &str) -> Result<(), String> {
    let pending = state
        .pending_pairings
        .write()
        .await
        .remove(device_id)
        .ok_or("No pending pairing")?;

    let secret = generate_device_secret();
    let encrypted = encrypt_with_pairing_code(&pending.code, &secret);

    let now = chrono::Utc::now().timestamp();
    let device = PairedDevice {
        device_id: device_id.to_string(),
        device_name: pending.device_name,
        browser: pending.browser,
        secret: BASE64_STANDARD.encode(secret),
        paired_at: now,
        last_seen: now,
        last_command_at: 0,
        command_count: 0,
    };
    state
        .devices
        .write()
        .await
        .insert(device_id.to_string(), device);
    state.save_devices().await?;

    if let Some(tx) = state.sender.read().await.as_ref() {
        let msg = serde_json::to_string(&RelayMessage {
            t: "pairing_accept".to_string(),
            device_id: Some(device_id.to_string()),
            encrypted_secret: Some(encrypted),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        tx.send(msg).await.map_err(|e| e.to_string())?;
    }

    *state.current_pairing_code.write().await = None;

    tracing::info!("[Relay] Pairing accepted for {}", device_id);
    Ok(())
}

pub async fn reject_pairing(state: &Arc<RelayState>, device_id: &str) -> Result<(), String> {
    state.pending_pairings.write().await.remove(device_id);

    if let Some(tx) = state.sender.read().await.as_ref() {
        let msg = serde_json::to_string(&RelayMessage {
            t: "pairing_reject".to_string(),
            device_id: Some(device_id.to_string()),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        tx.send(msg).await.map_err(|e| e.to_string())?;
    }

    tracing::info!("[Relay] Pairing rejected for {}", device_id);
    Ok(())
}

pub async fn remove_device(state: &Arc<RelayState>, device_id: &str) -> Result<(), String> {
    state.devices.write().await.remove(device_id);
    state.save_devices().await?;
    tracing::info!("[Relay] Device removed: {}", device_id);
    Ok(())
}

pub async fn get_status(state: &Arc<RelayState>) -> RelayStatus {
    let config = state.config.read().await;
    RelayStatus {
        enabled: config.enabled,
        server_url: config.server_url.clone(),
        connected: *state.connected.read().await,
        host_id: config.host_id.clone(),
        device_count: state.devices.read().await.len(),
        pairing_code: state.current_pairing_code.read().await.clone(),
    }
}

pub async fn get_devices(state: &Arc<RelayState>) -> Vec<PairedDevice> {
    state.devices.read().await.values().cloned().collect()
}

pub async fn set_enabled(
    app: &AppHandle,
    state: Arc<RelayState>,
    enabled: bool,
) -> Result<(), String> {
    {
        let config = state.config.read().await;
        let host_prefix = config.host_id.get(0..8).unwrap_or(config.host_id.as_str());
        tracing::info!(
            "[Relay] Set enabled={} (server_url={}, host_id {}…)",
            enabled,
            config.server_url,
            host_prefix
        );
    }
    {
        let mut config = state.config.write().await;
        config.enabled = enabled;
    }
    state.save_config().await?;

    let status = get_status(&state).await;
    let _ = app.emit("relay-status", status);

    if enabled {
        tokio::spawn(connect(app.clone(), state));
    } else {
        *state.connected.write().await = false;
    }

    Ok(())
}

pub async fn set_server_url(
    app: &AppHandle,
    state: Arc<RelayState>,
    server_url: String,
) -> Result<(), String> {
    let next = server_url.trim().to_string();
    if !(next.starts_with("ws://") || next.starts_with("wss://")) {
        return Err("Relay server URL must start with ws:// or wss://".to_string());
    }

    {
        let mut config = state.config.write().await;
        config.server_url = next;

        if config.host_id.trim().is_empty() {
            config.host_id = generate_host_id();
        }
        if config.master_secret.trim().is_empty() {
            config.master_secret = generate_master_secret();
        }
    }
    state.save_config().await?;

    let status = get_status(&state).await;
    let _ = app.emit("relay-status", status);
    Ok(())
}

pub async fn reset_host_id(app: &AppHandle, state: Arc<RelayState>) -> Result<(), String> {
    {
        let mut config = state.config.write().await;
        config.host_id = generate_host_id();
    }
    state.save_config().await?;

    let status = get_status(&state).await;
    let _ = app.emit("relay-status", status);
    Ok(())
}

impl Default for RelayMessage {
    fn default() -> Self {
        Self {
            t: String::new(),
            host_id: None,
            device_id: None,
            code: None,
            device_name: None,
            browser: None,
            encrypted_secret: None,
            nonce: None,
            box_data: None,
            error: None,
        }
    }
}
