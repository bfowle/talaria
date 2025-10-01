/// Storage-specific types for talaria-storage
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use talaria_core::types::{DatabaseSource, SequenceType, SHA256Hash, TaxonId};
use talaria_core::StorageStats;

// Custom serialization module for DateTime to handle MessagePack
mod datetime_serde {
    use chrono::{DateTime, TimeZone, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(date.timestamp())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let timestamp = i64::deserialize(deserializer)?;
        Ok(Utc
            .timestamp_opt(timestamp, 0)
            .single()
            .unwrap_or_else(Utc::now))
    }
}

/// Chunk storage format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChunkFormat {
    /// Binary MessagePack for metadata + Zstd for sequences
    #[default]
    Binary,
    /// Binary with dictionary-based compression
    BinaryDict { dict_id: u32 },
}

impl ChunkFormat {
    /// Detect format from data magic bytes
    pub fn detect(data: &[u8]) -> Self {
        // Detect zstd magic bytes: 0x28, 0xB5, 0x2F, 0xFD
        if data.len() >= 4 && data[0] == 0x28 && data[1] == 0xB5 && data[2] == 0x2F && data[3] == 0xFD {
            ChunkFormat::Binary
        } else {
            // No compression detected - treat as uncompressed
            ChunkFormat::Binary
        }
    }

    /// Check if data is compressed with zstd
    pub fn is_compressed(data: &[u8]) -> bool {
        data.len() >= 4 && data[0] == 0x28 && data[1] == 0xB5 && data[2] == 0x2F && data[3] == 0xFD
    }
}

/// Canonical representation of a sequence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalSequence {
    /// Hash of the sequence content ONLY (not headers)
    pub sequence_hash: SHA256Hash,

    /// The actual sequence data (amino acids or nucleotides)
    #[serde(with = "serde_bytes")]
    pub sequence: Vec<u8>,

    /// Length of the sequence
    pub length: usize,

    /// Type of sequence
    pub sequence_type: SequenceType,

    /// CRC64 checksum for quick validation
    pub checksum: u64,

    /// When this sequence was first seen (across all databases)
    #[serde(with = "datetime_serde")]
    pub first_seen: DateTime<Utc>,

    /// When this sequence was last seen
    #[serde(with = "datetime_serde")]
    pub last_seen: DateTime<Utc>,
}

/// A single representation of a sequence from a specific database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceRepresentation {
    /// Which database this representation comes from
    pub source: DatabaseSource,

    /// Original FASTA header from this database
    pub header: String,

    /// Extracted accession numbers
    pub accessions: Vec<String>,

    /// Description parsed from header
    pub description: Option<String>,

    /// Taxonomic ID (may differ between databases!)
    pub taxon_id: Option<TaxonId>,

    /// Database-specific metadata
    pub metadata: HashMap<String, String>,

    /// When we last saw this representation
    #[serde(with = "datetime_serde")]
    pub last_seen: DateTime<Utc>,
}

/// Collection of all known representations for a canonical sequence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceRepresentations {
    /// Hash of the canonical sequence this represents
    pub canonical_hash: SHA256Hash,

    /// All known representations
    pub representations: Vec<SequenceRepresentation>,
}

impl SequenceRepresentations {
    /// Get all representations
    pub fn representations(&self) -> &[SequenceRepresentation] {
        &self.representations
    }

    /// Add a new representation
    pub fn add_representation(&mut self, repr: SequenceRepresentation) {
        // Check if we already have a representation from this source
        if let Some(existing) = self
            .representations
            .iter_mut()
            .find(|r| r.source == repr.source && r.header == repr.header)
        {
            // Update last_seen timestamp
            existing.last_seen = repr.last_seen;
        } else {
            self.representations.push(repr);
        }
    }

    /// Get a representation for a specific source
    pub fn get_representation(&self, source: &DatabaseSource) -> Option<&SequenceRepresentation> {
        self.representations.iter().find(|r| &r.source == source)
    }
}

/// Reference to a sequence in a chunk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SequenceRef {
    pub chunk_hash: SHA256Hash,
    pub offset: usize,
    pub length: usize,
    pub sequence_id: String,
}

/// Classification of chunk content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkClassification {
    /// Full sequence data in FASTA format
    Full,
    /// Delta-compressed sequences referencing other chunks
    Delta {
        reference_hash: SHA256Hash,
        compression_ratio: f32,
    },
    /// Index chunk for fast lookups
    Index,
    /// Metadata-only chunk
    Metadata,
}

/// Manifest describing a chunk's contents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkManifest {
    /// Hash of this manifest (computed from sequence_refs)
    pub chunk_hash: SHA256Hash,

    /// References to canonical sequences
    pub sequence_refs: Vec<SHA256Hash>,

    /// Taxonomic scope of this chunk
    pub taxon_ids: Vec<TaxonId>,

    /// Organization strategy
    pub chunk_type: ChunkClassification,

    /// Statistics
    pub total_size: usize,
    pub sequence_count: usize,

    /// Temporal metadata
    #[serde(with = "datetime_serde")]
    pub created_at: DateTime<Utc>,

    /// Version information
    pub taxonomy_version: SHA256Hash,
    pub sequence_version: SHA256Hash,
}

/// Trait for sequence storage backends
pub trait SequenceStorageBackend: Send + Sync {
    /// Check if a canonical sequence exists
    fn sequence_exists(&self, hash: &SHA256Hash) -> Result<bool>;

    /// Batch check existence of multiple sequences - much faster for bulk operations
    /// Default implementation calls sequence_exists for each hash
    fn sequences_exist_batch(&self, hashes: &[SHA256Hash]) -> Result<Vec<bool>> {
        hashes
            .iter()
            .map(|hash| self.sequence_exists(hash))
            .collect()
    }

    /// Store a canonical sequence
    fn store_canonical(&self, sequence: &CanonicalSequence) -> Result<()>;

    /// Batch store multiple canonical sequences for improved I/O performance
    /// Default implementation calls store_canonical for each sequence
    fn store_canonical_batch(&self, sequences: &[CanonicalSequence]) -> Result<()> {
        for sequence in sequences {
            self.store_canonical(sequence)?;
        }
        Ok(())
    }

    /// Load a canonical sequence
    fn load_canonical(&self, hash: &SHA256Hash) -> Result<CanonicalSequence>;

    /// Store representations for a sequence
    fn store_representations(&self, representations: &SequenceRepresentations) -> Result<()>;

    /// Load representations for a sequence
    fn load_representations(&self, hash: &SHA256Hash) -> Result<SequenceRepresentations>;

    /// Get storage statistics
    fn get_stats(&self) -> Result<StorageStats>;

    /// List all sequence hashes in storage
    fn list_all_hashes(&self) -> Result<Vec<SHA256Hash>>;

    /// Get the size of a sequence
    fn get_sequence_size(&self, hash: &SHA256Hash) -> Result<usize>;

    /// Remove a sequence from storage
    fn remove_sequence(&self, hash: &SHA256Hash) -> Result<()>;

    /// Flush any pending writes to disk
    fn flush(&self) -> Result<()>;

    /// Get a reference to self as Any for downcasting
    fn as_any(&self) -> &dyn Any;
}