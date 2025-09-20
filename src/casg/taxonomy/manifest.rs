use crate::casg::types::{SHA256Hash, TaxonId};
/// Taxonomy manifest for versioned taxonomy databases
///
/// This manifest tracks taxonomy database versions and their content hashes,
/// enabling bi-temporal versioning of both sequences and taxonomic classifications
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Manifest for a specific version of taxonomy database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyManifest {
    /// Version identifier (e.g., "2024-03-15" for NCBI, "2024_04" for UniProt)
    pub version: String,

    /// When this version was created/downloaded
    pub created_at: DateTime<Utc>,

    /// Source of the taxonomy data
    pub source: TaxonomySource,

    /// Root hash of nodes.dmp or equivalent
    pub nodes_root: SHA256Hash,

    /// Root hash of names.dmp or equivalent
    pub names_root: SHA256Hash,

    /// Root hash of merged.dmp if present
    pub merged_root: Option<SHA256Hash>,

    /// Root hash of delnodes.dmp if present
    pub delnodes_root: Option<SHA256Hash>,

    /// Root hash of accession2taxid mappings
    pub accession2taxid_root: Option<SHA256Hash>,

    /// Root hash of UniProt idmapping if present
    pub idmapping_root: Option<SHA256Hash>,

    /// Index of chunks containing taxonomy data
    pub chunk_index: Vec<TaxonomyChunkMetadata>,

    /// Statistics about this taxonomy version
    pub stats: TaxonomyStats,

    /// ETag for change detection
    pub etag: Option<String>,

    /// Previous version this was derived from
    pub previous_version: Option<String>,
}

/// Source of taxonomy data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaxonomySource {
    NCBI {
        dump_date: DateTime<Utc>,
        ftp_url: String,
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

/// Metadata for a taxonomy data chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyChunkMetadata {
    /// SHA256 hash of the chunk
    pub hash: SHA256Hash,

    /// Type of data in this chunk
    pub data_type: TaxonomyDataType,

    /// Range of taxon IDs in this chunk (for efficient lookups)
    pub taxon_range: Option<(TaxonId, TaxonId)>,

    /// Size in bytes
    pub size: usize,

    /// Number of entries
    pub entry_count: usize,
}

/// Type of taxonomy data in a chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaxonomyDataType {
    Nodes,           // Taxonomy hierarchy
    Names,           // Taxon names
    Merged,          // Merged taxon IDs
    Deleted,         // Deleted taxon IDs
    Accession2Taxid, // Accession to taxon ID mappings
    IdMapping,       // UniProt ID mappings
}

/// Statistics about a taxonomy version
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaxonomyStats {
    pub total_taxa: usize,
    pub species_count: usize,
    pub genus_count: usize,
    pub family_count: usize,
    pub deleted_count: usize,
    pub merged_count: usize,
}

impl TaxonomyManifest {
    /// Create a new taxonomy manifest
    pub fn new(version: String, source: TaxonomySource) -> Self {
        Self {
            version,
            created_at: Utc::now(),
            source,
            nodes_root: SHA256Hash::zero(),
            names_root: SHA256Hash::zero(),
            merged_root: None,
            delnodes_root: None,
            accession2taxid_root: None,
            idmapping_root: None,
            chunk_index: Vec::new(),
            stats: TaxonomyStats::default(),
            etag: None,
            previous_version: None,
        }
    }

    /// Compute overall manifest hash
    pub fn compute_hash(&self) -> SHA256Hash {
        let mut data = Vec::new();

        // Include all root hashes
        data.extend_from_slice(&self.nodes_root.0);
        data.extend_from_slice(&self.names_root.0);

        if let Some(ref hash) = self.merged_root {
            data.extend_from_slice(&hash.0);
        }
        if let Some(ref hash) = self.delnodes_root {
            data.extend_from_slice(&hash.0);
        }
        if let Some(ref hash) = self.accession2taxid_root {
            data.extend_from_slice(&hash.0);
        }
        if let Some(ref hash) = self.idmapping_root {
            data.extend_from_slice(&hash.0);
        }

        // Include version and source info
        data.extend_from_slice(self.version.as_bytes());
        data.extend_from_slice(format!("{:?}", self.source).as_bytes());

        SHA256Hash::compute(&data)
    }

