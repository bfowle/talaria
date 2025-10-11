//! Content-Addressed Sequence Graph (SEQUOIA) System
//!
//! A modern approach to sequence database management using content-addressing,
//! Merkle DAGs, and taxonomy-aware chunking for efficient storage and verification.

// Core modules
pub mod traits;
pub mod types;

// Configuration
pub mod config;

// Storage and manifest
pub mod manifest;
pub mod storage;

// Chunking
pub mod chunker;

// Checkpoint system
pub mod checkpoint;

// Delta encoding
pub mod delta;

// Verification and validation
pub mod verification;

// Temporal and versioning
pub mod temporal;

// Operations
pub mod operations;

// Performance optimization
pub mod performance;

// Macro utilities
pub mod macros;

// Resilience and error recovery
pub mod resilience;

// Processing pipelines
pub mod processing;

// Taxonomy
pub mod taxonomy;

// Database management
pub mod database;

// Remote storage
pub mod remote;

// Backup functionality
pub mod backup;

// Cloud and sync
pub mod cloud;

// Download functionality
pub mod download;

// Re-export commonly used types
pub use chunker::{ChunkingStrategy, TaxonomicChunker};
pub use manifest::Manifest;
pub use storage::{
    ChunkAccessTracker, ChunkCompressor, ChunkIndexBuilder, ChunkMetadata, ChunkQuery,
    ChunkRelationships, CompressionConfig, DefaultChunkIndex, FormatDetector, IndexStatistics,
    JsonFormat, ManifestFormat, MessagePackFormat, OptimizationSuggestion, TalariaFormat,
};
pub use storage::{SequoiaStorage, StorageChunkInfo, StorageStats};
pub use types::*;

// Additional re-exports for tests
pub use delta::{
    CanonicalDelta, SequenceDeltaGenerator as DeltaGenerator,
    SequenceDeltaReconstructor as DeltaReconstructor,
};
pub use manifest::core::TALARIA_MAGIC;
pub use operations::state::ProcessingState as processing_state;
pub use operations::{
    format_bytes, AlignmentBasedSelector, AssemblyResult, ChunkAnalysis, DatabaseComparison,
    DatabaseDiffer, DiffResult, FastaAssembler, OperationType, ProcessingState, Reducer,
    ReductionManifest, ReductionParameters, ReferenceSelector, ReferenceSelectorImpl,
    SelectionAlgorithm, SelectionResult, SelectionStats, SequenceAnalysis, StorageMetrics,
    TaxonDistribution, TaxonomyAnalysis, TemporalManifestDiffer, TraitSelectionResult,
};
pub use storage::indices;
pub use storage::sequence::SequenceStorage as sequence_storage;
pub use talaria_storage::format;
pub use taxonomy::evolution::{
    MassReclassification, TaxonEvolutionReport, TaxonomyEvolutionTracker,
};
pub use taxonomy::filter as taxonomy_filter;
pub use temporal::{
    BiTemporalDatabase, RetroactiveAnalyzer, TaxonomicChangeType, TemporalIndex, TemporalQuery,
    Timeline, VersionInfo,
};
pub use types::{MerkleNode, MerkleProof};
pub use verification::MerkleDAG;
pub use verification::{ValidationResult, Validator, VerificationResult, Verifier};

// Repository structure that combines all components
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct SequoiaRepository {
    pub storage: SequoiaStorage,
    pub manifest: Manifest,
    pub taxonomy: taxonomy::TaxonomyManager,
    pub temporal: TemporalIndex,
}

impl SequoiaRepository {
    /// Initialize a new SEQUOIA repository
    pub fn init(base_path: &Path) -> Result<Self> {
        let storage = SequoiaStorage::new(base_path)?;
        let manifest = Manifest::new_with_path(base_path);
        let taxonomy = taxonomy::TaxonomyManager::load(base_path)?;
        let rocksdb = storage.sequence_storage.get_rocksdb();
        let temporal = TemporalIndex::new(base_path, rocksdb)?;

        Ok(Self {
            storage,
            manifest,
            taxonomy,
            temporal,
        })
    }

