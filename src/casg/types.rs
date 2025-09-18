/// Core types for the CASG system

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::fmt;

// Custom serialization module for DateTime to handle MessagePack
mod datetime_serde {
    use chrono::{DateTime, Utc, TimeZone};
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
        Ok(Utc.timestamp_opt(timestamp, 0).single().unwrap_or_else(Utc::now))
    }
}

/// SHA256 hash type
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SHA256Hash(#[serde(with = "serde_bytes")] pub [u8; 32]);

impl SHA256Hash {
    pub fn compute(data: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        Self(hash)
    }

    pub fn from_hex(hex: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(hex)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
        Ok(Self(hash))
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Create a zero hash (all zeros)
    pub fn zero() -> Self {
        Self([0u8; 32])
    }

    /// Check if hash is zero (uninitialized)
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }
}

impl fmt::Display for SHA256Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Merkle hash alias
pub type MerkleHash = SHA256Hash;

/// Serialized representation of a Merkle tree for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedMerkleTree {
    pub root_hash: MerkleHash,
    pub node_count: usize,
    #[serde(with = "serde_bytes")]
    pub serialized_nodes: Vec<u8>,  // Compact binary representation
}

/// Bi-temporal coordinate for versioning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiTemporalCoordinate {
    #[serde(with = "datetime_serde")]
    pub sequence_time: DateTime<Utc>,
    #[serde(with = "datetime_serde")]
    pub taxonomy_time: DateTime<Utc>,
}

impl BiTemporalCoordinate {
    /// Create a coordinate at a specific time (both dimensions)
    pub fn at(time: DateTime<Utc>) -> Self {
        Self {
            sequence_time: time,
            taxonomy_time: time,
        }
    }

    /// Create a coordinate at current time
    pub fn now() -> Self {
        let now = Utc::now();
        Self {
            sequence_time: now,
            taxonomy_time: now,
        }
    }

    /// Create with separate times
    pub fn new(sequence_time: DateTime<Utc>, taxonomy_time: DateTime<Utc>) -> Self {
        Self {
            sequence_time,
            taxonomy_time,
        }
    }
}

/// Taxonomic identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaxonId(pub u32);

impl fmt::Display for TaxonId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "taxid:{}", self.0)
    }
}

/// Reference to a sequence within a chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceRef {
    pub chunk_hash: SHA256Hash,
    pub offset: usize,
    pub length: usize,
    pub sequence_id: String,
}

/// A chunk containing sequences with taxonomic context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyAwareChunk {
    pub content_hash: SHA256Hash,
    pub taxonomy_version: MerkleHash,
    pub sequence_version: MerkleHash,
    pub taxon_ids: Vec<TaxonId>,
    pub sequences: Vec<SequenceRef>,
    #[serde(with = "serde_bytes")]
    pub sequence_data: Vec<u8>,  // The actual FASTA-format sequence data
    pub created_at: DateTime<Utc>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub size: usize,
    pub compressed_size: Option<usize>,
}

/// Discrepancy between taxonomy annotations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomicDiscrepancy {
    pub sequence_id: String,
    pub header_taxon: Option<TaxonId>,     // What the FASTA header claims
    pub mapped_taxon: Option<TaxonId>,     // What accession2taxid says
    pub inferred_taxon: Option<TaxonId>,   // What we infer from similarity
    pub confidence: f32,
    pub detection_date: DateTime<Utc>,
    pub discrepancy_type: DiscrepancyType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscrepancyType {
    Missing,           // No taxonomy information
    Conflict,          // Different sources disagree
    Outdated,          // Using old taxonomy
    Reclassified,      // Taxonomy has been updated
    Invalid,           // Invalid taxon ID
}

