use serde::{Deserialize, Serialize};

/// Errors an engine can return. The variants are coarse on purpose so that every
/// front-end can show the same states (no key / quota / network ...).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum EngineError {
    #[error("no API key configured")]
    MissingKey,
    #[error("API key was rejected")]
    InvalidKey,
    #[error("quota exceeded")]
    QuotaExceeded,
    #[error("too many requests, try again later")]
    RateLimited,
    #[error("network error: {0}")]
    Network(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("service error: {0}")]
    Service(String),
    #[error("cancelled")]
    Cancelled,
}
