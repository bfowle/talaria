/// Core types for the SEQUOIA system
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Re-export only primitive types from talaria-core
pub use talaria_core::types::{SHA256Hash, TaxonId};

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

// Additional methods for SHA256Hash (extending talaria-core's implementation)
use sha2::{Digest, Sha256};

pub trait SHA256HashExt {
    fn zero() -> SHA256Hash;
    fn is_zero(&self) -> bool;
    fn from_bytes(bytes: &[u8]) -> SHA256Hash;
}

impl SHA256HashExt for SHA256Hash {
    /// Create a zero hash (all zeros)
    fn zero() -> Self {
        Self::default()
    }

    /// Check if hash is zero (uninitialized)
    fn is_zero(&self) -> bool {
        self.as_ref().iter().all(|&b| b == 0)
    }

    /// Create hash from bytes by hashing them
    fn from_bytes(bytes: &[u8]) -> Self {
        Self::compute(bytes)
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
    pub serialized_nodes: Vec<u8>, // Compact binary representation
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

// TaxonId is now imported from talaria_core::types

/// Reference to a sequence within a chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceRef {
    pub chunk_hash: SHA256Hash,
    pub offset: usize,
    pub length: usize,
    pub sequence_id: String,
}

/// Discrepancy between taxonomy annotations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomicDiscrepancy {
    pub sequence_id: String,
    pub header_taxon: Option<TaxonId>, // What the FASTA header claims
    pub mapped_taxon: Option<TaxonId>, // What accession2taxid says
    pub inferred_taxon: Option<TaxonId>, // What we infer from similarity
    pub confidence: f32,
    pub detection_date: DateTime<Utc>,
    pub discrepancy_type: DiscrepancyType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscrepancyType {
    Missing,      // No taxonomy information
    Conflict,     // Different sources disagree
    Outdated,     // Using old taxonomy
    Reclassified, // Taxonomy has been updated
    Invalid,      // Invalid taxon ID
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
    pub taxonomy_dump_version: String, // e.g., "2024-03-15"

    /// Source database identifier
    pub source_database: Option<String>,

    pub chunk_index: Vec<ManifestMetadata>,
    pub discrepancies: Vec<TaxonomicDiscrepancy>,
    pub etag: String,
    pub previous_version: Option<String>,
}

/// Manifest metadata for tracking chunks with detailed statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestMetadata {
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
    pub sequence_proof: MerkleProof,  // Proves sequence existed
    pub taxonomy_proof: MerkleProof,  // Proves classification
    pub temporal_link: CrossTimeHash, // Links sequence to taxonomy version
    #[serde(with = "datetime_serde")]
    pub timestamp: DateTime<Utc>,
    pub attestation: CryptographicSeal, // Optional external timestamp
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
    pub target_chunk_size: usize,        // Target size in bytes
    pub max_chunk_size: usize,           // Maximum allowed size
    pub min_sequences_per_chunk: usize,  // Minimum sequences
    pub taxonomic_coherence: f32,        // 0.0 to 1.0
    pub special_taxa: Vec<SpecialTaxon>, // Special handling
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        Self {
            target_chunk_size: 10 * 1024 * 1024,  // 10MB
            max_chunk_size: 50 * 1024 * 1024,     // 50MB
            min_sequences_per_chunk: 10,
            taxonomic_coherence: 0.8,
            special_taxa: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecialTaxon {
    pub taxon_id: TaxonId,
    pub name: String,
    pub strategy: ChunkStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkStrategy {
    OwnChunks,         // Always separate chunks (e.g., E. coli)
    GroupWithSiblings, // Group with taxonomic siblings
    GroupAtLevel(u8),  // Group at specific taxonomic level
}

// Re-export from talaria-core
pub use talaria_core::types::UpdateStatus;

/// Classification of chunk content with embedded metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

/// Compression format for chunks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChunkFormat {
    /// Legacy JSON + gzip format (for backward compatibility)
    JsonGzip,
    /// Binary MessagePack for metadata + Zstd for sequences
    #[default]
    Binary,
    /// Binary with dictionary-based compression
    BinaryDict { dict_id: u32 },
}


impl ChunkFormat {
    /// Get the file extension for this format
    pub fn extension(&self) -> &str {
        // All formats use .tal extension for consistency
        ".tal"
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
            && data[3] == 0xfd
        {
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
pub struct TemporalDeltaChunk {
    /// Hash of this delta chunk
    pub content_hash: SHA256Hash,
    /// Reference to the base chunk this delta is computed from
    pub reference_hash: SHA256Hash,
    /// Type of chunk
    pub chunk_type: ChunkClassification,
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
    Insert { sequence_id: String, data: Vec<u8> },
    /// Apply modifications to reference sequence
    Modify {
        sequence_id: String,
        reference_offset: usize,
        operations: Vec<SeqEdit>,
    },
    /// Delete sequence (tombstone)
    Delete { sequence_id: String },
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
    pub chunk_type: ChunkClassification,
    pub taxon_ids: Vec<TaxonId>,
    pub sequence_count: usize,
    pub size: usize,
    pub compressed_size: Option<usize>,
    /// For delta chunks, the chain of references
    pub reference_chain: Vec<SHA256Hash>,
    /// Compression statistics
    pub compression_ratio: Option<f32>,
}

// ============================================================================
// CANONICAL SEQUENCE ARCHITECTURE - True Content-Addressed Storage
// ============================================================================

/// Trait for content that can be addressed by its hash
pub trait ContentAddressable {
    /// Compute the content hash for this item
    fn content_hash(&self) -> SHA256Hash;

    /// Serialize to bytes for storage
    fn to_bytes(&self) -> Result<Vec<u8>, anyhow::Error>;

    /// Deserialize from bytes
    fn from_bytes(bytes: &[u8]) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}

/// Trait for sequences that can have multiple representations
pub trait Representable {
    /// Get all representations for this item
    fn representations(&self) -> &[SequenceRepresentation];

    /// Add a new representation
    fn add_representation(&mut self, repr: SequenceRepresentation);

    /// Get representation for a specific source
    fn get_representation(&self, source: &DatabaseSource) -> Option<&SequenceRepresentation>;
}

/// Trait for items that can be indexed
pub trait Indexable {
    /// Get indexable keys (e.g., accessions)
    fn index_keys(&self) -> Vec<String>;

    /// Get taxonomic classification
    fn taxon_id(&self) -> Option<TaxonId>;
}

/// Trait for verifiable data structures
pub trait MerkleVerifiable {
    /// Compute Merkle root hash
    fn merkle_root(&self) -> MerkleHash;

    /// Verify inclusion proof
    fn verify_proof(&self, item_hash: &SHA256Hash, proof: &[SHA256Hash]) -> bool;
}

// Import SequenceType from talaria-core
pub use talaria_core::SequenceType;

// DatabaseSource is now imported from talaria-core
pub use talaria_core::types::database::DatabaseSourceInfo as DatabaseSource;

/// Canonical sequence - the pure biological sequence data
/// This is what gets deduplicated across all databases
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

impl ContentAddressable for CanonicalSequence {
    fn content_hash(&self) -> SHA256Hash {
        self.sequence_hash.clone()
    }

    fn to_bytes(&self) -> Result<Vec<u8>, anyhow::Error> {
        Ok(rmp_serde::to_vec(self)?)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, anyhow::Error> {
        Ok(rmp_serde::from_slice(bytes)?)
    }
}

/// Database-specific representation of a sequence
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
    pub timestamp: DateTime<Utc>,
}

/// Collection of all representations for a canonical sequence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceRepresentations {
    /// Hash of the canonical sequence this represents
    pub canonical_hash: SHA256Hash,

    /// All known representations
    pub representations: Vec<SequenceRepresentation>,
}

impl Representable for SequenceRepresentations {
    fn representations(&self) -> &[SequenceRepresentation] {
        &self.representations
    }

    fn add_representation(&mut self, repr: SequenceRepresentation) {
        // Check if we already have a representation from this source
        if let Some(existing) = self.representations.iter_mut()
            .find(|r| r.source == repr.source && r.header == repr.header) {
            // Update timestamp
            existing.timestamp = repr.timestamp;
        } else {
            self.representations.push(repr);
        }
    }

    fn get_representation(&self, source: &DatabaseSource) -> Option<&SequenceRepresentation> {
        self.representations.iter()
            .find(|r| r.source == *source)
    }
}

impl Indexable for SequenceRepresentations {
    fn index_keys(&self) -> Vec<String> {
        self.representations.iter()
            .flat_map(|r| r.accessions.clone())
            .collect()
    }

    fn taxon_id(&self) -> Option<TaxonId> {
        // Return the most common taxon_id from all representations
        let mut taxon_counts: HashMap<TaxonId, usize> = HashMap::new();
        for repr in &self.representations {
            if let Some(taxon) = repr.taxon_id {
                *taxon_counts.entry(taxon).or_default() += 1;
            }
        }
        taxon_counts.into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(taxon, _)| taxon)
    }
}

/// Chunk manifest - references to canonical sequences instead of containing them
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

impl ContentAddressable for ChunkManifest {
    fn content_hash(&self) -> SHA256Hash {
        // Hash is computed from the sorted sequence references
        let mut refs = self.sequence_refs.clone();
        refs.sort();
        let data: Vec<u8> = refs.iter()
            .flat_map(|h| h.as_bytes().iter())
            .copied()
            .collect();
        SHA256Hash::compute(&data)
    }

    fn to_bytes(&self) -> Result<Vec<u8>, anyhow::Error> {
        Ok(rmp_serde::to_vec(self)?)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, anyhow::Error> {
        Ok(rmp_serde::from_slice(bytes)?)
    }
}

impl MerkleVerifiable for ChunkManifest {
    fn merkle_root(&self) -> MerkleHash {
        // Build Merkle tree from sequence references
        if self.sequence_refs.is_empty() {
            return SHA256Hash::zero();
        }

        // Simple Merkle tree implementation
        let mut level = self.sequence_refs.clone();
        while level.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in level.chunks(2) {
                let combined = if chunk.len() == 2 {
                    let mut data = Vec::new();
                    data.extend(chunk[0].as_bytes());
                    data.extend(chunk[1].as_bytes());
                    SHA256Hash::compute(&data)
                } else {
                    chunk[0].clone()
                };
                next_level.push(combined);
            }
            level = next_level;
        }
        level[0].clone()
    }

    fn verify_proof(&self, item_hash: &SHA256Hash, proof: &[SHA256Hash]) -> bool {
        let mut current = item_hash.clone();
        for sibling in proof {
            let mut data = Vec::new();
            // Order matters in Merkle proof
            if current < *sibling {
                data.extend(current.as_bytes());
                data.extend(sibling.as_bytes());
            } else {
                data.extend(sibling.as_bytes());
                data.extend(current.as_bytes());
            }
            current = SHA256Hash::compute(&data);
        }
        current == self.merkle_root()
    }
}

use std::collections::HashMap;