/// Manifest for efficient update checking with Merkle tree support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalManifest {
    pub version: String,
    #[serde(with = "datetime_serde")]
    pub created_at: DateTime<Utc>,

    /// Bi-temporal versioning
    pub sequence_version: String,
    pub taxonomy_version: String,

    /// Bi-temporal coordinate for this manifest
    pub temporal_coordinate: Option<BiTemporalCoordinate>,

    /// Merkle roots - these are now actual tree roots
    pub taxonomy_root: MerkleHash,
    pub sequence_root: MerkleHash,

    /// Merkle tree for chunk verification
    pub chunk_merkle_tree: Option<SerializedMerkleTree>,

    /// Reference to the taxonomy manifest used
    pub taxonomy_manifest_hash: SHA256Hash,
    pub taxonomy_dump_version: String,  // e.g., "2024-03-15"

    /// Source database identifier
    pub source_database: Option<String>,

    pub chunk_index: Vec<ChunkMetadata>,
    pub discrepancies: Vec<TaxonomicDiscrepancy>,
    pub etag: String,
    pub previous_version: Option<String>,
}

/// Metadata for a single chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub hash: SHA256Hash,
    pub taxon_ids: Vec<TaxonId>,
    pub sequence_count: usize,
    pub size: usize,
    pub compressed_size: Option<usize>,
}


/// Merkle tree node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleNode {
    pub hash: MerkleHash,
    pub left: Option<Box<MerkleNode>>,
    pub right: Option<Box<MerkleNode>>,
    pub data: Option<Vec<u8>>,
}

impl MerkleNode {
    pub fn leaf(data: Vec<u8>) -> Self {
        let hash = SHA256Hash::compute(&data);
        Self {
            hash,
            left: None,
            right: None,
            data: Some(data),
        }
    }

    pub fn branch(left: MerkleNode, right: MerkleNode) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(left.hash.as_bytes());
        hasher.update(right.hash.as_bytes());
        let result = hasher.finalize();
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&result);

        Self {
            hash: SHA256Hash(hash_bytes),
            left: Some(Box::new(left)),
            right: Some(Box::new(right)),
            data: None,
        }
    }
}

/// Proof of inclusion in a Merkle tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf_hash: MerkleHash,
    pub root_hash: MerkleHash,
    pub path: Vec<ProofStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofStep {
    pub hash: MerkleHash,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Position {
    Left,
    Right,
}

/// Temporal proof linking sequences to taxonomy at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalProof {
    pub sequence_proof: MerkleProof,       // Proves sequence existed
    pub taxonomy_proof: MerkleProof,       // Proves classification
    pub temporal_link: CrossTimeHash,      // Links sequence to taxonomy version
    #[serde(with = "datetime_serde")]
    pub timestamp: DateTime<Utc>,
    pub attestation: CryptographicSeal,    // Optional external timestamp
}

/// Hash linking across time dimensions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossTimeHash {
    pub sequence_time: DateTime<Utc>,
    pub taxonomy_time: DateTime<Utc>,
    pub combined_hash: SHA256Hash,
}

/// Cryptographic seal for external verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptographicSeal {
    pub timestamp: DateTime<Utc>,
    pub signature: Vec<u8>,
    pub authority: String,
}

/// Result of a diff operation
#[derive(Debug)]
pub struct ManifestDiff {
    pub new_chunks: Vec<SHA256Hash>,
    pub removed_chunks: Vec<SHA256Hash>,
    pub modified_chunks: Vec<SHA256Hash>,
    pub taxonomy_changes: TaxonomyChanges,
}

/// Changes in taxonomy
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaxonomyChanges {
    pub reclassifications: Vec<Reclassification>,
    pub new_taxa: Vec<TaxonId>,
    pub deprecated_taxa: Vec<TaxonId>,
    pub merged_taxa: Vec<(TaxonId, TaxonId)>, // (old, new)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reclassification {
    pub taxon_id: TaxonId,
    pub old_parent: TaxonId,
    pub new_parent: TaxonId,
    pub reason: String,
}

