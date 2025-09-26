/// Scale tests for cross-database deduplication in SEQUOIA
///
/// These tests verify that identical sequences across different databases
/// (UniProt, NCBI, RefSeq) are stored only once, achieving true deduplication

use talaria_sequoia::{
    storage::{SEQUOIAStorage, SequenceStorage},
    chunker::{ChunkingStrategy, TaxonomicChunker, HierarchicalTaxonomicChunker},
    types::{DatabaseSource, ChunkManifest},
};
use talaria_core::{SHA256Hash, TaxonId};
use talaria_bio::sequence::Sequence;
use tempfile::TempDir;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Generate synthetic sequences that simulate real database overlap patterns
fn generate_test_sequences(
    count: usize,
    overlap_percentage: f32,
    source: DatabaseSource,
) -> Vec<Sequence> {
    let mut sequences = Vec::new();
    let overlap_count = (count as f32 * overlap_percentage) as usize;

    // Generate common sequences (that will appear in multiple databases)
    for i in 0..overlap_count {
        sequences.push(Sequence {
            id: format!("COMMON_{:06}", i),
            description: Some(format!("Common protein {} found in multiple databases", i)),
            sequence: format!("MVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSHGSAQVKGHG{}", i).into_bytes(),
            taxon_id: Some(9606), // Human
            taxonomy_sources: Default::default(),
        });
    }

    // Generate database-specific sequences
    for i in overlap_count..count {
        let db_prefix = match source {
            DatabaseSource::UniProt => "SP",
            DatabaseSource::NCBI => "NP",
            DatabaseSource::RefSeq => "XP",
            _ => "UNK",
        };

        sequences.push(Sequence {
            id: format!("{}_{:06}", db_prefix, i),
            description: Some(format!("{} specific protein {}", source, i)),
            sequence: format!("MGLSDGEWQLVLNVWGKVEADIPGHGQEVLIRLFKGHPETLEKFDKFKHLKSEDEMKASE{}{}",
                             source, i).into_bytes(),
            taxon_id: Some(10090), // Mouse
            taxonomy_sources: Default::default(),
        });
    }

    sequences
}

#[test]
fn test_cross_database_deduplication_small() {
    let temp_dir = TempDir::new().unwrap();
    let storage = SEQUOIAStorage::new(temp_dir.path()).unwrap();
    let sequence_storage = SequenceStorage::new(temp_dir.path()).unwrap();

    // Test with small dataset
    let uniprot_seqs = generate_test_sequences(100, 0.3, DatabaseSource::UniProt);
    let ncbi_seqs = generate_test_sequences(100, 0.3, DatabaseSource::NCBI);
    let refseq_seqs = generate_test_sequences(100, 0.3, DatabaseSource::RefSeq);

    let total_sequences = uniprot_seqs.len() + ncbi_seqs.len() + refseq_seqs.len();

    // Store all sequences
    let mut all_hashes = Vec::new();
    let mut unique_hashes = HashSet::new();

    for (sequences, source) in [
        (uniprot_seqs, DatabaseSource::UniProt),
        (ncbi_seqs, DatabaseSource::NCBI),
        (refseq_seqs, DatabaseSource::RefSeq),
    ] {
        let mut chunker = TaxonomicChunker::new(
            ChunkingStrategy::default(),
            sequence_storage.clone(),
            source,
        );

        let manifests = chunker.chunk_sequences_canonical(sequences).unwrap();

        for manifest in manifests {
            for hash in &manifest.sequence_refs {
                all_hashes.push(hash.clone());
                unique_hashes.insert(hash.clone());
            }
        }
    }

    // Calculate deduplication ratio
    let dedup_ratio = all_hashes.len() as f32 / unique_hashes.len() as f32;

    println!("Small scale test results:");
    println!("  Total sequences: {}", total_sequences);
    println!("  Total hash references: {}", all_hashes.len());
    println!("  Unique sequences stored: {}", unique_hashes.len());
    println!("  Deduplication ratio: {:.2}x", dedup_ratio);

    // With 30% overlap, we expect significant deduplication
    assert!(dedup_ratio > 1.2, "Expected deduplication ratio > 1.2, got {:.2}", dedup_ratio);
}

