pub mod optimizer;

// Re-export commonly used types
pub use optimizer::{
    CompressibleChunk, DuplicateChunk, OptimizationOptions, OptimizationResult,
    StandardStorageOptimizer, StorageAnalysis, StorageOptimizer, StorageStrategy,
};
