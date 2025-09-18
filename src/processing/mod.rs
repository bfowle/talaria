// Processing pipelines module

pub mod traits;
pub mod pipeline;

pub use traits::{
    SequenceProcessor, BatchProcessor, ProcessingPipeline,
    FilterProcessor, TransformProcessor, EnrichmentProcessor
};

pub use pipeline::{
    StandardProcessingPipeline, LowComplexityFilter, CaseTransformer,
    create_reduction_pipeline
};
