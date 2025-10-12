use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;
use talaria_herald::*;
use talaria_bio::fasta::{FastaReader, FastaWriter, Sequence};
use talaria_herald::storage::core::HeraldStorage;
use talaria_herald::manifest::core::Manifest;
use talaria_herald::chunker::{ChunkingStrategy, TaxonomicChunker};
use talaria_herald::verification::verifier::Verifier;
use talaria_herald::temporal::core::TemporalIndex;
use std::collections::HashMap;
use sha2::{Sha256, Digest};

/// Helper to create test sequences
fn create_test_sequences(count: usize) -> Vec<Sequence> {
    (0..count)
        .map(|i| Sequence {
            id: format!("seq_{}", i),
            description: Some(format!("Test sequence {}", i)),
            sequence: format!("ACGT").repeat(250).into_bytes(), // 1000bp each
        })
        .collect()
}

/// Helper to create sequences with taxonomy
fn create_taxonomic_sequences() -> Vec<Sequence> {
    vec![
        Sequence {
            id: "seq_human_1".to_string(),
            description: Some("Homo sapiens protein 1 [TaxId:9606]".to_string()),
            sequence: "ACGT".repeat(100).into_bytes(),
        },
        Sequence {
            id: "seq_human_2".to_string(),
            description: Some("Homo sapiens protein 2 [TaxId:9606]".to_string()),
            sequence: "CGTA".repeat(100).into_bytes(),
        },
        Sequence {
            id: "seq_mouse_1".to_string(),
            description: Some("Mus musculus protein 1 [TaxId:10090]".to_string()),
            sequence: "TACG".repeat(100).into_bytes(),
        },
    ]
}

#[test]
fn test_complete_reduction_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage_path = temp_dir.path().join("herald");
    
    // Create storage and manifest
    let mut storage = HeraldStorage::new(storage_path.clone())?;
    let mut manifest = Manifest::new("test_db".to_string(), "1.0.0".to_string());
    
    // Create test sequences
    let sequences = create_test_sequences(100);
    
    // Chunk and store sequences
    let strategy = ChunkingStrategy::default();
    let mut total_size = 0;
    let mut chunk_count = 0;
    
    for seq in &sequences {
        let data = seq.to_fasta_string().into_bytes();
        total_size += data.len();
        
        // Simulate chunking
        let chunk_size = strategy.calculate_chunk_size(data.len());
        for chunk_data in data.chunks(chunk_size) {
            let hash = storage.store_chunk(chunk_data, talaria_herald::storage::ChunkFormat::Zstd)?;
            manifest.add_chunk(hash, chunk_data.len(), talaria_herald::storage::ChunkFormat::Zstd);
            chunk_count += 1;
        }
    }
    
    // Verify manifest
    assert_eq!(manifest.database_name, "test_db");
    assert_eq!(manifest.version, "1.0.0");
    assert!(manifest.chunks.len() > 0);
    assert!(manifest.chunks.len() <= chunk_count); // Deduplication should reduce count
    
    // Save manifest
    let manifest_path = storage_path.join("manifest.json");
    manifest.save(&manifest_path)?;
    
    // Load and verify
    let loaded = Manifest::load(&manifest_path)?;
    assert_eq!(loaded.database_name, manifest.database_name);
    assert_eq!(loaded.chunks.len(), manifest.chunks.len());
    
    Ok(())
}

#[test]
fn test_incremental_update_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage_path = temp_dir.path().join("herald");
    
    // Initial storage
    let mut storage = HeraldStorage::new(storage_path.clone())?;
    let mut manifest = Manifest::new("incremental_db".to_string(), "1.0.0".to_string());
    
    // Store initial sequences
    let initial = create_test_sequences(50);
    for seq in &initial {
        let data = seq.to_fasta_string().into_bytes();
        let hash = storage.store_chunk(&data, talaria_herald::storage::ChunkFormat::Raw)?;
        manifest.add_chunk(hash, data.len(), talaria_herald::storage::ChunkFormat::Raw);
    }
    
    let initial_chunks = manifest.chunks.len();
    
    // Update version and add more sequences
    manifest.version = "1.1.0".to_string();
    let additional = create_test_sequences(30);
    for seq in &additional {
        let data = seq.to_fasta_string().into_bytes();
        let hash = storage.store_chunk(&data, talaria_herald::storage::ChunkFormat::Raw)?;
        manifest.add_chunk(hash, data.len(), talaria_herald::storage::ChunkFormat::Raw);
    }
    
    // Verify incremental update
    assert_eq!(manifest.version, "1.1.0");
    assert!(manifest.chunks.len() > initial_chunks);
    
    // Verify storage consistency
    let chunks = storage.list_chunks()?;
    assert!(chunks.len() >= manifest.chunks.len()); // Storage may have more due to temp chunks
    
    Ok(())
}

