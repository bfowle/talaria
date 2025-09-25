//! Content-Addressed Sequence Graph (SEQUOIA) System
//!
//! A modern approach to sequence database management using content-addressing,
//! Merkle DAGs, and taxonomy-aware chunking for efficient storage and verification.

// Core modules
pub mod types;
pub mod traits;

// Storage and manifest
pub mod storage;
pub mod manifest;

// Chunking
pub mod chunker;

// Delta encoding
pub mod delta;

// Verification and validation
pub mod verification;

// Temporal and versioning
pub mod temporal;

// Operations
pub mod operations;

// Taxonomy
pub mod taxonomy;

// Cloud and sync
pub mod cloud;

// Re-export commonly used types
pub use types::*;
pub use storage::{SEQUOIAStorage, StorageChunkInfo, StorageStats};
pub use manifest::Manifest;
pub use chunker::{TaxonomicChunker, ChunkingStrategy};
pub use storage::{
    ChunkIndexBuilder, ChunkQuery, ChunkAccessTracker, DefaultChunkIndex,
    ChunkRelationships, IndexStatistics, OptimizationSuggestion,
    ChunkCompressor, CompressionConfig,
    FormatDetector, ManifestFormat, JsonFormat, MessagePackFormat, TalariaFormat
};
pub use verification::MerkleDAG;
pub use types::{MerkleNode, MerkleProof};
pub use verification::{Verifier, VerificationResult, Validator, ValidationResult};
pub use operations::{
    FastaAssembler, AssemblyResult, TemporalManifestDiffer, DiffResult,
    ReductionManifest, ReductionParameters, ProcessingState, OperationType
};
pub use temporal::{
    TemporalIndex, BiTemporalDatabase, RetroactiveAnalyzer,
    VersionInfo, TemporalQuery, Timeline, TaxonomicChangeType
};
pub use delta::{SequenceDeltaGenerator as DeltaGenerator, SequenceDeltaReconstructor as DeltaReconstructor, CanonicalDelta};
pub use taxonomy::evolution::{TaxonomyEvolutionTracker, MassReclassification, TaxonEvolutionReport};

// Repository structure that combines all components
use std::path::Path;
use anyhow::Result;

pub struct SEQUOIARepository {
    pub storage: SEQUOIAStorage,
    pub manifest: Manifest,
    pub taxonomy: taxonomy::TaxonomyManager,
    pub temporal: TemporalIndex,
}

impl SEQUOIARepository {
    /// Initialize a new SEQUOIA repository
    pub fn init(base_path: &Path) -> Result<Self> {
        let storage = SEQUOIAStorage::new(base_path)?;
        let manifest = Manifest::new_with_path(base_path);
        let taxonomy = taxonomy::TaxonomyManager::new(base_path)?;
        let temporal = TemporalIndex::new(base_path)?;

        Ok(Self {
            storage,
            manifest,
            taxonomy,
            temporal,
        })
    }

    /// Open an existing SEQUOIA repository
    pub fn open(base_path: &Path) -> Result<Self> {
        let storage = SEQUOIAStorage::open(base_path)?;
        let manifest = Manifest::load(base_path).unwrap_or_else(|_| Manifest::new_with_path(base_path));
        let taxonomy = taxonomy::TaxonomyManager::load(base_path)?;
        let temporal = TemporalIndex::load(base_path)?;

        Ok(Self {
            storage,
            manifest,
            taxonomy,
            temporal,
        })
    }

    /// Save the repository state (manifest and indices)
    pub fn save(&self) -> Result<()> {
        // Save the manifest
        self.manifest.save()?;

        // Save temporal index if needed
        self.temporal.save()?;

        // Taxonomy manager saves itself automatically

        Ok(())
    }

    /// Check for updates (placeholder for now)
    pub async fn check_updates(&self) -> Result<bool> {
        // TODO: Implement actual update checking logic
        Ok(false)
    }

