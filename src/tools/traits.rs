/// Trait definitions for tool abstractions
///
/// This module provides unified interfaces for various bioinformatics tools,
/// enabling easy swapping and testing of different implementations.

use anyhow::Result;
use std::path::Path;
use crate::bio::sequence::Sequence;

/// Result from an alignment operation
#[derive(Debug, Clone)]
pub struct AlignmentResult {
    pub query_id: String,
    pub subject_id: String,
    pub identity: f64,
    pub alignment_length: usize,
    pub mismatches: usize,
    pub gap_opens: usize,
    pub query_start: usize,
    pub query_end: usize,
    pub subject_start: usize,
    pub subject_end: usize,
    pub evalue: f64,
    pub bit_score: f64,
    pub taxon_id: Option<u32>,
}

/// Common interface for sequence alignment tools
pub trait Aligner: Send + Sync {
    /// Perform sequence alignment search
    fn search(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
    ) -> Result<Vec<AlignmentResult>>;

    /// Perform batched sequence alignment search
    fn search_batched(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
        batch_size: usize,
    ) -> Result<Vec<AlignmentResult>>;

    /// Build an index from reference sequences
    fn build_index(
        &mut self,
        reference_path: &Path,
        index_path: &Path,
    ) -> Result<()>;

    /// Verify that the tool is properly installed
    fn verify_installation(&self) -> Result<()>;

    /// Check if this aligner supports taxonomy features
    fn supports_taxonomy(&self) -> bool;

    /// Get the name of this aligner
    fn name(&self) -> &str;

    /// Get recommended batch size for this aligner
    fn recommended_batch_size(&self) -> usize {
        5000
    }

    /// Check if the aligner supports protein sequences
    fn supports_protein(&self) -> bool {
        true
    }

    /// Check if the aligner supports nucleotide sequences  
    fn supports_nucleotide(&self) -> bool {
        true
    }
}

/// Configuration for alignment operations
#[derive(Debug, Clone)]
pub struct AlignmentConfig {
    pub max_target_seqs: usize,
    pub evalue_threshold: f64,
    pub identity_threshold: f64,
    pub coverage_threshold: f64,
    pub num_threads: usize,
    pub temp_dir: Option<std::path::PathBuf>,
}

impl Default for AlignmentConfig {
    fn default() -> Self {
        Self {
            max_target_seqs: 500,
            evalue_threshold: 1e-5,
            identity_threshold: 0.0,
            coverage_threshold: 0.0,
            num_threads: 1,
            temp_dir: None,
        }
    }
}

/// Extended aligner with configuration support
pub trait ConfigurableAligner: Aligner {
    /// Get current configuration
    fn config(&self) -> &AlignmentConfig;

    /// Set configuration
    fn set_config(&mut self, config: AlignmentConfig);

    /// Perform search with custom configuration
    fn search_with_config(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
        config: &AlignmentConfig,
    ) -> Result<Vec<AlignmentResult>>;
}