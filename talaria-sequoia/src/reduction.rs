/// SEQUOIA-based reduction manifest and management
///
/// Integrates the reduce/delta functionality with content-addressed storage,
/// providing cryptographic verification and efficient versioning.
use crate::types::*;
use crate::TargetAligner;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

/// Manifest for a reduced database
///
/// This represents a specific reduction of a database with a given profile.
/// It contains references to both the reference sequences (stored as chunks)
/// and the delta sequences (stored as delta chunks).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReductionManifest {
    /// Unique identifier for this reduction (hash of parameters + source)
    pub reduction_id: SHA256Hash,

    /// Profile name (e.g., "blast-30", "diamond-50")
    pub profile: String,

    /// Source database manifest this was reduced from
    pub source_manifest: SHA256Hash,

    /// Source database identifier
    pub source_database: String,

    /// Reduction parameters used
    pub parameters: ReductionParameters,

    /// Reference sequence chunks (the kept sequences)
    pub reference_chunks: Vec<ReferenceChunk>,

    /// Delta chunks (the compressed child sequences)
    pub delta_chunks: Vec<DeltaChunkRef>,

    /// Merkle root of all reference chunks
    pub reference_merkle_root: MerkleHash,

    /// Merkle root of all delta chunks
    pub delta_merkle_root: MerkleHash,

    /// Combined Merkle root for the entire reduction
    pub reduction_merkle_root: MerkleHash,

    /// Statistics about the reduction
    pub statistics: ReductionStatistics,

    /// When this reduction was created
    pub created_at: DateTime<Utc>,

    /// Version for compatibility
    pub version: String,

    /// Optional previous version of this reduction profile
    pub previous_version: Option<SHA256Hash>,
}

/// Parameters used for reduction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReductionParameters {
    /// Target reduction ratio (0.0 = auto-detect)
    pub reduction_ratio: f64,

    /// Target aligner optimization
    pub target_aligner: Option<TargetAligner>,

    /// Minimum sequence length
    pub min_length: usize,

    /// Similarity threshold for clustering
    pub similarity_threshold: f64,

    /// Whether taxonomy-aware reduction was used
    pub taxonomy_aware: bool,

    /// Whether alignment-based selection was used
    pub align_select: bool,

    /// Maximum alignment length
    pub max_align_length: usize,

    /// Whether deltas were disabled
    pub no_deltas: bool,
}

/// Reference to a chunk containing reference sequences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceChunk {
    /// Hash of the chunk content
    pub chunk_hash: SHA256Hash,

    /// Sequence IDs in this chunk
    pub sequence_ids: Vec<String>,

    /// Number of sequences
    pub sequence_count: usize,

    /// Uncompressed size
    pub size: usize,

    /// Compressed size if applicable
    pub compressed_size: Option<usize>,

    /// Taxonomic IDs if taxonomy-aware
    pub taxon_ids: Vec<TaxonId>,
}

/// Reference to a delta chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaChunkRef {
    /// Hash of the delta chunk
    pub chunk_hash: SHA256Hash,

    /// Reference chunk this delta is based on
    pub reference_chunk_hash: SHA256Hash,

    /// Number of child sequences in this chunk
    pub child_count: usize,

    /// Child sequence IDs
    pub child_ids: Vec<String>,

    /// Size of the delta chunk
    pub size: usize,

    /// Average number of delta operations per child
    pub avg_delta_ops: f32,
}

/// Statistics about a reduction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReductionStatistics {
    /// Original number of sequences
    pub original_sequences: usize,

    /// Number of reference sequences
    pub reference_sequences: usize,

    /// Number of child sequences
    pub child_sequences: usize,

    /// Original database size
    pub original_size: u64,

    /// Reduced size (references only)
    pub reduced_size: u64,

    /// Total size with deltas
    pub total_size_with_deltas: u64,

    /// Actual reduction ratio achieved
    pub actual_reduction_ratio: f64,

    /// Deduplication ratio from SEQUOIA
    pub deduplication_ratio: f64,

    /// Number of unique taxa covered
    pub unique_taxa: usize,

    /// Coverage percentage
    pub sequence_coverage: f64,

    /// Time taken for reduction
    pub reduction_time_secs: u64,
}

impl ReductionManifest {
    /// Create a new reduction manifest
    pub fn new(
        profile: String,
        source_manifest: SHA256Hash,
        source_database: String,
        parameters: ReductionParameters,
    ) -> Self {
        // Generate unique ID based on profile and source
        let id_string = format!("{}-{}-{}", profile, source_manifest, source_database);
        let reduction_id = SHA256Hash::compute(id_string.as_bytes());

        Self {
            reduction_id,
            profile,
            source_manifest,
            source_database,
            parameters,
            reference_chunks: Vec::new(),
            delta_chunks: Vec::new(),
            reference_merkle_root: SHA256Hash([0; 32]),
            delta_merkle_root: SHA256Hash([0; 32]),
            reduction_merkle_root: SHA256Hash([0; 32]),
            statistics: ReductionStatistics::default(),
            created_at: Utc::now(),
            version: "1.0.0".to_string(),
            previous_version: None,
        }
    }

