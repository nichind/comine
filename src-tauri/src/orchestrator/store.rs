use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::orchestrator::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedJob {
    pub id: String,
    pub request: DownloadRequest,
    pub status: JobStatus,
    pub backend: String,
    pub created_at: u64,
    pub retry_count: u32,
    pub last_error: Option<String>,
    pub title: Option<String>,
    pub thumbnail: Option<String>,
    pub temp_files: Vec<String>,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub downloaded_bytes: u64,
    #[serde(default)]
    pub total_bytes: Option<u64>,
}

impl From<&Job> for PersistedJob {
    fn from(job: &Job) -> Self {
        Self {
            id: job.id.clone(),
            request: job.request.clone(),
            status: job.status.clone(),
            backend: job.backend.clone(),
            created_at: job.created_at,
            retry_count: job.retry_count,
            last_error: job.last_error.clone(),
            title: job.title.clone(),
            thumbnail: job.thumbnail.clone(),
            temp_files: job.temp_files.clone(),
            progress: job.progress,
            downloaded_bytes: job.downloaded_bytes,
            total_bytes: job.total_bytes,
        }
    }
}

impl PersistedJob {
    pub fn into_job(self) -> Job {
        // Treat in-flight jobs as paused on load (previous process is gone).
        let status = if matches!(self.status, JobStatus::Downloading | JobStatus::Resolving) {
            JobStatus::Paused
        } else {
            self.status
        };

        Job {
            id: self.id,
            request: self.request,
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
            temp_files: self.temp_files,
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
        }
    }
}

pub struct JobStore {
    path: PathBuf,
}

impl JobStore {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let path = app_data_dir.join("jobs.json");
        Self { path }
    }

    pub fn load_jobs(&self) -> Result<Vec<Job>, BackendError> {
        if !self.path.exists() {
            tracing::info!("No jobs.json found at {:?}, starting fresh", self.path);
            return Ok(Vec::new());
        }
        let contents = std::fs::read_to_string(&self.path)
            .map_err(|e| BackendError::IoError(e.to_string()))?;
        let persisted: Vec<PersistedJob> = match serde_json::from_str(&contents) {
            Ok(jobs) => jobs,
            Err(e) => {
                tracing::warn!("Failed to deserialize jobs.json, starting fresh: {}", e);
                Vec::new()
            }
        };
        tracing::info!(
            "Loaded {} persisted jobs from {:?}",
            persisted.len(),
            self.path
        );
        Ok(persisted.into_iter().map(|p| p.into_job()).collect())
    }

    pub fn save_jobs(&self, jobs: &[Job]) -> Result<(), BackendError> {
        let json = Self::serialize_jobs(jobs)?;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || Self::write_atomic(&path, &json));
        Ok(())
    }

    pub fn save_jobs_sync(&self, jobs: &[Job]) -> Result<(), BackendError> {
        let json = Self::serialize_jobs(jobs)?;
        Self::write_atomic(&self.path, &json);
        Ok(())
    }

    fn serialize_jobs(jobs: &[Job]) -> Result<String, BackendError> {
        let persisted: Vec<PersistedJob> = jobs
            .iter()
            .filter(|j| !j.status.is_terminal())
            .map(PersistedJob::from)
            .collect();
        serde_json::to_string_pretty(&persisted).map_err(|e| BackendError::IoError(e.to_string()))
    }

    fn write_atomic(path: &std::path::Path, json: &str) {
        use std::io::Write;

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp_path = path.with_extension("json.tmp");

        let result = (|| -> std::io::Result<()> {
            let mut file = std::fs::File::create(&tmp_path)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?; // fsync before rename to ensure durability
            Ok(())
        })();

        if let Err(e) = result {
            tracing::error!(error = %e, "Failed to write jobs.json.tmp");
            return;
        }

        if let Err(e) = std::fs::rename(&tmp_path, path) {
            tracing::error!(error = %e, "Failed to rename jobs.json.tmp -> jobs.json");
            if let Err(e2) = std::fs::write(path, json) {
                tracing::error!(error = %e2, "Fallback direct write also failed");
            }
        } else {
            tracing::debug!("Persisted jobs to jobs.json");
        }
    }
}