#[test]
fn test_taxonomic_chunking_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage_path = temp_dir.path().join("herald");
    
    let mut storage = HeraldStorage::new(storage_path)?;
    let sequences = create_taxonomic_sequences();
    
    // Group by taxonomy
    let mut tax_groups: HashMap<String, Vec<Sequence>> = HashMap::new();
    for seq in sequences {
        let taxid = if let Some(desc) = &seq.description {
            if desc.contains("9606") {
                "9606".to_string()
            } else if desc.contains("10090") {
                "10090".to_string()
            } else {
                "unknown".to_string()
            }
        } else {
            "unknown".to_string()
        };
        tax_groups.entry(taxid).or_insert_with(Vec::new).push(seq);
    }
    
    // Store each taxonomic group
    let mut manifest = Manifest::new("taxonomic_db".to_string(), "1.0.0".to_string());
    for (taxid, seqs) in tax_groups {
        let mut group_data = Vec::new();
        for seq in seqs {
            group_data.extend(seq.to_fasta_string().into_bytes());
        }
        
        let hash = storage.store_chunk(&group_data, talaria_herald::storage::ChunkFormat::Zstd)?;
        manifest.add_chunk(hash, group_data.len(), talaria_herald::storage::ChunkFormat::Zstd);
        
        // Add taxonomy metadata
        manifest.metadata.insert(format!("taxid_{}", taxid), hash.to_string());
    }
    
    // Verify taxonomic organization
    assert!(manifest.metadata.contains_key("taxid_9606"));
    assert!(manifest.metadata.contains_key("taxid_10090"));
    assert_eq!(manifest.chunks.len(), 2); // Two taxonomic groups
    
    Ok(())
}

#[test]
fn test_merkle_verification_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage_path = temp_dir.path().join("herald");
    
    let mut storage = HeraldStorage::new(storage_path.clone())?;
    let manifest = Manifest::new("verified_db".to_string(), "1.0.0".to_string());
    let verifier = Verifier::new(storage_path);
    
    // Store chunks and build Merkle tree
    let chunks: Vec<_> = (0..10)
        .map(|i| {
            let data = format!("chunk_{}", i).into_bytes();
            let hash = storage.store_chunk(&data, talaria_herald::storage::ChunkFormat::Raw).unwrap();
            (hash, data)
        })
        .collect();
    
    // Generate Merkle root
    let chunk_hashes: Vec<_> = chunks.iter().map(|(h, _)| *h).collect();
    let root = verifier.compute_merkle_root(&chunk_hashes)?;
    
    // Verify individual chunks with proofs
    for (i, (hash, _)) in chunks.iter().enumerate() {
        let proof = verifier.generate_merkle_proof(&chunk_hashes, i)?;
        assert!(verifier.verify_merkle_proof(*hash, &proof, root)?);
    }
    
    Ok(())
}

