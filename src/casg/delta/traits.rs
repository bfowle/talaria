/// Trait definitions for delta encoding and reconstruction
///
/// Provides abstractions for different delta encoding strategies
/// and reconstruction algorithms.

use anyhow::Result;
use crate::bio::sequence::Sequence;
use crate::casg::types::*;

/// Configuration for delta generation
#[derive(Debug, Clone)]
pub struct DeltaGeneratorConfig {
    /// Maximum chunk size in bytes
    pub max_chunk_size: usize,
    /// Minimum similarity threshold for delta encoding
    pub min_similarity_threshold: f32,
    /// Whether to compress delta chunks
    pub enable_compression: bool,
    /// Target sequences per chunk for batching
    pub target_sequences_per_chunk: usize,
    /// Maximum delta operations before falling back to full storage
    pub max_delta_ops_threshold: usize,
}

impl Default for DeltaGeneratorConfig {
    fn default() -> Self {
        Self {
            max_chunk_size: 16 * 1024 * 1024, // 16MB
            min_similarity_threshold: 0.85,
            enable_compression: true,
            target_sequences_per_chunk: 1000,
            max_delta_ops_threshold: 100,
        }
    }
}

/// Common interface for delta generation strategies
pub trait DeltaGenerator: Send + Sync {
    /// Generate delta chunks from sequences with references
    fn generate_delta_chunks(
        &mut self,
        sequences: &[Sequence],
        references: &[Sequence],
        reference_hash: SHA256Hash,
    ) -> Result<Vec<DeltaChunk>>;

    /// Find the best reference sequence for a given sequence
    fn find_best_reference<'a>(
        &self,
        seq: &Sequence,
        references: &'a [Sequence],
    ) -> Result<(&'a Sequence, f32)>;

    /// Get the current configuration
    fn config(&self) -> &DeltaGeneratorConfig;

    /// Set configuration
    fn set_config(&mut self, config: DeltaGeneratorConfig);

    /// Calculate similarity between two sequences
    fn calculate_similarity(&self, seq1: &Sequence, seq2: &Sequence) -> f32;

    /// Check if delta encoding is worth it for a pair
    fn should_use_delta(
        &self,
        seq: &Sequence,
        reference: &Sequence,
        similarity: f32,
    ) -> bool;

    /// Get the name of this delta generator
    fn name(&self) -> &str;
}

/// Interface for delta reconstruction
pub trait DeltaReconstructor: Send + Sync {
    /// Reconstruct sequences from a delta chunk
    fn reconstruct_sequences(
        &self,
        delta_chunk: &DeltaChunk,
        reference_chunk: &TaxonomyAwareChunk,
    ) -> Result<Vec<Sequence>>;

    /// Apply delta operations to reconstruct a sequence
    fn apply_delta_operations(
        &self,
        operations: &[DeltaOperation],
        reference_data: &[u8],
    ) -> Result<Vec<u8>>;

    /// Apply a single delta operation
    fn apply_operation(
        &self,
        operation: &DeltaOperation,
        current_data: &mut Vec<u8>,
        reference_data: &[u8],
    ) -> Result<()>;

    /// Validate reconstructed sequence
    fn validate_reconstruction(
        &self,
        original_hash: &SHA256Hash,
        reconstructed: &[u8],
    ) -> Result<bool>;

    /// Get the name of this reconstructor
    fn name(&self) -> &str;
}

/// Extended delta generator with compression optimization
pub trait CompressionAwareDeltaGenerator: DeltaGenerator {
    /// Estimate compression ratio for delta encoding
    fn estimate_compression_ratio(
        &self,
        seq: &Sequence,
        reference: &Sequence,
    ) -> f32;

    /// Optimize delta operations for better compression
    fn optimize_for_compression(
        &self,
        operations: Vec<DeltaOperation>,
    ) -> Vec<DeltaOperation>;

    /// Choose optimal encoding strategy
    fn choose_encoding_strategy(
        &self,
        seq: &Sequence,
        candidates: &[Sequence],
    ) -> EncodingStrategy;
}

/// Encoding strategy decision
#[derive(Debug, Clone, PartialEq)]
pub enum EncodingStrategy {
    /// Use delta encoding with specified reference
    Delta { reference_index: usize },
    /// Store as full sequence
    Full,
    /// Use compressed full storage
    CompressedFull,
    /// Skip this sequence
    Skip,
}

/// Statistics about delta generation
#[derive(Debug, Clone)]
pub struct DeltaGenerationStats {
    pub total_sequences: usize,
    pub delta_encoded: usize,
    pub full_stored: usize,
    pub skipped: usize,
    pub avg_similarity: f32,
    pub avg_delta_ops: f32,
    pub compression_ratio: f32,
    pub processing_time_ms: u64,
}

/// Batch delta generator for parallel processing
pub trait BatchDeltaGenerator: DeltaGenerator {
    /// Generate deltas in parallel batches
    fn generate_parallel(
        &mut self,
        sequences: &[Sequence],
        references: &[Sequence],
        reference_hash: SHA256Hash,
        num_threads: usize,
    ) -> Result<Vec<DeltaChunk>>;

    /// Optimal batch size for this generator
    fn optimal_batch_size(&self) -> usize;

    /// Merge delta chunks from parallel processing
    fn merge_chunks(
        &self,
        chunks: Vec<DeltaChunk>,
    ) -> Result<Vec<DeltaChunk>>;
}