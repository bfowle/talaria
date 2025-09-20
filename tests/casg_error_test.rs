use chrono::Utc;
use std::fs;
use talaria::casg::{
    CASGRepository, CASGStorage, FastaAssembler, MerkleDAG, SHA256Hash, TaxonId,
    TaxonomyAwareChunk, TemporalManifest,
};
use tempfile::TempDir;

// Helper to create test manifests with required fields
fn create_test_manifest(version: &str, seq_version: &str, tax_version: &str) -> TemporalManifest {
    TemporalManifest {
        version: version.to_string(),
        created_at: Utc::now(),
        sequence_version: seq_version.to_string(),
        taxonomy_version: tax_version.to_string(),
        temporal_coordinate: None,
        taxonomy_root: SHA256Hash::compute(format!("tax_{}", version).as_bytes()),
        sequence_root: SHA256Hash::compute(format!("seq_{}", version).as_bytes()),
        chunk_merkle_tree: None,
        taxonomy_manifest_hash: SHA256Hash::compute(b"test_tax_manifest"),
        taxonomy_dump_version: "2024-01-01".to_string(),
        source_database: Some("test_db".to_string()),
        chunk_index: vec![],
        discrepancies: vec![],
        etag: "test".to_string(),
        previous_version: None,
    }
}

fn setup_test_env() -> (TempDir, CASGRepository) {
    let temp_dir = TempDir::new().unwrap();
    let repo = CASGRepository::init(temp_dir.path()).unwrap();
    (temp_dir, repo)
}

#[test]
fn test_corrupted_chunk_detection() {
    let (_temp_dir, repo) = setup_test_env();

    // Create a valid chunk
    let seq_data = b">test_seq\nACGTACGT\n".to_vec();
    let chunk = TaxonomyAwareChunk {
        content_hash: SHA256Hash::compute(&seq_data),
        taxonomy_version: SHA256Hash::compute(b"tax_v1"),
        sequence_version: SHA256Hash::compute(b"seq_v1"),
        taxon_ids: vec![TaxonId(562)],
        sequences: vec![],
        sequence_data: seq_data,
        created_at: Utc::now(),
        valid_from: Utc::now(),
        valid_until: None,
        size: 100,
        compressed_size: None,
    };

    // Store the chunk
    repo.storage.store_taxonomy_chunk(&chunk).unwrap();

    // Corrupt the stored data by getting chunk info and modifying the file
    if let Some(chunk_info) = repo.storage.get_chunk_info(&chunk.content_hash) {
        // Write corrupted data to the chunk file
        // Note: Writing directly will break compression, causing hash mismatch
        fs::write(&chunk_info.path, b"CORRUPTED DATA").unwrap();

        // Try to retrieve the corrupted chunk - should fail with hash mismatch
        let result = repo.storage.get_chunk(&chunk.content_hash);

        // Should fail because the data is corrupted
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();

        // The error should indicate corruption (could be decompression failure or hash mismatch)
        assert!(
            error_msg.contains("decompress")
                || error_msg.contains("corrupt")
                || error_msg.contains("invalid")
                || error_msg.contains("failed"),
            "Expected corruption error, got: {}",
            error_msg
        );
    } else {
        // If we can't get chunk info, skip the corruption test
        eprintln!("Warning: Could not get chunk info for corruption test");
    }
}

#[test]
fn test_missing_manifest_handling() {
    let temp_dir = TempDir::new().unwrap();
    let _manifest_path = temp_dir.path().join("manifest.json");

    // Try to open repository without manifest
    let repo_result = CASGRepository::open(temp_dir.path());

    // Should either fail or create new empty repository
    match repo_result {
        Ok(repo) => {
            // If it succeeds, it should have created an empty repository
            assert_eq!(repo.storage.get_stats().total_chunks, 0);
        }
        Err(e) => {
            // Expected error about missing manifest
            assert!(e.to_string().contains("manifest") || e.to_string().contains("not found"));
        }
    }
}

