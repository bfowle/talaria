/// Taxonomy-specific version store implementation
///
/// This module provides versioning for taxonomy databases (NCBI, UniProt)
/// using the existing VersionStore trait infrastructure.
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::taxonomy::manifest::{TaxonomyManifest, TaxonomySource as ManifestTaxonomySource};
use crate::temporal::version_store::{ListOptions, Version, VersionStore};
use crate::types::{SHA256Hash, SHA256HashExt};
use crate::DatabaseSource;

/// Taxonomy-specific version store that wraps the base VersionStore
pub struct TaxonomyVersionStore {
    /// Underlying version store (typically FilesystemVersionStore)
    base_store: Box<dyn VersionStore>,

    /// Base path for taxonomy data
    taxonomy_path: PathBuf,

    /// Cache of taxonomy manifests
    manifest_cache: HashMap<String, TaxonomyManifest>,
}

impl TaxonomyVersionStore {
    /// Create a new taxonomy version store
    pub fn new(base_store: Box<dyn VersionStore>, taxonomy_path: PathBuf) -> Result<Self> {
        fs::create_dir_all(&taxonomy_path)?;

        Ok(Self {
            base_store,
            taxonomy_path,
            manifest_cache: HashMap::new(),
        })
    }

    /// Create a taxonomy version from downloaded data
    pub async fn create_taxonomy_version(
        &mut self,
        source: TaxonomySource,
        data_path: &Path,
    ) -> Result<Version> {
        let version_id = match &source {
            TaxonomySource::NCBI { date } => format!("ncbi_{}", date.format("%Y%m%d")),
            TaxonomySource::UniProt { release, .. } => format!("uniprot_{}", release),
            TaxonomySource::Custom { name, version } => format!("{}_{}", name, version),
        };

        let version_path = self.taxonomy_path.join("versions").join(&version_id);
        fs::create_dir_all(&version_path)?;

        // Create taxonomy manifest
        let manifest = self
            .create_taxonomy_manifest(source.clone(), data_path)
            .await?;

        // Save manifest
        let manifest_path = version_path.join("taxonomy_manifest.tal");
        let manifest_bytes = rmp_serde::to_vec(&manifest)?;
        fs::write(&manifest_path, manifest_bytes)?;

        // Process and chunk the taxonomy data
        let chunk_count = self.chunk_taxonomy_data(data_path, &version_path).await?;

        // Create Version object with taxonomy-specific metadata
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), "taxonomy".to_string());
        metadata.insert("source".to_string(), source.name());

        if let TaxonomySource::NCBI { date } = &source {
            metadata.insert("ncbi_date".to_string(), date.format("%Y-%m-%d").to_string());
        }

        let version = Version {
            id: version_id.clone(),
            created_at: Utc::now(),
            manifest_path,
            size: self.calculate_version_size(&version_path)?,
            chunk_count,
            entry_count: manifest.stats.total_taxa,
            upstream_version: Some(source.version_string()),
            metadata,
        };

        // Store in cache
        self.manifest_cache.insert(version_id, manifest);

        Ok(version)
    }

    /// Create a taxonomy manifest from source data
    async fn create_taxonomy_manifest(
        &self,
        source: TaxonomySource,
        data_path: &Path,
    ) -> Result<TaxonomyManifest> {
        use crate::taxonomy::manifest::TaxonomyVersionStats;

        // Calculate hashes for taxonomy files
        let nodes_path = data_path.join("nodes.dmp");
        let names_path = data_path.join("names.dmp");

        let nodes_hash = if nodes_path.exists() {
            SHA256Hash::compute(&fs::read(&nodes_path)?)
        } else {
            SHA256Hash::zero()
        };

        let names_hash = if names_path.exists() {
            SHA256Hash::compute(&fs::read(&names_path)?)
        } else {
            SHA256Hash::zero()
        };

        // Check for optional files
        let merged_path = data_path.join("merged.dmp");
        let merged_hash = if merged_path.exists() {
            Some(SHA256Hash::compute(&fs::read(&merged_path)?))
        } else {
            None
        };

        let delnodes_path = data_path.join("delnodes.dmp");
        let delnodes_hash = if delnodes_path.exists() {
            Some(SHA256Hash::compute(&fs::read(&delnodes_path)?))
        } else {
            None
        };

        // Count taxa (simplified - in production would parse files)
        let taxa_count = self.count_taxa(&nodes_path)?;

        let manifest = TaxonomyManifest {
            version: source.version_string(),
            created_at: Utc::now(),
            source: source.clone().into(),
            nodes_root: nodes_hash,
            names_root: names_hash,
            merged_root: merged_hash,
            delnodes_root: delnodes_hash,
            accession2taxid_root: None, // Will be set when processing accession files
            idmapping_root: None,       // Will be set for UniProt sources
            chunk_index: Vec::new(),    // Will be populated during chunking
            stats: TaxonomyVersionStats {
                total_taxa: taxa_count,
                species_count: 0, // Would need to parse nodes.dmp for rank info
                genus_count: 0,
                family_count: 0,
                deleted_count: 0,
                merged_count: 0,
            },
            etag: None,
            previous_version: self.get_previous_version(&source).await.ok(),
        };

        Ok(manifest)
    }

    /// Chunk taxonomy data into SEQUOIA storage
    async fn chunk_taxonomy_data(&self, data_path: &Path, version_path: &Path) -> Result<usize> {
        // For now, just copy files - in production would chunk into SEQUOIA
        let data_dir = version_path.join("data");
        fs::create_dir_all(&data_dir)?;

        let mut chunk_count = 0;

        // Copy taxonomy files
        for file_name in &["nodes.dmp", "names.dmp", "merged.dmp", "delnodes.dmp"] {
            let src = data_path.join(file_name);
            if src.exists() {
                let dst = data_dir.join(file_name);
                fs::copy(&src, &dst)?;
                chunk_count += 1;
            }
        }

        // Handle accession2taxid files
        let acc_path = data_path.join("prot.accession2taxid.gz");
        if acc_path.exists() {
            fs::copy(&acc_path, data_dir.join("prot.accession2taxid.gz"))?;
            chunk_count += 1;
        }

        Ok(chunk_count)
    }

    /// Calculate the total size of a version
    fn calculate_version_size(&self, version_path: &Path) -> Result<usize> {
        let mut total_size = 0;

        // Recursively calculate directory size
        for entry in std::fs::read_dir(version_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                total_size += self.calculate_version_size(&path)?;
            } else if path.is_file() {
                total_size += entry.metadata()?.len() as usize;
            }
        }

        Ok(total_size)
    }

    /// Count taxa in nodes.dmp file
    fn count_taxa(&self, nodes_path: &Path) -> Result<usize> {
        if !nodes_path.exists() {
            return Ok(0);
        }

        use std::io::{BufRead, BufReader};
        let file = fs::File::open(nodes_path)?;
        let reader = BufReader::new(file);

        Ok(reader.lines().count())
    }

    /// Get the previous version for a taxonomy source
    async fn get_previous_version(&self, source: &TaxonomySource) -> Result<String> {
        let db_source = database_source_from_taxonomy(source);
        let versions = self
            .base_store
            .list_versions(
                &db_source,
                ListOptions {
                    limit: Some(2),
                    newest_first: true,
                    ..Default::default()
                },
            )
            .await?;

        if versions.len() >= 2 {
            Ok(versions[1].id.clone())
        } else {
            anyhow::bail!("No previous version found")
        }
    }

    /// Get a taxonomy manifest by version ID
    pub async fn get_taxonomy_manifest(&mut self, version_id: &str) -> Result<TaxonomyManifest> {
        // Check cache first
        if let Some(manifest) = self.manifest_cache.get(version_id) {
            return Ok(manifest.clone());
        }

        // Load from disk
        let version_path = self.taxonomy_path.join("versions").join(version_id);
        let manifest_path = version_path.join("taxonomy_manifest.tal");

        let manifest_bytes = fs::read(&manifest_path).context(format!(
            "Failed to read taxonomy manifest for version {}",
            version_id
        ))?;

        let manifest: TaxonomyManifest = rmp_serde::from_slice(&manifest_bytes)?;

        // Update cache
        self.manifest_cache
            .insert(version_id.to_string(), manifest.clone());

        Ok(manifest)
    }
}

