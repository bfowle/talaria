#![allow(dead_code)]

use super::types::*;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Mock implementation of ChunkStorage for testing
    struct MockChunkStorage {
        chunks: Arc<Mutex<HashMap<SHA256Hash, Vec<u8>>>>,
        compressed_chunks: Arc<Mutex<HashMap<SHA256Hash, bool>>>,
    }

    impl MockChunkStorage {
        fn new() -> Self {
            Self {
                chunks: Arc::new(Mutex::new(HashMap::new())),
                compressed_chunks: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    impl ChunkStorage for MockChunkStorage {
        fn store_chunk(&self, data: &[u8], compress: bool) -> Result<SHA256Hash> {
            let hash = SHA256Hash::compute(data);
            self.chunks.lock().unwrap().insert(hash, data.to_vec());
            self.compressed_chunks
                .lock()
                .unwrap()
                .insert(hash, compress);
            Ok(hash)
        }

        fn get_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>> {
            self.chunks
                .lock()
                .unwrap()
                .get(hash)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Chunk not found"))
        }

        fn has_chunk(&self, hash: &SHA256Hash) -> bool {
            self.chunks.lock().unwrap().contains_key(hash)
        }

        fn enumerate_chunks(&self) -> Vec<ChunkInfo> {
            self.chunks
                .lock()
                .unwrap()
                .iter()
                .map(|(hash, data)| ChunkInfo {
                    hash: *hash,
                    size: data.len(),
                })
                .collect()
        }

        fn verify_all(&self) -> Result<Vec<VerificationError>> {
            let mut errors = Vec::new();
            for (stored_hash, data) in self.chunks.lock().unwrap().iter() {
                let computed_hash = SHA256Hash::compute(data);
                if computed_hash != *stored_hash {
                    errors.push(VerificationError {
                        chunk_hash: *stored_hash,
                        error_type: VerificationErrorType::HashMismatch {
                            expected: *stored_hash,
                            actual: computed_hash,
                        },
                        context: Some("Mock verification".to_string()),
                    });
                }
            }
            Ok(errors)
        }

        fn get_stats(&self) -> StorageStats {
            let chunks = self.chunks.lock().unwrap();
            let compressed = self.compressed_chunks.lock().unwrap();
            let total_size: usize = chunks.values().map(|v| v.len()).sum();
            let compressed_count = compressed.values().filter(|&&c| c).count();

            StorageStats {
                total_chunks: chunks.len(),
                total_size,
                compressed_chunks: compressed_count,
                deduplication_ratio: 1.0,
                total_sequences: None,
                total_representations: None,
            }
        }

        fn gc(&mut self, referenced: &[SHA256Hash]) -> Result<GCResult> {
            let referenced_set: std::collections::HashSet<_> = referenced.iter().collect();
            let mut chunks_removed = 0;
            let mut bytes_freed = 0;

            let mut chunks = self.chunks.lock().unwrap();
            chunks.retain(|hash, data| {
                if referenced_set.contains(hash) {
                    true
                } else {
                    chunks_removed += 1;
                    bytes_freed += data.len();
                    false
                }
            });

            Ok(GCResult {
                removed_count: chunks_removed,
                freed_space: bytes_freed,
            })
        }
    }

    #[test]
    fn test_store_and_retrieve_chunk() {
        let storage = MockChunkStorage::new();
        let data = b"test data";

        // Store chunk
        let hash = storage.store_chunk(data, false).unwrap();
        assert_eq!(hash, SHA256Hash::compute(data));

        // Retrieve chunk
        let retrieved = storage.get_chunk(&hash).unwrap();
        assert_eq!(retrieved, data.to_vec());

        // Check existence
        assert!(storage.has_chunk(&hash));

        // Check non-existent chunk
        let fake_hash = SHA256Hash::compute(b"fake");
        assert!(!storage.has_chunk(&fake_hash));
    }

    #[test]
    fn test_chunk_enumeration() {
        let storage = MockChunkStorage::new();

        // Store multiple chunks
        let data1 = b"chunk 1";
        let data2 = b"chunk 2";
        let data3 = b"chunk 3";

        storage.store_chunk(data1, false).unwrap();
        storage.store_chunk(data2, true).unwrap();
        storage.store_chunk(data3, false).unwrap();

        // Enumerate chunks
        let chunks = storage.enumerate_chunks();
        assert_eq!(chunks.len(), 3);

        // Verify all chunks are present
        let sizes: Vec<_> = chunks.iter().map(|c| c.size).collect();
        assert!(sizes.contains(&data1.len()));
        assert!(sizes.contains(&data2.len()));
        assert!(sizes.contains(&data3.len()));
    }

    #[test]
    fn test_storage_stats() {
        let storage = MockChunkStorage::new();

        // Initial stats
        let stats = storage.get_stats();
        assert_eq!(stats.total_chunks, 0);
        assert_eq!(stats.total_size, 0);

        // Store chunks with different compression settings
        storage.store_chunk(b"uncompressed", false).unwrap();
        storage.store_chunk(b"compressed", true).unwrap();

        let stats = storage.get_stats();
        assert_eq!(stats.total_chunks, 2);
        assert_eq!(stats.total_size, 22); // 12 + 10
        assert_eq!(stats.compressed_chunks, 1);
    }

    #[test]
    fn test_garbage_collection() {
        let mut storage = MockChunkStorage::new();

        // Store chunks
        let hash1 = storage.store_chunk(b"keep me", false).unwrap();
        let _hash2 = storage.store_chunk(b"delete me", false).unwrap();
        let hash3 = storage.store_chunk(b"keep me too", false).unwrap();

        // Run GC keeping only hash1 and hash3
        let result = storage.gc(&[hash1, hash3]).unwrap();

        assert_eq!(result.removed_count, 1);
        assert_eq!(result.freed_space, 9); // "delete me".len()

        // Verify remaining chunks
        assert!(storage.has_chunk(&hash1));
        assert!(storage.has_chunk(&hash3));
        assert!(!storage.has_chunk(&SHA256Hash::compute(b"delete me")));
    }

    #[test]
    fn test_integrity_verification() {
        let storage = MockChunkStorage::new();

        // Store valid chunks
        storage.store_chunk(b"valid chunk 1", false).unwrap();
        storage.store_chunk(b"valid chunk 2", false).unwrap();

        // Verify all chunks - should return no errors for valid storage
        let errors = storage.verify_all().unwrap();
        assert_eq!(errors.len(), 0, "Valid chunks should not produce errors");
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let storage = Arc::new(MockChunkStorage::new());
        let mut handles = vec![];

        // Spawn multiple threads for concurrent writes
        for i in 0..10 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || {
                let data = format!("thread {}", i);
                storage_clone.store_chunk(data.as_bytes(), false).unwrap()
            });
            handles.push(handle);
        }

        // Wait for all threads and collect hashes
        let hashes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Verify all chunks are stored
        for hash in &hashes {
            assert!(storage.has_chunk(hash));
        }

        // Verify total count
        let stats = storage.get_stats();
        assert_eq!(stats.total_chunks, 10);
    }

    // Property-based test using quickcheck
    #[quickcheck_macros::quickcheck]
    fn prop_store_retrieve_identity(data: Vec<u8>) -> bool {
        let storage = MockChunkStorage::new();

        if data.is_empty() {
            return true; // Skip empty data
        }

        let hash = storage.store_chunk(&data, false).unwrap();
        let retrieved = storage.get_chunk(&hash).unwrap();

        retrieved == data
    }

    #[quickcheck_macros::quickcheck]
    fn prop_hash_consistency(data: Vec<u8>) -> bool {
        let storage = MockChunkStorage::new();

        if data.is_empty() {
            return true;
        }

        let expected_hash = SHA256Hash::compute(&data);
        let actual_hash = storage.store_chunk(&data, false).unwrap();

        expected_hash == actual_hash
    }
}
