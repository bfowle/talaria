//! Storage backend implementations for Talaria

pub mod core;
pub mod index;
pub mod cache;
pub mod optimization;
pub mod io;

// Re-export commonly used types and traits from core
pub use core::{
    // Traits
    ChunkStorage, DeltaStorage, ReductionStorage, TaxonomyStorage, RemoteStorage,
    StatefulStorage,
    // Types
    SHA256Hash, TaxonId, ChunkInfo, ChunkMetadata, DeltaChunk,
    StorageStats, GCResult, VerificationError, VerificationErrorType,
    TaxonomyStats, SyncResult, RemoteStatus,
    ChunkType, ReductionManifest, TaxonomyAwareChunk,
    OperationType, ProcessingState, SourceInfo,
};

// Re-export from index module
pub use index::{ChunkIndex, ChunkQuery, IndexStats, InMemoryChunkIndex};

// Re-export from cache module
pub use cache::{AlignmentCache, CachedAlignment};

// Re-export from optimization module
pub use optimization::{
    StorageOptimizer, StorageStrategy, OptimizationResult,
    OptimizationOptions, StorageAnalysis, StandardStorageOptimizer,
};

// Re-export from io module
pub use io::{
    write_metadata, load_metadata,
    write_ref2children, load_ref2children,
    DeltaRecord,
};