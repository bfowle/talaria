//! Storage and persistence layer for SEQUOIA

pub mod core;
pub mod sequence;
pub mod packed;
pub mod indices;
pub mod chunk_index;
pub mod compression;
pub mod format;

// Re-export main types
pub use core::{SEQUOIAStorage, StorageChunkInfo, StorageStats, GarbageCollectionStats,
               GCResult, VerificationError, VerificationErrorType,
               DetailedStorageStats, ChunkMetadata};
pub use sequence::SequenceStorage;
pub use packed::PackedSequenceStorage;
pub use indices::{SequenceIndices, BloomFilter, IndexStats};
pub use chunk_index::{ChunkIndexBuilder, ChunkQuery, ChunkAccessTracker, DefaultChunkIndex,
                       ChunkRelationships, IndexStatistics, OptimizationSuggestion};
pub use compression::{ChunkCompressor, CompressionConfig};
pub use format::{ManifestFormat, FormatDetector, JsonFormat, MessagePackFormat, TalariaFormat};