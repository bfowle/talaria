pub mod cache;
pub mod metadata;
pub mod optimizer;
pub mod index;
pub mod traits;

pub use traits::{
    ChunkStorage, DeltaStorage, ReductionStorage, TaxonomyStorage,
    RemoteStorage, StatefulStorage,
    StorageStats, GCResult, VerificationError, VerificationErrorType,
    TaxonomyStats, SyncResult, RemoteStatus
};
pub use optimizer::{StorageOptimizer, StandardStorageOptimizer, StorageStrategy, OptimizationResult, StorageAnalysis};
pub use index::{ChunkIndex, InMemoryChunkIndex, ChunkQuery, IndexStats};