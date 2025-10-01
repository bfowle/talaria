//! Verification and validation error types

use crate::types::SHA256Hash;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Error that occurs during verification of data integrity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationError {
    /// The hash of the chunk that failed verification
    pub chunk_hash: SHA256Hash,
    /// The specific type of error
    pub error_type: VerificationErrorType,
    /// Optional context about where the error occurred
    pub context: Option<String>,
}

/// Specific types of verification errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationErrorType {
    /// Hash mismatch between expected and actual
    HashMismatch {
        expected: SHA256Hash,
        actual: SHA256Hash,
    },
    /// Error reading data
    ReadError(String),
    /// Data is corrupted
    CorruptedData(String),
    /// Missing data
    MissingData(String),
    /// Invalid format
    InvalidFormat(String),
    /// Size mismatch
    SizeMismatch { expected: usize, actual: usize },
}

impl fmt::Display for VerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Verification error for chunk {}: ", self.chunk_hash)?;
        match &self.error_type {
            VerificationErrorType::HashMismatch { expected, actual } => {
                write!(
                    f,
                    "Hash mismatch (expected: {}, actual: {})",
                    expected, actual
                )
            }
            VerificationErrorType::ReadError(e) => write!(f, "Read error: {}", e),
            VerificationErrorType::CorruptedData(e) => write!(f, "Corrupted data: {}", e),
            VerificationErrorType::MissingData(e) => write!(f, "Missing data: {}", e),
            VerificationErrorType::InvalidFormat(e) => write!(f, "Invalid format: {}", e),
            VerificationErrorType::SizeMismatch { expected, actual } => {
                write!(
                    f,
                    "Size mismatch (expected: {}, actual: {})",
                    expected, actual
                )
            }
        }?;
        if let Some(ctx) = &self.context {
            write!(f, " [{}]", ctx)?;
        }
        Ok(())
    }
}

impl std::error::Error for VerificationError {}

impl VerificationError {
    /// Create a new verification error
    pub fn new(chunk_hash: SHA256Hash, error_type: VerificationErrorType) -> Self {
        Self {
            chunk_hash,
            error_type,
            context: None,
        }
    }

    /// Create a verification error with context
    pub fn with_context(
        chunk_hash: SHA256Hash,
        error_type: VerificationErrorType,
        context: impl Into<String>,
    ) -> Self {
        Self {
            chunk_hash,
            error_type,
            context: Some(context.into()),
        }
    }
}
