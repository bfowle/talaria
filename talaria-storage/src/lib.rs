//! Storage backend implementations for Talaria

pub mod backend;
pub mod cache;
pub mod compression;
pub mod core;
pub mod format;
pub mod index;
pub mod io;
pub mod optimization;
pub mod types;

// Re-export commonly used types and traits from core
pub use core::{
    ChunkInfo,
    ChunkMetadata,
    // Traits
    ChunkStorage,
    ChunkType,
    DeltaChunk,
    DeltaStorage,
    GCResult,
    OperationType,
    ProcessingState,
    ReductionManifest,
    ReductionStorage,
    RemoteStatus,
    RemoteStorage,
    // Types
    SHA256Hash,
    SourceInfo,
    StatefulStorage,
    StorageStats,
    SyncResult,
    TaxonId,
    TaxonomyAwareChunk,
    TaxonomyStats,
    TaxonomyStorage,
    VerificationError,
    VerificationErrorType,
};

// Re-export from index module
pub use index::{ChunkIndex, ChunkQuery, InMemoryChunkIndex, IndexStats};

// Re-export from cache module
pub use cache::{AlignmentCache, CachedAlignment};

// Re-export from optimization module
pub use optimization::{
    OptimizationOptions, OptimizationResult, StandardStorageOptimizer, StorageAnalysis,
    StorageOptimizer, StorageStrategy,
};

// Re-export from io module
pub use io::{load_metadata, load_ref2children, write_metadata, write_ref2children, DeltaRecord};
