/// Traits for delta encoding and reconstruction
use crate::bio::sequence::Sequence;
use crate::casg::types::{DeltaChunk, SHA256Hash};
use anyhow::Result;

/// Configuration for delta generation
#[derive(Debug, Clone)]
pub struct DeltaGeneratorConfig {
    pub min_delta_size: usize,
    pub max_delta_size: usize,
    pub compression_threshold: f64,
    pub enable_caching: bool,
    pub min_similarity_threshold: f32,
    pub max_delta_ops_threshold: usize,
    pub max_chunk_size: usize,
    pub target_sequences_per_chunk: usize,
    pub enable_compression: bool,
}

impl Default for DeltaGeneratorConfig {
    fn default() -> Self {
        Self {
            min_delta_size: 1024,
            max_delta_size: 100 * 1024 * 1024, // 100MB
            compression_threshold: 0.8,
            enable_caching: true,
            min_similarity_threshold: 0.85,
            max_delta_ops_threshold: 1000,
            max_chunk_size: 50 * 1024 * 1024, // 50MB
            target_sequences_per_chunk: 1000,
            enable_compression: true,
        }
    }
}

/// Trait for delta generation
pub trait DeltaGenerator: Send + Sync {
    /// Generate delta chunks from sequences
    fn generate_deltas(
        &mut self,
        sequences: &[Sequence],
        references: &[Sequence],
        reference_hash: SHA256Hash,
    ) -> Result<Vec<DeltaChunk>>;

    /// Set configuration
    fn set_config(&mut self, config: DeltaGeneratorConfig);

    /// Get current configuration
    fn get_config(&self) -> &DeltaGeneratorConfig;
}

/// Trait for delta reconstruction
pub trait DeltaReconstructor: Send + Sync {
    /// Reconstruct sequences from delta chunks
    fn reconstruct(
        &self,
        delta_chunks: &[DeltaChunk],
        reference_sequences: &[Sequence],
    ) -> Result<Vec<Sequence>>;

    /// Verify reconstruction correctness
    fn verify_reconstruction(
        &self,
        original: &[Sequence],
        reconstructed: &[Sequence],
    ) -> Result<bool>;
}