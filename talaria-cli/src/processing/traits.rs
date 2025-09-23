#![allow(dead_code)]

use talaria_bio::sequence::Sequence;
/// Trait definitions for sequence processing pipelines
///
/// Provides abstractions for various sequence processing operations
/// including filtering, transformation, and enrichment.
use anyhow::Result;
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
        F: FnMut(usize, usize) + Send + Sync;
}

/// Filtering processors
pub trait FilterProcessor: SequenceProcessor {
    /// Filter sequences based on criteria
    fn filter(&self, sequences: &mut Vec<Sequence>, criteria: FilterCriteria) -> Result<usize>;

    /// Set filter criteria
    fn set_criteria(&mut self, criteria: FilterCriteria);

    /// Get filter criteria
    fn get_criteria(&self) -> FilterCriteria;
}

/// Transformation processors
pub trait TransformProcessor: SequenceProcessor {
    /// Transform sequences
    fn transform(&self, sequence: &mut Sequence, operation: TransformOperation) -> Result<()>;

    /// Set transformation operation
    fn set_operation(&mut self, operation: TransformOperation);

    /// Get transformation operation
    fn get_operation(&self) -> TransformOperation;
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
    fn add_processor(&mut self, processor: Box<dyn SequenceProcessor>);

    /// Remove processor by name
    fn remove_processor(&mut self, name: &str) -> bool;

    /// Process sequences through the pipeline
    fn process(&self, sequences: &mut [Sequence]) -> Result<ProcessingResult>;

    /// Process sequences in batches
    fn process_batch(
        &self,
        sequences: &mut [Sequence],
        batch_size: usize,
    ) -> Result<ProcessingResult>;

    /// Get all processors
    fn get_processors(&self) -> &[Box<dyn SequenceProcessor>];

    /// Clear all processors
    fn clear_processors(&mut self);

    /// Set parallel processing
    fn set_parallel(&mut self, parallel: bool);

    /// Check if parallel processing is enabled
    fn is_parallel(&self) -> bool;
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
    pub name: String,
    pub enabled: bool,
    pub parallel: bool,
    pub batch_size: Option<usize>,
    pub parameters: HashMap<String, String>,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
            parallel: true,
            batch_size: None,
            parameters: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessingResult {
    pub processed: usize,
    pub filtered: usize,
    pub modified: usize,
    pub errors: Vec<String>,
    pub processing_time: std::time::Duration,
}

#[derive(Debug, Clone)]
pub struct ProcessingError {
    pub sequence_id: String,
    pub message: String,
    pub recoverable: bool,
}

pub enum FilterCriteria {
    MinLength(usize),
    MaxLength(usize),
    Pattern(String),
    Custom(Box<dyn Fn(&Sequence) -> bool + Send + Sync>),
}

impl std::fmt::Debug for FilterCriteria {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterCriteria::MinLength(n) => write!(f, "MinLength({})", n),
            FilterCriteria::MaxLength(n) => write!(f, "MaxLength({})", n),
            FilterCriteria::Pattern(p) => write!(f, "Pattern({})", p),
            FilterCriteria::Custom(_) => write!(f, "Custom(<function>)"),
        }
    }
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

pub enum TransformOperation {
    Uppercase,
    Lowercase,
    Reverse,
    Complement,
    Custom(Box<dyn Fn(&mut Sequence) + Send + Sync>),
}

impl std::fmt::Debug for TransformOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransformOperation::Uppercase => write!(f, "Uppercase"),
            TransformOperation::Lowercase => write!(f, "Lowercase"),
            TransformOperation::Reverse => write!(f, "Reverse"),
            TransformOperation::Complement => write!(f, "Complement"),
            TransformOperation::Custom(_) => write!(f, "Custom(<function>)"),
        }
    }
}

impl PartialEq for TransformOperation {
    fn eq(&self, other: &Self) -> bool {
        matches!((self, other),
            (TransformOperation::Uppercase, TransformOperation::Uppercase)
            | (TransformOperation::Lowercase, TransformOperation::Lowercase)
            | (TransformOperation::Reverse, TransformOperation::Reverse)
            | (TransformOperation::Complement, TransformOperation::Complement))
    }
}

impl Eq for TransformOperation {}

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
    SequenceModified {
        old_length: usize,
        new_length: usize,
    },
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

pub enum EnrichmentSource {
    TaxonomyDb(std::path::PathBuf),
    AnnotationFile(std::path::PathBuf),
    WebService(String),
    Custom(Box<dyn Fn(&Sequence) -> HashMap<String, String> + Send + Sync>),
}

impl std::fmt::Debug for EnrichmentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnrichmentSource::TaxonomyDb(path) => write!(f, "TaxonomyDb({:?})", path),
            EnrichmentSource::AnnotationFile(path) => write!(f, "AnnotationFile({:?})", path),
            EnrichmentSource::WebService(url) => write!(f, "WebService({})", url),
            EnrichmentSource::Custom(_) => write!(f, "Custom(<function>)"),
        }
    }
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
