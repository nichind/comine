use tauri::AppHandle;
use tracing::{debug, warn};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

use serde::{Deserialize, Serialize};

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct TorrentSearchFilters {
    pub query: String,
    /// "movie" | "tv"
    pub content_type: Option<String>,
    /// "4K" | "1080p" | "720p"
    pub quality: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u32>,
    pub min_rating: Option<f64>,
    pub language: Option<String>,
    /// "relevance" | "seeders" | "rating" | "date"
    pub sort: Option<String>,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct TorrentOption {
    pub quality: Option<String>,
    pub codec: Option<String>,
    pub seeders: Option<u32>,
    /// Size in bytes as returned by the API (integer).
    pub size_bytes: Option<u64>,
    pub quality_score: Option<u32>,
    pub magnet_url: Option<String>,
    pub torrent_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct TorrentSearchResult {
    pub title: String,
    pub year: Option<u32>,
    /// API field name is "contentType" (camelCase rename handled by serde).
    pub content_type: Option<String>,
    pub rating_imdb: Option<String>,
    pub poster_url: Option<String>,
    pub torrents: Vec<TorrentOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct TorrentSearchResponse {
    pub total: u32,
    pub page: u32,
    pub results: Vec<TorrentSearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct TorrentFileEntry {
    pub index: u32,
    pub path: String,
    pub size: u64,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const BASE_URL: &str = "https://torrentclaw.com/api/v1";

fn load_api_key(app: &AppHandle) -> Option<String> {
    use tauri_plugin_store::StoreExt;
    let store = app.store("settings.json").ok()?;
    store
        .get("torrentSearchApiKey")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .filter(|s| !s.is_empty())
}

fn build_client(timeout_secs: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))
}

fn apply_api_key(
    request: reqwest::RequestBuilder,
    api_key: Option<&str>,
) -> reqwest::RequestBuilder {
    match api_key {
        Some(key) => request.header("Authorization", format!("Bearer {}", key)),
        None => request,
    }
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn torrent_search(
    app: AppHandle,
    filters: TorrentSearchFilters,
) -> Result<TorrentSearchResponse, String> {
    debug!(
        "torrent_search: query={:?} type={:?} quality={:?} page={:?}",
        filters.query, filters.content_type, filters.quality, filters.page
    );

    let api_key = load_api_key(&app);
    let client = build_client(15)?;

    let mut params: Vec<(&str, String)> = vec![("q", filters.query.clone())];

    if let Some(ref ct) = filters.content_type {
        params.push(("type", ct.clone()));
    }
    if let Some(ref q) = filters.quality {
        params.push(("quality", q.clone()));
    }
    if let Some(ref g) = filters.genre {
        params.push(("genre", g.clone()));
    }
    if let Some(y) = filters.year {
        params.push(("year", y.to_string()));
    }
    if let Some(r) = filters.min_rating {
        params.push(("rating", r.to_string()));
    }
    if let Some(ref lang) = filters.language {
        params.push(("language", lang.clone()));
    }
    if let Some(ref sort) = filters.sort {
        params.push(("sort", sort.clone()));
    }
    if let Some(page) = filters.page {
        params.push(("page", page.to_string()));
    }

    let url = format!("{}/search", BASE_URL);
    let request = client.get(&url).query(&params);
    let request = apply_api_key(request, api_key.as_deref());

    let response = request
        .send()
        .await
        .map_err(|e| format!("Search request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("TorrentClaw API error: {}", response.status()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read search response: {}", e))?;

    serde_json::from_str::<TorrentSearchResponse>(&text).map_err(|e| {
        let preview = if text.len() > 300 {
            format!("{}...", &text[..300])
        } else {
            text.clone()
        };
        warn!("TorrentClaw parse error: {}. Response preview: {}", e, preview);
        format!("Failed to parse search response: {}", e)
    })
}

#[tauri::command]
pub async fn torrent_autocomplete(
    app: AppHandle,
    query: String,
) -> Result<Vec<String>, String> {
    debug!("torrent_autocomplete: query={:?}", query);

    let api_key = load_api_key(&app);
    let client = build_client(5)?;

    let url = format!("{}/autocomplete", BASE_URL);
    let request = client.get(&url).query(&[("q", &query)]);
    let request = apply_api_key(request, api_key.as_deref());

    let response = request
        .send()
        .await
        .map_err(|e| format!("Autocomplete request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("TorrentClaw API error: {}", response.status()));
    }

    response
        .json::<Vec<String>>()
        .await
        .map_err(|e| format!("Failed to parse autocomplete response: {}", e))
}

/// List files inside a torrent given its magnet URL.
///
/// Because `aria2c --show-files` only works on `.torrent` files, we:
/// 1. Fetch the metadata via `--bt-metadata-only=true --bt-save-metadata=true` into a temp dir.
/// 2. Locate the resulting `.torrent` file.
/// 3. Run `--show-files` on it and parse the output.
/// 4. Clean up the temp dir.
///
/// This command is desktop-only because aria2 is not available on mobile.
#[cfg(desktop)]
#[tauri::command]
pub async fn torrent_list_files(
    app: AppHandle,
    magnet_url: String,
) -> Result<Vec<TorrentFileEntry>, String> {
    debug!("torrent_list_files: magnet_url={:?}", magnet_url);

    let aria2_path = crate::deps::resolve_aria2_path(&app)
        .ok_or_else(|| "aria2 is not installed. Install it from the Dependencies settings page.".to_string())?;

    // Create a temporary directory for metadata download.
    let temp_dir = std::env::temp_dir().join(format!("comine_torrent_{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let cleanup = || {
        let dir = temp_dir.clone();
        tokio::spawn(async move {
            if let Err(e) = tokio::fs::remove_dir_all(&dir).await {
                warn!("Failed to clean up temp dir {:?}: {}", dir, e);
            }
        });
    };

    // Step 1: Download torrent metadata only.
    let torrent_file = match fetch_torrent_metadata(&aria2_path, &temp_dir, &magnet_url).await {
        Ok(path) => path,
        Err(e) => {
            cleanup();
            return Err(e);
        }
    };

    // Step 2: List files from the .torrent file.
    let entries = match list_files_from_torrent(&aria2_path, &torrent_file).await {
        Ok(entries) => entries,
        Err(e) => {
            cleanup();
            return Err(e);
        }
    };

    cleanup();
    Ok(entries)
}

/// Mobile stub — aria2 is not available on mobile.
#[cfg(mobile)]
#[tauri::command]
pub async fn torrent_list_files(
    _app: AppHandle,
    _magnet_url: String,
) -> Result<Vec<TorrentFileEntry>, String> {
    Err("Torrent file listing is not supported on mobile.".to_string())
}

// ── aria2 subprocess helpers (desktop only) ──────────────────────────────────

#[cfg(desktop)]
async fn fetch_torrent_metadata(
    aria2_path: &std::path::Path,
    temp_dir: &std::path::Path,
    magnet_url: &str,
) -> Result<std::path::PathBuf, String> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut cmd = crate::utils::new_command(aria2_path);
    cmd.args([
        "--bt-metadata-only=true",
        "--bt-save-metadata=true",
        "--enable-dht=true",
        "--bt-enable-lpd=true",
        "--listen-port=6881-6999",
        "--dht-listen-port=6881-6999",
        "--seed-ratio=0.0",
        "--summary-interval=5",
        "-d",
    ])
    .arg(temp_dir)
    .arg(magnet_url)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn aria2 for metadata fetch: {}", e))?;

    let stderr = child.stderr.take();
    if let Some(stderr) = stderr {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if !line.trim().is_empty() {
                    tracing::debug!(target: "aria2_meta", "{}", line);
                }
            }
        });
    }

    // Wait up to 30 seconds for metadata.
    let timeout = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        child.wait(),
    )
    .await
    .map_err(|_| "Timed out waiting for torrent metadata (30s). The swarm may be inactive.".to_string())?
    .map_err(|e| format!("aria2 metadata process error: {}", e))?;

    // aria2 exits with code 0 on success, but also exits 0 when it saves the
    // metadata before seeding. Any non-zero code is a failure.
    if !timeout.success() {
        return Err(format!(
            "aria2 failed to fetch torrent metadata (exit code {:?})",
            timeout.code()
        ));
    }

    // Find the .torrent file aria2 saved.
    find_torrent_file(temp_dir).await
}

#[cfg(desktop)]
async fn find_torrent_file(dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let mut read_dir = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| format!("Failed to read temp dir: {}", e))?;

    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("torrent") {
            return Ok(path);
        }
    }

    Err("aria2 did not produce a .torrent metadata file. The magnet link may be invalid or the torrent has no active peers.".to_string())
}

