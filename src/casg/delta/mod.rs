/// Delta encoding and reconstruction module

pub mod traits;

pub use traits::{
    DeltaGenerator, DeltaReconstructor, CompressionAwareDeltaGenerator,
    BatchDeltaGenerator, DeltaGeneratorConfig,
    DeltaGenerationStats, EncodingStrategy,
};