#[async_trait]
impl VersionStore for TaxonomyVersionStore {
    async fn list_versions(
        &self,
        source: &DatabaseSource,
        options: ListOptions,
    ) -> Result<Vec<Version>> {
        // Filter to only taxonomy versions
        let mut versions = self.base_store.list_versions(source, options).await?;

        // Filter by metadata type = "taxonomy"
        versions.retain(|v| {
            v.metadata
                .get("type")
                .map(|t| t == "taxonomy")
                .unwrap_or(false)
        });

        Ok(versions)
    }

    async fn current_version(&self, source: &DatabaseSource) -> Result<Version> {
        let versions = self
            .list_versions(
                source,
                ListOptions {
                    limit: Some(1),
                    newest_first: true,
                    ..Default::default()
                },
            )
            .await?;

        versions
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No taxonomy versions found"))
    }

    async fn get_version(&self, source: &DatabaseSource, version_id: &str) -> Result<Version> {
        self.base_store.get_version(source, version_id).await
    }

    async fn create_version(&mut self, source: &DatabaseSource) -> Result<Version> {
        // Create a new version through base store
        let mut version = self.base_store.create_version(source).await?;

        // Add taxonomy-specific metadata
        version
            .metadata
            .insert("type".to_string(), "taxonomy".to_string());

        Ok(version)
    }

