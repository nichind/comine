use std::sync::Arc;

use rusqlite::params;

use crate::database::Database;
use crate::orchestrator::types::*;

pub struct JobStore {
    db: Arc<Database>,
}

impl JobStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn load_jobs(&self) -> Result<Vec<Job>, BackendError> {
        let conn = self.db.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, request_json, status, backend, created_at, retry_count, last_error, title, thumbnail, temp_files_json, progress, downloaded_bytes, total_bytes FROM jobs",
            )
            .map_err(|e| BackendError::IoError(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let request_json: String = row.get(1)?;
                let status_json: String = row.get(2)?;
                let backend: String = row.get(3)?;
                let created_at: i64 = row.get(4)?;
                let retry_count: i64 = row.get(5)?;
                let last_error: Option<String> = row.get(6)?;
                let title: Option<String> = row.get(7)?;
                let thumbnail: Option<String> = row.get(8)?;
                let temp_files_json: String = row.get(9)?;
                let progress: f64 = row.get(10)?;
                let downloaded_bytes: i64 = row.get(11)?;
                let total_bytes: Option<i64> = row.get(12)?;

                Ok(JobRow {
                    id,
                    request_json,
                    status_json,
                    backend,
                    created_at: created_at as u64,
                    retry_count: retry_count as u32,
                    last_error,
                    title,
                    thumbnail,
                    temp_files_json,
                    progress,
                    downloaded_bytes: downloaded_bytes as u64,
                    total_bytes: total_bytes.map(|v| v as u64),
                })
            })
            .map_err(|e| BackendError::IoError(e.to_string()))?;

        let mut jobs = Vec::new();
        for row in rows {
            match row {
                Ok(r) => match r.into_job() {
                    Some(job) => jobs.push(job),
                    None => tracing::warn!("Failed to deserialize job from DB"),
                },
                Err(e) => tracing::warn!("Failed to read job row: {}", e),
            }
        }

        tracing::info!("Loaded {} persisted jobs from database", jobs.len());
        Ok(jobs)
    }

    pub fn save_jobs(&self, jobs: &[Job]) -> Result<(), BackendError> {
        let db = self.db.clone();
        let persisted: Vec<PersistedRow> = jobs
            .iter()
            .filter(|j| !j.status.is_terminal())
            .map(PersistedRow::from_job)
            .collect();

        tokio::task::spawn_blocking(move || {
            Self::write_to_db(&db, &persisted);
        });

        Ok(())
    }

    pub fn save_jobs_sync(&self, jobs: &[Job]) -> Result<(), BackendError> {
        let persisted: Vec<PersistedRow> = jobs
            .iter()
            .filter(|j| !j.status.is_terminal())
            .map(PersistedRow::from_job)
            .collect();

        Self::write_to_db(&self.db, &persisted);
        Ok(())
    }

    fn write_to_db(db: &Database, jobs: &[PersistedRow]) {
        let conn = db.conn();

        // Use a transaction: delete all, then insert current state
        if let Err(e) = conn.execute("DELETE FROM jobs", []) {
            tracing::error!("Failed to clear jobs table: {}", e);
            return;
        }

        for job in jobs {
            if let Err(e) = conn.execute(
                "INSERT INTO jobs (id, request_json, status, backend, created_at, retry_count, last_error, title, thumbnail, temp_files_json, progress, downloaded_bytes, total_bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    job.id,
                    job.request_json,
                    job.status_json,
                    job.backend,
                    job.created_at as i64,
                    job.retry_count as i64,
                    job.last_error,
                    job.title,
                    job.thumbnail,
                    job.temp_files_json,
                    job.progress,
                    job.downloaded_bytes as i64,
                    job.total_bytes.map(|v| v as i64),
                ],
            ) {
                tracing::error!("Failed to persist job {}: {}", job.id, e);
            }
        }
    }
}

struct JobRow {
    id: String,
    request_json: String,
    status_json: String,
    backend: String,
    created_at: u64,
    retry_count: u32,
    last_error: Option<String>,
    title: Option<String>,
    thumbnail: Option<String>,
    temp_files_json: String,
    progress: f64,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
}

impl JobRow {
    fn into_job(self) -> Option<Job> {
        let request: DownloadRequest = serde_json::from_str(&self.request_json).ok()?;
        let status: JobStatus =
            serde_json::from_str(&self.status_json).unwrap_or(JobStatus::Queued);
        let temp_files: Vec<String> =
            serde_json::from_str(&self.temp_files_json).unwrap_or_default();

        // Treat in-flight jobs as paused on load (previous process is gone).
        let status = if matches!(status, JobStatus::Downloading | JobStatus::Resolving) {
            JobStatus::Paused
        } else {
            status
        };

        Some(Job {
            id: self.id,
            request,
            status,
            backend: self.backend,
            created_at: self.created_at,
            started_at: None,
            completed_at: None,
            progress: self.progress,
            downloaded_bytes: self.downloaded_bytes,
            total_bytes: self.total_bytes,
            speed: None,
            eta: None,
            temp_files,
            retry_count: self.retry_count,
            last_error: self.last_error,
            title: self.title,
            thumbnail: self.thumbnail,
            author: None,
            author_url: None,
            duration: None,
            playlist_id: None,
            playlist_title: None,
            playlist_index: None,
            content_type: None,
            skip_proxy: false,
        })
    }
}

struct PersistedRow {
    id: String,
    request_json: String,
    status_json: String,
    backend: String,
    created_at: u64,
    retry_count: u32,
    last_error: Option<String>,
    title: Option<String>,
    thumbnail: Option<String>,
    temp_files_json: String,
    progress: f64,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
}

impl PersistedRow {
    fn from_job(job: &Job) -> Self {
        Self {
            id: job.id.clone(),
            request_json: serde_json::to_string(&job.request).unwrap_or_else(|_| "{}".to_string()),
            status_json: serde_json::to_string(&job.status)
                .unwrap_or_else(|_| "\"queued\"".to_string()),
            backend: job.backend.clone(),
            created_at: job.created_at,
            retry_count: job.retry_count,
            last_error: job.last_error.clone(),
            title: job.title.clone(),
            thumbnail: job.thumbnail.clone(),
            temp_files_json: serde_json::to_string(&job.temp_files)
                .unwrap_or_else(|_| "[]".to_string()),
            progress: job.progress,
            downloaded_bytes: job.downloaded_bytes,
            total_bytes: job.total_bytes,
        }
    }
}