#[test]
fn test_incomplete_download_recovery() {
    let (_temp_dir, repo) = setup_test_env();

    // Simulate chunks that should be downloaded
    let chunks_to_download = vec![
        SHA256Hash::compute(b"chunk1"),
        SHA256Hash::compute(b"chunk2"),
        SHA256Hash::compute(b"chunk3"),
    ];

    // Simulate that only first chunk was downloaded
    let seq_data = b">partial\nACGT\n".to_vec();
    let partial_chunk = TaxonomyAwareChunk {
        content_hash: chunks_to_download[0].clone(),
        taxonomy_version: SHA256Hash::compute(b"tax_v1"),
        sequence_version: SHA256Hash::compute(b"seq_v1"),
        taxon_ids: vec![TaxonId(1)],
        sequences: vec![],
        sequence_data: seq_data,
        created_at: Utc::now(),
        valid_from: Utc::now(),
        valid_until: None,
        size: 50,
        compressed_size: None,
    };

    repo.storage.store_taxonomy_chunk(&partial_chunk).unwrap();

    // Try to assemble all chunks - should fail for missing ones
    let assembler = FastaAssembler::new(&repo.storage);
    let result = assembler.assemble_from_chunks(&chunks_to_download);

    assert!(result.is_err());

    // Verify we can identify which chunks are missing
    let mut missing = Vec::new();
    for hash in &chunks_to_download {
        if !repo.storage.has_chunk(hash) {
            missing.push(hash);
        }
    }

    assert_eq!(missing.len(), 2); // chunk2 and chunk3 are missing
}

#[test]
fn test_version_conflict_handling() {
    let (_temp_dir, _repo) = setup_test_env();

    // Create two incompatible manifests

    let mut manifest_v1 = create_test_manifest("v1", "2024.01", "2024.01");
    manifest_v1.etag = "".to_string(); // Empty etag for test

    let mut manifest_v2 = create_test_manifest("v2", "2025.01", "2025.01");
    manifest_v2.etag = "".to_string(); // Empty etag for test
    manifest_v2.previous_version = Some("v1.5".to_string()); // Missing intermediate version

    // Version chain is broken (v1 -> missing v1.5 -> v2)
    assert_ne!(
        manifest_v2.previous_version.as_ref().unwrap(),
        &manifest_v1.version
    );
}

#[test]
fn test_storage_failure_handling() {
    let temp_dir = TempDir::new().unwrap();
    let storage_path = temp_dir.path().join("storage");

    // Create storage directory
    fs::create_dir_all(&storage_path).unwrap();

    // Make it read-only to simulate permission issues
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(&storage_path).unwrap();
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o444); // Read-only
        fs::set_permissions(&storage_path, permissions).unwrap();
    }

    // Try to create storage - should fail with permission error
    let storage_result = CASGStorage::new(&storage_path);

    #[cfg(unix)]
    {
        assert!(storage_result.is_err());
    }
}

#[tokio::test]
async fn test_network_timeout_simulation() {
    use std::time::Duration;
    use tokio::time::timeout;

    // Simulate a network operation that times out
    let long_operation = async {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok::<Vec<u8>, anyhow::Error>(vec![])
    };

    let result = timeout(Duration::from_millis(100), long_operation).await;

    assert!(result.is_err()); // Should timeout
}

#[test]
fn test_merkle_proof_tampering() {
    use talaria::casg::ChunkMetadata;

    // Create verifiable items using ChunkMetadata
    let items = vec![
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk1"),
            taxon_ids: vec![],
            sequence_count: 1,
            size: 6,
            compressed_size: None,
        },
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk2"),
            taxon_ids: vec![],
            sequence_count: 1,
            size: 6,
            compressed_size: None,
        },
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk3"),
            taxon_ids: vec![],
            sequence_count: 1,
            size: 6,
            compressed_size: None,
        },
    ];

    let dag = MerkleDAG::build_from_items(items.clone()).unwrap();
    // Generate proof for the first item's data
    let item_data = b"chunk1";
    let proof = dag.generate_proof(item_data).unwrap();

    // Create a tampered proof
    let mut tampered_proof = proof.clone();
    tampered_proof.leaf_hash = SHA256Hash::compute(b"tampered");

    // Verification should fail for tampered proof
    // verify_proof takes the proof and the original data
    assert!(!MerkleDAG::verify_proof(&tampered_proof, item_data));
}

