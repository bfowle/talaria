/// Processing pipelines module

pub mod traits;

pub use traits::{
    SequenceProcessor, BatchProcessor, FilterProcessor, TransformProcessor,
    EnrichmentProcessor, ProcessingPipeline, SequenceType, ProcessorConfig,
    ProcessingResult, ProcessingError, FilterCriteria, TaxonomyFilter,
    FilterResult, FilterStats, TransformationType, TransformResult,
    TransformChange, DataSource, DataSourceType, EnrichmentResult,
    PipelineResult, StageResult, PipelineWarning, WarningSeverity,
};