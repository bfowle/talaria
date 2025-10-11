pub mod traits;
pub mod types;

// Re-export commonly used types
pub use types::{
    ChunkInfo, ChunkMetadata, ChunkType, DeltaChunk, GCResult, RemoteStatus, SHA256Hash,
    StorageStats, SyncResult, TaxonId, TaxonomyStats, VerificationError, VerificationErrorType,
};
