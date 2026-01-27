//! Dependency error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DepsError {
    #[error("Download failed: {0}")]
    Download(#[from] DownloadError),

    #[error("Extraction failed: {0}")]
    Extract(#[from] ExtractError),

    #[error("Verification failed for {dep}: {reason}")]
    Verification { dep: &'static str, reason: String },

    #[error("Checksum mismatch for {dep}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        dep: &'static str,
        expected: String,
        actual: String,
    },

    #[error("Platform not supported: {0}")]
    UnsupportedPlatform(&'static str),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Path error: {0}")]
    Path(String),

    #[error("{0}")]
    Other(String),
}

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("HTTP error {status}: {message}")]
    Http { status: u16, message: String },

    #[error("Request failed: {0}")]
    Request(String),

    #[error("Rate limited (429): retry after backoff")]
    RateLimited,

    #[error("Access denied (403)")]
    AccessDenied,

    #[error("All proxy strategies failed: {0}")]
    AllStrategiesFailed(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Timeout")]
    Timeout,

    #[error("Cancelled")]
    Cancelled,

    #[error("Empty response")]
    EmptyResponse,

    #[error("Size mismatch: expected {expected} bytes, got {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
}

#[derive(Error, Debug)]
pub enum ExtractError {
    #[error("Archive not found: {0}")]
    ArchiveNotFound(String),

    #[error("Archive is empty")]
    EmptyArchive,

    #[error("Invalid archive format: {0}")]
    InvalidFormat(String),

    #[error("File not found in archive: {0}")]
    FileNotFound(String),

    #[error("IO error during extraction: {0}")]
    Io(String),
}

impl From<DepsError> for String {
    fn from(err: DepsError) -> Self {
        err.to_string()
    }
}

impl DepsError {
    pub fn verification(dep: &'static str, reason: impl Into<String>) -> Self {
        Self::Verification {
            dep,
            reason: reason.into(),
        }
    }

    pub fn checksum_mismatch(
        dep: &'static str,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::ChecksumMismatch {
            dep,
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    pub fn path(msg: impl Into<String>) -> Self {
        Self::Path(msg.into())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

impl DownloadError {
    pub fn http(status: u16, message: impl Into<String>) -> Self {
        Self::Http {
            status,
            message: message.into(),
        }
    }

    pub fn size_mismatch(expected: u64, actual: u64) -> Self {
        Self::SizeMismatch { expected, actual }
    }
}

impl ExtractError {
    pub fn archive_not_found(path: impl Into<String>) -> Self {
        Self::ArchiveNotFound(path.into())
    }

    pub fn io(msg: impl Into<String>) -> Self {
        Self::Io(msg.into())
    }
}

pub type DepsResult<T> = Result<T, DepsError>;
