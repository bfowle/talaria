pub mod optimizer;

// Re-export commonly used types
pub use optimizer::{
    StorageOptimizer, StorageStrategy, OptimizationResult,
    OptimizationOptions, StorageAnalysis, DuplicateChunk,
    CompressibleChunk, StandardStorageOptimizer,
};