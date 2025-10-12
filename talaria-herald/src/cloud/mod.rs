use crate::SHA256Hash;
/// Cloud storage abstraction for HERALD synchronization
///
/// Provides a unified interface for syncing HERALD repositories with various cloud storage providers
use anyhow::Result;
use async_trait::async_trait;
use indicatif::ProgressBar;
use std::path::{Path, PathBuf};

pub mod s3;
pub mod sync;

/// Configuration for cloud storage providers
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum CloudConfig {
    S3 {
        bucket: String,
        region: String,
        prefix: Option<String>,
        endpoint: Option<String>, // For S3-compatible services
    },
    GoogleCloud {
        bucket: String,
        project: String,
        prefix: Option<String>,
    },
    Azure {
        container: String,
        account: String,
        prefix: Option<String>,
    },
}

/// Metadata for a cloud object
#[derive(Debug, Clone)]
pub struct CloudObject {
    pub key: String,
    pub size: usize,
    pub etag: Option<String>,
    pub last_modified: chrono::DateTime<chrono::Utc>,
    pub storage_class: Option<String>,
}

/// Sync direction for cloud operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDirection {
    Upload,   // Local to cloud
    Download, // Cloud to local
    Bidirectional,
}

/// Sync options
#[derive(Debug, Clone)]
pub struct SyncOptions {
    pub direction: SyncDirection,
    pub delete_missing: bool,
    pub dry_run: bool,
    pub parallel_transfers: usize,
    pub bandwidth_limit: Option<usize>, // Bytes per second
    pub exclude_patterns: Vec<String>,
    pub include_patterns: Vec<String>,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            direction: SyncDirection::Bidirectional,
            delete_missing: false,
            dry_run: false,
            parallel_transfers: 4,
            bandwidth_limit: None,
            exclude_patterns: vec![],
            include_patterns: vec![],
        }
    }
}

/// Result of a sync operation
#[derive(Debug)]
pub struct SyncResult {
    pub uploaded: Vec<String>,
    pub downloaded: Vec<String>,
    pub deleted: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<(String, String)>, // (file, error message)
    pub bytes_transferred: usize,
    pub duration: std::time::Duration,
}

/// Common interface for cloud storage providers
#[async_trait]
pub trait CloudStorage: Send + Sync {
    /// List objects in the cloud storage
    async fn list_objects(&self, prefix: Option<&str>) -> Result<Vec<CloudObject>>;

    /// Check if an object exists
    async fn exists(&self, key: &str) -> Result<bool>;

    /// Get object metadata
    async fn get_metadata(&self, key: &str) -> Result<CloudObject>;

    /// Upload a file to cloud storage
    async fn upload(
        &self,
        local_path: &Path,
        key: &str,
        progress: Option<&ProgressBar>,
    ) -> Result<()>;

    /// Download a file from cloud storage
    async fn download(
        &self,
        key: &str,
        local_path: &Path,
        progress: Option<&ProgressBar>,
    ) -> Result<()>;

    /// Delete an object
    async fn delete(&self, key: &str) -> Result<()>;

    /// Batch delete objects
    async fn delete_batch(&self, keys: &[String]) -> Result<Vec<Result<()>>>;

    /// Get a pre-signed URL for temporary access
    async fn get_presigned_url(&self, key: &str, expires_in: std::time::Duration)
        -> Result<String>;

    /// Check if the storage is accessible
    async fn verify_access(&self) -> Result<()>;
}

/// Factory for creating cloud storage providers
pub fn create_storage(config: &CloudConfig) -> Result<Box<dyn CloudStorage>> {
    match config {
        CloudConfig::S3 { .. } => Ok(Box::new(s3::S3Storage::new(config)?)),
        CloudConfig::GoogleCloud { .. } => {
            anyhow::bail!("Google Cloud Storage not yet implemented")
        }
        CloudConfig::Azure { .. } => {
            anyhow::bail!("Azure Blob Storage not yet implemented")
        }
    }
}

/// Sync manager for coordinating cloud synchronization
pub struct CloudSyncManager {
    storage: Box<dyn CloudStorage>,
    local_path: PathBuf,
    remote_prefix: String,
}

impl CloudSyncManager {
    pub fn new(storage: Box<dyn CloudStorage>, local_path: PathBuf, remote_prefix: String) -> Self {
        Self {
            storage,
            local_path,
            remote_prefix,
        }
    }

    /// Perform synchronization
    pub async fn sync(&self, options: &SyncOptions) -> Result<SyncResult> {
        sync::perform_sync(
            &*self.storage,
            &self.local_path,
            &self.remote_prefix,
            options,
        )
        .await
    }

