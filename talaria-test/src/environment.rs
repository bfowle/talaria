//! Test environment management
//!
//! Provides isolated test environments with automatic cleanup using RAII.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tempfile::TempDir;
use anyhow::{Result, Context};
use once_cell::sync::Lazy;

/// Global registry to prevent environment variable conflicts
static ENV_REGISTRY: Lazy<Arc<Mutex<HashMap<String, String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Configuration for test environment
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Preserve workspace after test (for debugging)
    pub preserve_on_failure: bool,
    /// Enable verbose logging
    pub verbose: bool,
    /// Number of threads for parallel operations
    pub threads: Option<usize>,
    /// Custom prefix for test directories
    pub prefix: Option<String>,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            preserve_on_failure: false,
            verbose: false,
            threads: Some(1), // Single-threaded by default for determinism
            prefix: None,
        }
    }
}

/// Isolated test environment with automatic cleanup
pub struct TestEnvironment {
    /// Root temporary directory
    temp_dir: Option<TempDir>,
    /// Path to the test environment root
    root_path: PathBuf,
    /// Saved environment variables for restoration
    saved_env: HashMap<String, Option<String>>,
    /// Configuration
    config: TestConfig,
    /// Whether test failed (for preserve_on_failure)
    failed: Arc<Mutex<bool>>,
}

impl TestEnvironment {
    /// Create a new test environment with default config
    pub fn new() -> Result<Self> {
        Self::with_config(TestConfig::default())
    }

    /// Create a new test environment with custom config
    pub fn with_config(config: TestConfig) -> Result<Self> {
        // Create temporary directory
        let prefix = config.prefix.as_deref().unwrap_or("talaria-test");
        let temp_dir = TempDir::with_prefix(prefix)
            .context("Failed to create temporary directory")?;

        let root_path = temp_dir.path().to_path_buf();

        // Create standard subdirectories
        std::fs::create_dir_all(root_path.join("databases"))?;
        std::fs::create_dir_all(root_path.join("sequences"))?;
        std::fs::create_dir_all(root_path.join("cache"))?;
        std::fs::create_dir_all(root_path.join("tools"))?;
        std::fs::create_dir_all(root_path.join("workspace"))?;

        let mut env = Self {
            temp_dir: Some(temp_dir),
            root_path: root_path.clone(),
            saved_env: HashMap::new(),
            config,
            failed: Arc::new(Mutex::new(false)),
        };

        // Set up environment variables
        env.setup_environment()?;

        Ok(env)
    }

    /// Set up isolated environment variables
    fn setup_environment(&mut self) -> Result<()> {
        // Create owned strings for paths
        let home_path = self.root_path.to_string_lossy().to_string();
        let databases_path = self.root_path.join("databases").to_string_lossy().to_string();
        let cache_path = self.root_path.join("cache").to_string_lossy().to_string();
        let tools_path = self.root_path.join("tools").to_string_lossy().to_string();
        let workspace_path = self.root_path.join("workspace").to_string_lossy().to_string();

        let vars = vec![
            ("TALARIA_HOME", home_path.clone()),
            ("TALARIA_DATA_DIR", home_path),
            ("TALARIA_DATABASES_DIR", databases_path),
            ("TALARIA_CACHE_DIR", cache_path),
            ("TALARIA_TOOLS_DIR", tools_path),
            ("TALARIA_WORKSPACE_DIR", workspace_path),
        ];

        // Save and set environment variables
        for (key, value) in &vars {
            self.saved_env.insert(
                key.to_string(),
                std::env::var(key).ok()
            );
            std::env::set_var(key, value);
        }

        // Set test-specific variables
        if self.config.verbose {
            std::env::set_var("TALARIA_LOG", "debug");
        }

        if let Some(threads) = self.config.threads {
            std::env::set_var("TALARIA_THREADS", threads.to_string());
        }

        // Register this environment to prevent conflicts
        let mut registry = ENV_REGISTRY.lock().unwrap();
        registry.insert(self.root_path.to_string_lossy().to_string(), "active".to_string());

        Ok(())
    }

    /// Get the root path of the test environment
    pub fn root(&self) -> &Path {
        &self.root_path
    }

    /// Get path to databases directory
    pub fn databases_dir(&self) -> PathBuf {
        self.root_path.join("databases")
    }

    /// Get path to sequences directory
    pub fn sequences_dir(&self) -> PathBuf {
        self.root_path.join("sequences")
    }

    /// Get path to cache directory
    pub fn cache_dir(&self) -> PathBuf {
        self.root_path.join("cache")
    }

    /// Get path to workspace directory
    pub fn workspace_dir(&self) -> PathBuf {
        self.root_path.join("workspace")
    }

    /// Create a subdirectory in the test environment
    pub fn create_dir(&self, name: &str) -> Result<PathBuf> {
        let path = self.root_path.join(name);
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }

    /// Write a file in the test environment
    pub fn write_file(&self, path: impl AsRef<Path>, content: &[u8]) -> Result<()> {
        let full_path = self.root_path.join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(full_path, content)?;
        Ok(())
    }

    /// Read a file from the test environment
    pub fn read_file(&self, path: impl AsRef<Path>) -> Result<Vec<u8>> {
        let full_path = self.root_path.join(path);
        Ok(std::fs::read(full_path)?)
    }

    /// Mark test as failed (prevents cleanup if preserve_on_failure is set)
    pub fn mark_failed(&self) {
        *self.failed.lock().unwrap() = true;
    }

    /// Check if test failed
    pub fn is_failed(&self) -> bool {
        *self.failed.lock().unwrap()
    }

    /// Manually preserve the environment (for debugging)
    pub fn preserve(&mut self) {
        if let Some(temp_dir) = self.temp_dir.take() {
            let path = temp_dir.keep();
            println!("Test environment preserved at: {}", path.display());
        }
    }
}

impl Drop for TestEnvironment {
    fn drop(&mut self) {
        // Restore environment variables
        for (key, value) in &self.saved_env {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }

        // Unregister from global registry
        let mut registry = ENV_REGISTRY.lock().unwrap();
        registry.remove(&self.root_path.to_string_lossy().to_string());

        // Handle cleanup based on config and test result
        if self.config.preserve_on_failure && self.is_failed() {
            if let Some(temp_dir) = self.temp_dir.take() {
                let path = temp_dir.keep();
                eprintln!("Test failed - environment preserved at: {}", path.display());
            }
        }
        // Otherwise, temp_dir is automatically cleaned up when dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_creation() {
        let env = TestEnvironment::new().unwrap();
        assert!(env.root().exists());
        assert!(env.databases_dir().exists());
        assert!(env.sequences_dir().exists());
    }

    #[test]
    fn test_environment_isolation() {
        let env1 = TestEnvironment::new().unwrap();
        let env2 = TestEnvironment::new().unwrap();

        assert_ne!(env1.root(), env2.root());

        // Write to env1
        env1.write_file("test.txt", b"env1").unwrap();

        // Should not exist in env2
        assert!(!env2.root().join("test.txt").exists());
    }

    #[test]
    fn test_environment_cleanup() {
        let path = {
            let env = TestEnvironment::new().unwrap();
            let path = env.root().to_path_buf();
            assert!(path.exists());
            path
        }; // env dropped here

        // Directory should be cleaned up
        assert!(!path.exists());
    }
}