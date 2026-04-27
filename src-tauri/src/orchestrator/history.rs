use std::sync::Arc;

use rusqlite::params;
use tracing::{error, warn};

use crate::database::Database;
use crate::orchestrator::types::{HistoryItem, HistoryStats};

pub struct HistoryStore {
    db: Arc<Database>,
}

impl HistoryStore {
    pub fn new(db: Arc<Database>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    pub async fn add(&self, item: HistoryItem) -> HistoryItem {
        let db = self.db.clone();
        let item_clone = item.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            if let Err(e) = conn.execute(
                "INSERT OR REPLACE INTO history (id, url, title, author, author_url, thumbnail, extension, size, duration, file_path, downloaded_at, item_type, playlist_id, playlist_title, playlist_index, converted_format, download_source, is_favourite, is_directory, file_count, podcast_path, podcast_subtitle_path, podcast_status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
                params![
                    item_clone.id, item_clone.url, item_clone.title, item_clone.author,
                    item_clone.author_url, item_clone.thumbnail, item_clone.extension,
                    item_clone.size as i64, item_clone.duration, item_clone.file_path,
                    item_clone.downloaded_at as i64, item_clone.item_type,
                    item_clone.playlist_id, item_clone.playlist_title,
                    item_clone.playlist_index.map(|v| v as i64),
                    item_clone.converted_format, item_clone.download_source,
                    item_clone.is_favourite as i32,
                    item_clone.is_directory as i32,
                    item_clone.file_count.map(|v| v as i64),
                    item_clone.podcast_path,
                    item_clone.podcast_subtitle_path,
                    item_clone.podcast_status,
                ],
            ) {
                error!("Failed to insert history item: {}", e);
            }
        })
        .await
        .ok();
        item
    }

    pub async fn get_all(&self) -> Vec<HistoryItem> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || Self::get_all_sync(&db))
            .await
            .unwrap_or_default()
    }

    pub fn get_all_blocking(&self) -> Vec<HistoryItem> {
        Self::get_all_sync(&self.db)
    }

    fn get_all_sync(db: &Database) -> Vec<HistoryItem> {
        let conn = db.conn();
        let mut stmt = match conn.prepare(
            "SELECT id, url, title, author, author_url, thumbnail, extension, size, duration, file_path, downloaded_at, item_type, playlist_id, playlist_title, playlist_index, converted_format, download_source, is_favourite, is_directory, file_count, podcast_path, podcast_subtitle_path, podcast_status
             FROM history ORDER BY downloaded_at DESC",
        ) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to prepare history query: {}", e);
                return Vec::new();
            }
        };

        let rows = stmt.query_map([], |row| {
            Ok(HistoryItem {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                author: row.get(3)?,
                author_url: row.get(4)?,
                thumbnail: row.get(5)?,
                extension: row.get(6)?,
                size: row.get::<_, i64>(7)? as u64,
                duration: row.get(8)?,
                file_path: row.get(9)?,
                downloaded_at: row.get::<_, i64>(10)? as u64,
                item_type: row.get(11)?,
                playlist_id: row.get(12)?,
                playlist_title: row.get(13)?,
                playlist_index: row.get::<_, Option<i64>>(14)?.map(|v| v as u32),
                converted_format: row.get(15)?,
                download_source: row.get(16)?,
                is_favourite: row.get::<_, i32>(17)? != 0,
                is_directory: row.get::<_, i32>(18)? != 0,
                file_count: row.get::<_, Option<i64>>(19)?.map(|v| v as u32),
                podcast_path: row.get(20)?,
                podcast_subtitle_path: row.get(21)?,
                podcast_status: row.get(22)?,
            })
        });

        match rows {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                error!("Failed to query history: {}", e);
                Vec::new()
            }
        }
    }

    pub async fn remove(&self, id: &str) -> bool {
        let db = self.db.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            conn.execute("DELETE FROM history WHERE id = ?1", params![id])
                .map(|n| n > 0)
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }

    pub async fn clear(&self) {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            if let Err(e) = conn.execute("DELETE FROM history", []) {
                error!("Failed to clear history: {}", e);
            }
        })
        .await
        .ok();
    }

    pub async fn toggle_favourite(&self, id: &str) -> Option<bool> {
        let db = self.db.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            // Toggle and return new value
            conn.execute(
                "UPDATE history SET is_favourite = CASE WHEN is_favourite = 0 THEN 1 ELSE 0 END WHERE id = ?1",
                params![id],
            ).ok()?;
            conn.query_row(
                "SELECT is_favourite FROM history WHERE id = ?1",
                params![id],
                |row| row.get::<_, i32>(0).map(|v| v != 0),
            ).ok()
        })
        .await
        .ok()
        .flatten()
    }

    pub async fn set_favourite(&self, ids: &[String], value: bool) {
        let db = self.db.clone();
        let ids = ids.to_vec();
        let val = value as i32;
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            for id in &ids {
                if let Err(e) = conn.execute(
                    "UPDATE history SET is_favourite = ?1 WHERE id = ?2",
                    params![val, id],
                ) {
                    warn!("Failed to set favourite for {}: {}", id, e);
                }
            }
        })
        .await
        .ok();
    }

    pub async fn update_podcast_status(
        &self,
        id: &str,
        status: &str,
        podcast_path: Option<&str>,
        podcast_subtitle_path: Option<&str>,
    ) {
        let db = self.db.clone();
        let id = id.to_string();
        let status = status.to_string();
        let podcast_path = podcast_path.map(|s| s.to_string());
        let podcast_subtitle_path = podcast_subtitle_path.map(|s| s.to_string());
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            if let Err(e) = conn.execute(
                "UPDATE history SET podcast_status = ?1, podcast_path = ?2, podcast_subtitle_path = ?3 WHERE id = ?4",
                params![status, podcast_path, podcast_subtitle_path, id],
            ) {
                warn!("Failed to update podcast status for {}: {}", id, e);
            }
        })
        .await
        .ok();
    }

    pub async fn get_by_id(&self, id: &str) -> Option<crate::orchestrator::types::HistoryItem> {
        let db = self.db.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            conn.query_row(
                "SELECT id, url, title, author, author_url, thumbnail, extension, size, duration, file_path, downloaded_at, item_type, playlist_id, playlist_title, playlist_index, converted_format, download_source, is_favourite, is_directory, file_count, podcast_path, podcast_subtitle_path, podcast_status
                 FROM history WHERE id = ?1",
                params![id],
                |row| {
                    Ok(crate::orchestrator::types::HistoryItem {
                        id: row.get(0)?,
                        url: row.get(1)?,
                        title: row.get(2)?,
                        author: row.get(3)?,
                        author_url: row.get(4)?,
                        thumbnail: row.get(5)?,
                        extension: row.get(6)?,
                        size: row.get::<_, i64>(7)? as u64,
                        duration: row.get(8)?,
                        file_path: row.get(9)?,
                        downloaded_at: row.get::<_, i64>(10)? as u64,
                        item_type: row.get(11)?,
                        playlist_id: row.get(12)?,
                        playlist_title: row.get(13)?,
                        playlist_index: row.get::<_, Option<i64>>(14)?.map(|v| v as u32),
                        converted_format: row.get(15)?,
                        download_source: row.get(16)?,
                        is_favourite: row.get::<_, i32>(17)? != 0,
                        is_directory: row.get::<_, i32>(18)? != 0,
                        file_count: row.get::<_, Option<i64>>(19)?.map(|v| v as u32),
                        podcast_path: row.get(20)?,
                        podcast_subtitle_path: row.get(21)?,
                        podcast_status: row.get(22)?,
                    })
                },
            )
            .ok()
        })
        .await
        .ok()
        .flatten()
    }

    pub async fn update_duration(&self, id: &str, duration: f64) {
        let db = self.db.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            if let Err(e) = conn.execute(
                "UPDATE history SET duration = ?1 WHERE id = ?2",
                params![duration, id],
            ) {
                warn!("Failed to update duration for {}: {}", id, e);
            }
        })
        .await
        .ok();
    }

    pub async fn import(&self, new_items: Vec<HistoryItem>) -> usize {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            let mut added = 0usize;
            for item in &new_items {
                match conn.execute(
                    "INSERT OR IGNORE INTO history (id, url, title, author, author_url, thumbnail, extension, size, duration, file_path, downloaded_at, item_type, playlist_id, playlist_title, playlist_index, converted_format, download_source, is_favourite, is_directory, file_count, podcast_path, podcast_subtitle_path, podcast_status)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
                    params![
                        item.id, item.url, item.title, item.author,
                        item.author_url, item.thumbnail, item.extension,
                        item.size as i64, item.duration, item.file_path,
                        item.downloaded_at as i64, item.item_type,
                        item.playlist_id, item.playlist_title,
                        item.playlist_index.map(|v| v as i64),
                        item.converted_format, item.download_source,
                        item.is_favourite as i32,
                        item.is_directory as i32,
                        item.file_count.map(|v| v as i64),
                        item.podcast_path,
                        item.podcast_subtitle_path,
                        item.podcast_status,
                    ],
                ) {
                    Ok(n) => added += n,
                    Err(e) => warn!("Failed to import history item {}: {}", item.id, e),
                }
            }
            added
        })
        .await
        .unwrap_or(0)
    }

    pub async fn export(&self) -> String {
        let items = self.get_all().await;
        serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_string())
    }

    pub async fn restore_from_frontend(&self, new_items: Vec<HistoryItem>) {
        self.import(new_items).await;
    }

    pub async fn compute_stats(&self) -> HistoryStats {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let conn = db.conn();
            let mut stats = HistoryStats::default();

            // Aggregate stats
            if let Ok(row) = conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(size), 0), COALESCE(SUM(duration), 0.0), COALESCE(SUM(CASE WHEN is_favourite != 0 THEN 1 ELSE 0 END), 0) FROM history",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            ) {
                stats.total_downloads = row.0 as u64;
                stats.total_size = row.1 as u64;
                stats.total_duration = row.2;
                stats.favourites_count = row.3 as u64;
            }

            // Format counts
            if let Ok(mut stmt) = conn.prepare(
                "SELECT CASE WHEN extension = '' THEN 'unknown' ELSE extension END, COUNT(*) FROM history GROUP BY 1",
            ) {
                if let Ok(rows) = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                }) {
                    for row in rows.flatten() {
                        stats.format_counts.insert(row.0, row.1 as u64);
                    }
                }
            }

            stats
        })
        .await
        .unwrap_or_default()
    }
}
