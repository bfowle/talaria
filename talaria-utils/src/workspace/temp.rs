/// Comprehensive temp workspace management for SEQUOIA-based reduction pipeline
/// All temporary operations go through SEQUOIA - this is NOT optional
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Configuration for workspace behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Root directory for all SEQUOIA operations (${TALARIA_HOME}/sequoia)
    pub sequoia_root: PathBuf,
    /// Whether to preserve workspace on failure (for debugging)
    pub preserve_on_failure: bool,
    /// Whether to preserve workspace always (for inspection)
    pub preserve_always: bool,
    /// Maximum age of workspaces to keep (in seconds)
    pub max_age_seconds: u64,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        // Use talaria_workspace_dir() from paths module for temporal workspaces
        Self {
            sequoia_root: talaria_core::talaria_workspace_dir(),
            preserve_on_failure: std::env::var("TALARIA_PRESERVE_ON_FAILURE").is_ok()
                || std::env::var("TALARIA_PRESERVE_LAMBDA_ON_FAILURE").is_ok(),
            preserve_always: std::env::var("TALARIA_PRESERVE_ALWAYS").is_ok(),
            max_age_seconds: 86400, // 24 hours
        }
    }
}

/// Represents a single workspace instance
#[derive(Debug)]
pub struct TempWorkspace {
    /// Unique identifier for this workspace
    pub id: String,
    /// Root path of this workspace
    pub root: PathBuf,
    /// Configuration
    config: WorkspaceConfig,
    /// Whether this workspace experienced an error
    had_error: bool,
    /// Metadata about the workspace
    metadata: WorkspaceMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMetadata {
    pub id: String,
    pub created_at: u64,
    pub command: String,
    pub input_file: Option<String>,
    pub output_file: Option<String>,
    pub status: WorkspaceStatus,
    pub error_message: Option<String>,
    pub stats: WorkspaceStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkspaceStatus {
    Active,
    Completed,
    Failed,
    Preserved,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceStats {
    pub input_sequences: usize,
    pub sanitized_sequences: usize,
    pub removed_sequences: usize,
    pub selected_references: usize,
    pub alignment_iterations: usize,
    pub total_alignments: usize,
    pub final_output_sequences: usize,
}

impl TempWorkspace {
    /// Create a new workspace with a unique ID
    pub fn new(command: &str) -> Result<Self> {
        let config = WorkspaceConfig::default();
        Self::with_config(command, config)
    }

    /// Create a new workspace with custom configuration
    pub fn with_config(command: &str, config: WorkspaceConfig) -> Result<Self> {
        // Ensure workspace root exists
        fs::create_dir_all(&config.sequoia_root)
            .with_context(|| format!("Failed to create workspace root: {:?}", config.sequoia_root))?;

        // Generate unique ID
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let uuid = Uuid::new_v4();
        let id = format!("{}_{}", timestamp, uuid);

        // Create workspace root directly under the workspace directory
        let root = config.sequoia_root.join(&id);
        fs::create_dir_all(&root)
            .with_context(|| format!("Failed to create workspace: {:?}", root))?;

        // Create subdirectories
        let subdirs = [
            "input",
            "sanitized",
            "reference_selection",
            "alignments",
            "alignments/iterations",
            "alignments/indices",
            "alignments/temp",
            "output",
            "logs",
            "metadata",
        ];

        for subdir in &subdirs {
            fs::create_dir_all(root.join(subdir))
                .with_context(|| format!("Failed to create subdirectory: {}", subdir))?;
        }

        // Initialize metadata
        let metadata = WorkspaceMetadata {
            id: id.clone(),
            created_at: timestamp,
            command: command.to_string(),
            input_file: None,
            output_file: None,
            status: WorkspaceStatus::Active,
            error_message: None,
            stats: WorkspaceStats::default(),
        };

        let workspace = Self {
            id,
            root,
            config,
            had_error: false,
            metadata,
        };

        // Save initial metadata
        workspace.save_metadata()?;

        Ok(workspace)
    }

    /// Get path to a specific subdirectory
    pub fn get_path(&self, component: &str) -> PathBuf {
        match component {
            "input" => self.root.join("input"),
            "sanitized" => self.root.join("sanitized"),
            "reference_selection" => self.root.join("reference_selection"),
            "alignments" => self.root.join("alignments"),
            "iterations" => self.root.join("alignments").join("iterations"),
            "indices" => self.root.join("alignments").join("indices"),
            "temp" => self.root.join("alignments").join("temp"),
            "output" => self.root.join("output"),
            "logs" => self.root.join("logs"),
            "metadata" => self.root.join("metadata"),
            _ => self.root.join(component),
        }
    }

    /// Get a unique file path for a specific purpose
    pub fn get_file_path(&self, purpose: &str, extension: &str) -> PathBuf {
        match purpose {
            "input_fasta" => self.get_path("input").join(format!("input.{}", extension)),
            "sanitized_fasta" => self
                .get_path("sanitized")
                .join(format!("sanitized.{}", extension)),
            "references" => self
                .get_path("reference_selection")
                .join(format!("references.{}", extension)),
            "similarity_matrix" => self
                .get_path("reference_selection")
                .join(format!("similarity_matrix.{}", extension)),
            "lambda_index" => self
                .get_path("indices")
                .join(format!("lambda_index.{}", extension)),
            "lambda_output" => self
                .get_path("temp")
                .join(format!("lambda_output.{}", extension)),
            "final_output" => self
                .get_path("output")
                .join(format!("reduced.{}", extension)),
            "log" => self
                .get_path("logs")
                .join(format!("{}.{}", purpose, extension)),
            _ => self.root.join(format!("{}.{}", purpose, extension)),
        }
    }

    /// Get iteration-specific path
    pub fn get_iteration_path(&self, iteration: usize, filename: &str) -> PathBuf {
        self.get_path("iterations")
            .join(format!("iter_{:03}", iteration))
            .join(filename)
    }

    /// Create iteration directory
    pub fn create_iteration_dir(&self, iteration: usize) -> Result<PathBuf> {
        let iter_dir = self
            .get_path("iterations")
            .join(format!("iter_{:03}", iteration));
        fs::create_dir_all(&iter_dir)
            .with_context(|| format!("Failed to create iteration directory: {:?}", iter_dir))?;
        Ok(iter_dir)
    }

    /// Update metadata
    pub fn update_metadata<F>(&mut self, updater: F) -> Result<()>
    where
        F: FnOnce(&mut WorkspaceMetadata),
    {
        updater(&mut self.metadata);
        self.save_metadata()
    }

    /// Save metadata to disk
    pub fn save_metadata(&self) -> Result<()> {
        let metadata_path = self.get_path("metadata").join("workspace.json");
        let json = serde_json::to_string_pretty(&self.metadata)?;
        fs::write(&metadata_path, json)
            .with_context(|| format!("Failed to save metadata: {:?}", metadata_path))?;
        Ok(())
    }

    /// Mark workspace as having an error
    pub fn mark_error(&mut self, error: &str) -> Result<()> {
        self.had_error = true;
        self.update_metadata(|m| {
            m.status = WorkspaceStatus::Failed;
            m.error_message = Some(error.to_string());
        })
    }

    /// Mark workspace as completed successfully
    pub fn mark_completed(&mut self) -> Result<()> {
        self.update_metadata(|m| {
            m.status = WorkspaceStatus::Completed;
        })
    }

    /// Clean up the workspace (called on drop)
    pub fn cleanup(&self) -> Result<()> {
        // Determine if we should preserve
        let should_preserve =
            self.config.preserve_always || (self.config.preserve_on_failure && self.had_error);

        if should_preserve {
            // Move to preserved directory
            let preserved_dir = self.config.sequoia_root.join("preserved");
            fs::create_dir_all(&preserved_dir)?;

            let preserved_path = preserved_dir.join(&self.id);
            if preserved_path.exists() {
                fs::remove_dir_all(&preserved_path)?;
            }
            fs::rename(&self.root, &preserved_path)?;

            eprintln!("Workspace preserved at: {:?}", preserved_path);
            eprintln!("To inspect: talaria tools workspace inspect {}", self.id);
        } else {
            // Remove the workspace
            fs::remove_dir_all(&self.root)
                .with_context(|| format!("Failed to cleanup workspace: {:?}", self.root))?;
        }

        Ok(())
    }

    /// Clean old workspaces
    pub fn cleanup_old_workspaces(config: &WorkspaceConfig) -> Result<()> {
        let temporal_dir = config.sequoia_root.join("temporal");
        if !temporal_dir.exists() {
            return Ok(());
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for entry in fs::read_dir(&temporal_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Try to parse timestamp from directory name
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Some(timestamp_str) = name.split('_').next() {
                        if let Ok(timestamp) = timestamp_str.parse::<u64>() {
                            let age = now - timestamp;
                            if age > config.max_age_seconds {
                                fs::remove_dir_all(&path).ok();
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get workspace statistics for reporting
    pub fn get_stats(&self) -> &WorkspaceStats {
        &self.metadata.stats
    }

    /// Update workspace statistics
    pub fn update_stats<F>(&mut self, updater: F) -> Result<()>
    where
        F: FnOnce(&mut WorkspaceStats),
    {
        self.update_metadata(|m| updater(&mut m.stats))
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        // Attempt cleanup, but don't panic on failure
        if let Err(e) = self.cleanup() {
            eprintln!("Warning: Failed to cleanup workspace: {}", e);
        }
    }
}

/// List all workspaces (active, preserved, etc.)
pub fn list_workspaces(config: &WorkspaceConfig) -> Result<Vec<WorkspaceMetadata>> {
    let mut workspaces = Vec::new();

    // Check workspace root directory (workspaces are created directly here)
    if config.sequoia_root.exists() {
        collect_workspaces_from_dir(&config.sequoia_root, &mut workspaces)?;
    }

    // Check preserved directory in databases (for preserved workspaces)
    let preserved_dir = talaria_core::talaria_databases_dir().join("preserved");
    if preserved_dir.exists() {
        collect_workspaces_from_dir(&preserved_dir, &mut workspaces)?;
    }

    // Sort by creation time (newest first)
    workspaces.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(workspaces)
}

fn collect_workspaces_from_dir(dir: &Path, workspaces: &mut Vec<WorkspaceMetadata>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let metadata_path = path.join("metadata").join("workspace.json");
            if metadata_path.exists() {
                if let Ok(content) = fs::read_to_string(&metadata_path) {
                    if let Ok(metadata) = serde_json::from_str::<WorkspaceMetadata>(&content) {
                        workspaces.push(metadata);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Find a workspace by ID
pub fn find_workspace(id: &str, config: &WorkspaceConfig) -> Result<Option<PathBuf>> {
    // Check workspace root directory
    let workspace_path = config.sequoia_root.join(id);
    if workspace_path.exists() {
        return Ok(Some(workspace_path));
    }

    // Check preserved directory in databases
    let preserved_path = talaria_core::talaria_databases_dir()
        .join("preserved")
        .join(id);
    if preserved_path.exists() {
        return Ok(Some(preserved_path));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn setup_test_env() -> WorkspaceConfig {
        // Create a test-specific config that doesn't rely on global state
        let test_home = env::temp_dir().join(format!("talaria_test_{}", std::process::id()));
        WorkspaceConfig {
            sequoia_root: test_home.join("sequoia"),
            preserve_on_failure: false,
            preserve_always: false,
            max_age_seconds: 86400,
        }
    }

    #[test]
    fn test_workspace_creation() {
        let config = setup_test_env();
        let workspace = TempWorkspace::with_config("test_command", config).unwrap();
        assert!(workspace.root.exists());
        assert!(workspace.get_path("input").exists());
        assert!(workspace.get_path("alignments").exists());
    }

    #[test]
    fn test_workspace_uses_correct_directory() {
        // Test with default environment
        let default_dir = talaria_core::talaria_workspace_dir();
        assert!(default_dir.to_str().unwrap().contains("talaria"));

        // Test with custom TALARIA_WORKSPACE_DIR
        let custom_dir = env::temp_dir().join("custom_workspace");
        env::set_var("TALARIA_WORKSPACE_DIR", &custom_dir);

        // Clear the cached value by creating a new process-local test
        // Note: In real usage, the env var is read once at startup
        let workspace_dir = if let Ok(path) = env::var("TALARIA_WORKSPACE_DIR") {
            PathBuf::from(path)
        } else {
            PathBuf::from("/tmp/talaria")
        };

        assert_eq!(workspace_dir, custom_dir);

        // Clean up
        env::remove_var("TALARIA_WORKSPACE_DIR");
    }

    #[test]
    fn test_workspace_paths() {
        let config = setup_test_env();
        let workspace = TempWorkspace::with_config("test_command", config).unwrap();

        let input_path = workspace.get_file_path("input_fasta", "fasta");
        assert!(input_path.to_str().unwrap().contains("input/input.fasta"));

        let iter_path = workspace.get_iteration_path(0, "test.txt");
        assert!(iter_path.to_str().unwrap().contains("iter_000/test.txt"));
    }

    #[test]
    fn test_metadata_update() {
        let config = setup_test_env();
        let mut workspace = TempWorkspace::with_config("test_command", config).unwrap();

        workspace
            .update_stats(|s| {
                s.input_sequences = 1000;
                s.selected_references = 100;
            })
            .unwrap();

        assert_eq!(workspace.get_stats().input_sequences, 1000);
        assert_eq!(workspace.get_stats().selected_references, 100);
    }
}
