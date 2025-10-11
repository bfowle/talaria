// Storage-specific types (core types are now in talaria-core)

// Re-export core types from talaria-core
pub use talaria_core::{
    ChunkInfo, ChunkMetadata, ChunkType, DeltaChunk, GCResult, RemoteStatus, SHA256Hash,
    StorageStats, SyncResult, TaxonId, TaxonomyStats, VerificationError, VerificationErrorType,
};
