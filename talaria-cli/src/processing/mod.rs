#![allow(dead_code)]

// Processing pipelines module

pub mod pipeline;
pub mod traits;

pub use traits::{
    BatchProcessor, ProcessingPipeline,
};

pub use pipeline::create_reduction_pipeline;
