#![allow(dead_code)]

// This module defines storage trait abstractions for future extensibility.
// These traits will be implemented by various storage backends and plugins.
// TODO: Implement concrete storage backends for cloud providers (S3, GCS, Azure)

use talaria_sequoia::operations::state::{OperationType, ProcessingState, SourceInfo};
use talaria_sequoia::operations::reduction::ReductionManifest;
use talaria_sequoia::{SHA256Hash, TaxonId};
use talaria_sequoia::ChunkManifest;
use talaria_core::types::{ChunkInfo, ChunkType, DeltaChunk};
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
    /// Store a chunk manifest
    fn store_chunk_manifest(&self, chunk: &ChunkManifest) -> Result<SHA256Hash>;

    /// Get a chunk manifest
    fn get_chunk_manifest(&self, hash: &SHA256Hash) -> Result<ChunkManifest>;

    /// Find chunks by taxonomy ID
    fn find_chunks_by_taxon(&self, taxon_id: TaxonId) -> Result<Vec<SHA256Hash>>;

    /// Get taxonomy statistics
    fn get_taxonomy_stats(&self) -> Result<TaxonomyStats>;
}

/// Remote storage operations
pub trait RemoteStorage: ChunkStorage {
    /// Fetch chunk manifests from remote repository
    fn fetch_chunk_manifests(&mut self, hashes: &[SHA256Hash]) -> Result<Vec<ChunkManifest>>;

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

// Import all storage stats types from talaria-core
pub use talaria_core::{
    StorageStats, GCResult, TaxonomyStats, SyncResult, RemoteStatus,
    VerificationError,
};
