pub mod traits;
pub mod types;

// Re-export commonly used types
pub use types::{
    ChunkInfo, ChunkMetadata, ChunkType, DeltaChunk, GCResult, OperationType, ProcessingState,
    ReductionManifest, RemoteStatus, SHA256Hash, SourceInfo, StorageStats, SyncResult, TaxonId,
    TaxonomyAwareChunk, TaxonomyStats, VerificationError, VerificationErrorType,
};

pub use traits::{
    ChunkStorage, DeltaStorage, ReductionStorage, RemoteStorage, StatefulStorage, TaxonomyStorage,
};
