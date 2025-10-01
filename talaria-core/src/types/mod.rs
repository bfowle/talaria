/// Core types shared across all Talaria modules
pub mod aligner;
pub mod chunk;
pub mod database;
pub mod format;
pub mod hash;
pub mod sequence;
pub mod stats;
pub mod taxonomy;
pub mod version;

// Re-export commonly used types at module level
pub use aligner::TargetAligner;
pub use chunk::{ChunkInfo, ChunkMetadata, ChunkType, DeltaChunk};
pub use database::{
    DatabaseReference, DatabaseSource, DatabaseSourceInfo, NCBIDatabase, UniProtDatabase,
};
pub use format::OutputFormat;
pub use hash::SHA256Hash;
pub use sequence::SequenceType;
pub use stats::{
    DetailedStorageStats, GCResult, GarbageCollectionStats, RemoteStatus, StorageStats, SyncResult,
    TaxonomyStats,
};
pub use taxonomy::{TaxonId, TaxonomyDataSource};
pub use version::{DatabaseVersionInfo, TemporalVersionInfo, UpdateStatus};
