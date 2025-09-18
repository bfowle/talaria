/// Trait for abstract version storage backends
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::download::DatabaseSource;
// Versionable trait integrated directly here

/// Represents a version in the store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    /// Unique version identifier (e.g., "20250915_053033")
    pub id: String,
    /// When this version was created
    pub created_at: DateTime<Utc>,
    /// Path to manifest file
    pub manifest_path: PathBuf,
    /// Size of this version in bytes
    pub size: usize,
    /// Number of chunks in this version
    pub chunk_count: usize,
    /// Number of sequences/entries
    pub entry_count: usize,
    /// Upstream version if applicable
    pub upstream_version: Option<String>,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

impl Version {
    pub fn version(&self) -> &str {
        &self.id
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

/// Version listing options
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// Maximum number of versions to return
    pub limit: Option<usize>,
    /// Skip this many versions
    pub offset: Option<usize>,
    /// Sort order (true = newest first)
    pub newest_first: bool,
    /// Include metadata in results
    pub include_metadata: bool,
}

/// Result of a version operation
#[derive(Debug)]
pub enum VersionOperation {
    Created(Version),
    Updated(Version),
    Deleted(String),
    AliasSet { alias: String, version: String },
}

/// Trait for version storage backends
#[async_trait]
pub trait VersionStore: Send + Sync {
    /// List all versions for a database
    async fn list_versions(
        &self,
        source: &DatabaseSource,
        options: ListOptions,
    ) -> Result<Vec<Version>>;

    /// Get the current/latest version
    async fn current_version(&self, source: &DatabaseSource) -> Result<Version>;

    /// Get a specific version by ID
    async fn get_version(&self, source: &DatabaseSource, version_id: &str) -> Result<Version>;

    /// Create a new version
    async fn create_version(&mut self, source: &DatabaseSource) -> Result<Version>;

    /// Delete a version (if supported)
    async fn delete_version(&mut self, source: &DatabaseSource, version_id: &str) -> Result<()>;

    /// Update version aliases (current, stable, etc.)
    async fn update_alias(
        &mut self,
        source: &DatabaseSource,
        alias: &str,
        version_id: &str,
    ) -> Result<()>;

    /// Get version by alias
    async fn resolve_alias(&self, source: &DatabaseSource, alias: &str) -> Result<Version>;

    /// List all aliases for a database
    async fn list_aliases(&self, source: &DatabaseSource) -> Result<HashMap<String, String>>;

    /// Atomic version promotion (make a version current)
    async fn promote_version(
        &mut self,
        source: &DatabaseSource,
        version_id: &str,
    ) -> Result<()> {
        self.update_alias(source, "current", version_id).await
    }

    /// Check if a version exists
    async fn version_exists(&self, source: &DatabaseSource, version_id: &str) -> bool {
        self.get_version(source, version_id).await.is_ok()
    }

    /// Get storage path for a version
    fn get_version_path(&self, source: &DatabaseSource, version_id: &str) -> PathBuf;

    /// Clean up old versions (keep N most recent)
    async fn cleanup_old_versions(
        &mut self,
        source: &DatabaseSource,
        keep_count: usize,
    ) -> Result<Vec<String>>;

    /// Get total storage used by all versions
    async fn get_storage_usage(&self, source: &DatabaseSource) -> Result<usize>;

    /// Export version metadata for backup
    async fn export_metadata(&self, source: &DatabaseSource) -> Result<Vec<u8>>;

    /// Import version metadata from backup
    async fn import_metadata(&mut self, source: &DatabaseSource, data: &[u8]) -> Result<()>;
}

/// Filesystem-based version store implementation
pub struct FilesystemVersionStore {
    base_path: PathBuf,
}

impl FilesystemVersionStore {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn get_database_path(&self, source: &DatabaseSource) -> PathBuf {
        let (source_name, dataset) = self.get_source_dataset_names(source);
        self.base_path
            .join("versions")
            .join(source_name)
            .join(dataset)
    }

