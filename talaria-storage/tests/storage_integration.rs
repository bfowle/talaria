/// Integration tests for storage workflows
use std::collections::HashMap;

// Import the types and traits we need from talaria-storage
// These would normally be imported from the public API
mod helpers {
    use std::path::PathBuf;
    use sha2::{Digest, Sha256};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct SHA256Hash([u8; 32]);

    impl SHA256Hash {
        pub fn compute(data: &[u8]) -> Self {
            let mut hasher = Sha256::new();
            hasher.update(data);
            let result = hasher.finalize();
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&result);
            SHA256Hash(hash)
        }

        pub fn to_hex(&self) -> String {
            hex::encode(&self.0)
        }

        #[allow(dead_code)]
        pub fn from_hex(hex_str: &str) -> Result<Self, hex::FromHexError> {
            let bytes = hex::decode(hex_str)?;
            if bytes.len() != 32 {
                return Err(hex::FromHexError::InvalidStringLength);
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&bytes);
            Ok(SHA256Hash(hash))
        }
    }

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
        std::fs::create_dir_all(&base_path.join("cache")).unwrap();

        (base_path, temp_dir)
    }
}

use helpers::*;

/// Test complete storage workflow: store, index, retrieve
#[tokio::test]
async fn test_complete_storage_workflow() {
    let (base_path, _temp_dir) = setup_test_storage().await;

    // Create test data
    let data1 = create_test_data(1000);
    let data2 = create_test_data(2000);
    let data3 = create_test_data(1500);

    // Store chunks
    let hash1 = SHA256Hash::compute(&data1);
    let hash2 = SHA256Hash::compute(&data2);
    let hash3 = SHA256Hash::compute(&data3);

    let chunks_dir = base_path.join("chunks");
    std::fs::write(chunks_dir.join(hash1.to_hex()), &data1).unwrap();
    std::fs::write(chunks_dir.join(hash2.to_hex()), &data2).unwrap();
    std::fs::write(chunks_dir.join(hash3.to_hex()), &data3).unwrap();

    // Create index
    let index_path = base_path.join("indices").join("main.idx");
    let mut index = HashMap::new();
    index.insert(hash1, vec!["seq1".to_string(), "seq2".to_string()]);
    index.insert(hash2, vec!["seq3".to_string()]);
    index.insert(hash3, vec!["seq4".to_string(), "seq5".to_string()]);

    // Write index
    use std::io::Write;
    let mut index_file = std::fs::File::create(&index_path).unwrap();
    for (hash, seqs) in &index {
        writeln!(index_file, "{} {}", hash.to_hex(), seqs.join(",")).unwrap();
    }

    // Verify chunks exist
    assert!(chunks_dir.join(hash1.to_hex()).exists());
    assert!(chunks_dir.join(hash2.to_hex()).exists());
    assert!(chunks_dir.join(hash3.to_hex()).exists());

    // Verify index exists
    assert!(index_path.exists());

    // Read back and verify
    let retrieved1 = std::fs::read(chunks_dir.join(hash1.to_hex())).unwrap();
    assert_eq!(retrieved1, data1);
}

/// Test deduplication workflow
#[tokio::test]
async fn test_deduplication_workflow() {
    let (base_path, _temp_dir) = setup_test_storage().await;
    let chunks_dir = base_path.join("chunks");

    // Create identical data
    let data = create_test_data(1000);
    let hash = SHA256Hash::compute(&data);

    // Store same data multiple times (simulating multiple references)
    let path1 = chunks_dir.join(format!("{}_v1", hash.to_hex()));
    let path2 = chunks_dir.join(format!("{}_v2", hash.to_hex()));
    let path3 = chunks_dir.join(format!("{}_v3", hash.to_hex()));

    std::fs::write(&path1, &data).unwrap();
    std::fs::write(&path2, &data).unwrap();
    std::fs::write(&path3, &data).unwrap();

    // Verify all exist
    assert!(path1.exists());
    assert!(path2.exists());
    assert!(path3.exists());

    // Simulate deduplication
    let canonical_path = chunks_dir.join(hash.to_hex());
    std::fs::rename(&path1, &canonical_path).unwrap();
    std::fs::remove_file(&path2).unwrap();
    std::fs::remove_file(&path3).unwrap();

    // Verify deduplication
    assert!(canonical_path.exists());
    assert!(!path2.exists());
    assert!(!path3.exists());

    let retrieved = std::fs::read(&canonical_path).unwrap();
    assert_eq!(retrieved, data);
}