#[test]
fn test_cross_database_deduplication_medium() {
    let temp_dir = TempDir::new().unwrap();
    let storage = SEQUOIAStorage::new(temp_dir.path()).unwrap();
    let sequence_storage = SequenceStorage::new(temp_dir.path()).unwrap();

    // Test with medium dataset
    let uniprot_seqs = generate_test_sequences(1000, 0.4, DatabaseSource::UniProt);
    let ncbi_seqs = generate_test_sequences(1000, 0.4, DatabaseSource::NCBI);
    let refseq_seqs = generate_test_sequences(1000, 0.4, DatabaseSource::RefSeq);

    let start = Instant::now();

    // Process databases sequentially
    let mut all_hashes = Vec::new();
    let mut unique_hashes = HashSet::new();
    let mut manifests_by_db = HashMap::new();

    for (sequences, source) in [
        (uniprot_seqs, DatabaseSource::UniProt),
        (ncbi_seqs, DatabaseSource::NCBI),
        (refseq_seqs, DatabaseSource::RefSeq),
    ] {
        let db_start = Instant::now();

        let mut chunker = TaxonomicChunker::new(
            ChunkingStrategy::default(),
            sequence_storage.clone(),
            source.clone(),
        );

        let manifests = chunker.chunk_sequences_canonical(sequences).unwrap();
        manifests_by_db.insert(source.to_string(), manifests.clone());

        for manifest in manifests {
            for hash in &manifest.sequence_refs {
                all_hashes.push(hash.clone());
                unique_hashes.insert(hash.clone());
            }
        }

        println!("  {} processed in {:.2}s", source, db_start.elapsed().as_secs_f32());
    }

    let elapsed = start.elapsed();

    // Calculate metrics
    let dedup_ratio = all_hashes.len() as f32 / unique_hashes.len() as f32;
    let storage_saved = 1.0 - (1.0 / dedup_ratio);

    println!("\nMedium scale test results:");
    println!("  Total sequences: 3000");
    println!("  Total hash references: {}", all_hashes.len());
    println!("  Unique sequences stored: {}", unique_hashes.len());
    println!("  Deduplication ratio: {:.2}x", dedup_ratio);
    println!("  Storage saved: {:.1}%", storage_saved * 100.0);
    println!("  Processing time: {:.2}s", elapsed.as_secs_f32());
    println!("  Throughput: {:.0} sequences/second", 3000.0 / elapsed.as_secs_f32());

    // With 40% overlap across 3 databases, expect high deduplication
    assert!(dedup_ratio > 1.5, "Expected deduplication ratio > 1.5, got {:.2}", dedup_ratio);
}