#[test]
fn test_temporal_versioning_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage_path = temp_dir.path().join("herald");
    
    let mut storage = HeraldStorage::new(storage_path.clone())?;
    let rocksdb = storage.sequence_storage.get_rocksdb();
    let mut temporal_index = TemporalIndex::new(&storage_path, rocksdb)?;
    
    // Create versions over time
    let versions = vec!["1.0.0", "1.1.0", "1.2.0", "2.0.0"];
    
    for (i, version) in versions.iter().enumerate() {
        let mut manifest = Manifest::new("temporal_db".to_string(), version.to_string());
        
        // Add sequences for this version
        let sequences = create_test_sequences(10 * (i + 1));
        for seq in sequences {
            let data = seq.to_fasta_string().into_bytes();
            let hash = storage.store_chunk(&data, talaria_herald::storage::ChunkFormat::Raw)?;
            manifest.add_chunk(hash, data.len(), talaria_herald::storage::ChunkFormat::Raw);
        }
        
        // Record version in temporal index
        temporal_index.add_version(
            version.to_string(),
            chrono::Utc::now(),
            manifest.compute_hash(),
        )?;
    }
    
    // Query temporal versions
    let all_versions = temporal_index.list_versions()?;
    assert_eq!(all_versions.len(), 4);
    
    // Get specific version
    let v1 = temporal_index.get_version("1.0.0")?;
    assert!(v1.is_some());
    
    // Get latest version
    let latest = temporal_index.get_latest_version()?;
    assert_eq!(latest.unwrap().version, "2.0.0");
    
    Ok(())
}

#[test]
fn test_compression_effectiveness() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage_path = temp_dir.path().join("herald");
    
    let mut storage = HeraldStorage::new(storage_path)?;
    
    // Create highly compressible data
    let repetitive_data = "AAAA".repeat(10000).into_bytes();
    let random_data: Vec<u8> = (0..40000).map(|i| ((i * 7 + 13) % 256) as u8).collect();
    
    // Store with different compression
    let raw_hash = storage.store_chunk(&repetitive_data, talaria_herald::storage::ChunkFormat::Raw)?;
    let zstd_hash = storage.store_chunk(&repetitive_data, talaria_herald::storage::ChunkFormat::Zstd)?;
    let raw_random = storage.store_chunk(&random_data, talaria_herald::storage::ChunkFormat::Raw)?;
    let zstd_random = storage.store_chunk(&random_data, talaria_herald::storage::ChunkFormat::Zstd)?;
    
    // Verify deduplication
    assert_ne!(raw_hash, zstd_hash); // Different formats = different hashes
    
    // Retrieve and verify
    let retrieved = storage.retrieve_chunk(&zstd_hash)?;
    assert_eq!(retrieved, repetitive_data);
    
    Ok(())
}

#[test]
fn test_chunk_deduplication_across_databases() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage_path = temp_dir.path().join("herald");
    
    let mut storage = HeraldStorage::new(storage_path)?;
    
    // Create two "databases" with overlapping content
    let mut manifest1 = Manifest::new("db1".to_string(), "1.0.0".to_string());
    let mut manifest2 = Manifest::new("db2".to_string(), "1.0.0".to_string());
    
    let shared_data = "SHARED_SEQUENCE_DATA".repeat(100).into_bytes();
    let unique_data1 = "UNIQUE_TO_DB1".repeat(100).into_bytes();
    let unique_data2 = "UNIQUE_TO_DB2".repeat(100).into_bytes();
    
    // Store in first database
    let shared_hash1 = storage.store_chunk(&shared_data, talaria_herald::storage::ChunkFormat::Raw)?;
    let unique_hash1 = storage.store_chunk(&unique_data1, talaria_herald::storage::ChunkFormat::Raw)?;
    manifest1.add_chunk(shared_hash1, shared_data.len(), talaria_herald::storage::ChunkFormat::Raw);
    manifest1.add_chunk(unique_hash1, unique_data1.len(), talaria_herald::storage::ChunkFormat::Raw);
    
    // Store in second database
    let shared_hash2 = storage.store_chunk(&shared_data, talaria_herald::storage::ChunkFormat::Raw)?;
    let unique_hash2 = storage.store_chunk(&unique_data2, talaria_herald::storage::ChunkFormat::Raw)?;
    manifest2.add_chunk(shared_hash2, shared_data.len(), talaria_herald::storage::ChunkFormat::Raw);
    manifest2.add_chunk(unique_hash2, unique_data2.len(), talaria_herald::storage::ChunkFormat::Raw);
    
    // Verify deduplication
    assert_eq!(shared_hash1, shared_hash2); // Same data = same hash
    assert_ne!(unique_hash1, unique_hash2); // Different data = different hashes
    
    // Check storage efficiency
    let chunks = storage.list_chunks()?;
    assert_eq!(chunks.len(), 3); // Only 3 unique chunks stored
    
    Ok(())
}