    /// Check if manifest represents a complete taxonomy
    pub fn is_complete(&self) -> bool {
        !self.nodes_root.is_zero() && !self.names_root.is_zero()
    }

    /// Check if accession mappings are available
    pub fn has_accession_mappings(&self) -> bool {
        self.accession2taxid_root.is_some() || self.idmapping_root.is_some()
    }

    /// Get source name as string
    pub fn source_name(&self) -> &str {
        match &self.source {
            TaxonomySource::NCBI { .. } => "NCBI",
            TaxonomySource::UniProt { .. } => "UniProt",
            TaxonomySource::Custom { name, .. } => name,
        }
    }

    /// Serialize to MessagePack format
    pub fn to_msgpack(&self) -> Result<Vec<u8>> {
        rmp_serde::to_vec(self).context("Failed to serialize taxonomy manifest")
    }

    /// Deserialize from MessagePack format
    pub fn from_msgpack(data: &[u8]) -> Result<Self> {
        rmp_serde::from_slice(data).context("Failed to deserialize taxonomy manifest")
    }

    /// Add a chunk to the index
    pub fn add_chunk(&mut self, metadata: TaxonomyChunkMetadata) {
        self.chunk_index.push(metadata);
    }

    /// Find chunks containing a specific taxon ID
    pub fn find_chunks_for_taxon(&self, taxon_id: TaxonId) -> Vec<&TaxonomyChunkMetadata> {
        self.chunk_index
            .iter()
            .filter(|chunk| {
                if let Some((min, max)) = &chunk.taxon_range {
                    taxon_id.0 >= min.0 && taxon_id.0 <= max.0
                } else {
                    // Include chunks without range (like names, merged, etc.)
                    true
                }
            })
            .collect()
    }

    /// Get total size of all chunks
    pub fn total_size(&self) -> usize {
        self.chunk_index.iter().map(|c| c.size).sum()
    }

    /// Create a diff with another manifest
    pub fn diff(&self, other: &TaxonomyManifest) -> TaxonomyDiff {
        TaxonomyDiff {
            old_version: self.version.clone(),
            new_version: other.version.clone(),
            nodes_changed: self.nodes_root != other.nodes_root,
            names_changed: self.names_root != other.names_root,
            merged_changed: self.merged_root != other.merged_root,
            deleted_changed: self.delnodes_root != other.delnodes_root,
            accession_changed: self.accession2taxid_root != other.accession2taxid_root,
            idmapping_changed: self.idmapping_root != other.idmapping_root,
            stats_diff: TaxonomyStatsDiff {
                total_taxa_delta: other.stats.total_taxa as i64 - self.stats.total_taxa as i64,
                species_delta: other.stats.species_count as i64 - self.stats.species_count as i64,
                genus_delta: other.stats.genus_count as i64 - self.stats.genus_count as i64,
                family_delta: other.stats.family_count as i64 - self.stats.family_count as i64,
            },
        }
    }
}

/// Difference between two taxonomy manifests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyDiff {
    pub old_version: String,
    pub new_version: String,
    pub nodes_changed: bool,
    pub names_changed: bool,
    pub merged_changed: bool,
    pub deleted_changed: bool,
    pub accession_changed: bool,
    pub idmapping_changed: bool,
    pub stats_diff: TaxonomyStatsDiff,
}

/// Statistics difference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyStatsDiff {
    pub total_taxa_delta: i64,
    pub species_delta: i64,
    pub genus_delta: i64,
    pub family_delta: i64,
}

