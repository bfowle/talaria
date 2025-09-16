/// Trait definitions for reference selection strategies
///
/// Provides abstractions for different algorithms to select
/// representative sequences from a larger set.

use anyhow::Result;
use crate::bio::sequence::Sequence;
use std::collections::{HashMap, HashSet};

/// Result of reference selection
#[derive(Debug, Clone)]
pub struct SelectionResult {
    /// Selected reference sequences
    pub references: Vec<Sequence>,
    /// Mapping of reference ID to child sequence IDs
    pub children: HashMap<String, Vec<String>>,
    /// Set of discarded sequence IDs
    pub discarded: HashSet<String>,
    /// Statistics about the selection
    pub stats: SelectionStats,
}

/// Statistics about the selection process
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelectionStats {
    pub total_sequences: usize,
    pub selected_references: usize,
    pub assigned_children: usize,
    pub discarded_sequences: usize,
    pub coverage_ratio: f64,
    pub avg_children_per_reference: f64,
    pub selection_time_ms: u64,
}

/// Common interface for reference selection strategies
pub trait ReferenceSelector: Send + Sync {
    /// Select reference sequences from a set
    fn select_references(
        &self,
        sequences: Vec<Sequence>,
        target_ratio: f64,
    ) -> Result<SelectionResult>;

    /// Calculate coverage of references over all sequences
    fn calculate_coverage(
        &self,
        references: &[Sequence],
        all_sequences: &[Sequence],
    ) -> f64;

    /// Get the name of this selection strategy
    fn strategy_name(&self) -> &str;

    /// Estimate memory requirements for selection
    fn estimate_memory_usage(&self, num_sequences: usize) -> usize;

    /// Check if strategy supports incremental selection
    fn supports_incremental(&self) -> bool {
        false
    }

    /// Get recommended parameters for a dataset
    fn recommend_parameters(
        &self,
        num_sequences: usize,
        avg_sequence_length: usize,
    ) -> RecommendedParams;
}

/// Alignment-based reference selection
pub trait AlignmentBasedSelector: ReferenceSelector {
    /// Select references using alignment scores
    fn select_with_alignments(
        &self,
        sequences: Vec<Sequence>,
        alignments: &[AlignmentScore],
        target_ratio: f64,
    ) -> Result<SelectionResult>;

    /// Calculate alignment scores between sequences
    fn calculate_alignments(
        &self,
        sequences: &[Sequence],
    ) -> Result<Vec<AlignmentScore>>;

    /// Get minimum alignment score threshold
    fn min_alignment_score(&self) -> f64;

    /// Set minimum alignment score threshold
    fn set_min_alignment_score(&mut self, score: f64);
}

/// Taxonomy-aware reference selection
pub trait TaxonomyAwareSelector: ReferenceSelector {
    /// Select references considering taxonomy
    fn select_with_taxonomy(
        &self,
        sequences: Vec<Sequence>,
        taxonomy_map: &HashMap<String, u32>,
        target_ratio: f64,
    ) -> Result<SelectionResult>;

    /// Get taxonomy weight for selection
    fn get_taxonomy_weight(&self, taxon_id: u32) -> f64;

    /// Set taxonomy weights
    fn set_taxonomy_weights(&mut self, weights: HashMap<u32, f64>);

    /// Ensure coverage of important taxa
    fn ensure_taxa_coverage(
        &self,
        selections: &mut Vec<Sequence>,
        required_taxa: &[u32],
        sequences: &[Sequence],
    ) -> Result<()>;
}

/// Clustering-based reference selection
pub trait ClusteringSelector: ReferenceSelector {
    /// Select references using clustering
    fn select_with_clustering(
        &self,
        sequences: Vec<Sequence>,
        num_clusters: usize,
    ) -> Result<SelectionResult>;

    /// Get cluster assignments
    fn get_cluster_assignments(
        &self,
        sequences: &[Sequence],
        num_clusters: usize,
    ) -> Result<Vec<usize>>;

    /// Select centroid from cluster
    fn select_centroid(
        &self,
        cluster_sequences: &[Sequence],
    ) -> Result<Sequence>;

    /// Get clustering algorithm name
    fn clustering_algorithm(&self) -> &str;
}

/// Incremental selection for streaming data
pub trait IncrementalSelector: ReferenceSelector {
    /// Add new sequences incrementally
    fn add_sequences(
        &mut self,
        new_sequences: Vec<Sequence>,
    ) -> Result<SelectionUpdate>;

    /// Remove sequences
    fn remove_sequences(
        &mut self,
        sequence_ids: &[String],
    ) -> Result<SelectionUpdate>;

    /// Rebalance selection after changes
    fn rebalance(&mut self) -> Result<SelectionUpdate>;

    /// Get current state
    fn get_state(&self) -> SelectionState;

    /// Restore from state
    fn restore_state(&mut self, state: SelectionState) -> Result<()>;
}

// Supporting types

#[derive(Debug, Clone)]
pub struct AlignmentScore {
    pub seq1_id: String,
    pub seq2_id: String,
    pub score: f64,
    pub identity: f64,
    pub coverage: f64,
}

#[derive(Debug, Clone)]
pub struct RecommendedParams {
    pub target_ratio: f64,
    pub min_length: usize,
    pub similarity_threshold: f64,
    pub use_taxonomy: bool,
    pub batch_size: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SelectionUpdate {
    pub added_references: Vec<String>,
    pub removed_references: Vec<String>,
    pub reassigned_children: HashMap<String, String>,
    pub new_stats: SelectionStats,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelectionState {
    pub reference_ids: Vec<String>,
    pub children_map: HashMap<String, Vec<String>>,
    pub discarded_ids: HashSet<String>,
    pub stats: SelectionStats,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Selection strategy configuration
#[derive(Debug, Clone)]
pub struct SelectorConfig {
    pub min_sequence_length: usize,
    pub similarity_threshold: f64,
    pub taxonomy_aware: bool,
    pub use_alignments: bool,
    pub parallel_threads: usize,
    pub memory_limit_mb: Option<usize>,
}

impl Default for SelectorConfig {
    fn default() -> Self {
        Self {
            min_sequence_length: 50,
            similarity_threshold: 0.9,
            taxonomy_aware: false,
            use_alignments: false,
            parallel_threads: 1,
            memory_limit_mb: None,
        }
    }
}