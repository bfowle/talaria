//! Storage and persistence layer for SEQUOIA

pub mod chunk_index;
pub mod core;
pub mod indices;
pub mod sequence;

// Import backend types from talaria-storage
pub use talaria_storage::backend::{RocksDBBackend, RocksDBConfig};
pub use talaria_storage::compression::{ChunkCompressor, CompressionConfig};
pub use talaria_storage::format::{
    FormatDetector, JsonFormat, ManifestFormat, MessagePackFormat, TalariaFormat,
};

// Re-export main types
pub use chunk_index::{
    ChunkAccessTracker, ChunkIndexBuilder, ChunkQuery, ChunkRelationships, DefaultChunkIndex,
    IndexStatistics, OptimizationSuggestion,
};
pub use core::{
    ChunkMetadata, DetailedStorageStats, GCResult, GarbageCollectionStats, SequoiaStorage,
    StorageChunkInfo, StorageStats, VerificationError, VerificationErrorType,
};
pub use indices::{BloomFilter, IndexStats, SequenceIndices};
pub use sequence::SequenceStorage;