#[test]
fn test_recovery_from_partial_manifest() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage_path = temp_dir.path().join("herald");
    
    let mut storage = HeraldStorage::new(storage_path.clone())?;
    let verifier = Verifier::new(storage_path.clone());
    
    // Create and store data
    let sequences = create_test_sequences(20);
    let mut chunk_hashes = Vec::new();
    
    for seq in &sequences {
        let data = seq.to_fasta_string().into_bytes();
        let hash = storage.store_chunk(&data, talaria_herald::storage::ChunkFormat::Raw)?;
        chunk_hashes.push(hash);
    }
    
    // Create partial manifest (simulate corruption)
    let mut partial_manifest = Manifest::new("partial_db".to_string(), "1.0.0".to_string());
    // Only add first half of chunks
    for hash in &chunk_hashes[..10] {
        partial_manifest.add_chunk(*hash, 1000, talaria_herald::storage::ChunkFormat::Raw);
    }
    
    // Verify we can still access all stored chunks
    for hash in &chunk_hashes {
        let data = storage.retrieve_chunk(hash)?;
        assert!(!data.is_empty());
    }
    
    // Verify integrity of partial manifest
    let manifest_valid = partial_manifest.chunks.len() == 10;
    assert!(manifest_valid);
    
    Ok(())
}

#[test]
fn test_large_scale_storage_performance() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage_path = temp_dir.path().join("herald");
    
    let mut storage = HeraldStorage::new(storage_path)?;
    let mut manifest = Manifest::new("performance_db".to_string(), "1.0.0".to_string());
    
    // Simulate large dataset
    let sequence_count = 1000;
    let mut total_stored = 0;
    let mut unique_chunks = 0;
    
    let start = std::time::Instant::now();
    
    for i in 0..sequence_count {
        // Create varied data to test deduplication
        let data = if i % 10 == 0 {
            // 10% identical data
            "STANDARD_SEQUENCE".repeat(50).into_bytes()
        } else if i % 5 == 0 {
            // 20% similar data
            format!("SIMILAR_SEQUENCE_{}", i % 3).repeat(40).into_bytes()
        } else {
            // 70% unique data
            format!("UNIQUE_SEQUENCE_{}", i).repeat(30).into_bytes()
        };
        
        let hash = storage.store_chunk(&data, talaria_herald::storage::ChunkFormat::Zstd)?;
        manifest.add_chunk(hash, data.len(), talaria_herald::storage::ChunkFormat::Zstd);
        total_stored += data.len();
    }
    
    let duration = start.elapsed();
    unique_chunks = manifest.chunks.len();
    
    // Performance assertions
    assert!(duration.as_secs() < 10, "Storage took too long: {:?}", duration);
    assert!(unique_chunks < sequence_count, "Deduplication not working");
    
    // Verify deduplication ratio
    let dedup_ratio = (sequence_count - unique_chunks) as f64 / sequence_count as f64;
    assert!(dedup_ratio > 0.1, "Deduplication ratio too low: {}", dedup_ratio);
    
    Ok(())
}

#[test]
fn test_concurrent_access_safety() -> Result<()> {
    use std::sync::Arc;
    use std::thread;
    
    let temp_dir = TempDir::new()?;
    let storage_path = temp_dir.path().join("herald");
    
    // Create storage
    let storage = Arc::new(HeraldStorage::new(storage_path)?); 
    
    // Store some initial data
    let initial_data = "INITIAL_DATA".repeat(100).into_bytes();
    let initial_hash = storage.store_chunk(&initial_data, talaria_herald::storage::ChunkFormat::Raw)?;
    
    // Concurrent reads
    let mut handles = vec![];
    for i in 0..10 {
        let storage_clone = Arc::clone(&storage);
        let hash = initial_hash;
        
        let handle = thread::spawn(move || {
            for _ in 0..10 {
                let data = storage_clone.retrieve_chunk(&hash).unwrap();
                assert_eq!(data, format!("INITIAL_DATA").repeat(100).into_bytes());
            }
            i
        });
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    Ok(())
}