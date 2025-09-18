/// Traits for chunking strategies
use crate::bio::sequence::Sequence;
use crate::casg::types::{ChunkMetadata, TaxonId};
use anyhow::Result;

/// Statistics from chunking operation
#[derive(Debug, Clone)]
pub struct ChunkingStats {
    pub total_chunks: usize,
    pub total_sequences: usize,
    pub avg_chunk_size: usize,
    pub compression_ratio: f64,
}

/// Base trait for chunking strategies
pub trait Chunker: Send + Sync {
    /// Chunk sequences into metadata
    fn chunk_sequences(&mut self, sequences: &[Sequence]) -> Result<Vec<ChunkMetadata>>;

    /// Get chunking statistics
    fn get_stats(&self) -> ChunkingStats;

    /// Set chunk size parameters
    fn set_chunk_size(&mut self, min_size: usize, max_size: usize);
}

/// Trait for taxonomy-aware chunking
pub trait TaxonomyAwareChunker: Chunker {
    /// Chunk sequences by taxonomy
    fn chunk_by_taxonomy(&mut self, sequences: &[Sequence], taxonomy_map: &[(String, TaxonId)]) -> Result<Vec<ChunkMetadata>>;

    /// Set taxonomy grouping threshold
    fn set_taxonomy_threshold(&mut self, threshold: usize);
}