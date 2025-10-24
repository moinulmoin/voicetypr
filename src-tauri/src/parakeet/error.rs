use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParakeetError {
    #[error("failed to spawn parakeet sidecar: {0}")]
    SpawnError(String),
    #[error("sidecar io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("failed to encode command: {0}")]
    EncodeError(#[from] serde_json::Error),
    #[error("sidecar returned error: {code} - {message}")]
    SidecarError { code: String, message: String },
    #[error("sidecar terminated unexpectedly")]
    Terminated,
    #[error("invalid transcription response payload")]
    InvalidResponse,
    #[error("{0}")]
    Unavailable(String),
}