/// Configuration for chunking strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingStrategy {
    pub target_chunk_size: usize,           // Target size in bytes
    pub max_chunk_size: usize,              // Maximum allowed size
    pub min_sequences_per_chunk: usize,     // Minimum sequences
    pub taxonomic_coherence: f32,           // 0.0 to 1.0
    pub special_taxa: Vec<SpecialTaxon>,    // Special handling
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecialTaxon {
    pub taxon_id: TaxonId,
    pub name: String,
    pub strategy: ChunkStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkStrategy {
    OwnChunks,          // Always separate chunks (e.g., E. coli)
    GroupWithSiblings,  // Group with taxonomic siblings
    GroupAtLevel(u8),   // Group at specific taxonomic level
}

/// Update check result
#[derive(Debug)]
pub struct UpdateStatus {
    pub updates_available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub changes_summary: String,
    pub estimated_download_size: usize,
}

/// Information about a stored chunk
#[derive(Debug, Clone)]
pub struct ChunkInfo {
    pub hash: SHA256Hash,
    pub path: std::path::PathBuf,
    pub size: usize,
    pub compressed: bool,
    pub format: ChunkFormat,
}

/// Type of chunk content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChunkType {
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

/// Compression format for chunks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkFormat {
    /// Legacy JSON + gzip format (for backward compatibility)
    JsonGzip,
    /// Binary MessagePack for metadata + Zstd for sequences
    Binary,
    /// Binary with dictionary-based compression
    BinaryDict { dict_id: u32 },
}

impl Default for ChunkFormat {
    fn default() -> Self {
        ChunkFormat::Binary  // Use efficient format by default
    }
}

impl ChunkFormat {
    /// Get the file extension for this format
    pub fn extension(&self) -> &str {
        match self {
            ChunkFormat::JsonGzip => ".json.gz",
            ChunkFormat::Binary => ".bin.zst",
            ChunkFormat::BinaryDict { .. } => ".dict.zst",
        }
    }

    /// Detect format from file contents
    pub fn detect(data: &[u8]) -> Self {
        // Check for gzip magic bytes (1f 8b)
        if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
            return ChunkFormat::JsonGzip;
        }

        // Check for Zstandard magic bytes (28 b5 2f fd)
        if data.len() >= 4
            && data[0] == 0x28
            && data[1] == 0xb5
            && data[2] == 0x2f
            && data[3] == 0xfd {
            // Could be Binary or BinaryDict, check for dictionary
            // For now, assume Binary (dictionary detection would need more context)
            return ChunkFormat::Binary;
        }

        // Default to legacy format if unknown
        ChunkFormat::JsonGzip
    }
}

/// Delta chunk containing compressed sequence differences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaChunk {
    /// Hash of this delta chunk
    pub content_hash: SHA256Hash,
    /// Reference to the base chunk this delta is computed from
    pub reference_hash: SHA256Hash,
    /// Type of chunk
    pub chunk_type: ChunkType,
    /// Taxonomy information
    pub taxonomy_version: MerkleHash,
    pub taxon_ids: Vec<TaxonId>,
    /// Delta operations to reconstruct sequences
    pub deltas: Vec<DeltaOperation>,
    /// Sequence references in this chunk
    pub sequences: Vec<SequenceRef>,
    /// Temporal metadata
    pub created_at: DateTime<Utc>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    /// Size information
    pub original_size: usize,
    pub compressed_size: usize,
    pub compression_ratio: f32,
}

/// Operations for delta reconstruction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeltaOperation {
    /// Use sequence from reference chunk
    UseReference {
        sequence_id: String,
        reference_offset: usize,
        length: usize,
    },
    /// Insert new sequence data
    Insert {
        sequence_id: String,
        data: Vec<u8>,
    },
    /// Apply modifications to reference sequence
    Modify {
        sequence_id: String,
        reference_offset: usize,
        operations: Vec<SeqEdit>,
    },
    /// Delete sequence (tombstone)
    Delete {
        sequence_id: String,
    },
}

/// Sequence edit operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SeqEdit {
    Substitute { pos: usize, new_base: u8 },
    Insert { pos: usize, bases: Vec<u8> },
    Delete { pos: usize, count: usize },
}

/// Extended chunk metadata with delta support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedChunkMetadata {
    pub hash: SHA256Hash,
    pub chunk_type: ChunkType,
    pub taxon_ids: Vec<TaxonId>,
    pub sequence_count: usize,
    pub size: usize,
    pub compressed_size: Option<usize>,
    /// For delta chunks, the chain of references
    pub reference_chain: Vec<SHA256Hash>,
    /// Compression statistics
    pub compression_ratio: Option<f32>,
}