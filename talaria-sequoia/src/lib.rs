//! Content-Addressed Sequence Graph (SEQUOIA) System
//!
//! A modern approach to sequence database management using content-addressing,
//! Merkle DAGs, and taxonomy-aware chunking for efficient storage and verification.

// Core modules
pub mod types;
pub mod traits;
pub mod version_store;

// Storage and manifest
pub mod storage;
pub mod manifest;
pub mod taxonomy_manifest;

// Chunking and compression
pub mod chunker;
pub mod chunk_index;
pub mod compression;
pub mod format;

// Delta encoding
pub mod delta;
pub mod delta_generator;
pub mod delta_reconstructor;

// Merkle and verification
pub mod merkle;
pub mod verifier;
pub mod validator;

// Temporal and evolution
pub mod temporal;
pub mod temporal_renderable;
pub mod retroactive;
pub mod evolution_tracker;

// Operations
pub mod assembler;
pub mod differ;
pub mod reduction;
pub mod processing_state;

// Taxonomy
pub mod taxonomy;

// Cloud and sync
pub mod cloud;

// Re-export commonly used types
pub use types::*;
pub use storage::SEQUOIAStorage;
pub use manifest::Manifest;
pub use chunker::{Chunker, TaxonomicChunker, TaxonomyAwareChunker};
pub use chunk_index::{
    ChunkIndexBuilder, ChunkQuery, ChunkAccessTracker, DefaultChunkIndex,
    ChunkRelationships, IndexStatistics, OptimizationSuggestion,
};
pub use compression::ChunkCompressor;
pub use format::{FormatDetector, ManifestFormat, JsonFormat, MessagePackFormat, TalariaFormat};
pub use merkle::{MerkleDAG, MerkleVerifiable};
pub use verifier::{SEQUOIAVerifier, VerificationResult};
pub use validator::{
    TemporalManifestValidator, StandardTemporalManifestValidator, ValidationOptions, ValidationResult,
};
pub use differ::{
    TemporalManifestDiffer, StandardTemporalManifestDiffer, DiffResult, DiffOptions, ChangeType,
};
pub use reduction::{ReductionManifest, ReductionParameters};
pub use assembler::FastaAssembler;
pub use evolution_tracker::{TaxonomyEvolutionTracker, MassReclassification, TaxonEvolutionReport};
pub use processing_state::{ProcessingState, ProcessingStateManager, OperationType, SourceInfo};
pub use temporal::TemporalIndex;
pub use retroactive::RetroactiveAnalyzer;

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

    /// Check for updates (placeholder for now)
    pub async fn check_updates(&self) -> Result<bool> {
        // TODO: Implement actual update checking logic
        Ok(false)
    }

    /// Verify the integrity of the repository
    pub fn verify(&self) -> Result<()> {
        // Verify storage integrity
        self.storage.verify_integrity()?;

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

    /// Load sequences from chunks
    pub fn load_sequences_from_chunks(
        &self,
        chunk_hashes: &[SHA256Hash],
    ) -> Result<Vec<talaria_bio::sequence::Sequence>> {
        let mut sequences = Vec::new();

        for hash in chunk_hashes {
            let chunk_data = self.storage.get_chunk(hash)?;
            let chunk: types::TaxonomyAwareChunk = bincode::deserialize(&chunk_data)?;

            // Parse FASTA data from the chunk
            let fasta_sequences = talaria_bio::fasta::parse_fasta_from_bytes(&chunk.sequence_data)?;

            for seq in fasta_sequences {
                sequences.push(seq);
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

// Database source types needed by taxonomy
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DatabaseSource {
    UniProt(UniProtDatabase),
    NCBI(NCBIDatabase),
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum UniProtDatabase {
    SwissProt,
    TrEMBL,
    IdMapping,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NCBIDatabase {
    Taxonomy,
    ProtAccession2TaxId,
    RefSeq,
    NR,
    NT,
}

impl std::fmt::Display for UniProtDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UniProtDatabase::SwissProt => write!(f, "SwissProt"),
            UniProtDatabase::TrEMBL => write!(f, "TrEMBL"),
            UniProtDatabase::IdMapping => write!(f, "IdMapping"),
        }
    }
}

impl std::fmt::Display for NCBIDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NCBIDatabase::Taxonomy => write!(f, "Taxonomy"),
            NCBIDatabase::ProtAccession2TaxId => write!(f, "ProtAccession2TaxId"),
            NCBIDatabase::RefSeq => write!(f, "RefSeq"),
            NCBIDatabase::NR => write!(f, "NR"),
            NCBIDatabase::NT => write!(f, "NT"),
        }
    }
}

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