    /// Sync a specific chunk
    pub async fn sync_chunk(
        &self,
        chunk_hash: &SHA256Hash,
        direction: SyncDirection,
    ) -> Result<()> {
        let chunk_key = format!("{}/chunks/{}", self.remote_prefix, chunk_hash.to_hex());
        let local_chunk_path = self.local_path.join("chunks").join(chunk_hash.to_hex());

        match direction {
            SyncDirection::Upload => {
                if local_chunk_path.exists() {
                    self.storage
                        .upload(&local_chunk_path, &chunk_key, None)
                        .await?;
                }
            }
            SyncDirection::Download => {
                if !local_chunk_path.exists() {
                    self.storage
                        .download(&chunk_key, &local_chunk_path, None)
                        .await?;
                }
            }
            SyncDirection::Bidirectional => {
                // Check both sides and sync the newer one
                if local_chunk_path.exists() {
                    let local_metadata = std::fs::metadata(&local_chunk_path)?;
                    let local_modified = local_metadata.modified()?;

                    if let Ok(cloud_metadata) = self.storage.get_metadata(&chunk_key).await {
                        // Compare timestamps and sync newer version
                        let cloud_modified = cloud_metadata.last_modified;
                        let local_time: chrono::DateTime<chrono::Utc> = local_modified.into();

                        if local_time > cloud_modified {
                            self.storage
                                .upload(&local_chunk_path, &chunk_key, None)
                                .await?;
                        } else if cloud_modified > local_time {
                            self.storage
                                .download(&chunk_key, &local_chunk_path, None)
                                .await?;
                        }
                    } else {
                        // Cloud doesn't have it, upload
                        self.storage
                            .upload(&local_chunk_path, &chunk_key, None)
                            .await?;
                    }
                } else if self.storage.exists(&chunk_key).await? {
                    // Local doesn't have it, download
                    self.storage
                        .download(&chunk_key, &local_chunk_path, None)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Get sync status
    pub async fn get_status(&self) -> Result<SyncStatus> {
        let local_chunks = self.count_local_chunks()?;
        let cloud_objects = self.storage.list_objects(Some(&self.remote_prefix)).await?;

        // Extract cloud chunk hashes
        let cloud_chunk_hashes: std::collections::HashSet<String> = cloud_objects
            .iter()
            .filter(|o| o.key.contains("/chunks/"))
            .filter_map(|o| {
                // Extract hash from path like "prefix/chunks/ab/cd/abcd..."
                o.key.split('/').next_back().map(|s| s.to_string())
            })
            .collect();

        let cloud_chunks = cloud_chunk_hashes.len();

        // Get local chunk hashes
        let local_chunk_hashes = self.get_local_chunk_hashes()?;

        // Calculate pending uploads (local chunks not in cloud)
        let pending_uploads = local_chunk_hashes
            .iter()
            .filter(|hash| !cloud_chunk_hashes.contains(*hash))
            .count();

        // Calculate pending downloads (cloud chunks not local)
        let pending_downloads = cloud_chunk_hashes
            .iter()
            .filter(|hash| !local_chunk_hashes.contains(hash.as_str()))
            .count();

        Ok(SyncStatus {
            local_chunks,
            cloud_chunks,
            last_sync: self.get_last_sync_time()?,
            pending_uploads,
            pending_downloads,
        })
    }

    fn get_local_chunk_hashes(&self) -> Result<std::collections::HashSet<String>> {
        let chunks_dir = self.local_path.join("chunks");
        let mut hashes = std::collections::HashSet::new();

        if !chunks_dir.exists() {
            return Ok(hashes);
        }

        // Walk through chunk directories
        for entry in std::fs::read_dir(&chunks_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                // This is a prefix directory like "ab"
                for chunk_entry in std::fs::read_dir(entry.path())? {
                    let chunk_entry = chunk_entry?;
                    if chunk_entry.path().is_dir() {
                        // This is a hash directory like "cd"
                        for file_entry in std::fs::read_dir(chunk_entry.path())? {
                            let file_entry = file_entry?;
                            if let Some(name) = file_entry.file_name().to_str() {
                                // Remove any extension to get the hash
                                let hash = name.split('.').next().unwrap_or(name);
                                hashes.insert(hash.to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok(hashes)
    }

    fn count_local_chunks(&self) -> Result<usize> {
        let chunks_dir = self.local_path.join("chunks");
        if !chunks_dir.exists() {
            return Ok(0);
        }

        let count = std::fs::read_dir(chunks_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_file())
            .count();

        Ok(count)
    }

    fn get_last_sync_time(&self) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
        let sync_file = self.local_path.join(".last_sync");
        if !sync_file.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(sync_file)?;
        let timestamp = chrono::DateTime::parse_from_rfc3339(&content)?;
        Ok(Some(timestamp.with_timezone(&chrono::Utc)))
    }

    pub fn update_last_sync_time(&self) -> Result<()> {
        let sync_file = self.local_path.join(".last_sync");
        let now = chrono::Utc::now();
        std::fs::write(sync_file, now.to_rfc3339())?;
        Ok(())
    }
}

/// Status of cloud synchronization
#[derive(Debug)]
pub struct SyncStatus {
    pub local_chunks: usize,
    pub cloud_chunks: usize,
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
    pub pending_uploads: usize,
    pub pending_downloads: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_options_default() {
        let options = SyncOptions::default();
        assert_eq!(options.direction, SyncDirection::Bidirectional);
        assert!(!options.delete_missing);
        assert_eq!(options.parallel_transfers, 4);
    }
}
