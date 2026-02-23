use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use tracing::{error, info, warn};

use crate::orchestrator::types::HistoryItem;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new(data_dir: &Path) -> Result<Arc<Self>, String> {
        let db_path = data_dir.join("comine.db");

        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open database at {:?}: {}", db_path, e))?;

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA synchronous=NORMAL;
             PRAGMA foreign_keys=ON;",
        )
        .map_err(|e| format!("Failed to set database pragmas: {}", e))?;

        let db = Arc::new(Self {
            conn: Mutex::new(conn),
        });

        db.init_schema()?;
        db.run_migrations();
        db.migrate_from_json(data_dir);

        info!("Database initialized at {:?}", db_path);
        Ok(db)
    }

    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn init_schema(&self) -> Result<(), String> {
        let conn = self.conn();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                author TEXT NOT NULL DEFAULT '',
                author_url TEXT,
                thumbnail TEXT NOT NULL DEFAULT '',
                extension TEXT NOT NULL DEFAULT '',
                size INTEGER NOT NULL DEFAULT 0,
                duration REAL NOT NULL DEFAULT 0.0,
                file_path TEXT NOT NULL DEFAULT '',
                downloaded_at INTEGER NOT NULL DEFAULT 0,
                item_type TEXT NOT NULL DEFAULT 'video',
                playlist_id TEXT,
                playlist_title TEXT,
                playlist_index INTEGER,
                converted_format TEXT,
                download_source TEXT,
                is_favourite INTEGER NOT NULL DEFAULT 0,
                is_directory INTEGER NOT NULL DEFAULT 0,
                file_count INTEGER
            );

            CREATE INDEX IF NOT EXISTS idx_history_downloaded_at ON history(downloaded_at DESC);
            CREATE INDEX IF NOT EXISTS idx_history_item_type ON history(item_type);
            CREATE INDEX IF NOT EXISTS idx_history_is_favourite ON history(is_favourite);
            CREATE INDEX IF NOT EXISTS idx_history_playlist_id ON history(playlist_id);

            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                request_json TEXT NOT NULL,
                status TEXT NOT NULL,
                backend TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL DEFAULT 0,
                retry_count INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                title TEXT,
                thumbnail TEXT,
                temp_files_json TEXT NOT NULL DEFAULT '[]',
                progress REAL NOT NULL DEFAULT 0.0,
                downloaded_bytes INTEGER NOT NULL DEFAULT 0,
                total_bytes INTEGER
            );

            CREATE TABLE IF NOT EXISTS stats (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                total_downloads INTEGER NOT NULL DEFAULT 0,
                successful_downloads INTEGER NOT NULL DEFAULT 0,
                failed_downloads INTEGER NOT NULL DEFAULT 0,
                total_size_mb REAL NOT NULL DEFAULT 0.0,
                first_launch TEXT NOT NULL DEFAULT '',
                installation_id TEXT NOT NULL DEFAULT '',
                last_sync_time TEXT
            );

            INSERT OR IGNORE INTO stats (id) VALUES (1);",
        )
        .map_err(|e| format!("Failed to initialize database schema: {}", e))?;

        Ok(())
    }

    fn run_migrations(&self) {
        let conn = self.conn();

        // Add is_directory and file_count columns for multi-file download support
        let has_is_directory: bool = conn
            .prepare("SELECT is_directory FROM history LIMIT 0")
            .is_ok();

        if !has_is_directory {
            if let Err(e) = conn.execute_batch(
                "ALTER TABLE history ADD COLUMN is_directory INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE history ADD COLUMN file_count INTEGER;",
            ) {
                warn!(
                    "Migration: adding directory columns failed (may already exist): {}",
                    e
                );
            } else {
                info!("Migration: added is_directory and file_count columns to history");
            }
        }
    }

    fn migrate_from_json(&self, data_dir: &Path) {
        self.migrate_history_json(data_dir);
        self.migrate_jobs_json(data_dir);
        self.migrate_stats_json(data_dir);
    }

    fn migrate_history_json(&self, data_dir: &Path) {
        let json_path = data_dir.join("history_backend.json");
        if !json_path.exists() {
            return;
        }

        // Check if we already have data (avoid re-migration)
        {
            let conn = self.conn();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
                .unwrap_or(0);
            if count > 0 {
                // DB already has data, rename JSON as backup
                let backup = data_dir.join("history_backend.json.bak");
                let _ = std::fs::rename(&json_path, &backup);
                info!(
                    "History DB already populated ({} items), backed up JSON",
                    count
                );
                return;
            }
        }

        let json_str = match std::fs::read_to_string(&json_path) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to read history JSON for migration: {}", e);
                return;
            }
        };

        let items: Vec<HistoryItem> = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse history JSON for migration: {}", e);
                return;
            }
        };

        if items.is_empty() {
            let backup = data_dir.join("history_backend.json.bak");
            let _ = std::fs::rename(&json_path, &backup);
            return;
        }

        info!(
            "Migrating {} history items from JSON to SQLite",
            items.len()
        );

        let conn = self.conn();
        let tx = match conn.unchecked_transaction() {
            Ok(tx) => tx,
            Err(e) => {
                error!("Failed to start migration transaction: {}", e);
                return;
            }
        };

        for item in &items {
            if let Err(e) = tx.execute(
                "INSERT OR IGNORE INTO history (id, url, title, author, author_url, thumbnail, extension, size, duration, file_path, downloaded_at, item_type, playlist_id, playlist_title, playlist_index, converted_format, download_source, is_favourite, is_directory, file_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                params![
                    item.id, item.url, item.title, item.author,
                    item.author_url, item.thumbnail, item.extension,
                    item.size as i64, item.duration, item.file_path,
                    item.downloaded_at as i64, item.item_type,
                    item.playlist_id, item.playlist_title, item.playlist_index.map(|v| v as i64),
                    item.converted_format, item.download_source,
                    item.is_favourite as i32,
                    item.is_directory as i32,
                    item.file_count.map(|v| v as i64),
                ],
            ) {
                warn!("Failed to migrate history item {}: {}", item.id, e);
            }
        }

        if let Err(e) = tx.commit() {
            error!("Failed to commit history migration: {}", e);
            return;
        }

        let backup = data_dir.join("history_backend.json.bak");
        if let Err(e) = std::fs::rename(&json_path, &backup) {
            warn!("Failed to rename history JSON to backup: {}", e);
        } else {
            info!("History migration complete, JSON backed up to {:?}", backup);
        }
    }

    fn migrate_jobs_json(&self, data_dir: &Path) {
        let json_path = data_dir.join("jobs.json");
        if !json_path.exists() {
            return;
        }

        // Check if we already have data
        {
            let conn = self.conn();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
                .unwrap_or(0);
            if count > 0 {
                let backup = data_dir.join("jobs.json.bak");
                let _ = std::fs::rename(&json_path, &backup);
                info!("Jobs DB already populated ({} jobs), backed up JSON", count);
                return;
            }
        }

        let json_str = match std::fs::read_to_string(&json_path) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to read jobs JSON for migration: {}", e);
                return;
            }
        };

        // Use serde_json::Value for flexible parsing since PersistedJob may differ
        let jobs: Vec<serde_json::Value> = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse jobs JSON for migration: {}", e);
                return;
            }
        };

        if jobs.is_empty() {
            let backup = data_dir.join("jobs.json.bak");
            let _ = std::fs::rename(&json_path, &backup);
            return;
        }

        info!("Migrating {} jobs from JSON to SQLite", jobs.len());

        let conn = self.conn();
        let tx = match conn.unchecked_transaction() {
            Ok(tx) => tx,
            Err(e) => {
                error!("Failed to start jobs migration transaction: {}", e);
                return;
            }
        };

        for job in &jobs {
            let id = job.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            let request_json = job
                .get("request")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "{}".to_string());
            let status = job
                .get("status")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "\"queued\"".to_string());
            let backend = job
                .get("backend")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let created_at = job.get("created_at").and_then(|v| v.as_u64()).unwrap_or(0);
            let retry_count = job.get("retry_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let last_error = job.get("last_error").and_then(|v| v.as_str());
            let title = job.get("title").and_then(|v| v.as_str());
            let thumbnail = job.get("thumbnail").and_then(|v| v.as_str());
            let temp_files = job
                .get("temp_files")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "[]".to_string());
            let progress = job.get("progress").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let downloaded_bytes = job
                .get("downloaded_bytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let total_bytes = job.get("total_bytes").and_then(|v| v.as_u64());

            if let Err(e) = tx.execute(
                "INSERT OR IGNORE INTO jobs (id, request_json, status, backend, created_at, retry_count, last_error, title, thumbnail, temp_files_json, progress, downloaded_bytes, total_bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    id, request_json, status, backend,
                    created_at as i64, retry_count as i64,
                    last_error, title, thumbnail, temp_files,
                    progress, downloaded_bytes as i64,
                    total_bytes.map(|v| v as i64),
                ],
            ) {
                warn!("Failed to migrate job {}: {}", id, e);
            }
        }

        if let Err(e) = tx.commit() {
            error!("Failed to commit jobs migration: {}", e);
            return;
        }

        let backup = data_dir.join("jobs.json.bak");
        if let Err(e) = std::fs::rename(&json_path, &backup) {
            warn!("Failed to rename jobs JSON to backup: {}", e);
        } else {
            info!("Jobs migration complete, JSON backed up");
        }
    }

    fn migrate_stats_json(&self, data_dir: &Path) {
        let json_path = data_dir.join("stats.json");
        if !json_path.exists() {
            return;
        }

        // Check if stats have been populated (total_downloads > 0 or installation_id set)
        {
            let conn = self.conn();
            let has_data: bool = conn
                .query_row(
                    "SELECT total_downloads > 0 OR installation_id != '' FROM stats WHERE id = 1",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if has_data {
                let backup = data_dir.join("stats.json.bak");
                let _ = std::fs::rename(&json_path, &backup);
                info!("Stats DB already populated, backed up JSON");
                return;
            }
        }

        let json_str = match std::fs::read_to_string(&json_path) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to read stats JSON for migration: {}", e);
                return;
            }
        };

        let stats: serde_json::Value = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse stats JSON for migration: {}", e);
                return;
            }
        };

        let total_downloads = stats
            .get("total_downloads")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let successful_downloads = stats
            .get("successful_downloads")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let failed_downloads = stats
            .get("failed_downloads")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let total_size_mb = stats
            .get("total_size_mb")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let first_launch = stats
            .get("first_launch")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let installation_id = stats
            .get("installation_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let last_sync_time = stats.get("last_sync_time").and_then(|v| v.as_str());

        let conn = self.conn();
        if let Err(e) = conn.execute(
            "UPDATE stats SET total_downloads = ?1, successful_downloads = ?2, failed_downloads = ?3, total_size_mb = ?4, first_launch = ?5, installation_id = ?6, last_sync_time = ?7 WHERE id = 1",
            params![
                total_downloads as i64,
                successful_downloads as i64,
                failed_downloads as i64,
                total_size_mb,
                first_launch,
                installation_id,
                last_sync_time,
            ],
        ) {
            error!("Failed to migrate stats: {}", e);
            return;
        }

        drop(conn);

        let backup = data_dir.join("stats.json.bak");
        if let Err(e) = std::fs::rename(&json_path, &backup) {
            warn!("Failed to rename stats JSON to backup: {}", e);
        } else {
            info!("Stats migration complete, JSON backed up");
        }
    }
}
