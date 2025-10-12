#![allow(dead_code)]

use anyhow::Result;
/// Traits for reference selection
use serde::{Deserialize, Serialize};
use talaria_bio::sequence::Sequence;

/// Result from trait-based selection
pub struct TraitSelectionResult {
    pub references: Vec<Sequence>,
    pub stats: SelectionStats,
}

/// Statistics from selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionStats {
    pub total_sequences: usize,
    pub references_selected: usize,
    pub coverage: f64,
    pub avg_identity: f64,
}

/// Score for sequence alignment
#[derive(Debug, Clone)]
pub struct AlignmentScore {
    pub seq1_id: String,
    pub seq2_id: String,
    pub score: f64,
    pub identity: f64,
    pub coverage: f64,
}

/// Recommended parameters from selector
#[derive(Debug, Clone)]
pub struct RecommendedParams {
    pub batch_size: usize,
    pub min_length: usize,
    pub similarity_threshold: f64,
}

/// Main trait for reference selection
pub trait ReferenceSelector: Send + Sync {
    /// Select references from sequences
    fn select_references(
        &self,
        sequences: Vec<Sequence>,
        target_ratio: f64,
    ) -> Result<TraitSelectionResult>;

    /// Get selection statistics
    fn get_stats(&self) -> SelectionStats;

    /// Get recommended parameters
    fn recommend_params(&self, num_sequences: usize) -> RecommendedParams;
}

/// Trait for alignment-based selection
pub trait AlignmentBasedSelector: ReferenceSelector {
    /// Select references using alignment scores
    fn select_with_alignments(
        &self,
        sequences: Vec<Sequence>,
        alignments: &[AlignmentScore],
    ) -> Result<TraitSelectionResult>;

    /// Compute alignment scores between sequences
    fn compute_alignment_scores(&self, sequences: &[Sequence]) -> Vec<AlignmentScore>;

    /// Set minimum alignment score threshold
    fn set_min_alignment_score(&mut self, score: f64);
}
