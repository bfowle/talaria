/// HERALD-specific workspace utilities
/// HERALD is the ENTIRE SYSTEM - all operations go through HERALD
use anyhow::{Context, Result};
use crypto_hash::{digest, Algorithm};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::temp::{TempWorkspace, WorkspaceConfig};

/// HERALD content addressing for workspace files
pub struct HeraldWorkspaceManager {
    /// Root directory for HERALD operations
    pub herald_root: PathBuf,
    /// Current workspace (if any)
    pub workspace: Option<TempWorkspace>,
}

impl HeraldWorkspaceManager {
    /// Initialize HERALD workspace manager
    pub fn new() -> Result<Self> {
        // Use the centralized workspace directory configuration
        let herald_root = talaria_core::system::paths::talaria_workspace_dir();

        // Ensure HERALD structure exists
        Self::initialize_herald_structure(&herald_root)?;

        Ok(Self {
            herald_root,
            workspace: None,
        })
    }

    /// Initialize the HERALD directory structure
    fn initialize_herald_structure(root: &Path) -> Result<()> {
        // For workspace directory, we only need the root - workspaces will be created as subdirectories
        // No need for subdirectories like temporal, preserved, etc. since this IS the temporal directory
        fs::create_dir_all(root)
            .with_context(|| format!("Failed to create workspace directory: {:?}", root))?;

        Ok(())
    }

    /// Create a new workspace for a reduction operation
    pub fn create_workspace(&mut self, command: &str) -> Result<TempWorkspace> {
        let config = WorkspaceConfig::default();
        let workspace = TempWorkspace::with_config(command, config)?;
        Ok(workspace)
    }

    /// Get current workspace
    pub fn get_workspace(&mut self) -> Option<&mut TempWorkspace> {
        self.workspace.as_mut()
    }

    /// Content-address a file and store it in HERALD
    pub fn store_content(&self, file_path: &Path) -> Result<String> {
        let content =
            fs::read(file_path).with_context(|| format!("Failed to read file: {:?}", file_path))?;

        // Calculate content hash
        let hash = digest(Algorithm::SHA256, &content);
        let hash_hex = hex::encode(&hash);

        // Store in content-addressed storage
        let content_dir = self.herald_root.join("content");
        fs::create_dir_all(&content_dir)
            .with_context(|| format!("Failed to create content directory: {:?}", content_dir))?;
        let stored_path = content_dir.join(&hash_hex);

        if !stored_path.exists() {
            fs::write(&stored_path, &content)
                .with_context(|| format!("Failed to store content: {:?}", stored_path))?;
        }

        Ok(hash_hex)
    }

    /// Retrieve content by hash
    pub fn get_content(&self, hash: &str) -> Result<Vec<u8>> {
        let content_path = self.herald_root.join("content").join(hash);
        fs::read(&content_path).with_context(|| format!("Content not found: {}", hash))
    }

    /// Link content to a workspace file
    pub fn link_content(&self, hash: &str, workspace_path: &Path) -> Result<()> {
        let content_path = self.herald_root.join("content").join(hash);

        // Create hard link if possible, otherwise copy
        if let Err(_) = fs::hard_link(&content_path, workspace_path) {
            fs::copy(&content_path, workspace_path)?;
        }

        Ok(())
    }

    /// Cache alignment results
    pub fn cache_alignment(
        &self,
        query_hash: &str,
        db_hash: &str,
        result: &[u8],
    ) -> Result<String> {
        let cache_dir = self.herald_root.join("cache").join("alignments");
        fs::create_dir_all(&cache_dir)?;

        // Create cache key from query and database hashes
        let cache_key = format!("{}_{}", query_hash, db_hash);
        let cache_path = cache_dir.join(&cache_key);

        fs::write(&cache_path, result)?;

        Ok(cache_key)
    }

    /// Retrieve cached alignment
    pub fn get_cached_alignment(&self, query_hash: &str, db_hash: &str) -> Result<Option<Vec<u8>>> {
        let cache_key = format!("{}_{}", query_hash, db_hash);
        let cache_path = self
            .herald_root
            .join("cache")
            .join("alignments")
            .join(&cache_key);

        if cache_path.exists() {
            Ok(Some(fs::read(&cache_path)?))
        } else {
            Ok(None)
        }
    }

    /// Log operation to HERALD logs
    pub fn log_operation(&self, operation: &str, details: &str) -> Result<()> {
        let log_dir = self.herald_root.join("logs");

        // Ensure the logs directory exists
        fs::create_dir_all(&log_dir)?;

        let log_file = log_dir.join("operations.log");

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)?;

        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        writeln!(file, "[{}] {} - {}", timestamp, operation, details)?;

