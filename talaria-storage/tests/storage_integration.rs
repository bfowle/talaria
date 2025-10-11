/// Integration tests for storage backend functionality
///
/// These tests focus on the low-level storage operations provided by talaria-storage,
/// including RocksDB backend, compression, and caching. Business logic traits are
/// tested in talaria-sequoia.
use std::path::PathBuf;
use talaria_core::types::SHA256Hash;

mod helpers {
    use super::*;

    pub fn create_test_data(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }

    pub async fn setup_test_storage() -> (PathBuf, tempfile::TempDir) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();

        // Create necessary directories
        std::fs::create_dir_all(&base_path.join("chunks")).unwrap();
        std::fs::create_dir_all(&base_path.join("manifests")).unwrap();
        std::fs::create_dir_all(&base_path.join("indices")).unwrap();

        (base_path, temp_dir)
    }
}

#[cfg(test)]
mod backend_tests {
    use super::*;
    use chrono::Utc;
    use helpers::*;
    use talaria_core::SequenceType;
    use talaria_storage::backend::RocksDBBackend;
    use talaria_storage::types::{CanonicalSequence, SequenceStorageBackend};

    #[tokio::test]
    async fn test_rocksdb_backend_initialization() {
        let (base_path, _temp_dir) = setup_test_storage().await;
        let backend = RocksDBBackend::new(&base_path.join("rocksdb"));
        assert!(backend.is_ok(), "Should initialize RocksDB backend");
    }

    #[tokio::test]
    async fn test_backend_store_and_retrieve_sequence() {
        let (base_path, _temp_dir) = setup_test_storage().await;
        let backend = RocksDBBackend::new(&base_path.join("rocksdb")).unwrap();

        // Create a canonical sequence
        let data = create_test_data(1024);
        let hash = SHA256Hash::compute(&data);
        let sequence = CanonicalSequence {
            sequence_hash: hash,
            sequence: data.clone(),
            length: data.len(),
            sequence_type: SequenceType::DNA,
            checksum: 0,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
        };

        // Store sequence
        backend.store_canonical(&sequence).unwrap();

        // Check existence
        assert!(
            backend.sequence_exists(&hash).unwrap(),
            "Sequence should exist"
        );

        // Retrieve sequence
        let retrieved = backend.load_canonical(&hash).unwrap();
        assert_eq!(
            retrieved.sequence, data,
            "Retrieved sequence should match original"
        );
        assert_eq!(retrieved.sequence_hash, hash, "Hash should match");
    }

    #[tokio::test]
    async fn test_backend_batch_operations() {
        let (base_path, _temp_dir) = setup_test_storage().await;
        let backend = RocksDBBackend::new(&base_path.join("rocksdb")).unwrap();

        // Create multiple sequences
        let sequences: Vec<_> = (0..5)
            .map(|i| {
                let data = format!("test sequence {}", i).into_bytes();
                let hash = SHA256Hash::compute(&data);
                CanonicalSequence {
                    sequence_hash: hash,
                    sequence: data.clone(),
                    length: data.len(),
                    sequence_type: SequenceType::Protein,
                    checksum: i as u64,
                    first_seen: Utc::now(),
                    last_seen: Utc::now(),
                }
            })
            .collect();

        let hashes: Vec<_> = sequences.iter().map(|s| s.sequence_hash).collect();

        // Store in batch
        backend.store_canonical_batch(&sequences).unwrap();

        // Check existence in batch
        let exists = backend.sequences_exist_batch(&hashes).unwrap();
        assert!(exists.iter().all(|&e| e), "All sequences should exist");

        // Verify individual retrieval
        for seq in &sequences {
            let retrieved = backend.load_canonical(&seq.sequence_hash).unwrap();
            assert_eq!(retrieved.sequence, seq.sequence);
        }
    }
}

#[cfg(test)]
mod compression_tests {
    use super::*;
    use helpers::*;
    use talaria_storage::compression::{ChunkCompressor, CompressionConfig};
    use talaria_storage::types::ChunkFormat;

