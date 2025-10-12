//! Storage and persistence layer for HERALD

pub mod chunk_index;
pub mod core;
pub mod indices;
pub mod sequence;
pub mod traits;

// Import backend types from talaria-storage
pub use talaria_storage::backend::{RocksDBBackend, RocksDBConfig};
pub use talaria_storage::compression::{ChunkCompressor, CompressionConfig};
pub use talaria_storage::format::{
    FormatDetector, JsonFormat, ManifestFormat, MessagePackFormat, TalariaFormat,
};

// Re-export storage traits
pub use traits::{
    ChunkStorage, DeltaStorage, ManifestStorage, RemoteStorage, StateManagement, StatefulStorage,
    TaxonomyStorage,
};

// Re-export main types
pub use chunk_index::{
    ChunkAccessTracker, ChunkIndexBuilder, ChunkQuery, ChunkRelationships, DefaultChunkIndex,
    IndexStatistics, OptimizationSuggestion,
};
pub use core::{
    ChunkMetadata, DetailedStorageStats, GCResult, GarbageCollectionStats, HeraldStorage,
    StorageChunkInfo, StorageStats, VerificationError, VerificationErrorType,
};
pub use indices::{BloomFilter, IndexStats, SequenceIndices};
pub use sequence::SequenceStorage;
