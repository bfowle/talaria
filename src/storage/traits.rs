use crate::casg::processing_state::{OperationType, ProcessingState, SourceInfo};
use crate::casg::reduction::ReductionManifest;
use crate::casg::types::*;
/// Storage trait hierarchy for Talaria
///
/// Provides abstractions for different storage backends including
/// local filesystem, cloud storage, and content-addressed storage.
use anyhow::Result;

/// Basic chunk storage operations
pub trait ChunkStorage: Send + Sync {
    /// Store a chunk in the storage backend
    fn store_chunk(&self, data: &[u8], compress: bool) -> Result<SHA256Hash>;

    /// Retrieve a chunk from storage
    fn get_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>>;

    /// Check if a chunk exists
    fn has_chunk(&self, hash: &SHA256Hash) -> bool;

    /// Enumerate all chunks in storage
    fn enumerate_chunks(&self) -> Vec<ChunkInfo>;

    /// Verify integrity of all stored chunks
    fn verify_all(&self) -> Result<Vec<VerificationError>>;

    /// Get statistics about the storage
    fn get_stats(&self) -> StorageStats;

    /// Garbage collect unreferenced chunks
    fn gc(&mut self, referenced: &[SHA256Hash]) -> Result<GCResult>;
}

/// Delta chunk storage operations
pub trait DeltaStorage: ChunkStorage {
    /// Store a delta chunk
    fn store_delta_chunk(&self, chunk: &DeltaChunk) -> Result<SHA256Hash>;

    /// Retrieve a delta chunk
    fn get_delta_chunk(&self, hash: &SHA256Hash) -> Result<DeltaChunk>;

    /// Find delta chunk containing a specific child sequence
    fn find_delta_for_child(&self, child_id: &str) -> Result<Option<SHA256Hash>>;

    /// Get all delta chunks for a reference chunk
    fn get_deltas_for_reference(&self, reference_hash: &SHA256Hash) -> Result<Vec<SHA256Hash>>;

    /// Find delta chunks for a reference
    fn find_delta_chunks_for_reference(
        &self,
        reference_hash: &SHA256Hash,
    ) -> Result<Vec<SHA256Hash>>;

    /// Get chunk type for a hash
    fn get_chunk_type(&self, hash: &SHA256Hash) -> Result<ChunkType>;
}

/// Reduction manifest storage operations
pub trait ReductionStorage: DeltaStorage {
    /// Store a reduction manifest
    fn store_reduction_manifest(&self, manifest: &ReductionManifest) -> Result<SHA256Hash>;

    /// Get a reduction manifest by profile name
    fn get_reduction_by_profile(&self, profile: &str) -> Result<Option<ReductionManifest>>;

    /// List all available reduction profiles
    fn list_reduction_profiles(&self) -> Result<Vec<String>>;

    /// Delete a reduction profile
    fn delete_reduction_profile(&self, profile: &str) -> Result<()>;
}

/// Taxonomy-aware chunk storage
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
    fn fetch_chunks(&mut self, hashes: &[SHA256Hash]) -> Result<Vec<TaxonomyAwareChunk>>;

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

/// Storage statistics
#[derive(Debug)]
pub struct StorageStats {
    pub total_chunks: usize,
    pub total_size: usize,
    pub compressed_chunks: usize,
    pub deduplication_ratio: f32,
}

/// Garbage collection result
#[derive(Debug)]
pub struct GCResult {
    pub removed_count: usize,
    pub freed_space: usize,
}

/// Verification error
#[derive(Debug)]
pub struct VerificationError {
    pub chunk_hash: SHA256Hash,
    pub error_type: VerificationErrorType,
}

#[derive(Debug)]
pub enum VerificationErrorType {
    HashMismatch {
        expected: SHA256Hash,
        actual: SHA256Hash,
    },
    ReadError(String),
    CorruptedData(String),
}

/// Taxonomy statistics
#[derive(Debug)]
pub struct TaxonomyStats {
    pub total_taxa: usize,
    pub sequences_per_taxon: std::collections::HashMap<TaxonId, usize>,
    pub chunks_per_taxon: std::collections::HashMap<TaxonId, usize>,
}

/// Sync result
#[derive(Debug)]
pub struct SyncResult {
    pub uploaded: Vec<SHA256Hash>,
    pub downloaded: Vec<SHA256Hash>,
    pub conflicts: Vec<SHA256Hash>,
    pub bytes_transferred: usize,
}

/// Remote repository status
#[derive(Debug)]
pub struct RemoteStatus {
    pub connected: bool,
    pub remote_chunks: usize,
    pub local_chunks: usize,
    pub pending_sync: usize,
}