    /// Verify the integrity of the repository
    pub fn verify(&self) -> Result<()> {
        // Verify storage integrity
        // TODO: Implement verify_integrity for SEQUOIAStorage
        // self.storage.verify_integrity()?;

        // Verify manifest
        if let Err(e) = self.manifest.verify() {
            anyhow::bail!("Manifest verification failed: {}", e);
        }

        // Verify temporal index if present
        if self.temporal.base_path.exists() {
            // Basic temporal index check
            let _ = self.temporal.get_current_version()?;
        }

        Ok(())
    }

    /// Load sequences from chunk manifests
    pub fn load_sequences_from_chunks(
        &self,
        chunk_hashes: &[SHA256Hash],
    ) -> Result<Vec<talaria_bio::sequence::Sequence>> {
        let mut sequences = Vec::new();

        for hash in chunk_hashes {
            let chunk_data = self.storage.get_chunk(hash)?;
            let manifest: types::ChunkManifest = bincode::deserialize(&chunk_data)?;

            // Load actual sequences from canonical storage
            for seq_hash in &manifest.sequence_refs {
                if let Ok(canonical) = self.storage.sequence_storage.load_canonical(seq_hash) {
                    // Convert canonical to bio sequence
                    // This is a simplified conversion - you may need to load representations too
                    sequences.push(talaria_bio::sequence::Sequence {
                        id: seq_hash.to_hex(),
                        description: None,
                        sequence: canonical.sequence,
                        taxon_id: None,
                        taxonomy_sources: Default::default(),
                    });
                }
            }
        }

        Ok(sequences)
    }

    /// Extract sequences for a specific taxon
    pub fn extract_taxon(&self, taxon: &str) -> Result<Vec<talaria_bio::sequence::Sequence>> {
        // Parse taxon ID
        let taxon_id_num: u32 = taxon.parse()
            .map_err(|_| anyhow::anyhow!("Invalid taxon ID: {}", taxon))?;
        let taxon_id = types::TaxonId(taxon_id_num);

        // Get manifest data
        let manifest_data = self.manifest.get_data()
            .ok_or_else(|| anyhow::anyhow!("No manifest loaded"))?;

        // Find chunks containing this taxon
        let mut relevant_chunks = Vec::new();
        for chunk_info in &manifest_data.chunk_index {
            if chunk_info.taxon_ids.contains(&taxon_id) {
                relevant_chunks.push(chunk_info.hash.clone());
            }
        }

        if relevant_chunks.is_empty() {
            return Ok(Vec::new());
        }

        // Load sequences from relevant chunks and filter
        let all_sequences = self.load_sequences_from_chunks(&relevant_chunks)?;
        let filtered: Vec<_> = all_sequences.into_iter()
            .filter(|seq| seq.taxon_id == Some(taxon_id_num))
            .collect();

        Ok(filtered)
    }
}

// Import DatabaseSource types from talaria-core
pub use talaria_core::{DatabaseSource, UniProtDatabase, NCBIDatabase};

// CLI-related types that SEQUOIA needs
// These should ideally be moved to a shared crate
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TargetAligner {
    Lambda,
    Blast,
    Kraken,
    Diamond,
    MMseqs2,
    Generic,
}

// Output-related stubs for SEQUOIA modules that need them
pub mod output {
    #[derive(Debug, Clone)]
    pub struct TreeNode {
        pub label: String,
        pub value: Option<String>,
        pub children: Vec<TreeNode>,
    }

    impl TreeNode {
        pub fn new(label: &str) -> Self {
            Self {
                label: label.to_string(),
                value: None,
                children: Vec::new(),
            }
        }

        pub fn with_value(mut self, value: String) -> Self {
            self.value = Some(value);
            self
        }

        pub fn with_children(mut self, children: Vec<TreeNode>) -> Self {
            self.children = children;
            self
        }

        pub fn add_child(mut self, child: TreeNode) -> Self {
            self.children.push(child);
            self
        }
    }

    pub fn create_standard_table() -> comfy_table::Table {
        comfy_table::Table::new()
    }

    pub fn format_number(n: usize) -> String {
        n.to_string()
    }

    pub fn header_cell(s: &str) -> comfy_table::Cell {
        comfy_table::Cell::new(s)
    }
}