    /// Add reference chunks to the manifest
    pub fn add_reference_chunks(&mut self, chunks: Vec<ReferenceChunk>) {
        self.reference_chunks.extend(chunks);
    }

    /// Add delta chunks to the manifest
    pub fn add_delta_chunks(&mut self, chunks: Vec<DeltaChunkRef>) {
        self.delta_chunks.extend(chunks);
    }

    /// Compute and update all Merkle roots
    pub fn compute_merkle_roots(&mut self) -> Result<()> {
        use crate::merkle::MerkleDAG;
        use crate::types::ChunkMetadata;

        // Build Merkle tree for reference chunks
        if !self.reference_chunks.is_empty() {
            let ref_chunks: Vec<ChunkMetadata> = self
                .reference_chunks
                .iter()
                .map(|c| ChunkMetadata {
                    hash: c.chunk_hash.clone(),
                    taxon_ids: Vec::new(),
                    sequence_count: c.sequence_ids.len(),
                    size: c.size,
                    compressed_size: c.compressed_size,
                })
                .collect();

            let ref_dag = MerkleDAG::build_from_items(ref_chunks)?;
            self.reference_merkle_root = ref_dag.root_hash().unwrap_or(SHA256Hash([0; 32]));
        }

        // Build Merkle tree for delta chunks
        if !self.delta_chunks.is_empty() {
            let delta_chunks_meta: Vec<ChunkMetadata> = self
                .delta_chunks
                .iter()
                .map(|c| ChunkMetadata {
                    hash: c.chunk_hash.clone(),
                    taxon_ids: Vec::new(),
                    sequence_count: 1, // Delta chunks typically represent individual sequences
                    size: c.size,
                    compressed_size: None,
                })
                .collect();

            let delta_dag = MerkleDAG::build_from_items(delta_chunks_meta)?;
            self.delta_merkle_root = delta_dag.root_hash().unwrap_or(SHA256Hash([0; 32]));
        }

        // Compute combined root
        let mut combined = Vec::new();
        combined.extend_from_slice(self.reference_merkle_root.as_bytes());
        combined.extend_from_slice(self.delta_merkle_root.as_bytes());
        self.reduction_merkle_root = SHA256Hash::compute(&combined);

        Ok(())
    }

    /// Calculate statistics for the reduction
    pub fn calculate_statistics(
        &mut self,
        original_sequences: usize,
        original_size: u64,
        reduction_time_secs: u64,
    ) {
        let reference_sequences = self.reference_chunks.iter().map(|c| c.sequence_count).sum();

        let child_sequences = self.delta_chunks.iter().map(|c| c.child_count).sum();

        let reduced_size = self.reference_chunks.iter().map(|c| c.size).sum::<usize>() as u64;

        let delta_size = self.delta_chunks.iter().map(|c| c.size).sum::<usize>() as u64;

        let total_size_with_deltas = reduced_size + delta_size;

        let actual_reduction_ratio = if original_sequences > 0 {
            reference_sequences as f64 / original_sequences as f64
        } else {
            0.0
        };

        // Calculate deduplication ratio (how much we saved through content addressing)
        let deduplication_ratio = if total_size_with_deltas > 0 {
            let compressed_total = self
                .reference_chunks
                .iter()
                .map(|c| c.compressed_size.unwrap_or(c.size))
                .sum::<usize>() as u64;
            1.0 - (compressed_total as f64 / total_size_with_deltas as f64)
        } else {
            0.0
        };

        let unique_taxa = self
            .reference_chunks
            .iter()
            .flat_map(|c| &c.taxon_ids)
            .collect::<std::collections::HashSet<_>>()
            .len();

        let sequence_coverage = if original_sequences > 0 {
            (reference_sequences + child_sequences) as f64 / original_sequences as f64
        } else {
            0.0
        };

        self.statistics = ReductionStatistics {
            original_sequences,
            reference_sequences,
            child_sequences,
            original_size,
            reduced_size,
            total_size_with_deltas,
            actual_reduction_ratio,
            deduplication_ratio,
            unique_taxa,
            sequence_coverage,
            reduction_time_secs,
        };
    }

    /// Verify the integrity of the reduction using Merkle proofs
    pub fn verify_integrity(&self) -> Result<bool> {
        // Recompute roots and compare
        let mut temp = self.clone();
        temp.compute_merkle_roots()?;

        Ok(temp.reduction_merkle_root == self.reduction_merkle_root)
    }

    /// Get all chunk hashes needed for this reduction
    pub fn get_all_chunk_hashes(&self) -> Vec<SHA256Hash> {
        let mut hashes = Vec::new();

        for chunk in &self.reference_chunks {
            hashes.push(chunk.chunk_hash.clone());
        }

        for chunk in &self.delta_chunks {
            hashes.push(chunk.chunk_hash.clone());
        }

        hashes
    }

