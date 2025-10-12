//! Test utilities for the Talaria workspace
//!
//! This crate provides common test helpers, fixtures, and utilities for testing
//! across the Talaria workspace. It follows the pattern of tokio-test, providing
//! a centralized location for test infrastructure.
//!
//! # Features
//!
//! - **Test Environment**: Isolated test environments with automatic cleanup
//! - **Storage Helpers**: Test utilities for HERALD storage operations
//! - **Mock Implementations**: Mock versions of core components for testing
//! - **Fixtures**: Common test data and FASTA sequences
//! - **Assertions**: Custom assertions for bioinformatics data

pub mod assertions;
pub mod environment;
pub mod fixtures;
pub mod mock;
pub mod storage;

// Re-export commonly used items
pub use environment::{TestConfig, TestEnvironment};
pub use fixtures::{create_test_fasta, generate_sequences, TestSequence};
pub use mock::{create_test_download_state, MockAligner, MockDownloadSource, MockTaxonomyManager};
pub use storage::{StorageFixture, TestStorage};

// Re-export test dependencies for convenience
pub use anyhow::{Context, Result};
pub use tempfile;

/// Initialize test logging (call once per test module)
pub fn init_test_logging() {
    let _ = env_logger::builder().is_test(true).try_init();
}

/// Run a test with a clean environment
///
/// # Example
/// ```rust
/// use talaria_test::with_test_env;
///
/// #[test]
/// fn test_something() {
///     with_test_env(|env| {
///         // Test code here has isolated environment
///         let storage = env.create_storage()?;
///         // ...
///         Ok(())
///     }).unwrap();
/// }
/// ```
pub fn with_test_env<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&TestEnvironment) -> Result<R>,
{
    let env = TestEnvironment::new()?;
    let result = f(&env);
    // Cleanup happens automatically via Drop
    result
}

/// Run a test with a configured environment
pub fn with_configured_env<F, R>(config: TestConfig, f: F) -> Result<R>
where
    F: FnOnce(&TestEnvironment) -> Result<R>,
{
    let env = TestEnvironment::with_config(config)?;
    let result = f(&env);
    result
}
