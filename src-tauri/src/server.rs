use serde::{Deserialize, Serialize};
use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::AtomicU16;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tiny_http::{Header, Method, Request, Response, Server};

static SERVER_RUNNING: AtomicBool = AtomicBool::new(false);
static SERVER_PORT: AtomicU16 = AtomicU16::new(0);
static SERVER_TOKEN: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));

static QUEUE_ITEMS: LazyLock<Mutex<Vec<QueueItem>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static HISTORY_ITEMS: LazyLock<Mutex<Vec<HistoryItem>>> = LazyLock::new(|| Mutex::new(Vec::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub id: String,
    pub url: String,
    pub status: String,
    #[serde(default)]
    pub status_message: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub thumbnail: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub speed: String,
    #[serde(default)]
    pub eta: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub added_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub thumbnail: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub completed_at: f64,
}

pub fn update_queue(items: Vec<QueueItem>) {
    if let Ok(mut queue) = QUEUE_ITEMS.lock() {
        *queue = items;
    }
}

pub fn update_history(items: Vec<HistoryItem>) {
    if let Ok(mut history) = HISTORY_ITEMS.lock() {
        *history = items;
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DownloadRequest {
    url: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    options: Option<DownloadOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadOptions {
    #[serde(default)]
    video_quality: Option<String>,
    #[serde(default)]
    download_mode: Option<String>,
    #[serde(default)]
    audio_quality: Option<String>,
    #[serde(default)]
    remux: Option<bool>,
    #[serde(default)]
    convert_to_mp4: Option<bool>,
    #[serde(default)]
    embed_thumbnail: Option<bool>,
    #[serde(default)]
    clear_metadata: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenRequest {
    #[serde(rename = "filePath")]
    file_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CancelRequest {
    url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CookiesRequest {
    domain: String,
    #[serde(rename = "sourceUrl")]
    #[serde(default)]
    source_url: Option<String>,
    cookies: Vec<Cookie>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    #[serde(default)]
    secure: bool,
    #[serde(default)]
    http_only: bool,
    #[serde(default)]
    expiration_date: Option<f64>,
}

pub fn start_server(app: AppHandle, port: u16, token: String) {
    if token.trim().is_empty() {
        log::error!("[Server] Refusing to start without auth token");
        return;
    }

    if SERVER_RUNNING.swap(true, Ordering::SeqCst) {
        log::warn!("[Server] Already running");
        return;
    }

    SERVER_PORT.store(port, Ordering::SeqCst);

    if let Ok(mut t) = SERVER_TOKEN.lock() {
        *t = token;
    }

    let addr = format!("127.0.0.1:{}", port);
    log::info!("[Server] Starting on {}", addr);

    thread::spawn(move || {
        let server = match Server::http(&addr) {
            Ok(s) => s,
            Err(e) => {
                log::error!("[Server] Failed to start: {}", e);
                SERVER_RUNNING.store(false, Ordering::SeqCst);
                SERVER_PORT.store(0, Ordering::SeqCst);
                return;
            }
        };

        for request in server.incoming_requests() {
            if !SERVER_RUNNING.load(Ordering::SeqCst) {
                break;
            }
            handle_request(&app, request);
        }

        SERVER_RUNNING.store(false, Ordering::SeqCst);
        SERVER_PORT.store(0, Ordering::SeqCst);
        log::info!("[Server] Stopped");
    });
}

pub fn stop_server() {
    SERVER_RUNNING.store(false, Ordering::SeqCst);

    // Unblock the accept loop promptly by poking the current port.
    let port = SERVER_PORT.load(Ordering::SeqCst);
    if port != 0 {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            let _ = stream
                .write_all(b"GET /ping HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
            let _ = stream.flush();
        }
    }
}

pub fn is_running() -> bool {
    SERVER_RUNNING.load(Ordering::SeqCst)
}

fn handle_request(app: &AppHandle, mut request: Request) {
    let cors_headers = vec![
        Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
        Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
        Header::from_bytes("Access-Control-Allow-Headers", "Content-Type, X-Comine-Token").unwrap(),
        Header::from_bytes("Content-Type", "application/json").unwrap(),
    ];

    if request.method() == &Method::Options {
        let mut response = Response::empty(200);
        for h in cors_headers {
            response.add_header(h);
        }
        let _ = request.respond(response);
        return;
    }

    if !is_authorized(&request) {
        let (status, body) = (401, r#"{"error":"Unauthorized"}"#.to_string());
        let mut response = Response::from_string(body).with_status_code(status);
        for h in cors_headers {
            response.add_header(h);
        }
        let _ = request.respond(response);
        return;
    }

    let path = request.url().split('?').next().unwrap_or("/");
    let method = request.method().clone();

    let result = match (method, path) {
        (Method::Get, "/ping") => handle_ping(),
        (Method::Get, "/status") => handle_status(app),
        (Method::Get, path) if path.starts_with("/status/") => {
            let url = urlencoding::decode(&path[8..])
                .map(|s| s.to_string())
                .unwrap_or_default();
            handle_status_single(app, &url)
        }
        (Method::Get, "/history") => handle_history(app),
        (Method::Post, "/download") => {
            let body = read_body(&mut request);
            handle_download(app, &body)
        }
        (Method::Post, "/cancel") => {
            let body = read_body(&mut request);
            handle_cancel(app, &body)
        }
        (Method::Post, "/open") => {
            let body = read_body(&mut request);
            handle_open(app, &body)
        }
        (Method::Post, "/reveal") => {
            let body = read_body(&mut request);
            handle_reveal(app, &body)
        }
        (Method::Post, "/cookies") => {
            let body = read_body(&mut request);
            handle_cookies(app, &body)
        }
        _ => (404, r#"{"error":"Not found"}"#.to_string()),
    };

    let (status, body) = result;
    let mut response = Response::from_string(body).with_status_code(status);
    for h in cors_headers {
        response.add_header(h);
    }
    let _ = request.respond(response);
}

fn is_authorized(request: &Request) -> bool {
    let expected = SERVER_TOKEN
        .lock()
        .ok()
        .map(|t| t.clone())
        .unwrap_or_default();

    if expected.trim().is_empty() {
        // If no token is set, fail closed.
        return false;
    }

    let provided = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Comine-Token"))
        .map(|h| h.value.as_str());

    match provided {
        Some(p) => p == expected,
        None => false,
    }
}

fn read_body(request: &mut Request) -> String {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    body
}

fn handle_ping() -> (u16, String) {
    (200, r#"{"ok":true}"#.to_string())
}

fn handle_status(_app: &AppHandle) -> (u16, String) {
    // Read queue items from shared state
    let items = match QUEUE_ITEMS.lock() {
        Ok(queue) => queue.clone(),
        Err(_) => Vec::new(),
    };

    let json = serde_json::json!({
        "queue": items,
        "count": items.len()
    });

    (
        200,
        serde_json::to_string(&json).unwrap_or_else(|_| r#"{"queue":[],"count":0}"#.to_string()),
    )
}

fn handle_status_single(_app: &AppHandle, url: &str) -> (u16, String) {
    let items = match QUEUE_ITEMS.lock() {
        Ok(queue) => queue.clone(),
        Err(_) => Vec::new(),
    };

    if let Some(item) = items.iter().find(|i| i.url == url) {
        let json =
            serde_json::to_string(item).unwrap_or_else(|_| r#"{"state":"unknown"}"#.to_string());
        (200, json)
    } else {
        (200, r#"{"state":"not_found"}"#.to_string())
    }
}

fn handle_history(_app: &AppHandle) -> (u16, String) {
    let items = match HISTORY_ITEMS.lock() {
        Ok(history) => history.clone(),
        Err(_) => Vec::new(),
    };

    (
        200,
        serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string()),
    )
}

fn handle_download(app: &AppHandle, body: &str) -> (u16, String) {
    let req: DownloadRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return (400, format!(r#"{{"error":"{}"}}"#, e)),
    };

    log::info!("[Server] Download request: {}", req.url);

    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| format!("local-{}", d.as_millis()))
        .unwrap_or_else(|_| "local-0".to_string());

    let open_app = match req.mode.as_deref() {
        Some("quick") | Some("quick_download") => false,
        Some("open") => true,
        None => false,
        Some(_) => false,
    };

    // Emit to frontend using the existing extension event contract
    if app
        .emit(
            "extension-download",
            serde_json::json!({
                "url": req.url,
                "title": req.title,
                "thumbnail": req.thumbnail,
                "id": id,
                "openApp": open_app,
                "fromRelay": false,
                "options": req.options,
            }),
        )
        .is_err()
    {
        return (500, r#"{"error":"Failed to emit"}"#.to_string());
    }

    (200, r#"{"ok":true}"#.to_string())
}

fn handle_cancel(app: &AppHandle, body: &str) -> (u16, String) {
    let req: CancelRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return (400, format!(r#"{{"error":"{}"}}"#, e)),
    };

    log::info!("[Server] Cancel request: {}", req.url);

    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| format!("local-{}", d.as_millis()))
        .unwrap_or_else(|_| "local-0".to_string());

    if app
        .emit(
            "extension-cancel",
            serde_json::json!({
                "url": req.url,
                "id": id,
                "fromRelay": false,
            }),
        )
        .is_err()
    {
        return (500, r#"{"error":"Failed to emit"}"#.to_string());
    }

    (200, r#"{"ok":true}"#.to_string())
}

fn handle_open(app: &AppHandle, body: &str) -> (u16, String) {
    let req: OpenRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return (400, format!(r#"{{"error":"{}"}}"#, e)),
    };

    log::info!("[Server] Open request: {}", req.file_path);

    if app.emit("server-open", &req.file_path).is_err() {
        return (500, r#"{"error":"Failed to emit"}"#.to_string());
    }

    (200, r#"{"ok":true}"#.to_string())
}

fn handle_reveal(app: &AppHandle, body: &str) -> (u16, String) {
    let req: OpenRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return (400, format!(r#"{{"error":"{}"}}"#, e)),
    };

    log::info!("[Server] Reveal request: {}", req.file_path);

    if app.emit("server-reveal", &req.file_path).is_err() {
        return (500, r#"{"error":"Failed to emit"}"#.to_string());
    }

    (200, r#"{"ok":true}"#.to_string())
}

fn handle_cookies(app: &AppHandle, body: &str) -> (u16, String) {
    let req: CookiesRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return (400, format!(r#"{{"error":"{}"}}"#, e)),
    };

    log::info!(
        "[Server] Cookies received for domain: {} ({} cookies)",
        req.domain,
        req.cookies.len()
    );

    // Convert to Netscape cookie file format (used by yt-dlp)
    let mut lines = vec!["# Netscape HTTP Cookie File".to_string()];

    // Some sites (e.g. Vimeo) rely on session cookies for auth. If we write an expiry of 0,
    // some parsers may treat them as already-expired and ignore them.
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let default_session_expires = now_secs.saturating_add(30 * 24 * 60 * 60);

    for cookie in &req.cookies {
        // Format: domain, include_subdomains, path, secure, expires, name, value
        // Preserve host-only vs domain-cookie semantics:
        // - Leading dot => include subdomains (TRUE)
        // - No leading dot => host-only (FALSE)
        let domain_str = cookie.domain.clone();
        let include_subdomains = if domain_str.starts_with('.') {
            "TRUE"
        } else {
            "FALSE"
        };
        let secure = if cookie.secure { "TRUE" } else { "FALSE" };

        let expires = cookie
            .expiration_date
            .map(|e| e as i64)
            .unwrap_or(default_session_expires)
            .to_string();

        lines.push(format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            domain_str, include_subdomains, cookie.path, secure, expires, cookie.name, cookie.value
        ));
    }

    let content = lines.join("\n");

    log::info!("[Server] Cookies formatted ({} lines)", lines.len());

    // Emit event so frontend can update the customCookies setting
    let _ = app.emit(
        "extension-cookies",
        serde_json::json!({
            "domain": req.domain,
            "sourceUrl": req.source_url,
            "count": req.cookies.len(),
            "cookies": content,
        }),
    );

    (
        200,
        format!(r#"{{"ok":true,"count":{}}}"#, req.cookies.len()),
    )
}
