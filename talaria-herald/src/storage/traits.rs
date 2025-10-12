/// Storage trait hierarchy for Talaria HERALD system
///
/// This module defines the core storage traits used throughout the HERALD system.
/// These traits were moved here from talaria-storage to avoid circular dependencies
/// and maintain proper architectural layering.
use crate::operations::{OperationType, ProcessingState, SourceInfo};
use crate::types::*;
use anyhow::Result;
use talaria_core::error::VerificationError;
use talaria_core::types::{
    RemoteStatus, SHA256Hash, StorageStats, SyncResult, TaxonId, TaxonomyStats,
};

// Re-export StorageChunkInfo for compatibility
pub use super::core::StorageChunkInfo;

/// Basic chunk storage operations
pub trait ChunkStorage: Send + Sync {
    /// Store a chunk in the storage backend
    fn store_chunk(&self, data: &[u8], compress: bool) -> Result<SHA256Hash>;

    /// Store multiple chunks in a batch for better performance
    fn store_chunks_batch(&self, chunks: &[(Vec<u8>, bool)]) -> Result<Vec<SHA256Hash>> {
        // Default implementation: store one by one (for backward compatibility)
        let mut hashes = Vec::with_capacity(chunks.len());
        for (data, compress) in chunks {
            hashes.push(self.store_chunk(data, *compress)?);
        }
        Ok(hashes)
    }

    /// Retrieve a chunk from storage
    fn get_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>>;

    /// Check if a chunk exists
    fn has_chunk(&self, hash: &SHA256Hash) -> bool;

    /// Enumerate all chunks in storage
    fn enumerate_chunks(&self) -> Vec<StorageChunkInfo>;

    /// Verify integrity of all stored chunks
    fn verify_all(&self) -> Result<Vec<VerificationError>>;

    /// Get statistics about the storage
    fn get_stats(&self) -> StorageStats;

    /// Remove a chunk (if supported)
    fn remove_chunk(&self, hash: &SHA256Hash) -> Result<()>;
}

/// Manifest storage operations
pub trait ManifestStorage: Send + Sync {
    /// Store a chunk manifest
    fn store_chunk_manifest(&self, manifest: &ChunkManifest) -> Result<SHA256Hash>;

    /// Load a chunk manifest
    fn load_chunk(&self, hash: &SHA256Hash) -> Result<ChunkManifest>;

    /// Get sequence root hash
    fn get_sequence_root(&self) -> Result<crate::MerkleHash>;
}

/// Delta chunk storage operations
pub trait DeltaStorage: Send + Sync {
    /// Store a delta chunk
    fn store_delta_chunk(&self, chunk: &TemporalDeltaChunk) -> Result<SHA256Hash>;

    /// Get a delta chunk
    fn get_delta_chunk(&self, hash: &SHA256Hash) -> Result<TemporalDeltaChunk>;

    /// Find delta for a child ID
    fn find_delta_for_child(&self, child_id: &str) -> Result<Option<SHA256Hash>>;

    /// Get all deltas for a reference
    fn get_deltas_for_reference(&self, reference_hash: &SHA256Hash) -> Result<Vec<SHA256Hash>>;
}

/// Processing state management trait
pub trait StateManagement: Send + Sync {
    /// Update processing state with completed chunks
    fn update_processing_state(&self, completed_chunks: &[SHA256Hash]) -> Result<()>;

    /// Mark processing as complete
    fn complete_processing(&self) -> Result<()>;

    /// Get current processing state
    fn get_current_state(&self) -> Result<Option<ProcessingState>>;

    /// List resumable operations
    fn list_resumable_operations(&self) -> Result<Vec<(String, ProcessingState)>>;
}

/// Taxonomy-aware storage trait
pub trait TaxonomyStorage: ChunkStorage {
    /// Store a taxonomy-aware chunk
    fn store_taxonomy_chunk(&self, chunk: &TaxonomyAwareChunk) -> Result<SHA256Hash>;

    /// Get a taxonomy-aware chunk
    fn get_taxonomy_chunk(&self, hash: &SHA256Hash) -> Result<TaxonomyAwareChunk>;

    /// Find chunks by taxonomy ID
    fn find_chunks_by_taxon(&self, taxon_id: TaxonId) -> Result<Vec<SHA256Hash>>;

    /// Get taxonomy statistics
    fn get_taxonomy_stats(&self) -> Result<TaxonomyStats>;
}

/// Remote storage operations
pub trait RemoteStorage: ChunkStorage {
    /// Fetch chunks from remote repository
    fn fetch_chunks(&mut self, hashes: &[SHA256Hash]) -> Result<Vec<ChunkManifest>>;

    /// Push chunks to remote repository
    fn push_chunks(&self, hashes: &[SHA256Hash]) -> Result<()>;

    /// Sync with remote repository
    fn sync(&mut self) -> Result<SyncResult>;

    /// Get remote repository status
    fn get_remote_status(&self) -> Result<RemoteStatus>;
}

/// Processing state aware storage
pub trait StatefulStorage: ChunkStorage {
    /// Start a new processing operation
    fn start_processing(
        &self,
        operation: OperationType,
        manifest_hash: SHA256Hash,
        manifest_version: String,
        total_chunks: usize,
        source_info: SourceInfo,
    ) -> Result<String>;

    /// Check for resumable operation
    fn check_resumable(
        &self,
        database: &str,
        operation: &OperationType,
        manifest_hash: &SHA256Hash,
        manifest_version: &str,
    ) -> Result<Option<ProcessingState>>;

    /// Update processing state with completed chunks
    fn update_processing_state(&self, completed_chunks: &[SHA256Hash]) -> Result<()>;

    /// Complete current processing operation
    fn complete_processing(&self) -> Result<()>;

    /// Get current processing state
    fn get_current_state(&self) -> Result<Option<ProcessingState>>;

    /// List all resumable operations
    fn list_resumable_operations(&self) -> Result<Vec<(String, ProcessingState)>>;

    /// Clean up expired processing states
    fn cleanup_expired_states(&self) -> Result<usize>;
}
