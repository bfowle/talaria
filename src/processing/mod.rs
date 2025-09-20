// Processing pipelines module

pub mod pipeline;
pub mod traits;

pub use traits::{
    BatchProcessor, EnrichmentProcessor, FilterProcessor, ProcessingPipeline, SequenceProcessor,
    TransformProcessor,
};

pub use pipeline::{
    create_reduction_pipeline, CaseTransformer, LowComplexityFilter, StandardProcessingPipeline,
};