impl TaxonomyDiff {
    /// Check if any changes occurred
    pub fn has_changes(&self) -> bool {
        self.nodes_changed
            || self.names_changed
            || self.merged_changed
            || self.deleted_changed
            || self.accession_changed
            || self.idmapping_changed
    }

    /// Get a summary of changes
    pub fn summary(&self) -> String {
        let mut changes = Vec::new();

        if self.nodes_changed {
            changes.push("taxonomy hierarchy");
        }
        if self.names_changed {
            changes.push("taxon names");
        }
        if self.merged_changed {
            changes.push("merged taxa");
        }
        if self.deleted_changed {
            changes.push("deleted taxa");
        }
        if self.accession_changed {
            changes.push("accession mappings");
        }
        if self.idmapping_changed {
            changes.push("ID mappings");
        }

        if changes.is_empty() {
            format!(
                "No changes between {} and {}",
                self.old_version, self.new_version
            )
        } else {
            format!(
                "Changes from {} to {}: {}",
                self.old_version,
                self.new_version,
                changes.join(", ")
            )
        }
    }
}


// SHA256Hash methods are already implemented in crate::casg::types

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_creation() {
        let manifest = TaxonomyManifest::new(
            "2024-03-15".to_string(),
            TaxonomySource::NCBI {
                dump_date: Utc::now(),
                ftp_url: "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/".to_string(),
            },
        );

        assert_eq!(manifest.version, "2024-03-15");
        assert_eq!(manifest.source_name(), "NCBI");
        assert!(!manifest.is_complete());
        assert!(!manifest.has_accession_mappings());
    }

    #[test]
    fn test_manifest_diff() {
        let mut manifest1 = TaxonomyManifest::new(
            "2024-03-01".to_string(),
            TaxonomySource::NCBI {
                dump_date: Utc::now(),
                ftp_url: "".to_string(),
            },
        );
        manifest1.stats.total_taxa = 1000;

        let mut manifest2 = manifest1.clone();
        manifest2.version = "2024-03-15".to_string();
        manifest2.nodes_root = SHA256Hash::compute(b"new_nodes");
        manifest2.stats.total_taxa = 1050;

        let diff = manifest1.diff(&manifest2);
        assert!(diff.has_changes());
        assert!(diff.nodes_changed);
        assert_eq!(diff.stats_diff.total_taxa_delta, 50);
    }

    #[test]
    fn test_chunk_lookup() {
        let mut manifest = TaxonomyManifest::new(
            "test".to_string(),
            TaxonomySource::Custom {
                name: "test".to_string(),
                version: "1.0".to_string(),
            },
        );

        manifest.add_chunk(TaxonomyChunkMetadata {
            hash: SHA256Hash::compute(b"chunk1"),
            data_type: TaxonomyDataType::Nodes,
            taxon_range: Some((TaxonId(1), TaxonId(1000))),
            size: 1024,
            entry_count: 100,
        });

        manifest.add_chunk(TaxonomyChunkMetadata {
            hash: SHA256Hash::compute(b"chunk2"),
            data_type: TaxonomyDataType::Nodes,
            taxon_range: Some((TaxonId(1001), TaxonId(2000))),
            size: 1024,
            entry_count: 100,
        });

        let chunks = manifest.find_chunks_for_taxon(TaxonId(500));
        assert_eq!(chunks.len(), 1);

        let chunks = manifest.find_chunks_for_taxon(TaxonId(1500));
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_manifest_serialization() {
        let manifest = TaxonomyManifest::new(
            "2024_04".to_string(),
            TaxonomySource::UniProt {
                release: "2024_04".to_string(),
                date: Utc::now(),
            },
        );

        let msgpack = manifest.to_msgpack().unwrap();
        let deserialized = TaxonomyManifest::from_msgpack(&msgpack).unwrap();

        assert_eq!(deserialized.version, manifest.version);
        assert_eq!(deserialized.source_name(), manifest.source_name());
    }
}
