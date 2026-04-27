use std::sync::Arc;

use rusqlite::params;
use tauri::AppHandle;
use tracing::{error, info, warn};

use crate::database::Database;
use crate::orchestrator::types::{AppStats, Broadcast};

const STATS_URL: &str = "https://stats.comine.app/";
const BROADCAST_URL: &str = "https://stats.comine.app/broadcast";
const SYNC_INTERVAL_SECS: u64 = 3600;

pub struct StatsStore {
    db: Arc<Database>,
}

impl StatsStore {
    pub fn new(db: Arc<Database>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    /// Seed first_launch and installation_id if not yet set.
    /// Called from the async init block (off the main thread) so that
    /// the lazy Database connection is not triggered on iOS's main thread.
    pub fn ensure_seeded(&self) {
        let conn = self.db.conn();
        let (first_launch, installation_id): (String, String) = conn
            .query_row(
                "SELECT first_launch, installation_id FROM stats WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or_default();

        let mut needs_update = false;
        let new_first_launch = if first_launch.is_empty() {
            needs_update = true;
            chrono::Utc::now().to_rfc3339()
        } else {
            first_launch
        };
        let new_installation_id = if installation_id.is_empty() {
            needs_update = true;
            uuid::Uuid::new_v4().to_string()
        } else {
            installation_id
        };

        if needs_update {
            let _ = conn.execute(
                "UPDATE stats SET first_launch = ?1, installation_id = ?2 WHERE id = 1",
                params![new_first_launch, new_installation_id],
            );
        }

        info!("Stats store initialized from database");
    }

    pub fn start(self: &Arc<Self>, app: AppHandle) {
        let store = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            let send_enabled = crate::store_utils::get_bool(&app, "sendStats", true);
            store.post_stats(&app, send_enabled, true).await;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(SYNC_INTERVAL_SECS)).await;
                let send_enabled = crate::store_utils::get_bool(&app, "sendStats", true);
                store.post_stats(&app, send_enabled, false).await;
            }
        });
    }

    pub async fn record_completion(&self, size_bytes: u64) {
        let db = self.db.clone();
        let size_mb = size_bytes as f64 / (1024.0 * 1024.0);
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            if let Err(e) = conn.execute(
                "UPDATE stats SET total_downloads = total_downloads + 1, successful_downloads = successful_downloads + 1, total_size_mb = total_size_mb + ?1 WHERE id = 1",
                params![size_mb],
            ) {
                error!("Failed to record completion: {}", e);
            }
        })
        .await
        .ok();
    }

    pub async fn record_failure(&self) {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            if let Err(e) = conn.execute(
                "UPDATE stats SET total_downloads = total_downloads + 1, failed_downloads = failed_downloads + 1 WHERE id = 1",
                [],
            ) {
                error!("Failed to record failure: {}", e);
            }
        })
        .await
        .ok();
    }

    pub async fn get(&self) -> AppStats {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            conn.query_row(
                "SELECT total_downloads, total_size_mb, successful_downloads, failed_downloads, first_launch, installation_id, last_sync_time FROM stats WHERE id = 1",
                [],
                |row| {
                    Ok(AppStats {
                        total_downloads: row.get::<_, i64>(0)? as u64,
                        total_size_mb: row.get(1)?,
                        successful_downloads: row.get::<_, i64>(2)? as u64,
                        failed_downloads: row.get::<_, i64>(3)? as u64,
                        first_launch: row.get(4)?,
                        installation_id: row.get(5)?,
                        last_sync_time: row.get(6)?,
                    })
                },
            )
            .unwrap_or_default()
        })
        .await
        .unwrap_or_default()
    }

    pub async fn backfill_from_history(
        &self,
        _history: &crate::orchestrator::history::HistoryStore,
    ) {
        let stats = self.get().await;
        if stats.total_downloads > 0 {
            return;
        }

        // Use the database directly — count and sum from the history table
        let db = self.db.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            let result: Option<(i64, f64, i64)> = conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(size), 0) / (1024.0 * 1024.0), COALESCE(MIN(downloaded_at), 0) FROM history",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            ).ok();

            if let Some((count, size_mb, oldest_ts)) = result {
                if count == 0 {
                    return false;
                }

                let first_launch = if oldest_ts > 0 {
                    chrono::DateTime::from_timestamp(oldest_ts, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                if let Err(e) = conn.execute(
                    "UPDATE stats SET total_downloads = ?1, successful_downloads = ?1, total_size_mb = ?2 WHERE id = 1 AND total_downloads = 0",
                    params![count, size_mb],
                ) {
                    error!("Failed to backfill stats: {}", e);
                    return false;
                }

                if !first_launch.is_empty() {
                    let _ = conn.execute(
                        "UPDATE stats SET first_launch = ?1 WHERE id = 1 AND first_launch = ''",
                        params![first_launch],
                    );
                }

                info!("Backfilled stats from {} history items", count);
                true
            } else {
                false
            }
        })
        .await
        .unwrap_or(false);
    }

    pub async fn post_stats(&self, app: &AppHandle, send_enabled: bool, force: bool) {
        if !send_enabled {
            return;
        }

        let stats = self.get().await;

        if !force {
            if let Some(ref ts) = stats.last_sync_time {
                if let Ok(last) = chrono::DateTime::parse_from_rfc3339(ts) {
                    let elapsed = chrono::Utc::now().signed_duration_since(last);
                    if elapsed.num_seconds() < SYNC_INTERVAL_SECS as i64 {
                        return;
                    }
                }
            }
        }

        let version = app
            .config()
            .version
            .clone()
            .unwrap_or_else(|| "0.0.0".to_string());
        let platform = current_platform();
        let locale = crate::store_utils::get_str(app, "language", "en");

        let payload = serde_json::json!({
            "id": stats.installation_id,
            "platform": platform,
            "version": version,
            "locale": locale,
            "stats": {
                "total_downloads": stats.total_downloads,
                "successful_downloads": stats.successful_downloads,
                "total_size_mb": stats.total_size_mb.round() as u64,
                "first_launch": stats.first_launch,
            }
        });

        info!("Posting stats payload: {}", payload);

        let client = match crate::utils::http_client() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to create HTTP client for stats: {}", e);
                return;
            }
        };

        match client
            .post(STATS_URL)
            .header("Content-Type", "application/json")
            .header("Referer", "http://tauri.localhost")
            .json(&payload)
            .send()
            .await
        {
            Ok(res) => {
                info!("Stats posted, response: {}", res.status());
                let now = chrono::Utc::now().to_rfc3339();
                let db = self.db.clone();
                tokio::task::spawn_blocking(move || {
                    let conn = db.conn();
                    let _ = conn.execute(
                        "UPDATE stats SET last_sync_time = ?1 WHERE id = 1",
                        params![now],
                    );
                })
                .await
                .ok();
            }
            Err(e) => {
                warn!("Failed to post stats: {}", e);
            }
        }
    }

    pub async fn fetch_broadcasts(&self, app: &AppHandle) -> Vec<Broadcast> {
        let version = app
            .config()
            .version
            .clone()
            .unwrap_or_else(|| "0.0.0".to_string());
        let platform = current_platform();

        let client = match crate::utils::http_client() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to create HTTP client for broadcasts: {}", e);
                return Vec::new();
            }
        };

        let response = match client.get(BROADCAST_URL).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                warn!("Broadcast fetch returned status {}", r.status());
                return Vec::new();
            }
            Err(e) => {
                warn!("Failed to fetch broadcasts: {}", e);
                return Vec::new();
            }
        };

        let broadcasts: Vec<Broadcast> = match response.json().await {
            Ok(b) => b,
            Err(e) => {
                warn!("Failed to parse broadcasts: {}", e);
                return Vec::new();
            }
        };

        broadcasts
            .into_iter()
            .filter(|bc| {
                if let Some(ref platforms) = bc.platforms {
                    let ps: Vec<&str> = platforms.split(',').map(|p| p.trim()).collect();
                    if !ps
                        .iter()
                        .any(|p| p.eq_ignore_ascii_case("all") || p.eq_ignore_ascii_case(&platform))
                    {
                        return false;
                    }
                }
                if let Some(ref min) = bc.min_version {
                    if compare_versions(&version, min) < 0 {
                        return false;
                    }
                }
                if let Some(ref max) = bc.max_version {
                    if compare_versions(&version, max) > 0 {
                        return false;
                    }
                }
                true
            })
            .collect()
    }
}

fn current_platform() -> String {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unknown"
    }
    .to_string()
}

fn compare_versions(a: &str, b: &str) -> i32 {
    let parse = |s: &str| -> Vec<u32> { s.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let pa = parse(a);
    let pb = parse(b);
    let len = pa.len().max(pb.len());
    for i in 0..len {
        let va = pa.get(i).copied().unwrap_or(0);
        let vb = pb.get(i).copied().unwrap_or(0);
        if va > vb {
            return 1;
        }
        if va < vb {
            return -1;
        }
    }
    0
}