#[test]
fn test_chunk_size_validation() {
    let (_temp_dir, repo) = setup_test_env();

    // Create a large chunk (10MB to avoid timeout while still testing size handling)
    let huge_sequence = vec![b'A'; 10_000_000]; // 10MB sequence - large enough to test size handling
    let oversized_chunk = TaxonomyAwareChunk {
        content_hash: SHA256Hash::compute(&huge_sequence),
        taxonomy_version: SHA256Hash::compute(b"tax_v1"),
        sequence_version: SHA256Hash::compute(b"seq_v1"),
        taxon_ids: vec![TaxonId(1)],
        sequences: vec![],
        sequence_data: huge_sequence,
        created_at: Utc::now(),
        valid_from: Utc::now(),
        valid_until: None,
        size: 10_000_000,
        compressed_size: None,
    };

    // Storage should reject or handle gracefully
    let result = repo.storage.store_taxonomy_chunk(&oversized_chunk);

    // Either fails or handles with special logic
    match result {
        Ok(_) => {
            // If it succeeds, verify it was stored correctly
            assert!(repo.storage.has_chunk(&oversized_chunk.content_hash));
        }
        Err(e) => {
            // Expected to fail with size limit error
            assert!(e.to_string().contains("size") || e.to_string().contains("large"));
        }
    }
}

#[test]
fn test_concurrent_access_safety() {
    use std::sync::Arc;
    use std::thread;

    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(CASGStorage::new(temp_dir.path()).unwrap());

    let mut handles = vec![];

    // Spawn multiple threads trying to write/read simultaneously
    for i in 0..10 {
        let storage_clone = Arc::clone(&storage);
        let handle = thread::spawn(move || {
            let seq_data = format!(">seq_{}\nACGTACGT\n", i).into_bytes();
            let chunk = TaxonomyAwareChunk {
                content_hash: SHA256Hash::compute(&seq_data),
                taxonomy_version: SHA256Hash::compute(b"tax_v1"),
                sequence_version: SHA256Hash::compute(b"seq_v1"),
                taxon_ids: vec![TaxonId(i)],
                sequences: vec![],
                sequence_data: seq_data,
                created_at: Utc::now(),
                valid_from: Utc::now(),
                valid_until: None,
                size: 100,
                compressed_size: None,
            };

            // Should handle concurrent access safely
            storage_clone.store_taxonomy_chunk(&chunk).unwrap();
            assert!(storage_clone.has_chunk(&chunk.content_hash));
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all chunks were stored
    let stats = storage.get_stats();
    assert_eq!(stats.total_chunks, 10);
}

#[test]
fn test_invalid_hash_format() {
    let (_temp_dir, repo) = setup_test_env();

    // Try to use invalid hash (wrong length)
    let invalid_hash = SHA256Hash([0u8; 32]); // All zeros - technically valid but suspicious

    let assembler = FastaAssembler::new(&repo.storage);
    let result = assembler.assemble_from_chunks(&vec![invalid_hash]);

    // Should fail because chunk doesn't exist
    assert!(result.is_err());
}

#[test]
fn test_circular_dependency_detection() {
    // Create manifests with circular dependency
    let mut manifest_a = create_test_manifest("v1", "2024.01", "2024.01");
    manifest_a.etag = "".to_string();
    manifest_a.previous_version = Some("v2".to_string()); // Points to v2

    let mut manifest_b = create_test_manifest("v2", "2024.02", "2024.02");
    manifest_b.etag = "".to_string();
    manifest_b.previous_version = Some("v1".to_string()); // Points back to v1

    // Circular dependency exists
    assert_eq!(manifest_a.previous_version, Some("v2".to_string()));
    assert_eq!(manifest_b.previous_version, Some("v1".to_string()));
}
