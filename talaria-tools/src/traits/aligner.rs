/// Traits for alignment tools
use talaria_bio::sequence::Sequence;
use anyhow::Result;
use std::path::Path;

/// Summary of an alignment search result
#[derive(Debug, Clone)]
pub struct AlignmentSummary {
    pub query_id: String,
    pub reference_id: String,
    pub identity: f32,
    pub alignment_length: usize,
    pub mismatches: usize,
    pub gap_opens: usize,
    pub query_start: usize,
    pub query_end: usize,
    pub ref_start: usize,
    pub ref_end: usize,
    pub e_value: f64,
    pub bit_score: f32,
}

/// Configuration for alignment tools
#[derive(Debug, Clone)]
pub struct AlignmentConfig {
    pub max_results: Option<usize>,
    pub min_identity: Option<f32>,
    pub max_evalue: Option<f64>,
    pub threads: Option<usize>,
}

/// Trait for alignment tools
pub trait Aligner: Send + Sync {
    /// Perform alignment search
    fn search(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
    ) -> Result<Vec<AlignmentSummary>>;

    /// Get tool version
    fn version(&self) -> Result<String>;

    /// Check if tool is available
    fn is_available(&self) -> bool;

    /// Get recommended batch size
    fn recommended_batch_size(&self) -> usize {
        1000
    }

    /// Check if supports protein sequences
    fn supports_protein(&self) -> bool {
        true
    }

    /// Check if supports nucleotide sequences
    fn supports_nucleotide(&self) -> bool {
        true
    }
}

/// Trait for configurable alignment tools
pub trait ConfigurableAligner: Aligner {
    /// Set configuration
    fn set_config(&mut self, config: AlignmentConfig);

    /// Get current configuration
    fn get_config(&self) -> &AlignmentConfig;

    /// Set output path
    fn set_output_path(&mut self, path: &Path);

    /// Set temporary directory
    fn set_temp_dir(&mut self, path: &Path);
}