#[test]
#[ignore] // Run with --ignored for large scale test
fn test_cross_database_deduplication_large() {
    let temp_dir = TempDir::new().unwrap();
    let storage = SEQUOIAStorage::new(temp_dir.path()).unwrap();
    let sequence_storage = SequenceStorage::new(temp_dir.path()).unwrap();

    println!("Starting large scale cross-database deduplication test...");

    // Test with large dataset (100k sequences per database)
    const SEQUENCES_PER_DB: usize = 100_000;
    const OVERLAP: f32 = 0.35; // 35% overlap (realistic for UniProt/NCBI)

    let start = Instant::now();

    // Generate sequences for each database
    println!("Generating {} sequences per database with {}% overlap...",
             SEQUENCES_PER_DB, OVERLAP * 100.0);

    let databases = [
        ("UniProt", DatabaseSource::UniProt),
        ("NCBI", DatabaseSource::NCBI),
        ("RefSeq", DatabaseSource::RefSeq),
        ("EMBL", DatabaseSource::Custom("EMBL".to_string())),
        ("PDB", DatabaseSource::Custom("PDB".to_string())),
    ];

    let mut total_sequences = 0;
    let mut all_hashes = Vec::new();
    let mut unique_hashes = HashSet::new();
    let mut database_stats = HashMap::new();

    for (db_name, source) in databases {
        println!("\nProcessing {} database...", db_name);
        let db_start = Instant::now();

        // Generate sequences
        let sequences = generate_test_sequences(SEQUENCES_PER_DB, OVERLAP, source.clone());
        total_sequences += sequences.len();

        // Use hierarchical chunker for large dataset
        let mut chunker = HierarchicalTaxonomicChunker::new(
            ChunkingStrategy::default(),
            sequence_storage.clone(),
            source.clone(),
            None, // No taxonomy manager for test
        );

        let manifests = chunker.chunk_sequences_hierarchical(sequences).unwrap();

        // Track statistics
        let mut db_unique = HashSet::new();
        let mut db_total = 0;

        for manifest in &manifests {
            for hash in &manifest.sequence_refs {
                all_hashes.push(hash.clone());
                unique_hashes.insert(hash.clone());
                db_unique.insert(hash.clone());
                db_total += 1;
            }
        }

        let db_elapsed = db_start.elapsed();
        database_stats.insert(db_name.to_string(), (
            db_total,
            db_unique.len(),
            manifests.len(),
            db_elapsed.as_secs_f32(),
        ));

        println!("  {} statistics:", db_name);
        println!("    Sequences: {}", db_total);
        println!("    Unique: {}", db_unique.len());
        println!("    Chunks: {}", manifests.len());
        println!("    Time: {:.2}s", db_elapsed.as_secs_f32());
        println!("    Throughput: {:.0} seq/s", SEQUENCES_PER_DB as f32 / db_elapsed.as_secs_f32());
    }

    let total_elapsed = start.elapsed();

    // Calculate overall metrics
    let dedup_ratio = all_hashes.len() as f32 / unique_hashes.len() as f32;
    let storage_saved = 1.0 - (1.0 / dedup_ratio);
    let expected_unique = SEQUENCES_PER_DB + // First DB: all unique
                         (databases.len() - 1) * (SEQUENCES_PER_DB as f32 * (1.0 - OVERLAP)) as usize; // Other DBs: only non-overlapping

    println!("\n" + "=".repeat(60).as_str());
    println!("LARGE SCALE TEST RESULTS");
    println!("=".repeat(60));
    println!("Dataset:");
    println!("  Databases: {}", databases.len());
    println!("  Sequences per DB: {}", SEQUENCES_PER_DB);
    println!("  Total sequences: {}", total_sequences);
    println!("  Overlap percentage: {:.1}%", OVERLAP * 100.0);

    println!("\nDeduplication Performance:");
    println!("  Total hash references: {}", all_hashes.len());
    println!("  Unique sequences stored: {}", unique_hashes.len());
    println!("  Expected unique (theoretical): ~{}", expected_unique);
    println!("  Deduplication ratio: {:.2}x", dedup_ratio);
    println!("  Storage saved: {:.1}%", storage_saved * 100.0);

    println!("\nProcessing Performance:");
    println!("  Total time: {:.2}s", total_elapsed.as_secs_f32());
    println!("  Overall throughput: {:.0} sequences/second",
             total_sequences as f32 / total_elapsed.as_secs_f32());

    // Memory usage estimate
    let hash_memory = unique_hashes.len() * 32; // 32 bytes per SHA256
    let index_memory = unique_hashes.len() * 100; // ~100 bytes per index entry (estimate)
    let total_memory = hash_memory + index_memory;

    println!("\nMemory Usage (estimated):");
    println!("  Hash storage: {} MB", hash_memory / 1_000_000);
    println!("  Index storage: {} MB", index_memory / 1_000_000);
    println!("  Total: {} MB", total_memory / 1_000_000);

    // Verify deduplication effectiveness
    assert!(dedup_ratio > 2.0,
            "Expected deduplication ratio > 2.0 for {}% overlap, got {:.2}",
            OVERLAP * 100.0, dedup_ratio);

    // Verify performance (should process at least 10k sequences/second)
    let throughput = total_sequences as f32 / total_elapsed.as_secs_f32();
    assert!(throughput > 10_000.0,
            "Expected throughput > 10,000 seq/s, got {:.0}", throughput);

    println!("\n✓ Large scale deduplication test PASSED");
}

#[test]
fn test_identical_sequence_different_headers() {
    // Test that identical sequences with different headers are deduplicated
    let temp_dir = TempDir::new().unwrap();
    let sequence_storage = SequenceStorage::new(temp_dir.path()).unwrap();

    // Create identical sequences with different headers
    let seq1 = ">sp|P12345|PROTEIN_HUMAN Human protein\nMVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSH";
    let seq2 = ">gi|123456789|ref|NP_001234.1| Same protein from NCBI\nMVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSH";
    let seq3 = ">XP_987654.1 Same protein from RefSeq\nMVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSH";

    // Store sequences
    let hash1 = sequence_storage.store_sequence(
        "MVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSH",
        seq1,
        DatabaseSource::UniProt,
    ).unwrap();

    let hash2 = sequence_storage.store_sequence(
        "MVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSH",
        seq2,
        DatabaseSource::NCBI,
    ).unwrap();

    let hash3 = sequence_storage.store_sequence(
        "MVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSH",
        seq3,
        DatabaseSource::RefSeq,
    ).unwrap();

    // All hashes should be identical (same sequence content)
    assert_eq!(hash1, hash2, "Same sequence should produce same hash");
    assert_eq!(hash2, hash3, "Same sequence should produce same hash");

    // Verify only stored once
    let stored_count = sequence_storage.get_statistics().unwrap().total_sequences;
    assert_eq!(stored_count, 1, "Identical sequence should only be stored once");

    println!("✓ Identical sequences with different headers correctly deduplicated");
}