    /// Open an existing SEQUOIA repository
    pub fn open(base_path: &Path) -> Result<Self> {
        let storage = SequoiaStorage::open(base_path)?;
        let manifest =
            Manifest::load(base_path).unwrap_or_else(|_| Manifest::new_with_path(base_path));
        let taxonomy = taxonomy::TaxonomyManager::load(base_path)?;
        let rocksdb = storage.sequence_storage.get_rocksdb();
        let temporal = TemporalIndex::load(base_path, rocksdb)?;

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

    /// Check for updates from remote repository
    pub async fn check_updates(&self) -> Result<bool> {
        use crate::remote::ChunkClient;

        // Check if remote is configured
        let remote_url = std::env::var("TALARIA_MANIFEST_SERVER")
            .or_else(|_| std::env::var("TALARIA_REMOTE_REPO"))
            .or_else(|_| std::env::var("TALARIA_CHUNK_SERVER"));

        let remote_url = match remote_url {
            Ok(url) if !url.is_empty() => url,
            _ => return Ok(false), // No remote configured
        };

        // Create client and check remote manifest
        let client = ChunkClient::new(Some(remote_url.clone()))?;
        let remote_manifest = client.fetch_manifest().await?;

        // Compare versions
        let current_version = self.manifest.version().unwrap_or_default();
        let remote_version = remote_manifest.version.clone();

        // Parse timestamps from version strings (format: YYYY-MM-DD_HHMMSS)
        let parse_version = |v: &str| -> Option<i64> {
            // Try to parse as timestamp or date string
            v.parse::<i64>()
                .ok()
                .or_else(|| {
                    chrono::NaiveDateTime::parse_from_str(v, "%Y-%m-%d_%H%M%S")
                        .ok()
                        .map(|dt| dt.and_utc().timestamp())
                })
                .or_else(|| {
                    chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d")
                        .ok()
                        .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
                })
        };

        // Check if remote is newer
        match (
            parse_version(&current_version),
            parse_version(&remote_version),
        ) {
            (Some(current), Some(remote)) => Ok(remote > current),
            _ => {
                // Fallback to string comparison
                Ok(remote_version > current_version)
            }
        }
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
        let taxon_id_num: u32 = taxon
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid taxon ID: {}", taxon))?;
        let taxon_id = types::TaxonId(taxon_id_num);

        // Get manifest data
        let manifest_data = self
            .manifest
            .get_data()
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
        let filtered: Vec<_> = all_sequences
            .into_iter()
            .filter(|seq| seq.taxon_id == Some(taxon_id_num))
            .collect();

        Ok(filtered)
    }

    /// Extract sequences for exact taxon ID only
    pub fn extract_taxon_exact(
        &self,
        taxon_id: types::TaxonId,
    ) -> Result<Vec<talaria_bio::sequence::Sequence>> {
        self.extract_taxon(&taxon_id.0.to_string())
    }

    /// Extract sequences for taxon ID and all its descendants
    pub fn extract_taxon_with_descendants(
        &self,
        taxon_id: types::TaxonId,
    ) -> Result<Vec<talaria_bio::sequence::Sequence>> {
        // Load taxonomy to find descendants
        let taxonomy_path = talaria_core::system::paths::talaria_taxonomy_current_dir();
        let nodes_path = taxonomy_path.join("tree/nodes.dmp");

        if !nodes_path.exists() {
            // Fallback to exact match if taxonomy not available
            return self.extract_taxon_exact(taxon_id);
        }

        // Build list of taxon IDs (parent + descendants)
        let mut taxon_ids = HashSet::new();
        taxon_ids.insert(taxon_id);

        // Read taxonomy to find descendants
        use std::io::{BufRead, BufReader};
        let file = std::fs::File::open(&nodes_path)?;
        let reader = BufReader::new(file);

        // First pass: build parent-child relationships
        let mut children: HashMap<types::TaxonId, Vec<types::TaxonId>> = HashMap::new();
        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                if let (Ok(child_id), Ok(parent_id)) =
                    (parts[0].parse::<u32>(), parts[2].parse::<u32>())
                {
                    children
                        .entry(types::TaxonId(parent_id))
                        .or_default()
                        .push(types::TaxonId(child_id));
                }
            }
        }

        // Recursively find all descendants
        let mut to_process = vec![taxon_id];
        while let Some(current) = to_process.pop() {
            if let Some(child_list) = children.get(&current) {
                for child in child_list {
                    if taxon_ids.insert(*child) {
                        to_process.push(*child);
                    }
                }
            }
        }

        // Now extract sequences for all taxon IDs
        let manifest_data = self
            .manifest
            .get_data()
            .ok_or_else(|| anyhow::anyhow!("No manifest loaded"))?;

        // Find chunks containing any of these taxa
        let mut relevant_chunks = HashSet::new();
        for chunk_info in &manifest_data.chunk_index {
            for tid in &chunk_info.taxon_ids {
                if taxon_ids.contains(tid) {
                    relevant_chunks.insert(chunk_info.hash.clone());
                    break;
                }
            }
        }

        if relevant_chunks.is_empty() {
            return Ok(Vec::new());
        }

        // Load sequences from relevant chunks and filter
        let chunk_vec: Vec<_> = relevant_chunks.into_iter().collect();
        let all_sequences = self.load_sequences_from_chunks(&chunk_vec)?;
        let filtered: Vec<_> = all_sequences
            .into_iter()
            .filter(|seq| {
                seq.taxon_id
                    .map_or(false, |tid| taxon_ids.contains(&types::TaxonId(tid)))
            })
            .collect();

        Ok(filtered)
    }
}

// Import DatabaseSource types from talaria-core
pub use talaria_core::{DatabaseSource, NCBIDatabase, TargetAligner, UniProtDatabase};