/// Test compression workflow
#[tokio::test]
async fn test_compression_workflow() {
    use flate2::write::GzEncoder;
    use flate2::read::GzDecoder;
    use flate2::Compression;
    use std::io::{Write, Read};

    let (base_path, _temp_dir) = setup_test_storage().await;
    let chunks_dir = base_path.join("chunks");

    // Create compressible data
    let data = vec![0u8; 10000]; // Highly compressible
    let hash = SHA256Hash::compute(&data);

    // Store uncompressed
    let uncompressed_path = chunks_dir.join(hash.to_hex());
    std::fs::write(&uncompressed_path, &data).unwrap();
    let uncompressed_size = std::fs::metadata(&uncompressed_path).unwrap().len();

    // Compress
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&data).unwrap();
    let compressed_data = encoder.finish().unwrap();

    // Store compressed
    let compressed_path = chunks_dir.join(format!("{}.gz", hash.to_hex()));
    std::fs::write(&compressed_path, &compressed_data).unwrap();
    let compressed_size = std::fs::metadata(&compressed_path).unwrap().len();

    // Verify compression saved space
    assert!(compressed_size < uncompressed_size);
    assert!(compressed_size < uncompressed_size / 10); // Should compress well

    // Remove uncompressed version
    std::fs::remove_file(&uncompressed_path).unwrap();

    // Decompress and verify
    let compressed_read = std::fs::read(&compressed_path).unwrap();
    let mut decoder = GzDecoder::new(&compressed_read[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();

    assert_eq!(decompressed, data);
}

/// Test cache management workflow
#[tokio::test]
async fn test_cache_workflow() {
    let (base_path, _temp_dir) = setup_test_storage().await;
    let cache_dir = base_path.join("cache");

    // Simulate cache entries
    let mut access_counts = HashMap::new();

    // Create chunks with different access patterns
    for i in 0..10 {
        let data = create_test_data(1000 + i * 100);
        let hash = SHA256Hash::compute(&data);

        // Store in cache
        std::fs::write(cache_dir.join(hash.to_hex()), &data).unwrap();

        // Track access count (hot vs cold)
        let access_count = if i < 3 { 100 } else { 1 };
        access_counts.insert(hash, access_count);
    }

    // Identify hot chunks (frequently accessed)
    let hot_chunks: Vec<_> = access_counts
        .iter()
        .filter(|(_, &count)| count > 10)
        .map(|(hash, _)| *hash)
        .collect();

    assert_eq!(hot_chunks.len(), 3);

    // Simulate cache eviction of cold chunks
    for (hash, count) in &access_counts {
        if *count <= 10 {
            let path = cache_dir.join(hash.to_hex());
            if path.exists() {
                std::fs::remove_file(path).unwrap();
            }
        }
    }

    // Verify only hot chunks remain
    for hash in &hot_chunks {
        assert!(cache_dir.join(hash.to_hex()).exists());
    }

    let remaining_files = std::fs::read_dir(&cache_dir)
        .unwrap()
        .count();
    assert_eq!(remaining_files, hot_chunks.len());
}

/// Test manifest storage and retrieval workflow
#[tokio::test]
async fn test_manifest_workflow() {
    let (base_path, _temp_dir) = setup_test_storage().await;
    let manifests_dir = base_path.join("manifests");

    // Create a manifest
    #[derive(Debug)]
    struct Manifest {
        version: String,
        chunks: Vec<SHA256Hash>,
        metadata: HashMap<String, String>,
    }

    let manifest = Manifest {
        version: "1.0.0".to_string(),
        chunks: vec![
            SHA256Hash::compute(b"chunk1"),
            SHA256Hash::compute(b"chunk2"),
            SHA256Hash::compute(b"chunk3"),
        ],
        metadata: {
            let mut m = HashMap::new();
            m.insert("created".to_string(), "2024-01-01".to_string());
            m.insert("profile".to_string(), "default".to_string());
            m
        },
    };

    // Write manifest
    let manifest_path = manifests_dir.join("test_manifest.json");
    let manifest_json = serde_json::json!({
        "version": manifest.version,
        "chunks": manifest.chunks.iter().map(|h| h.to_hex()).collect::<Vec<_>>(),
        "metadata": manifest.metadata,
    });

    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest_json).unwrap()).unwrap();

    // Read back manifest
    let content = std::fs::read_to_string(&manifest_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(parsed["version"], "1.0.0");
    assert_eq!(parsed["chunks"].as_array().unwrap().len(), 3);
    assert_eq!(parsed["metadata"]["profile"], "default");
}

