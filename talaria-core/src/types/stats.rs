//! Storage and system statistics types

use serde::{Deserialize, Serialize};

/// Storage statistics for tracking storage usage and efficiency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    /// Total number of chunks stored
    pub total_chunks: usize,
    /// Total size in bytes
    pub total_size: usize,
    /// Number of compressed chunks
    pub compressed_chunks: usize,
    /// Deduplication ratio (higher is better)
    pub deduplication_ratio: f32,
    /// Total number of sequences (optional)
    pub total_sequences: Option<usize>,
    /// Total number of representations (optional)
    pub total_representations: Option<usize>,
}

impl StorageStats {
    /// Create basic storage stats
    pub fn new(
        total_chunks: usize,
        total_size: usize,
        compressed_chunks: usize,
        deduplication_ratio: f32,
    ) -> Self {
        Self {
            total_chunks,
            total_size,
            compressed_chunks,
            deduplication_ratio,
            total_sequences: None,
            total_representations: None,
        }
    }

    /// Create sequence storage stats
    pub fn for_sequences(
        total_sequences: usize,
        total_representations: usize,
        total_size: usize,
        deduplication_ratio: f32,
    ) -> Self {
        Self {
            total_chunks: 0,
            total_size,
            compressed_chunks: 0,
            deduplication_ratio,
            total_sequences: Some(total_sequences),
            total_representations: Some(total_representations),
        }
    }
}

/// Garbage collection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCResult {
    /// Number of items removed
    pub removed_count: usize,
    /// Space freed in bytes
    pub freed_space: usize,
}

/// Garbage collection statistics with detailed metrics
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GarbageCollectionStats {
    /// Number of chunks deleted
    pub chunks_deleted: usize,
    /// Bytes freed
    pub bytes_freed: usize,
    /// Number of delta chains compacted
    pub chains_compacted: usize,
}

/// Detailed storage statistics with additional metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedStorageStats {
    /// Total number of chunks
    pub chunk_count: usize,
    /// Total size in bytes
    pub total_size: usize,
    /// Number of compressed chunks
    pub compressed_chunks: usize,
    /// Compression ratio (0.0 to 1.0)
    pub compression_ratio: f32,
    /// Total number of sequences
    pub sequence_count: usize,
    /// Number of unique sequences
    pub unique_sequences: usize,
    /// Deduplication ratio
    pub deduplication_ratio: f32,
}

/// Taxonomy statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyStats {
    /// Total number of taxa
    pub total_taxa: usize,
    /// Number of sequences per taxon
    pub sequences_per_taxon: std::collections::HashMap<crate::TaxonId, usize>,
    /// Number of chunks per taxon
    pub chunks_per_taxon: std::collections::HashMap<crate::TaxonId, usize>,
}

/// Sync result for remote operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    /// Chunks uploaded
    pub uploaded: Vec<crate::SHA256Hash>,
    /// Chunks downloaded
    pub downloaded: Vec<crate::SHA256Hash>,
    /// Conflicting chunks
    pub conflicts: Vec<crate::SHA256Hash>,
    /// Total bytes transferred
    pub bytes_transferred: usize,
}

/// Remote repository status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteStatus {
    /// Whether remote is connected
    pub connected: bool,
    /// Number of chunks on remote
    pub remote_chunks: usize,
    /// Number of chunks locally
    pub local_chunks: usize,
    /// Number of chunks pending sync
    pub pending_sync: usize,
}