use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use talaria_core::system::paths;
use talaria_storage::backend::RocksDBBackend;

/// Manages database backups using RocksDB BackupEngine
pub struct BackupManager {
    backups_dir: PathBuf,
    databases_dir: PathBuf,
}

/// Backup metadata stored alongside RocksDB backups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub id: u32,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new() -> Result<Self> {
        let backups_dir = paths::talaria_backups_dir();
        let databases_dir = paths::talaria_databases_dir();

        // Ensure backups directory exists
        fs::create_dir_all(&backups_dir)?;

        Ok(Self {
            backups_dir,
            databases_dir,
        })
    }

    /// Get the RocksDB backup directory path
    fn rocksdb_backup_dir(&self) -> PathBuf {
        self.backups_dir.join("rocksdb")
    }

    /// Get the metadata directory path
    fn metadata_dir(&self) -> PathBuf {
        self.backups_dir.join("metadata")
    }

    /// Get the RocksDB data directory path
    fn rocksdb_data_dir(&self) -> PathBuf {
        self.databases_dir.join("data")
    }

    /// Create a new backup with a given name
    ///
    /// This creates:
    /// 1. A RocksDB backup using BackupEngine
    /// 2. Metadata JSON with name, description, and creation time
    pub fn create_backup(
        &self,
        rocksdb: &RocksDBBackend,
        name: &str,
        description: Option<String>,
    ) -> Result<BackupMetadata> {
        // Ensure backup directories exist
        let backup_dir = self.rocksdb_backup_dir();
        let metadata_dir = self.metadata_dir();
        fs::create_dir_all(&backup_dir)?;
        fs::create_dir_all(&metadata_dir)?;

        // Check if backup name already exists
        let metadata_path = metadata_dir.join(format!("{}.json", name));
        if metadata_path.exists() {
            anyhow::bail!("Backup '{}' already exists", name);
        }

        tracing::info!("Creating backup '{}'...", name);
        tracing::debug!("  Flushing database to disk...");

        // Create RocksDB backup (with flush)
        let backup_id = rocksdb
            .create_backup(&backup_dir, true)
            .context("Failed to create RocksDB backup")?;

        // Get backup info
        let backups = RocksDBBackend::list_backups(&backup_dir)?;
        let (_, timestamp, size_bytes) = backups
            .iter()
            .find(|(id, _, _)| *id == backup_id)
            .ok_or_else(|| anyhow::anyhow!("Backup ID {} not found", backup_id))?;

        let created_at = DateTime::from_timestamp(*timestamp, 0).unwrap_or_else(|| Utc::now());

        // Create metadata
        let metadata = BackupMetadata {
            id: backup_id,
            name: name.to_string(),
            description: description.clone(),
            created_at,
            size_bytes: *size_bytes,
        };

        // Save metadata
        fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;

        tracing::info!("✓ Backup '{}' created successfully", name);
        tracing::info!("  Backup ID: {}", backup_id);
        tracing::info!("  Size: {:.2} MB", size_bytes / 1_048_576);

        Ok(metadata)
    }

    /// List all available backups
    pub fn list_backups(&self) -> Result<Vec<BackupMetadata>> {
        let metadata_dir = self.metadata_dir();

        if !metadata_dir.exists() {
            return Ok(Vec::new());
        }

        let mut backups = Vec::new();

        for entry in fs::read_dir(&metadata_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let metadata_str = fs::read_to_string(&path)?;
                let metadata: BackupMetadata = serde_json::from_str(&metadata_str)?;
                backups.push(metadata);
            }
        }

        // Sort by creation date (newest first)
        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(backups)
    }

    /// Get metadata for a specific backup by name
    pub fn get_backup_metadata(&self, name: &str) -> Result<BackupMetadata> {
        let metadata_path = self.metadata_dir().join(format!("{}.json", name));

        if !metadata_path.exists() {
            anyhow::bail!("Backup '{}' not found", name);
        }

        let metadata_str = fs::read_to_string(&metadata_path)?;
        let metadata: BackupMetadata = serde_json::from_str(&metadata_str)?;

        Ok(metadata)
    }

    /// Restore from a backup by name
    ///
    /// This restores the RocksDB database from the backup.
    /// WARNING: This will shut down the current database and replace it!
    pub fn restore_backup(&self, name: &str) -> Result<()> {
        let metadata = self.get_backup_metadata(name)?;
        let backup_dir = self.rocksdb_backup_dir();
        let restore_dir = self.rocksdb_data_dir();

        tracing::info!("Restoring backup '{}'...", name);
        tracing::info!(
            "  Created: {}",
            metadata.created_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
        if let Some(ref desc) = metadata.description {
            tracing::info!("  Description: {}", desc);
        }

        tracing::info!("  Backup ID: {}", metadata.id);
        tracing::info!("  Size: {:.2} MB", metadata.size_bytes as f64 / 1_048_576.0);

        // Verify backup exists
        RocksDBBackend::verify_backup(&backup_dir, metadata.id)
            .context("Backup verification failed")?;
        tracing::info!("✓ Backup verification passed");

        // NOTE: In practice, you would need to:
        // 1. Shut down the current RocksDB instance
        // 2. Move/backup the current data directory
        // 3. Restore the backup
        // 4. Restart the RocksDB instance
        //
        // This is a simplified implementation that assumes the database is not running
        tracing::warn!("⚠ WARNING: This will replace the current database!");
        tracing::warn!("  Please ensure the database is shut down before proceeding.");

        // For now, just restore to a temporary location for safety
        let temp_restore = self
            .backups_dir
            .join(format!("restore_temp_{}", metadata.id));
        if temp_restore.exists() {
            fs::remove_dir_all(&temp_restore)?;
        }

        RocksDBBackend::restore_from_latest_backup(&backup_dir, &temp_restore)
            .context("Failed to restore backup")?;

        tracing::info!(
            "✓ Backup restored to temporary location: {}",
            temp_restore.display()
        );
        tracing::warn!(
            "  Manual step required: Replace {} with restored data",
            restore_dir.display()
        );

        Ok(())
    }

    /// Verify a backup by name
    pub fn verify_backup(&self, name: &str) -> Result<()> {
        let metadata = self.get_backup_metadata(name)?;
        let backup_dir = self.rocksdb_backup_dir();

        tracing::info!("Verifying backup '{}'...", name);
        tracing::info!("  Backup ID: {}", metadata.id);

        RocksDBBackend::verify_backup(&backup_dir, metadata.id)
            .context("Backup verification failed")?;

        tracing::info!("✓ Backup '{}' verification passed", name);
        Ok(())
    }

    /// Delete a backup by name
    pub fn delete_backup(&self, name: &str) -> Result<()> {
        let metadata = self.get_backup_metadata(name)?;
        let metadata_path = self.metadata_dir().join(format!("{}.json", name));

        tracing::info!("Deleting backup '{}'...", name);
        tracing::info!("  Backup ID: {}", metadata.id);

        // Note: RocksDB BackupEngine doesn't support deleting specific backups by ID
        // We can only purge old backups. For now, just delete the metadata.
        fs::remove_file(&metadata_path)?;

        tracing::info!("✓ Backup '{}' metadata deleted", name);
        tracing::info!("  Note: RocksDB backup files remain. Use 'purge' to clean old backups.");

        Ok(())
    }

    /// Purge old backups, keeping only the specified number of recent backups
    pub fn purge_old_backups(&self, num_to_keep: usize) -> Result<()> {
        let backup_dir = self.rocksdb_backup_dir();

        tracing::info!(
            "Purging old backups, keeping {} most recent...",
            num_to_keep
        );

        RocksDBBackend::purge_old_backups(&backup_dir, num_to_keep)
            .context("Failed to purge old backups")?;

        tracing::info!("✓ Old backups purged successfully");

        // Also clean up metadata for deleted backups
        self.cleanup_orphaned_metadata()?;

        Ok(())
    }

    /// Remove metadata files for backups that no longer exist in RocksDB
    fn cleanup_orphaned_metadata(&self) -> Result<()> {
        let backup_dir = self.rocksdb_backup_dir();
        let metadata_dir = self.metadata_dir();

        if !metadata_dir.exists() {
            return Ok(());
        }

        // Get list of existing backup IDs
        let existing_backups = RocksDBBackend::list_backups(&backup_dir)?;
        let existing_ids: std::collections::HashSet<u32> =
            existing_backups.iter().map(|(id, _, _)| *id).collect();

        // Remove metadata for non-existent backups
        let mut removed = 0;
        for entry in fs::read_dir(&metadata_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(metadata_str) = fs::read_to_string(&path) {
                    if let Ok(metadata) = serde_json::from_str::<BackupMetadata>(&metadata_str) {
                        if !existing_ids.contains(&metadata.id) {
                            fs::remove_file(&path)?;
                            removed += 1;
                        }
                    }
                }
            }
        }

        if removed > 0 {
            tracing::info!("  Cleaned up {} orphaned metadata files", removed);
        }

        Ok(())
    }
}

impl Default for BackupManager {
    fn default() -> Self {
        Self::new().expect("Failed to create BackupManager")
    }
}