    /// Get a mapping of child IDs to their reference chunks
    pub fn get_child_to_reference_map(&self) -> HashMap<String, SHA256Hash> {
        let mut map = HashMap::new();

        for delta_chunk in &self.delta_chunks {
            for child_id in &delta_chunk.child_ids {
                map.insert(child_id.clone(), delta_chunk.reference_chunk_hash.clone());
            }
        }

        map
    }
}

impl Default for ReductionStatistics {
    fn default() -> Self {
        Self {
            original_sequences: 0,
            reference_sequences: 0,
            child_sequences: 0,
            original_size: 0,
            reduced_size: 0,
            total_size_with_deltas: 0,
            actual_reduction_ratio: 0.0,
            deduplication_ratio: 0.0,
            unique_taxa: 0,
            sequence_coverage: 0.0,
            reduction_time_secs: 0,
        }
    }
}

/// Manager for reduction operations in SEQUOIA
pub struct ReductionManager {
    storage: crate::SEQUOIAStorage,
}

impl ReductionManager {
    pub fn new(storage: crate::SEQUOIAStorage) -> Self {
        Self { storage }
    }

    /// Save a reduction manifest
    pub fn save_manifest(&mut self, manifest: &ReductionManifest) -> Result<()> {
        let manifest_data = serde_json::to_vec(manifest)?;
        let manifest_hash = SHA256Hash::compute(&manifest_data);

        // Store the manifest as a special chunk
        self.storage
            .store_raw_chunk(&manifest_hash, manifest_data)?;

        Ok(())
    }

    /// Load a reduction manifest by its hash
    pub fn load_manifest(&self, hash: &SHA256Hash) -> Result<ReductionManifest> {
        let data = self.storage.get_chunk(hash)?;
        let manifest: ReductionManifest = serde_json::from_slice(&data)?;
        Ok(manifest)
    }

    /// List all available reduction profiles
    pub fn list_profiles(&self) -> Result<Vec<(String, SHA256Hash)>> {
        let profiles_dir = self.storage.base_path.join("profiles");
        let mut profiles = Vec::new();

        if profiles_dir.exists() {
            for entry in fs::read_dir(&profiles_dir)? {
                let entry = entry?;
                if entry.path().is_file() {
                    if let Some(profile_name) = entry.file_name().to_str() {
                        // Read the hash from the profile file
                        let hash_str = fs::read_to_string(entry.path())?;
                        if let Ok(hash) = SHA256Hash::from_hex(hash_str.trim()) {
                            profiles.push((profile_name.to_string(), hash));
                        }
                    }
                }
            }
        }

        Ok(profiles)
    }

    /// Get the latest version of a specific profile
    pub fn get_latest_profile(&self, profile_name: &str) -> Result<Option<ReductionManifest>> {
        // Use the storage's get_reduction_by_profile method
        self.storage.get_reduction_by_profile(profile_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reduction_manifest_creation() {
        let params = ReductionParameters {
            reduction_ratio: 0.3,
            target_aligner: Some(TargetAligner::Blast),
            min_length: 100,
            similarity_threshold: 0.9,
            taxonomy_aware: true,
            align_select: false,
            max_align_length: 10000,
            no_deltas: false,
        };

        let source_hash = SHA256Hash::compute(b"source");
        let manifest = ReductionManifest::new(
            "blast-30".to_string(),
            source_hash,
            "uniprot/swissprot".to_string(),
            params,
        );

        assert_eq!(manifest.profile, "blast-30");
        assert_eq!(manifest.source_database, "uniprot/swissprot");
    }

    #[test]
    fn test_merkle_root_computation() {
        let mut manifest = ReductionManifest::new(
            "test".to_string(),
            SHA256Hash::compute(b"source"),
            "test/db".to_string(),
            ReductionParameters::default(),
        );

        // Add some reference chunks
        manifest.add_reference_chunks(vec![ReferenceChunk {
            chunk_hash: SHA256Hash::compute(b"ref1"),
            sequence_ids: vec!["seq1".to_string()],
            sequence_count: 1,
            size: 1000,
            compressed_size: Some(500),
            taxon_ids: vec![TaxonId(9606)],
        }]);

        // Compute roots
        manifest.compute_merkle_roots().unwrap();

        // Verify we have non-zero roots
        assert_ne!(manifest.reference_merkle_root, SHA256Hash([0; 32]));
        assert_ne!(manifest.reduction_merkle_root, SHA256Hash([0; 32]));
    }
}

impl Default for ReductionParameters {
    fn default() -> Self {
        Self {
            reduction_ratio: 0.0,
            target_aligner: None,
            min_length: 0,
            similarity_threshold: 0.9,
            taxonomy_aware: false,
            align_select: false,
            max_align_length: 10000,
            no_deltas: false,
        }
    }
}
