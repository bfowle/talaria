pub mod types;
pub mod traits;

// Re-export commonly used types
pub use types::{
    SHA256Hash, TaxonId, ChunkInfo, ChunkMetadata, DeltaChunk,
    StorageStats, GCResult, VerificationError, VerificationErrorType,
    TaxonomyStats, SyncResult, RemoteStatus,
    ChunkType, ReductionManifest, TaxonomyAwareChunk,
    OperationType, ProcessingState, SourceInfo,
};

pub use traits::{
    ChunkStorage, DeltaStorage, ReductionStorage,
    TaxonomyStorage, RemoteStorage, StatefulStorage,
};