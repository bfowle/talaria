/// Trait definitions for sequence processing pipelines
///
/// Provides abstractions for various sequence processing operations
/// including filtering, transformation, and enrichment.

use anyhow::Result;
use crate::bio::sequence::Sequence;
use std::collections::HashMap;

/// Common interface for sequence processors
pub trait SequenceProcessor: Send + Sync {
    /// Process sequences in place
    fn process(&self, sequences: &mut [Sequence]) -> Result<ProcessingResult>;

    /// Get processor name
    fn name(&self) -> &str;

    /// Check if processor can handle sequence type
    fn supports_type(&self, seq_type: SequenceType) -> bool;

    /// Get processor configuration
    fn config(&self) -> ProcessorConfig;

    /// Estimate processing time
    fn estimate_time(&self, num_sequences: usize) -> std::time::Duration;
}

/// Batch processing for efficiency
pub trait BatchProcessor: SequenceProcessor {
    /// Process sequences in batches
    fn process_batch(
        &self,
        sequences: &mut [Sequence],
        batch_size: usize,
    ) -> Result<ProcessingResult>;

    /// Get optimal batch size
    fn optimal_batch_size(&self) -> usize;

    /// Process with progress callback
    fn process_with_progress<F>(
        &self,
        sequences: &mut [Sequence],
        batch_size: usize,
        progress_fn: F,
    ) -> Result<ProcessingResult>
    where
        F: Fn(usize, usize) + Send + Sync;
}

/// Filtering processors
pub trait FilterProcessor: SequenceProcessor {
    /// Filter sequences based on criteria
    fn filter(&self, sequences: Vec<Sequence>) -> Result<FilterResult>;

    /// Get filter criteria
    fn criteria(&self) -> &FilterCriteria;

    /// Set filter criteria
    fn set_criteria(&mut self, criteria: FilterCriteria);

    /// Check if sequence passes filter
    fn passes_filter(&self, sequence: &Sequence) -> bool;
}

/// Transformation processors
pub trait TransformProcessor: SequenceProcessor {
    /// Transform sequences
    fn transform(&self, sequence: &mut Sequence) -> Result<TransformResult>;

    /// Get transformation type
    fn transformation_type(&self) -> TransformationType;

    /// Check if transformation is reversible
    fn is_reversible(&self) -> bool;

    /// Reverse transformation if possible
    fn reverse(&self, sequence: &mut Sequence) -> Result<TransformResult>;
}

/// Enrichment processors
pub trait EnrichmentProcessor: SequenceProcessor {
    /// Enrich sequences with additional data
    fn enrich(&self, sequences: &mut [Sequence]) -> Result<EnrichmentResult>;

    /// Get data sources
    fn data_sources(&self) -> Vec<DataSource>;

    /// Add data source
    fn add_data_source(&mut self, source: DataSource) -> Result<()>;

    /// Get enrichment fields
    fn enrichment_fields(&self) -> Vec<String>;
}

/// Pipeline for chaining processors
pub trait ProcessingPipeline: Send + Sync {
    /// Add processor to pipeline
    fn add_processor(&mut self, processor: Box<dyn SequenceProcessor>) -> Result<()>;

    /// Remove processor by name
    fn remove_processor(&mut self, name: &str) -> Result<()>;

    /// Execute pipeline
    fn execute(&self, sequences: &mut [Sequence]) -> Result<PipelineResult>;

    /// Get pipeline stages
    fn stages(&self) -> Vec<String>;

    /// Validate pipeline configuration
    fn validate(&self) -> Result<Vec<PipelineWarning>>;

    /// Get estimated total time
    fn estimate_total_time(&self, num_sequences: usize) -> std::time::Duration;
}

// Supporting types

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceType {
    Protein,
    Nucleotide,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    pub parallel: bool,
    pub num_threads: usize,
    pub memory_limit_mb: Option<usize>,
    pub timeout_seconds: Option<u64>,
    pub custom_params: HashMap<String, String>,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            parallel: true,
            num_threads: 1,
            memory_limit_mb: None,
            timeout_seconds: None,
            custom_params: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessingResult {
    pub processed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub processing_time_ms: u64,
    pub errors: Vec<ProcessingError>,
}

#[derive(Debug, Clone)]
pub struct ProcessingError {
    pub sequence_id: String,
    pub message: String,
    pub recoverable: bool,
}

#[derive(Debug, Clone)]
pub struct FilterCriteria {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub min_quality: Option<f64>,
    pub allowed_characters: Option<Vec<u8>>,
    pub regex_pattern: Option<String>,
    pub taxonomy_filter: Option<TaxonomyFilter>,
}

#[derive(Debug, Clone)]
pub struct TaxonomyFilter {
    pub include_taxa: Vec<u32>,
    pub exclude_taxa: Vec<u32>,
    pub min_rank: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FilterResult {
    pub passed: Vec<Sequence>,
    pub filtered: Vec<Sequence>,
    pub stats: FilterStats,
}

#[derive(Debug, Clone)]
pub struct FilterStats {
    pub total: usize,
    pub passed: usize,
    pub filtered: usize,
    pub filter_reasons: HashMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformationType {
    ReverseComplement,
    Translation,
    Transcription,
    CaseConversion,
    Trimming,
    Masking,
    Custom,
}

#[derive(Debug, Clone)]
pub struct TransformResult {
    pub success: bool,
    pub changes: Vec<TransformChange>,
}

#[derive(Debug, Clone)]
pub enum TransformChange {
    SequenceModified { old_length: usize, new_length: usize },
    HeaderModified(String),
    MetadataAdded(String, String),
}

#[derive(Debug, Clone)]
pub struct DataSource {
    pub name: String,
    pub source_type: DataSourceType,
    pub path: Option<std::path::PathBuf>,
    pub cache_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataSourceType {
    File,
    Database,
    WebService,
    Cache,
}

#[derive(Debug, Clone)]
pub struct EnrichmentResult {
    pub enriched_count: usize,
    pub fields_added: HashMap<String, usize>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub stages_completed: usize,
    pub total_time_ms: u64,
    pub stage_results: Vec<StageResult>,
}

#[derive(Debug, Clone)]
pub struct StageResult {
    pub stage_name: String,
    pub success: bool,
    pub time_ms: u64,
    pub sequences_processed: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PipelineWarning {
    pub stage: String,
    pub message: String,
    pub severity: WarningSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningSeverity {
    Low,
    Medium,
    High,
}