    #[test]
    fn test_chunk_compression() {
        let config = CompressionConfig::default();
        let mut compressor = ChunkCompressor::new(config);

        let data = create_test_data(10000);

        // Compress
        let compressed = compressor
            .compress(&data, ChunkFormat::Binary, None)
            .unwrap();
        assert!(
            compressed.len() < data.len(),
            "Compressed data should be smaller"
        );

        // Decompress
        let decompressed = compressor.decompress(&compressed, None).unwrap();
        assert_eq!(
            decompressed, data,
            "Decompressed data should match original"
        );
    }

    #[test]
    fn test_compression_with_dictionary() {
        let config = CompressionConfig {
            level: 3,
            use_dictionaries: true,
            dict_min_samples: 10,
            dict_max_size: 10000,
            cache_dictionaries: true,
        };
        let mut compressor = ChunkCompressor::new(config);

        // Test data
        let data = create_test_data(1000);
        let format = ChunkFormat::Binary;

        // Compress and decompress
        let compressed = compressor.compress(&data, format, Some(9606)).unwrap();
        let decompressed = compressor.decompress(&compressed, Some(format)).unwrap();
        assert_eq!(decompressed, data, "Should handle dictionary compression");
    }
}

#[cfg(test)]
mod cache_tests {
    use talaria_storage::cache::AlignmentCache;

    #[test]
    fn test_alignment_cache_basic_operations() {
        let cache = AlignmentCache::new(100);

        // Test empty cache
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        // Test insertion
        let ref_id = "ref1".to_string();
        let query_id = "query1".to_string();
        let alignment = talaria_storage::cache::CachedAlignment {
            score: 100,
            alignment: vec![1, 2, 3, 4],
        };

        cache.insert(ref_id.clone(), query_id.clone(), alignment.clone());
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());

        // Test retrieval
        let retrieved = cache.get(&ref_id, &query_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().score, 100);

        // Test cache miss
        let miss = cache.get("ref2", "query2");
        assert!(miss.is_none());

        // Test clear
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_alignment_cache_size_limit() {
        let cache = AlignmentCache::new(2); // Small cache for testing

        // Insert entries up to the limit
        for i in 0..3 {
            let alignment = talaria_storage::cache::CachedAlignment {
                score: i as i32,
                alignment: vec![i as u8],
            };
            cache.insert(format!("ref{}", i), format!("query{}", i), alignment);
        }

        // Cache should respect size limit
        assert!(cache.len() <= 2, "Cache should not exceed maximum size");
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;
    use talaria_core::types::{ChunkMetadata, TaxonId};
    use talaria_storage::index::{ChunkIndex, ChunkQuery, InMemoryChunkIndex};

    #[tokio::test]
    async fn test_chunk_index_operations() {
        let mut index = InMemoryChunkIndex::new();

        // Create test metadata
        let hash1 = SHA256Hash::compute(b"chunk1");
        let hash2 = SHA256Hash::compute(b"chunk2");

        let metadata1 = ChunkMetadata {
            hash: hash1,
            size: 1000,
            sequence_count: 10,
            taxon_ids: vec![TaxonId(9606)], // Human
            compressed_size: Some(500),
            compression_ratio: Some(2.0),
        };

        let metadata2 = ChunkMetadata {
            hash: hash2,
            size: 2000,
            sequence_count: 20,
            taxon_ids: vec![TaxonId(562)], // E. coli
            compressed_size: Some(800),
            compression_ratio: Some(2.5),
        };

        // Add chunks to index
        index.add_chunk(metadata1.clone()).await.unwrap();
        index.add_chunk(metadata2.clone()).await.unwrap();

        // Test retrieval
        let retrieved = index.get_metadata(&hash1).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().hash, hash1);

        // Test query by taxon
        let query = ChunkQuery {
            taxon_ids: Some(vec![TaxonId(9606)]),
            ..Default::default()
        };
        let results = index.query(query).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hash, hash1);

        // Test statistics
        let stats = index.get_stats().await.unwrap();
        assert_eq!(stats.total_chunks, 2);
        assert_eq!(stats.unique_taxons, 2);
    }
}
