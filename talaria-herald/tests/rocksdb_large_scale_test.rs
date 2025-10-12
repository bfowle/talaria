//! Large-scale integration tests for RocksDB backend
//!
//! These tests simulate real-world usage patterns with UniRef50-scale data

use std::time::Instant;
use talaria_core::DatabaseSource;
use talaria_herald::storage::sequence::SequenceStorage;
use talaria_herald::storage::HeraldStorage;
use tempfile::TempDir;

/// Generate realistic protein sequences similar to UniRef50
fn generate_protein_sequences(count: usize) -> Vec<(String, String)> {
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    let amino_acids = b"ARNDCQEGHILKMFPSTWYV";
    let mut rng = StdRng::seed_from_u64(42); // Deterministic for testing

    (0..count)
        .map(|i| {
            // Generate variable-length sequences (100-500 AA)
            let length = rng.gen_range(100..=500);
            let seq: String = (0..length)
                .map(|_| {
                    let idx = rng.gen_range(0..amino_acids.len());
                    amino_acids[idx] as char
                })
                .collect();

            // UniRef50-style header
            let header = format!(
                ">UniRef50_P{:05} Cluster: Hypothetical protein n={} Tax=Bacteria TaxID=2 RepID=A0A000_BACSU",
                i, rng.gen_range(1..100)
            );

            (seq, header)
        })
        .collect()
}

#[test]
#[ignore] // Run with --ignored flag as this takes time
fn test_large_scale_batch_processing() {
    let temp_dir = TempDir::new().unwrap();
    let storage = SequenceStorage::new(temp_dir.path()).unwrap();

    println!("Testing large-scale batch processing (50k sequences)...");

    // Generate 50k sequences (typical batch size mentioned in requirements)
    let sequences = generate_protein_sequences(50_000);

    let start = Instant::now();

    // Process in batches like real workflow
    const BATCH_SIZE: usize = 10_000;
    for (batch_idx, chunk) in sequences.chunks(BATCH_SIZE).enumerate() {
        let batch_start = Instant::now();

        let batch: Vec<_> = chunk
            .iter()
            .map(|(seq, header)| {
                (
                    seq.as_str(),
                    header.as_str(),
                    DatabaseSource::UniProt(talaria_core::UniProtDatabase::UniRef50),
                )
            })
            .collect();

        let results = storage.store_sequences_batch(batch).unwrap();

        let new_sequences = results.iter().filter(|(_, is_new)| *is_new).count();
        let batch_time = batch_start.elapsed();

        println!(
            "  Batch {}: {} sequences ({} new) in {:.2}s",
            batch_idx + 1,
            chunk.len(),
            new_sequences,
            batch_time.as_secs_f64()
        );
    }

    let total_time = start.elapsed();
    println!("Total processing time: {:.2}s", total_time.as_secs_f64());

    // Verify target: Should complete in under 60 seconds (vs 1-2 hours with old system)
    assert!(
        total_time.as_secs() < 60,
        "Processing 50k sequences took too long: {:.2}s (target: <60s)",
        total_time.as_secs_f64()
    );

    // Save indices
    storage.save_indices().unwrap();

    // Verify all sequences were stored
    let all_hashes = storage.list_all_hashes().unwrap();
    assert_eq!(all_hashes.len(), 50_000, "Not all sequences were stored");
}

#[test]
#[ignore]
fn test_deduplication_at_scale() {
    let temp_dir = TempDir::new().unwrap();
    let storage = SequenceStorage::new(temp_dir.path()).unwrap();

    println!("Testing deduplication with 100k sequences (50% duplicates)...");

    // Generate 50k unique sequences
    let unique_sequences = generate_protein_sequences(50_000);

    // Create dataset with duplicates
    let mut all_sequences = unique_sequences.clone();
    all_sequences.extend(unique_sequences.clone()); // Now we have 100k with 50% duplicates

    // Shuffle to simulate real-world mixed input
    use rand::rngs::StdRng;
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    let mut rng = StdRng::seed_from_u64(42);
    all_sequences.shuffle(&mut rng);

    let start = Instant::now();

    // Process all sequences
    for (seq, header) in &all_sequences {
        storage
            .store_sequence(
                seq,
                header,
                DatabaseSource::UniProt(talaria_core::UniProtDatabase::UniRef50),
            )
            .unwrap();
    }

    let total_time = start.elapsed();
    println!("Processing time: {:.2}s", total_time.as_secs_f64());

    // Verify deduplication worked
    let unique_count = storage.list_all_hashes().unwrap().len();
    assert_eq!(
        unique_count, 50_000,
        "Deduplication failed: expected 50k unique, got {}",
        unique_count
    );

    println!(
        "Successfully deduplicated to {} unique sequences",
        unique_count
    );
}

