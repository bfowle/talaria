/// Trait definitions for chunking strategies
///
/// Provides abstractions for different sequence chunking algorithms
/// to support various storage and retrieval patterns.

use anyhow::Result;
use crate::bio::sequence::Sequence;
use crate::casg::types::*;

/// Common interface for sequence chunking strategies
pub trait Chunker: Send + Sync {
    /// Chunk sequences into taxonomy-aware chunks
    fn chunk_sequences(
        &self,
        sequences: Vec<Sequence>,
    ) -> Result<Vec<TaxonomyAwareChunk>>;

    /// Merge multiple chunks into a single chunk
    fn merge_chunks(
        &self,
        chunks: Vec<TaxonomyAwareChunk>,
    ) -> Result<TaxonomyAwareChunk>;

    /// Split a chunk into multiple smaller chunks
    fn split_chunk(
        &self,
        chunk: &TaxonomyAwareChunk,
        max_size: usize,
    ) -> Result<Vec<TaxonomyAwareChunk>>;

    /// Get the chunking strategy configuration
    fn strategy(&self) -> &ChunkingStrategy;

    /// Check if a sequence should be added to current chunk
    fn should_add_to_chunk(
        &self,
        chunk_size: usize,
        seq_size: usize,
        chunk_taxa: &[TaxonId],
        seq_taxon: Option<TaxonId>,
    ) -> bool;

    /// Get the name of this chunking strategy
    fn name(&self) -> &str;
}

/// Extended chunker with taxonomy support
pub trait TaxonomyAwareChunker: Chunker {
    /// Load taxonomy mapping
    fn load_taxonomy_mapping(&mut self, mapping: std::collections::HashMap<String, TaxonId>);

    /// Get taxon ID for a sequence
    fn get_taxon_id(&self, sequence: &Sequence) -> Result<TaxonId>;

    /// Apply special taxonomy rules
    fn apply_special_taxa_rules(
        &self,
        chunks: Vec<TaxonomyAwareChunk>,
    ) -> Result<Vec<TaxonomyAwareChunk>>;

    /// Calculate taxonomic coherence of a chunk
    fn calculate_coherence(&self, chunk: &TaxonomyAwareChunk) -> f32;
}

/// Delta-aware chunker for optimized delta encoding
pub trait DeltaAwareChunker: Chunker {
    /// Chunk sequences optimized for delta encoding
    fn chunk_for_deltas(
        &self,
        sequences: Vec<Sequence>,
        references: &[Sequence],
    ) -> Result<Vec<DeltaOptimizedChunk>>;

    /// Find optimal reference for a group of sequences
    fn find_optimal_reference(
        &self,
        sequences: &[Sequence],
        candidates: &[Sequence],
    ) -> Option<usize>;
}

/// Statistics about chunking operation
#[derive(Debug, Clone)]
pub struct ChunkingStats {
    pub total_sequences: usize,
    pub total_chunks: usize,
    pub avg_chunk_size: usize,
    pub max_chunk_size: usize,
    pub min_chunk_size: usize,
    pub avg_sequences_per_chunk: f32,
    pub taxonomic_coherence: f32,
    pub compression_ratio: f32,
}

/// Chunk optimized for delta encoding
#[derive(Debug, Clone)]
pub struct DeltaOptimizedChunk {
    pub reference_sequences: Vec<Sequence>,
    pub delta_candidates: Vec<Sequence>,
    pub predicted_compression: f32,
}

/// Configuration for adaptive chunking
#[derive(Debug, Clone)]
pub struct AdaptiveChunkingConfig {
    pub initial_chunk_size: usize,
    pub min_chunk_size: usize,
    pub max_chunk_size: usize,
    pub size_adjustment_factor: f32,
    pub coherence_threshold: f32,
}

/// Adaptive chunker that adjusts strategy based on data
pub trait AdaptiveChunker: Chunker {
    /// Get current configuration
    fn config(&self) -> &AdaptiveChunkingConfig;

    /// Update configuration based on statistics
    fn adapt(&mut self, stats: &ChunkingStats);

    /// Reset to initial configuration
    fn reset(&mut self);
}