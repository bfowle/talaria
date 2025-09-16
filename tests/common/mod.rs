/// Common test utilities for Talaria tests
///
/// This module provides shared test setup and utilities to avoid duplicating
/// test-specific code in the main source files.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use talaria::utils::temp_workspace::{TempWorkspace, WorkspaceConfig};

/// Test environment that manages temporary directories and cleanup
pub struct TestEnvironment {
    temp_dir: TempDir,
    pub talaria_home: PathBuf,
}

impl TestEnvironment {
    /// Create a new test environment with a unique temporary directory
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let talaria_home = temp_dir.path().join("talaria_test");
        std::fs::create_dir_all(&talaria_home).expect("Failed to create test home");

        TestEnvironment {
            temp_dir,
            talaria_home,
        }
    }

    /// Get a path within the test environment
    pub fn path(&self, relative: &str) -> PathBuf {
        self.talaria_home.join(relative)
    }
}

/// Create a test workspace with explicit configuration
/// This avoids relying on environment variables which can cause issues with OnceLock
pub fn create_test_workspace(name: &str) -> Result<TempWorkspace, Box<dyn std::error::Error>> {
    // Create a temporary directory that will persist for the test duration
    let temp_dir = tempfile::tempdir()?;
    let config = WorkspaceConfig {
        casg_root: temp_dir.path().join("casg"),
        preserve_on_failure: false,
        preserve_always: false,
        max_age_seconds: 86400,
    };

    Ok(TempWorkspace::with_config(name, config)?)
}

/// Create a test workspace wrapped in Arc<Mutex> for thread-safe access
pub fn create_shared_test_workspace(name: &str) -> Result<Arc<Mutex<TempWorkspace>>, Box<dyn std::error::Error>> {
    let workspace = create_test_workspace(name)?;
    Ok(Arc::new(Mutex::new(workspace)))
}

/// Create a workspace config for testing with a specific base directory
#[allow(dead_code)]
pub fn test_workspace_config(base_dir: &Path) -> WorkspaceConfig {
    WorkspaceConfig {
        casg_root: base_dir.join("casg"),
        preserve_on_failure: false,
        preserve_always: false,
        max_age_seconds: 86400,
    }
}

/// Setup a complete test environment with workspace
#[allow(dead_code)]
pub fn setup_test_with_workspace(test_name: &str) -> Result<(TestEnvironment, TempWorkspace), Box<dyn std::error::Error>> {
    let env = TestEnvironment::new();
    let config = test_workspace_config(&env.talaria_home);
    let workspace = TempWorkspace::with_config(test_name, config)?;
    Ok((env, workspace))
}

// Tests for the common module are in tests/common_test.rs to avoid
// running them with every integration test that uses this module