    async fn delete_version(&mut self, source: &DatabaseSource, version_id: &str) -> Result<()> {
        // Clean up cached manifest
        self.manifest_cache.remove(version_id);
        self.base_store.delete_version(source, version_id).await
    }

    async fn update_alias(
        &mut self,
        source: &DatabaseSource,
        alias: &str,
        version_id: &str,
    ) -> Result<()> {
        self.base_store
            .update_alias(source, alias, version_id)
            .await
    }

    async fn resolve_alias(&self, source: &DatabaseSource, alias: &str) -> Result<Version> {
        self.base_store.resolve_alias(source, alias).await
    }

    async fn list_aliases(&self, source: &DatabaseSource) -> Result<HashMap<String, String>> {
        self.base_store.list_aliases(source).await
    }

    fn get_version_path(&self, source: &DatabaseSource, version_id: &str) -> PathBuf {
        self.base_store.get_version_path(source, version_id)
    }

    async fn cleanup_old_versions(
        &mut self,
        source: &DatabaseSource,
        keep_count: usize,
    ) -> Result<Vec<String>> {
        self.base_store
            .cleanup_old_versions(source, keep_count)
            .await
    }

    async fn get_storage_usage(&self, source: &DatabaseSource) -> Result<usize> {
        self.base_store.get_storage_usage(source).await
    }

    async fn export_metadata(&self, source: &DatabaseSource) -> Result<Vec<u8>> {
        self.base_store.export_metadata(source).await
    }

    async fn import_metadata(&mut self, source: &DatabaseSource, data: &[u8]) -> Result<()> {
        self.base_store.import_metadata(source, data).await
    }
}

/// Taxonomy source information
#[derive(Debug, Clone)]
pub enum TaxonomySource {
    NCBI {
        date: DateTime<Utc>,
    },
    UniProt {
        release: String,
        date: DateTime<Utc>,
    },
    Custom {
        name: String,
        version: String,
    },
}

impl TaxonomySource {
    pub fn name(&self) -> String {
        match self {
            Self::NCBI { .. } => "ncbi".to_string(),
            Self::UniProt { .. } => "uniprot".to_string(),
            Self::Custom { name, .. } => name.clone(),
        }
    }

    pub fn version_string(&self) -> String {
        match self {
            Self::NCBI { date } => date.format("%Y-%m-%d").to_string(),
            Self::UniProt { release, .. } => release.clone(),
            Self::Custom { version, .. } => version.clone(),
        }
    }
}

impl From<TaxonomySource> for ManifestTaxonomySource {
    fn from(source: TaxonomySource) -> Self {
        match source {
            TaxonomySource::NCBI { date } => ManifestTaxonomySource::NCBI {
                dump_date: date,
                ftp_url: "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/".to_string(),
            },
            TaxonomySource::UniProt { release, date } => {
                ManifestTaxonomySource::UniProt { release, date }
            }
            TaxonomySource::Custom { name, version } => {
                ManifestTaxonomySource::Custom { name, version }
            }
        }
    }
}

/// Create a DatabaseSource from a TaxonomySource
pub fn database_source_from_taxonomy(source: &TaxonomySource) -> talaria_core::DatabaseSource {
    use talaria_core::{DatabaseSource, NCBIDatabase, UniProtDatabase};

    match source {
        TaxonomySource::NCBI { .. } => DatabaseSource::NCBI(NCBIDatabase::Taxonomy),
        TaxonomySource::UniProt { .. } => DatabaseSource::UniProt(UniProtDatabase::IdMapping),
        TaxonomySource::Custom { name, .. } => DatabaseSource::Custom(name.clone()),
    }
}