        Ok(())
    }

    /// Clean up old cached content
    pub fn cleanup_cache(&self, max_age_days: u64) -> Result<usize> {
        let cache_dir = self.herald_root.join("cache");
        let mut removed = 0;

        if cache_dir.exists() {
            let max_age = std::time::Duration::from_secs(max_age_days * 86400);
            let now = std::time::SystemTime::now();

            for entry in fs::read_dir(&cache_dir)? {
                let entry = entry?;
                let path = entry.path();

                let metadata = match fs::metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let modified = match metadata.modified() {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let age = match now.duration_since(modified) {
                    Ok(a) => a,
                    Err(_) => continue,
                };

                if age <= max_age {
                    continue;
                }

                if path.is_file() {
                    fs::remove_file(&path)?;
                } else {
                    fs::remove_dir_all(&path)?;
                }
                removed += 1;
            }
        }

        Ok(removed)
    }

    /// Get HERALD statistics
    pub fn get_statistics(&self) -> Result<HeraldStatistics> {
        let mut stats = HeraldStatistics::default();

        // Count temporal workspaces
        let temporal_dir = self.herald_root.join("temporal");
        if temporal_dir.exists() {
            stats.active_workspaces = fs::read_dir(&temporal_dir)?.count();
        }

        // Count preserved workspaces
        let preserved_dir = self.herald_root.join("preserved");
        if preserved_dir.exists() {
            stats.preserved_workspaces = fs::read_dir(&preserved_dir)?.count();
        }

        // Count content objects
        let content_dir = self.herald_root.join("content");
        if content_dir.exists() {
            stats.content_objects = fs::read_dir(&content_dir)?.count();
        }

        // Count cached alignments
        let cache_dir = self.herald_root.join("cache").join("alignments");
        if cache_dir.exists() {
            stats.cached_alignments = fs::read_dir(&cache_dir)?.count();
        }

        // Calculate total size
        stats.total_size_bytes = Self::calculate_dir_size(&self.herald_root)?;

        Ok(stats)
    }

    fn calculate_dir_size(path: &Path) -> Result<u64> {
        let mut size = 0;

        if path.is_file() {
            size += fs::metadata(path)?.len();
        } else if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                size += Self::calculate_dir_size(&entry.path())?;
            }
        }

        Ok(size)
    }
}

#[derive(Debug, Default)]
pub struct HeraldStatistics {
    pub active_workspaces: usize,
    pub preserved_workspaces: usize,
    pub content_objects: usize,
    pub cached_alignments: usize,
    pub total_size_bytes: u64,
}

impl HeraldStatistics {
    pub fn print(&self) {
        println!("HERALD System Statistics:");
        println!("  Active Workspaces:    {}", self.active_workspaces);
        println!("  Preserved Workspaces: {}", self.preserved_workspaces);
        println!("  Content Objects:      {}", self.content_objects);
        println!("  Cached Alignments:    {}", self.cached_alignments);
        println!(
            "  Total Size:           {:.2} GB",
            self.total_size_bytes as f64 / 1_073_741_824.0
        );
    }
}

/// Helper to ensure HERALD operations are atomic
pub struct HeraldTransaction {
    operations: Vec<HeraldOperation>,
    rollback_operations: Vec<HeraldOperation>,
}

#[derive(Debug)]
enum HeraldOperation {
    WriteFile { path: PathBuf, content: Vec<u8> },
    DeleteFile { path: PathBuf },
    CreateDir { path: PathBuf },
    DeleteDir { path: PathBuf },
}

impl HeraldTransaction {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            rollback_operations: Vec::new(),
        }
    }

    pub fn write_file(&mut self, path: PathBuf, content: Vec<u8>) {
        self.operations.push(HeraldOperation::WriteFile {
            path: path.clone(),
            content,
        });
        self.rollback_operations
            .push(HeraldOperation::DeleteFile { path });
    }

    pub fn create_dir(&mut self, path: PathBuf) {
        self.operations
            .push(HeraldOperation::CreateDir { path: path.clone() });
        self.rollback_operations
            .push(HeraldOperation::DeleteDir { path });
    }

    pub fn commit(self) -> Result<()> {
        for op in &self.operations {
            if let Err(e) = Self::execute_operation(op) {
                // Rollback on failure
                for rollback_op in self.rollback_operations.iter().rev() {
                    Self::execute_operation(rollback_op).ok();
                }
                return Err(e);
            }
        }
        Ok(())
    }

    fn execute_operation(op: &HeraldOperation) -> Result<()> {
        match op {
            HeraldOperation::WriteFile { path, content } => {
                fs::write(path, content)?;
            }
            HeraldOperation::DeleteFile { path } => {
                if path.exists() {
                    fs::remove_file(path)?;
                }
            }
            HeraldOperation::CreateDir { path } => {
                fs::create_dir_all(path)?;
            }
            HeraldOperation::DeleteDir { path } => {
                if path.exists() {
                    fs::remove_dir_all(path)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_herald_initialization() {
        let manager = HeraldWorkspaceManager::new().unwrap();
        // The workspace directory should exist
        assert!(manager.herald_root.exists());
        // Note: temporal and content directories are not created during initialization
        // They are created on-demand when needed
    }

    #[test]
    fn test_content_addressing() {
        let manager = HeraldWorkspaceManager::new().unwrap();

        // Create test content
        let test_content = b"test content";
        let temp_file = std::env::temp_dir().join("test_file.txt");
        fs::write(&temp_file, test_content).unwrap();

        // Store and retrieve
        let hash = manager.store_content(&temp_file).unwrap();
        let retrieved = manager.get_content(&hash).unwrap();

        assert_eq!(test_content.to_vec(), retrieved);

        // Cleanup
        fs::remove_file(&temp_file).unwrap();
    }
}