    fn get_source_dataset_names(&self, source: &DatabaseSource) -> (String, String) {
        use crate::download::{NCBIDatabase, UniProtDatabase};

        match source {
            DatabaseSource::UniProt(UniProtDatabase::SwissProt) => {
                ("uniprot".to_string(), "swissprot".to_string())
            }
            DatabaseSource::UniProt(UniProtDatabase::TrEMBL) => {
                ("uniprot".to_string(), "trembl".to_string())
            }
            DatabaseSource::NCBI(NCBIDatabase::NR) => ("ncbi".to_string(), "nr".to_string()),
            DatabaseSource::NCBI(NCBIDatabase::NT) => ("ncbi".to_string(), "nt".to_string()),
            DatabaseSource::Custom(name) => ("custom".to_string(), name.clone()),
            _ => ("unknown".to_string(), "unknown".to_string()),
        }
    }
}

#[async_trait]
impl VersionStore for FilesystemVersionStore {
    async fn list_versions(
        &self,
        source: &DatabaseSource,
        options: ListOptions,
    ) -> Result<Vec<Version>> {
        let db_path = self.get_database_path(source);
        let mut versions = Vec::new();

        if !db_path.exists() {
            return Ok(versions);
        }

        // Read all version directories
        for entry in std::fs::read_dir(&db_path)? {
            let entry = entry?;
            let path = entry.path();

            // Skip symlinks and non-directories
            if !path.is_dir() || path.is_symlink() {
                continue;
            }

            let dir_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Check if it looks like a version directory (timestamp format)
            if dir_name.len() == 15 && dir_name.chars().nth(8) == Some('_') {
                let manifest_path = path.join("manifest.tal");
                if !manifest_path.exists() {
                    let manifest_path = path.join("manifest.json");
                    if !manifest_path.exists() {
                        continue;
                    }
                }

                // Get directory metadata for size
                let size = Self::get_directory_size(&path).await?;

                versions.push(Version {
                    id: dir_name.to_string(),
                    created_at: Self::parse_timestamp_version(dir_name)?,
                    manifest_path,
                    size,
                    chunk_count: 0,  // Would need to read manifest
                    entry_count: 0,   // Would need to read manifest
                    upstream_version: None,
                    metadata: HashMap::new(),
                });
            }
        }

        // Sort by creation date
        versions.sort_by_key(|v| v.created_at);
        if options.newest_first {
            versions.reverse();
        }

        // Apply offset and limit
        let start = options.offset.unwrap_or(0);
        let end = options.limit.map(|l| start + l).unwrap_or(versions.len());

        Ok(versions.into_iter().skip(start).take(end - start).collect())
    }

    async fn current_version(&self, source: &DatabaseSource) -> Result<Version> {
        let db_path = self.get_database_path(source);
        let current_link = db_path.join("current");

        if current_link.exists() {
            let target = std::fs::read_link(&current_link)?;
            let version_id = target.file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid current symlink"))?;

            self.get_version(source, version_id).await
        } else {
            // No current link, get the newest version
            let versions = self.list_versions(source, ListOptions {
                limit: Some(1),
                newest_first: true,
                ..Default::default()
            }).await?;

            versions.into_iter().next()
                .ok_or_else(|| anyhow::anyhow!("No versions found"))
        }
    }

    async fn get_version(&self, source: &DatabaseSource, version_id: &str) -> Result<Version> {
        let version_path = self.get_version_path(source, version_id);

        if !version_path.exists() {
            anyhow::bail!("Version {} not found", version_id);
        }

        let manifest_path = version_path.join("manifest.tal");
        let manifest_path = if manifest_path.exists() {
            manifest_path
        } else {
            version_path.join("manifest.json")
        };

        let size = Self::get_directory_size(&version_path).await?;

        Ok(Version {
            id: version_id.to_string(),
            created_at: Self::parse_timestamp_version(version_id)?,
            manifest_path,
            size,
            chunk_count: 0,
            entry_count: 0,
            upstream_version: None,
            metadata: HashMap::new(),
        })
    }

    async fn create_version(&mut self, source: &DatabaseSource) -> Result<Version> {
        let version_id = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let version_path = self.get_version_path(source, &version_id);

        std::fs::create_dir_all(&version_path)?;

        Ok(Version {
            id: version_id,
            created_at: Utc::now(),
            manifest_path: version_path.join("manifest.tal"),
            size: 0,
            chunk_count: 0,
            entry_count: 0,
            upstream_version: None,
            metadata: HashMap::new(),
        })
    }