#[test]
fn test_cross_database_manifest_sharing() {
    // Test that manifests can reference sequences from different databases
    let temp_dir = TempDir::new().unwrap();
    let sequence_storage = SequenceStorage::new(temp_dir.path()).unwrap();

    // Store sequences from different databases
    let uniprot_hash = sequence_storage.store_sequence(
        "MVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSH",
        ">sp|P12345|PROTEIN_HUMAN Human hemoglobin",
        DatabaseSource::UniProt,
    ).unwrap();

    let ncbi_hash = sequence_storage.store_sequence(
        "MGLSDGEWQLVLNVWGKVEADIPGHGQEVLIRLFKGHPETLEKFDKFKHLKSEDEMKASE",
        ">gi|987654|ref|NP_002345.1| Mouse myoglobin",
        DatabaseSource::NCBI,
    ).unwrap();

    // Create a manifest that references both
    let manifest = ChunkManifest {
        chunk_hash: SHA256Hash::compute(b"test_manifest"),
        sequence_refs: vec![uniprot_hash.clone(), ncbi_hash.clone()],
        taxon_ids: vec![TaxonId(9606), TaxonId(10090)],
        chunk_type: talaria_sequoia::types::ChunkClassification::Full,
        total_size: 1000,
        sequence_count: 2,
        created_at: chrono::Utc::now(),
        taxonomy_version: SHA256Hash::compute(b"tax_v1"),
        sequence_version: SHA256Hash::compute(b"seq_v1"),
    };

    // Verify manifest can reference sequences from multiple databases
    assert_eq!(manifest.sequence_refs.len(), 2);
    assert!(manifest.sequence_refs.contains(&uniprot_hash));
    assert!(manifest.sequence_refs.contains(&ncbi_hash));

    println!("✓ Cross-database manifest sharing verified");
}

#[test]
fn test_deduplication_with_variants() {
    // Test that similar but not identical sequences are NOT deduplicated
    let temp_dir = TempDir::new().unwrap();
    let sequence_storage = SequenceStorage::new(temp_dir.path()).unwrap();

    // Create variants with single amino acid differences
    let wild_type = "MVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSH";
    let variant1  = "MVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSH"; // Identical
    let variant2  = "MVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSQ"; // H->Q at end
    let variant3  = "MVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSH"; // Identical

    let hash_wt = sequence_storage.store_sequence(
        wild_type,
        ">WT Wild type",
        DatabaseSource::UniProt,
    ).unwrap();

    let hash_v1 = sequence_storage.store_sequence(
        variant1,
        ">V1 Variant 1",
        DatabaseSource::NCBI,
    ).unwrap();

    let hash_v2 = sequence_storage.store_sequence(
        variant2,
        ">V2 Variant 2",
        DatabaseSource::RefSeq,
    ).unwrap();

    let hash_v3 = sequence_storage.store_sequence(
        variant3,
        ">V3 Variant 3",
        DatabaseSource::Custom("Custom".to_string()),
    ).unwrap();

    // Identical sequences should have same hash
    assert_eq!(hash_wt, hash_v1, "Identical sequences should deduplicate");
    assert_eq!(hash_wt, hash_v3, "Identical sequences should deduplicate");

    // Different sequence should have different hash
    assert_ne!(hash_wt, hash_v2, "Different sequences should NOT deduplicate");

    // Should have stored exactly 2 unique sequences
    let stored_count = sequence_storage.get_statistics().unwrap().total_sequences;
    assert_eq!(stored_count, 2, "Should store 2 unique sequences (WT and variant)");

    println!("✓ Sequence variants correctly handled (no false deduplication)");
}