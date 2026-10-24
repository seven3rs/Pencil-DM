use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("invalid engine manifest: {0}")]
    Manifest(String),
    #[error("integrity check failed for {what}: expected {expected}, got {actual}")]
    HashMismatch {
        what: String,
        expected: String,
        actual: String,
    },
    #[error("unsafe archive entry: {0}")]
    UnsafeArchive(String),
    #[error("archive error: {0}")]
    Archive(#[from] zip::result::ZipError),
    #[error("size limit exceeded ({0} bytes)")]
    TooLarge(u64),
    #[error("refusing non-https url: {0}")]
    InsecureUrl(String),
    #[error("download engine not found on this system")]
    NotFound,
    #[error("download engine failed to start: {0}")]
    Start(String),
    #[error("engine error: {0}")]
    Rpc(String),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, EngineError>;