    async fn delete_version(&mut self, source: &DatabaseSource, version_id: &str) -> Result<()> {
        let version_path = self.get_version_path(source, version_id);

        if version_path.exists() {
            std::fs::remove_dir_all(&version_path)?;
        }

        Ok(())
    }

    async fn update_alias(
        &mut self,
        source: &DatabaseSource,
        alias: &str,
        version_id: &str,
    ) -> Result<()> {
        let db_path = self.get_database_path(source);
        let alias_path = db_path.join(alias);

        // Remove existing symlink if it exists
        if alias_path.exists() {
            std::fs::remove_file(&alias_path)?;
        }

        // Create new symlink
        #[cfg(unix)]
        std::os::unix::fs::symlink(version_id, &alias_path)?;

        #[cfg(windows)]
        {
            let version_path = self.get_version_path(source, version_id);
            std::os::windows::fs::symlink_dir(version_path, &alias_path)?;
        }

        Ok(())
    }

    async fn resolve_alias(&self, source: &DatabaseSource, alias: &str) -> Result<Version> {
        let db_path = self.get_database_path(source);
        let alias_path = db_path.join(alias);

        if alias_path.exists() && alias_path.is_symlink() {
            let target = std::fs::read_link(&alias_path)?;
            let version_id = target.file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid alias symlink"))?;

            self.get_version(source, version_id).await
        } else {
            anyhow::bail!("Alias '{}' not found", alias)
        }
    }

    async fn list_aliases(&self, source: &DatabaseSource) -> Result<HashMap<String, String>> {
        let db_path = self.get_database_path(source);
        let mut aliases = HashMap::new();

        if !db_path.exists() {
            return Ok(aliases);
        }

        for entry in std::fs::read_dir(&db_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_symlink() {
                let name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                if let Ok(target) = std::fs::read_link(&path) {
                    if let Some(version) = target.file_name().and_then(|n| n.to_str()) {
                        aliases.insert(name, version.to_string());
                    }
                }
            }
        }

        Ok(aliases)
    }

    fn get_version_path(&self, source: &DatabaseSource, version_id: &str) -> PathBuf {
        self.get_database_path(source).join(version_id)
    }

    async fn cleanup_old_versions(
        &mut self,
        source: &DatabaseSource,
        keep_count: usize,
    ) -> Result<Vec<String>> {
        let versions = self.list_versions(source, ListOptions {
            newest_first: true,
            ..Default::default()
        }).await?;

        let mut deleted = Vec::new();

        for version in versions.iter().skip(keep_count) {
            self.delete_version(source, &version.id).await?;
            deleted.push(version.id.clone());
        }

        Ok(deleted)
    }

    async fn get_storage_usage(&self, source: &DatabaseSource) -> Result<usize> {
        let db_path = self.get_database_path(source);
        Self::get_directory_size(&db_path).await
    }

    async fn export_metadata(&self, source: &DatabaseSource) -> Result<Vec<u8>> {
        let versions = self.list_versions(source, ListOptions::default()).await?;
        let aliases = self.list_aliases(source).await?;

        let export = serde_json::json!({
            "versions": versions,
            "aliases": aliases,
            "exported_at": Utc::now(),
        });

        Ok(serde_json::to_vec(&export)?)
    }

    async fn import_metadata(&mut self, _source: &DatabaseSource, _data: &[u8]) -> Result<()> {
        // This would restore version metadata from a backup
        // For filesystem store, the actual data is in the directories
        Ok(())
    }
}

impl FilesystemVersionStore {
    async fn get_directory_size(path: &PathBuf) -> Result<usize> {
        let mut size = 0;

        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    size += Box::pin(Self::get_directory_size(&path)).await?;
                } else {
                    size += entry.metadata()?.len() as usize;
                }
            }
        }

        Ok(size)
    }

    fn parse_timestamp_version(version: &str) -> Result<DateTime<Utc>> {
        if version.len() == 15 && version.chars().nth(8) == Some('_') {
            let dt = DateTime::parse_from_str(
                &format!("{} {}:{}:{}",
                    &version[0..8],
                    &version[9..11],
                    &version[11..13],
                    &version[13..15]
                ),
                "%Y%m%d %H:%M:%S"
            )?.with_timezone(&Utc);
            Ok(dt)
        } else {
            // Non-timestamp version, use current time
            Ok(Utc::now())
        }
    }
}