#[cfg(desktop)]
async fn list_files_from_torrent(
    aria2_path: &std::path::Path,
    torrent_file: &std::path::Path,
) -> Result<Vec<TorrentFileEntry>, String> {
    use std::process::Stdio;

    let mut cmd = crate::utils::new_command(aria2_path);
    cmd.arg("--show-files")
        .arg(torrent_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        cmd.output(),
    )
    .await
    .map_err(|_| "Timed out running aria2 --show-files".to_string())?
    .map_err(|e| format!("Failed to run aria2 --show-files: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_show_files_output(&stdout)
}

/// Parse the tabular output of `aria2c --show-files`.
///
/// Format produced by aria2:
/// ```text
/// idx|path/length
/// ===+===========
///   1|path/to/movie.mkv
///    |  4.2GiB (4500000000)
///   2|path/to/subs.srt
///    |  50KiB (51200)
/// ```
///
/// We read index lines (those starting with a digit after optional whitespace),
/// capture their path, then on the very next line capture the byte count from
/// the parenthesised number.
#[cfg(desktop)]
fn parse_show_files_output(output: &str) -> Result<Vec<TorrentFileEntry>, String> {
    let mut entries: Vec<TorrentFileEntry> = Vec::new();

    // Skip the header lines (idx|... and ===+...)
    let data_lines: Vec<&str> = output
        .lines()
        .skip_while(|l| !l.trim_start().starts_with(|c: char| c.is_ascii_digit()))
        .collect();

    let mut i = 0;
    while i < data_lines.len() {
        let line = data_lines[i];

        // Index lines: optionally indented digits followed by '|'
        let trimmed = line.trim_start();
        if let Some(pipe_pos) = trimmed.find('|') {
            let index_part = trimmed[..pipe_pos].trim();
            if let Ok(index) = index_part.parse::<u32>() {
                let path = trimmed[pipe_pos + 1..].trim().to_string();

                // The next line should contain the size.
                let size = if i + 1 < data_lines.len() {
                    let size_line = data_lines[i + 1];
                    parse_size_from_line(size_line)
                } else {
                    0
                };

                entries.push(TorrentFileEntry { index, path, size });
                i += 2; // consume both the index line and the size line
                continue;
            }
        }

        i += 1;
    }

    if entries.is_empty() {
        return Err("aria2 --show-files produced no file entries. The .torrent file may be malformed.".to_string());
    }

    Ok(entries)
}

/// Extract the byte count from a size line like `  4.2GiB (4500000000)`.
/// Returns 0 if the parenthesised value cannot be found or parsed.
#[cfg(desktop)]
fn parse_size_from_line(line: &str) -> u64 {
    if let (Some(open), Some(close)) = (line.rfind('('), line.rfind(')')) {
        if open < close {
            let inner = &line[open + 1..close];
            // Strip commas (e.g. "4,500,000,000") before parsing.
            let clean: String = inner.chars().filter(|c| c.is_ascii_digit()).collect();
            return clean.parse::<u64>().unwrap_or(0);
        }
    }
    0
}
