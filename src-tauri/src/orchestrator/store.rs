use std::path::PathBuf;
use std::sync::RwLock;

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
            post_process_index: 0,
        }
    }
}

pub struct JobStore {
    path: PathBuf,
    cache: RwLock<Vec<PersistedJob>>,
}

impl JobStore {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let path = app_data_dir.join("jobs.json");

        let cache = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to read queue.json");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        Self {
            path,
            cache: RwLock::new(cache),
        }
    }

    pub fn load_jobs(&self) -> Result<Vec<Job>, BackendError> {
        let cache = self
            .cache
            .read()
            .map_err(|e| BackendError::Other(e.to_string()))?;
        Ok(cache.iter().cloned().map(|p| p.into_job()).collect())
    }

    pub fn save_jobs(&self, jobs: &[Job]) -> Result<(), BackendError> {
        let persisted: Vec<PersistedJob> = jobs
            .iter()
            .filter(|j| !j.status.is_terminal())
            .map(PersistedJob::from)
            .collect();

        {
            let mut cache = self
                .cache
                .write()
                .map_err(|e| BackendError::Other(e.to_string()))?;
            *cache = persisted.clone();
        }

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BackendError::IoError(e.to_string()))?;
        }

        let json = serde_json::to_string_pretty(&persisted)
            .map_err(|e| BackendError::IoError(e.to_string()))?;

        std::fs::write(&self.path, json).map_err(|e| BackendError::IoError(e.to_string()))?;

        tracing::debug!(count = persisted.len(), "Persisted jobs to queue.json");
        Ok(())
    }
}