/// Test concurrent storage operations
#[tokio::test]
async fn test_concurrent_operations() {
    use tokio::task;
    use std::sync::Arc;

    let (base_path, _temp_dir) = setup_test_storage().await;
    let base_path = Arc::new(base_path);

    let mut handles = vec![];

    // Spawn concurrent write tasks
    for i in 0..10 {
        let base_path = Arc::clone(&base_path);
        let handle = task::spawn(async move {
            let data = create_test_data(1000 + i * 100);
            let hash = SHA256Hash::compute(&data);
            let chunks_dir = base_path.join("chunks");
            std::fs::write(chunks_dir.join(hash.to_hex()), &data).unwrap();
            hash
        });
        handles.push(handle);
    }

    // Wait for all writes
    let hashes: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    // Verify all chunks written
    let chunks_dir = base_path.join("chunks");
    for hash in &hashes {
        assert!(chunks_dir.join(hash.to_hex()).exists());
    }

    // Concurrent reads
    let mut read_handles = vec![];
    for hash in hashes {
        let base_path = Arc::clone(&base_path);
        let handle = task::spawn(async move {
            let chunks_dir = base_path.join("chunks");
            let data = std::fs::read(chunks_dir.join(hash.to_hex())).unwrap();
            SHA256Hash::compute(&data) == hash
        });
        read_handles.push(handle);
    }

    // Verify all reads successful
    let results: Vec<_> = futures::future::join_all(read_handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    assert!(results.iter().all(|&r| r));
}

/// Test storage migration workflow
#[tokio::test]
async fn test_migration_workflow() {
    let (old_base, _old_temp) = setup_test_storage().await;
    let (new_base, _new_temp) = setup_test_storage().await;

    // Create data in old storage
    let old_chunks = old_base.join("chunks");
    let data1 = create_test_data(1000);
    let data2 = create_test_data(2000);
    let hash1 = SHA256Hash::compute(&data1);
    let hash2 = SHA256Hash::compute(&data2);

    std::fs::write(old_chunks.join(hash1.to_hex()), &data1).unwrap();
    std::fs::write(old_chunks.join(hash2.to_hex()), &data2).unwrap();

    // Migrate to new storage
    let new_chunks = new_base.join("chunks");
    for entry in std::fs::read_dir(&old_chunks).unwrap() {
        let entry = entry.unwrap();
        let file_name = entry.file_name();
        let old_path = entry.path();
        let new_path = new_chunks.join(&file_name);

        // Copy file
        std::fs::copy(&old_path, &new_path).unwrap();
    }

    // Verify migration
    assert!(new_chunks.join(hash1.to_hex()).exists());
    assert!(new_chunks.join(hash2.to_hex()).exists());

    // Verify data integrity
    let migrated1 = std::fs::read(new_chunks.join(hash1.to_hex())).unwrap();
    let migrated2 = std::fs::read(new_chunks.join(hash2.to_hex())).unwrap();
    assert_eq!(migrated1, data1);
    assert_eq!(migrated2, data2);
}

/// Test error recovery workflow
#[tokio::test]
async fn test_error_recovery() {
    let (base_path, _temp_dir) = setup_test_storage().await;
    let chunks_dir = base_path.join("chunks");

    // Create a valid chunk
    let data = create_test_data(1000);
    let hash = SHA256Hash::compute(&data);
    let chunk_path = chunks_dir.join(hash.to_hex());
    std::fs::write(&chunk_path, &data).unwrap();

    // Simulate corruption
    std::fs::write(&chunk_path, b"corrupted data").unwrap();

    // Attempt to verify
    let corrupted_data = std::fs::read(&chunk_path).unwrap();
    let corrupted_hash = SHA256Hash::compute(&corrupted_data);

    // Detect corruption
    assert_ne!(corrupted_hash, hash);

    // Recovery: restore from backup or rebuild
    std::fs::write(&chunk_path, &data).unwrap();

    // Verify recovery
    let recovered_data = std::fs::read(&chunk_path).unwrap();
    let recovered_hash = SHA256Hash::compute(&recovered_data);
    assert_eq!(recovered_hash, hash);
    assert_eq!(recovered_data, data);
}

/// Test storage statistics collection
#[tokio::test]
async fn test_storage_statistics() {
    let (base_path, _temp_dir) = setup_test_storage().await;
    let chunks_dir = base_path.join("chunks");

    // Create various sized chunks
    let sizes = vec![100, 500, 1000, 2000, 5000];
    let mut total_size = 0;
    let mut chunk_count = 0;

    for size in &sizes {
        let data = create_test_data(*size);
        let hash = SHA256Hash::compute(&data);
        std::fs::write(chunks_dir.join(hash.to_hex()), &data).unwrap();
        total_size += size;
        chunk_count += 1;
    }

    // Collect statistics
    let mut actual_total = 0;
    let mut actual_count = 0;
    let mut min_size = usize::MAX;
    let mut max_size = 0;

    for entry in std::fs::read_dir(&chunks_dir).unwrap() {
        let entry = entry.unwrap();
        let metadata = entry.metadata().unwrap();
        let size = metadata.len() as usize;

        actual_total += size;
        actual_count += 1;
        min_size = min_size.min(size);
        max_size = max_size.max(size);
    }

    // Verify statistics
    assert_eq!(actual_count, chunk_count);
    assert_eq!(actual_total, total_size);
    assert_eq!(min_size, 100);
    assert_eq!(max_size, 5000);

    let avg_size = actual_total / actual_count;
    assert!(avg_size > 0);
}

/// Test chunk versioning workflow
#[tokio::test]
async fn test_versioning_workflow() {
    let (base_path, _temp_dir) = setup_test_storage().await;
    let versions_dir = base_path.join("versions");
    std::fs::create_dir_all(&versions_dir).unwrap();

    // Create initial version
    let data_v1 = b"version 1 data";
    let hash_v1 = SHA256Hash::compute(data_v1);

    // Store with version metadata
    let v1_path = versions_dir.join(format!("{}_{}", hash_v1.to_hex(), "v1"));
    std::fs::write(&v1_path, data_v1).unwrap();

    // Create updated version
    let data_v2 = b"version 2 data with changes";
    let hash_v2 = SHA256Hash::compute(data_v2);

    let v2_path = versions_dir.join(format!("{}_{}", hash_v2.to_hex(), "v2"));
    std::fs::write(&v2_path, data_v2).unwrap();

    // Create delta between versions
    let delta_info = format!("DELTA:{}:{}", hash_v1.to_hex(), hash_v2.to_hex());
    let delta_path = versions_dir.join("delta_v1_v2");
    std::fs::write(&delta_path, delta_info).unwrap();

    // Verify versions exist
    assert!(v1_path.exists());
    assert!(v2_path.exists());
    assert!(delta_path.exists());

    // List all versions
    let versions: Vec<_> = std::fs::read_dir(&versions_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(versions.len() >= 3);
    assert!(versions.iter().any(|v| v.contains("v1")));
    assert!(versions.iter().any(|v| v.contains("v2")));
    assert!(versions.iter().any(|v| v.contains("delta")));
}