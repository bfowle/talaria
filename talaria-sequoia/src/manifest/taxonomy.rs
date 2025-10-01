use crate::types::{SHA256HashExt, *};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Manifest for taxonomy data stored as SEQUOIA chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyManifest {
    /// Version identifier (e.g., "2024-03-15")
    pub version: String,

    /// When this taxonomy dump was created
    pub created_at: DateTime<Utc>,

    /// Source of the taxonomy data
    pub source: TaxonomySource,

    /// Merkle root of nodes.dmp chunks
    pub nodes_root: SHA256Hash,

    /// Merkle root of names.dmp chunks
    pub names_root: SHA256Hash,

    /// Merkle root of merged.dmp chunks (for merged taxon IDs)
    pub merged_root: Option<SHA256Hash>,

    /// Merkle root of delnodes.dmp chunks (for deleted taxon IDs)
    pub delnodes_root: Option<SHA256Hash>,

    /// Merkle root of accession2taxid chunks
    pub accession2taxid_root: Option<SHA256Hash>,

    /// For UniProt: idmapping chunks
    pub idmapping_root: Option<SHA256Hash>,

    /// Chunk index for all taxonomy files
    pub chunk_index: Vec<TaxonomyChunkMetadata>,

    /// Statistics about this taxonomy version
    pub stats: TaxonomyManifestStats,

    /// ETag for checking updates
    pub etag: Option<String>,

    /// Previous version for chaining
    pub previous_version: Option<String>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyChunkMetadata {
    /// Which file this chunk belongs to
    pub file_type: TaxonomyFileType,

    /// Hash of this chunk
    pub hash: SHA256Hash,

    /// Range of taxon IDs in this chunk (for efficient lookups)
    pub taxon_range: Option<(TaxonId, TaxonId)>,

    /// Size of the chunk
    pub byte_size: usize,

    /// Compressed size if applicable
    pub compressed_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaxonomyFileType {
    Nodes,           // nodes.dmp
    Names,           // names.dmp
    Merged,          // merged.dmp
    Deleted,         // delnodes.dmp
    Accession2Taxid, // prot.accession2taxid
    IdMapping,       // UniProt idmapping.dat
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaxonomyManifestStats {
    /// Total number of taxon IDs
    pub total_taxa: usize,

    /// Number of species-level taxa
    pub species_count: usize,

    /// Number of genus-level taxa
    pub genus_count: usize,

    /// Total accession mappings
    pub accession_count: Option<usize>,

    /// Number of merged taxon IDs
    pub merged_count: Option<usize>,

    /// Number of deleted taxon IDs
    pub deleted_count: Option<usize>,
}

impl TaxonomyManifest {
    /// Create a new taxonomy manifest
    pub fn new(source: TaxonomySource, version: String) -> Self {
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
            stats: TaxonomyManifestStats::default(),
            etag: None,
            previous_version: None,
        }
    }

    /// Check if this manifest is newer than another
    pub fn is_newer_than(&self, other: &Self) -> bool {
        self.created_at > other.created_at
    }

    /// Compute diff with another manifest
    pub fn diff(&self, other: &Self) -> TaxonomyDiff {
        let mut new_chunks = Vec::new();
        let modified_chunks = Vec::new();
        let mut deleted_chunks = Vec::new();

        // Build hash maps for efficient lookup
        let self_chunks: HashMap<SHA256Hash, &TaxonomyChunkMetadata> = self
            .chunk_index
            .iter()
            .map(|c| (c.hash.clone(), c))
            .collect();
        let other_chunks: HashMap<SHA256Hash, &TaxonomyChunkMetadata> = other
            .chunk_index
            .iter()
            .map(|c| (c.hash.clone(), c))
            .collect();

        // Find new and modified chunks
        for chunk in &self.chunk_index {
            if !other_chunks.contains_key(&chunk.hash) {
                new_chunks.push(chunk.hash.clone());
            }
        }

        // Find deleted chunks
        for chunk in &other.chunk_index {
            if !self_chunks.contains_key(&chunk.hash) {
                deleted_chunks.push(chunk.hash.clone());
            }
        }

        TaxonomyDiff {
            new_chunks,
            modified_chunks,
            deleted_chunks,
            stats_changes: self.compute_stats_changes(&other.stats),
        }
    }

    fn compute_stats_changes(&self, other: &TaxonomyManifestStats) -> StatsChanges {
        StatsChanges {
            taxa_added: self.stats.total_taxa.saturating_sub(other.total_taxa),
            taxa_removed: other.total_taxa.saturating_sub(self.stats.total_taxa),
            species_changed: (self.stats.species_count as i64 - other.species_count as i64)
                .unsigned_abs() as usize,
            accessions_changed: match (self.stats.accession_count, other.accession_count) {
                (Some(a), Some(b)) => Some((a as i64 - b as i64).unsigned_abs() as usize),
                _ => None,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaxonomyDiff {
    pub new_chunks: Vec<SHA256Hash>,
    pub modified_chunks: Vec<SHA256Hash>,
    pub deleted_chunks: Vec<SHA256Hash>,
    pub stats_changes: StatsChanges,
}

#[derive(Debug, Clone)]
pub struct StatsChanges {
    pub taxa_added: usize,
    pub taxa_removed: usize,
    pub species_changed: usize,
    pub accessions_changed: Option<usize>,
}

impl TaxonomyDiff {
    /// Calculate total download size needed
    pub fn download_size(&self, chunk_sizes: &HashMap<SHA256Hash, usize>) -> usize {
        self.new_chunks
            .iter()
            .chain(&self.modified_chunks)
            .filter_map(|hash| chunk_sizes.get(hash))
            .sum()
    }

    /// Check if any updates are needed
    pub fn has_updates(&self) -> bool {
        !self.new_chunks.is_empty() || !self.modified_chunks.is_empty()
    }
}
