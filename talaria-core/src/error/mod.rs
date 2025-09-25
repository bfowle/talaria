//! Core error types for Talaria

pub mod verification;

use thiserror::Error;
pub use verification::{VerificationError, VerificationErrorType};

/// Main error type for Talaria operations
#[derive(Error, Debug)]
pub enum TalariaError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Path error: {0}")]
    Path(String),

    #[error("Version error: {0}")]
    Version(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Parsing error: {0}")]
    Parse(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Other error: {0}")]
    Other(String),
}

/// Result type alias for Talaria operations
pub type TalariaResult<T> = Result<T, TalariaError>;

// Conversion implementations for common error types
impl From<serde_json::Error> for TalariaError {
    fn from(err: serde_json::Error) -> Self {
        TalariaError::Serialization(err.to_string())
    }
}

impl From<anyhow::Error> for TalariaError {
    fn from(err: anyhow::Error) -> Self {
        TalariaError::Other(err.to_string())
    }
}