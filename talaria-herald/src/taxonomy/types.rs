//! Types for taxonomy management

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Decision on whether to create new version or append
#[derive(Debug)]
pub enum VersionDecision {
    CreateNew { copy_forward: bool, reason: String },
    AppendToCurrent,
    UserCancelled,
}

/// Manifest format for serialization
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaxonomyManifestFormat {
    Json,
    Talaria, // MessagePack-based binary format
}

impl TaxonomyManifestFormat {
    /// Get file extension for this format
    pub fn extension(&self) -> &str {
        match self {
            Self::Json => "json",
            Self::Talaria => "tal",
        }
    }
}

/// Installed component with metadata and provenance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledComponent {
    pub source: String,
    pub checksum: String,
    pub size: u64,
    pub downloaded_at: DateTime<Utc>,
    pub source_version: Option<String>,
    pub carried_from: Option<String>,
    pub file_path: PathBuf,
    pub compressed: bool,
    pub format: String,
}

/// Audit entry for tracking changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub component: String,
    pub user: Option<String>,
    pub details: String,
}

/// Taxonomy manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyManifest {
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expected_components: Vec<String>,
    pub installed_components: Vec<InstalledComponent>,
    pub components: Vec<InstalledComponent>,
    pub history: Vec<AuditEntry>,
    pub policy: TaxonomyVersionPolicy,
}

impl TaxonomyManifest {
    /// Read manifest from file
    pub fn read_from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        use anyhow::Context;

        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let content = std::fs::read_to_string(path).with_context(|| {
                format!("Failed to read manifest file as UTF-8: {}", path.display())
            })?;
            Ok(serde_json::from_str(&content)?)
        } else {
            // Try MessagePack format (binary format, don't read as string)
            let bytes = std::fs::read(path)
                .with_context(|| format!("Failed to read manifest file: {}", path.display()))?;
            Ok(rmp_serde::from_slice(&bytes)?)
        }
    }

    /// Write manifest to file
    pub fn write_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let content = serde_json::to_string_pretty(self)?;
            std::fs::write(path, content)?;
        } else {
            // Use MessagePack format
            let bytes = rmp_serde::to_vec(self)?;
            std::fs::write(path, bytes)?;
        }
        Ok(())
    }

    /// Create a new empty manifest
    pub fn new(version: String) -> Self {
        let now = Utc::now();
        Self {
            version,
            created_at: now,
            updated_at: now,
            expected_components: Vec::new(),
            installed_components: Vec::new(),
            components: Vec::new(),
            history: Vec::new(),
            policy: TaxonomyVersionPolicy::default(),
        }
    }

    /// Save manifest to RocksDB
    pub fn save_to_rocksdb(
        &self,
        rocksdb: &talaria_storage::backend::RocksDBBackend,
        version: &str,
    ) -> anyhow::Result<()> {
        let key = format!("taxonomy_manifest:{}", version);
        let data = bincode::serialize(self)?;
        rocksdb.put_manifest(&key, &data)?;
        Ok(())
    }

    /// Load manifest from RocksDB
    pub fn load_from_rocksdb(
        rocksdb: &talaria_storage::backend::RocksDBBackend,
        version: &str,
    ) -> anyhow::Result<Option<Self>> {
        let key = format!("taxonomy_manifest:{}", version);
        if let Some(data) = rocksdb.get_manifest(&key)? {
            let manifest = bincode::deserialize(&data)?;
            Ok(Some(manifest))
        } else {
            Ok(None)
        }
    }

    /// Load latest manifest from RocksDB
    pub fn load_latest_from_rocksdb(
        rocksdb: &talaria_storage::backend::RocksDBBackend,
    ) -> anyhow::Result<Option<Self>> {
        // Try to get current version alias
        if let Some(version_bytes) = rocksdb.get_manifest("taxonomy_alias:current")? {
            let version = String::from_utf8(version_bytes)?;
            Self::load_from_rocksdb(rocksdb, &version)
        } else {
            Ok(None)
        }
    }

    /// Save as current version
    pub fn save_as_current(
        &self,
        rocksdb: &talaria_storage::backend::RocksDBBackend,
    ) -> anyhow::Result<()> {
        // Save the manifest
        self.save_to_rocksdb(rocksdb, &self.version)?;

        // Update current alias
        rocksdb.put_manifest("taxonomy_alias:current", self.version.as_bytes())?;

        Ok(())
    }
}

/// Taxonomy version policy
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TaxonomyVersionPolicy {
    AlwaysAppend,
    AlwaysNewVersion,
    AskUser,
    Smart, // Decide based on the nature of changes
}

impl Default for TaxonomyVersionPolicy {
    fn default() -> Self {
        Self::Smart
    }
}
