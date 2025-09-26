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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_error_display() {
        // Test each error variant's display
        let io_error = TalariaError::Io(io::Error::new(io::ErrorKind::NotFound, "file not found"));
        assert!(format!("{}", io_error).contains("IO error"));

        let ser_error = TalariaError::Serialization("invalid JSON".to_string());
        assert_eq!(format!("{}", ser_error), "Serialization error: invalid JSON");

        let config_error = TalariaError::Configuration("missing field".to_string());
        assert_eq!(format!("{}", config_error), "Configuration error: missing field");

        let path_error = TalariaError::Path("invalid path".to_string());
        assert_eq!(format!("{}", path_error), "Path error: invalid path");

        let version_error = TalariaError::Version("version mismatch".to_string());
        assert_eq!(format!("{}", version_error), "Version error: version mismatch");

        let storage_error = TalariaError::Storage("disk full".to_string());
        assert_eq!(format!("{}", storage_error), "Storage error: disk full");

        let database_error = TalariaError::Database("connection failed".to_string());
        assert_eq!(format!("{}", database_error), "Database error: connection failed");

        let network_error = TalariaError::Network("timeout".to_string());
        assert_eq!(format!("{}", network_error), "Network error: timeout");

        let parse_error = TalariaError::Parse("invalid syntax".to_string());
        assert_eq!(format!("{}", parse_error), "Parsing error: invalid syntax");

        let input_error = TalariaError::InvalidInput("negative value".to_string());
        assert_eq!(format!("{}", input_error), "Invalid input: negative value");

        let not_found = TalariaError::NotFound("resource".to_string());
        assert_eq!(format!("{}", not_found), "Not found: resource");

        let exists = TalariaError::AlreadyExists("file.txt".to_string());
        assert_eq!(format!("{}", exists), "Already exists: file.txt");

        let cancelled = TalariaError::Cancelled;
        assert_eq!(format!("{}", cancelled), "Operation cancelled");

        let other = TalariaError::Other("unknown".to_string());
        assert_eq!(format!("{}", other), "Other error: unknown");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "access denied");
        let talaria_err: TalariaError = io_err.into();

        match talaria_err {
            TalariaError::Io(e) => {
                assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
            }
            _ => panic!("Expected Io error variant"),
        }
    }

    #[test]
    fn test_serde_json_error_conversion() {
        let json_str = "{invalid json}";
        let parse_result: Result<serde_json::Value, serde_json::Error> = serde_json::from_str(json_str);

        assert!(parse_result.is_err());
        let talaria_err: TalariaError = parse_result.unwrap_err().into();

        match talaria_err {
            TalariaError::Serialization(msg) => {
                assert!(msg.contains("key must be a string"));
            }
            _ => panic!("Expected Serialization error variant"),
        }
    }

    #[test]
    fn test_anyhow_error_conversion() {
        let anyhow_err = anyhow::anyhow!("custom error message");
        let talaria_err: TalariaError = anyhow_err.into();

        match talaria_err {
            TalariaError::Other(msg) => {
                assert_eq!(msg, "custom error message");
            }
            _ => panic!("Expected Other error variant"),
        }
    }

    #[test]
    fn test_error_result_type() {
        fn returns_ok() -> TalariaResult<String> {
            Ok("success".to_string())
        }

        fn returns_err() -> TalariaResult<String> {
            Err(TalariaError::NotFound("item".to_string()))
        }

        assert!(returns_ok().is_ok());
        assert_eq!(returns_ok().unwrap(), "success");

        assert!(returns_err().is_err());
        match returns_err().unwrap_err() {
            TalariaError::NotFound(msg) => assert_eq!(msg, "item"),
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn test_error_chaining() {
        // Test that errors can be chained through conversions
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file.txt");
        let talaria_err: TalariaError = io_err.into();

        // Can use in Result context
        let result: TalariaResult<()> = Err(talaria_err);
        assert!(result.is_err());
    }

    #[test]
    fn test_error_debug_format() {
        let error = TalariaError::Configuration("test config error".to_string());
        let debug_str = format!("{:?}", error);

        assert!(debug_str.contains("Configuration"));
        assert!(debug_str.contains("test config error"));
    }

    #[test]
    fn test_error_equality() {
        // Test that errors implement Debug trait properly
        let err1 = TalariaError::NotFound("file1.txt".to_string());
        let err2 = TalariaError::NotFound("file1.txt".to_string());
        let err3 = TalariaError::NotFound("file2.txt".to_string());

        // Same error type and message should format the same
        assert_eq!(format!("{:?}", err1), format!("{:?}", err2));
        assert_ne!(format!("{:?}", err1), format!("{:?}", err3));
    }

    #[test]
    fn test_error_source_chain() {
        // Test that IO errors preserve their source
        let io_err = io::Error::new(io::ErrorKind::Other, "underlying cause");
        let talaria_err: TalariaError = io_err.into();

        // The error should be properly wrapped
        match talaria_err {
            TalariaError::Io(ref e) => {
                assert_eq!(e.kind(), io::ErrorKind::Other);
                assert!(e.to_string().contains("underlying cause"));
            }
            _ => panic!("Expected Io variant"),
        }
    }

    #[test]
    fn test_error_is_type_checking() {
        let not_found = TalariaError::NotFound("resource".to_string());
        let io_err = TalariaError::Io(io::Error::new(io::ErrorKind::NotFound, "file"));
        let cancelled = TalariaError::Cancelled;

        // Helper functions to check error types
        fn is_not_found(err: &TalariaError) -> bool {
            matches!(err, TalariaError::NotFound(_))
        }

        fn is_io_error(err: &TalariaError) -> bool {
            matches!(err, TalariaError::Io(_))
        }

        fn is_cancelled(err: &TalariaError) -> bool {
            matches!(err, TalariaError::Cancelled)
        }

        assert!(is_not_found(&not_found));
        assert!(!is_not_found(&io_err));
        assert!(is_io_error(&io_err));
        assert!(is_cancelled(&cancelled));
    }
}