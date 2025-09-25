/// Core types shared across all Talaria modules
pub mod hash;
pub mod taxonomy;
pub mod chunk;
pub mod database;
pub mod stats;
pub mod format;
pub mod sequence;
pub mod version;

// Re-export commonly used types at module level
pub use hash::SHA256Hash;
pub use taxonomy::{TaxonId, TaxonomyDataSource};
pub use chunk::{ChunkInfo, ChunkMetadata, DeltaChunk, ChunkType};
pub use database::{DatabaseSource, DatabaseSourceInfo, UniProtDatabase, NCBIDatabase, DatabaseReference};
pub use stats::{
    StorageStats, GCResult, GarbageCollectionStats, DetailedStorageStats,
    TaxonomyStats, SyncResult, RemoteStatus,
};
pub use format::OutputFormat;
pub use sequence::SequenceType;
pub use version::{DatabaseVersionInfo, TemporalVersionInfo, UpdateStatus};