#[test]
#[ignore]
fn test_multi_database_integration() {
    let temp_dir = TempDir::new().unwrap();
    let storage = SequenceStorage::new(temp_dir.path()).unwrap();

    println!("Testing multi-database integration (UniProt + NCBI)...");

    // Generate sequences from different sources
    let uniprot_seqs = generate_protein_sequences(25_000);
    let ncbi_seqs = generate_protein_sequences(25_000);

    let start = Instant::now();

    // Store UniProt sequences
    for (seq, header) in &uniprot_seqs {
        storage
            .store_sequence(
                seq,
                header,
                DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt),
            )
            .unwrap();
    }

    // Store NCBI sequences (some will be duplicates)
    for (seq, header) in &ncbi_seqs {
        storage
            .store_sequence(
                seq,
                header,
                DatabaseSource::NCBI(talaria_core::NCBIDatabase::NR),
            )
            .unwrap();
    }

    let total_time = start.elapsed();
    println!(
        "Processing time for 50k sequences from 2 databases: {:.2}s",
        total_time.as_secs_f64()
    );

    // Verify cross-database deduplication
    let unique_count = storage.list_all_hashes().unwrap().len();
    println!(
        "Unique sequences after cross-database dedup: {}",
        unique_count
    );

    // Should have some deduplication between databases
    assert!(
        unique_count < 50_000,
        "No cross-database deduplication occurred"
    );
}

#[test]
#[ignore]
fn test_herald_storage_with_chunks() {
    let temp_dir = TempDir::new().unwrap();
    let herald = HeraldStorage::new(temp_dir.path()).unwrap();

    println!("Testing HERALD storage with chunk operations...");

    // Store multiple chunks
    let mut chunk_hashes = Vec::new();
    for i in 0..1000 {
        let data = format!("Test chunk data {}", i).into_bytes();
        let hash = herald.store_chunk(&data, true).unwrap(); // With compression
        chunk_hashes.push(hash);
    }

    println!("Stored {} chunks", chunk_hashes.len());

    // Verify all chunks are retrievable
    for hash in &chunk_hashes[..10] {
        // Test first 10
        let data = herald.get_chunk(hash).unwrap();
        assert!(!data.is_empty());
    }

    // Test enumeration
    let all_chunks = herald.enumerate_chunks();
    assert_eq!(all_chunks.len(), 1000);

    // Test statistics
    let stats = herald.get_stats();
    println!("Storage stats: {:?}", stats);
}

#[test]
fn test_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(SequenceStorage::new(temp_dir.path()).unwrap());

    println!("Testing concurrent access with multiple threads...");

    let sequences = generate_protein_sequences(10_000);
    let sequences = Arc::new(sequences);

    // Spawn multiple threads to process sequences concurrently
    let handles: Vec<_> = (0..4)
        .map(|thread_id| {
            let storage = Arc::clone(&storage);
            let sequences = Arc::clone(&sequences);

            thread::spawn(move || {
                let start = thread_id * 2500;
                let end = start + 2500;

                for (seq, header) in &sequences[start..end] {
                    storage
                        .store_sequence(
                            seq,
                            header,
                            DatabaseSource::Custom(format!("thread_{}", thread_id)),
                        )
                        .unwrap();
                }
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all sequences were stored
    let all_hashes = storage.list_all_hashes().unwrap();
    assert_eq!(all_hashes.len(), 10_000);

    println!("Successfully processed 10k sequences concurrently");
}

#[test]
#[ignore]
fn test_rebuild_index_performance() {
    let temp_dir = TempDir::new().unwrap();
    let storage = SequenceStorage::new(temp_dir.path()).unwrap();

    println!("Testing index rebuild performance...");

    // Populate with sequences
    let sequences = generate_protein_sequences(25_000);
    for (seq, header) in &sequences {
        storage
            .store_sequence(
                seq,
                header,
                DatabaseSource::UniProt(talaria_core::UniProtDatabase::TrEMBL),
            )
            .unwrap();
    }

    // Time index rebuild
    let start = Instant::now();
    storage.rebuild_index().unwrap();
    let rebuild_time = start.elapsed();

    println!(
        "Index rebuild for 25k sequences: {:.2}s",
        rebuild_time.as_secs_f64()
    );

    // Should be fast
    assert!(
        rebuild_time.as_secs() < 10,
        "Index rebuild too slow: {:.2}s",
        rebuild_time.as_secs_f64()
    );
}

#[test]
fn test_persistence_across_restarts() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();

    let sequences = generate_protein_sequences(1000);
    let mut stored_hashes = Vec::new();

    // Store sequences and close storage
    {
        let storage = SequenceStorage::new(&path).unwrap();
        for (seq, header) in &sequences {
            let hash = storage
                .store_sequence(seq, header, DatabaseSource::Custom("test".to_string()))
                .unwrap();
            stored_hashes.push(hash);
        }
        storage.save_indices().unwrap();
        // Storage dropped here, RocksDB closed
    }

    // Reopen and verify
    {
        let storage = SequenceStorage::new(&path).unwrap();

        // Verify all sequences are still there
        for hash in &stored_hashes {
            assert!(storage.canonical_exists(hash).unwrap());
        }

        // Verify count
        let all_hashes = storage.list_all_hashes().unwrap();
        assert_eq!(all_hashes.len(), 1000);

        println!("Successfully verified persistence across restart");
    }
}
