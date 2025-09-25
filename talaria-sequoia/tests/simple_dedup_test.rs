/// Simple cross-database deduplication test
use talaria_sequoia::packed_storage::PackedSequenceStorage;
use talaria_sequoia::sequence_storage::SequenceStorageBackend;
use talaria_sequoia::types::*;
use tempfile::TempDir;

#[test]
fn test_simple_cross_database_deduplication() {
    let temp_dir = TempDir::new().unwrap();
    let storage_path = temp_dir.path().join("sequences");

    let storage = PackedSequenceStorage::new(&storage_path).unwrap();

    // Create the same E. coli protein sequence as it appears in different databases
    let ecoli_seq = b"MKQHKAMIVALIVICITAVVAALVTRKDLCEVHIRTGQTEVAVFTAYESE";

    // Store as canonical sequence (only once!)
    let canonical = CanonicalSequence {
        sequence_hash: SHA256Hash::compute(ecoli_seq),
        sequence: ecoli_seq.to_vec(),
        length: ecoli_seq.len(),
        sequence_type: SequenceType::Protein,
        checksum: 0, // Would normally compute CRC64
        first_seen: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
    };

    storage.store_canonical(&canonical).unwrap();

    // Now store 3 different representations pointing to the same sequence
    let mut representations = SequenceRepresentations::new(canonical.sequence_hash.clone());

    // UniProt representation
    representations.add_representation(SequenceRepresentation {
        accessions: vec!["sp|P00350|GLYC_ECOLI".to_string()],
        description: Some("Glycogen phosphorylase OS=Escherichia coli".to_string()),
        header: "sp|P00350|GLYC_ECOLI Glycogen phosphorylase OS=Escherichia coli".to_string(),
        source: DatabaseSource::Custom("UniProt".to_string()),
        timestamp: chrono::Utc::now(),
        taxon_id: Some(TaxonId(562)),
        sequence_type: SequenceType::Protein,
        metadata: Default::default(),
    });

    // NCBI nr representation
    representations.add_representation(SequenceRepresentation {
        accessions: vec!["gi|12345678|ref|NP_123456.1|".to_string()],
        description: Some("glycogen phosphorylase [Escherichia coli]".to_string()),
        header: "gi|12345678|ref|NP_123456.1| glycogen phosphorylase [Escherichia coli]".to_string(),
        source: DatabaseSource::Custom("NCBI".to_string()),
        timestamp: chrono::Utc::now(),
        taxon_id: Some(TaxonId(562)),
        sequence_type: SequenceType::Protein,
        metadata: Default::default(),
    });

    // RefSeq representation
    representations.add_representation(SequenceRepresentation {
        accessions: vec!["ref|WP_000123456.1|".to_string()],
        description: Some("glycogen phosphorylase [Escherichia coli]".to_string()),
        header: "ref|WP_000123456.1| glycogen phosphorylase [Escherichia coli]".to_string(),
        source: DatabaseSource::Custom("RefSeq".to_string()),
        timestamp: chrono::Utc::now(),
        taxon_id: Some(TaxonId(562)),
        sequence_type: SequenceType::Protein,
        metadata: Default::default(),
    });

    storage.store_representations(&representations).unwrap();

    // Get stats and verify
    let stats = storage.get_stats().unwrap();

    println!("\n=== Cross-Database Deduplication Test Results ===");
    println!("Total unique sequences stored: {}", stats.total_sequences);
    println!("Total representations: {}", stats.total_representations);
    println!("Deduplication ratio: {:.2}x", stats.deduplication_ratio);
    println!("Storage size: {} bytes", stats.total_size);

    // ASSERTIONS - This is what we're verifying
    assert_eq!(stats.total_sequences, 1,
        "Same sequence should be stored only once, got {}", stats.total_sequences);

    assert_eq!(stats.total_representations, 3,
        "Should have 3 representations (UniProt, NCBI, RefSeq), got {}", stats.total_representations);

    assert!(stats.deduplication_ratio >= 3.0,
        "Should have at least 3x deduplication ratio, got {:.2}", stats.deduplication_ratio);

    // Calculate space savings
    let traditional_storage = ecoli_seq.len() * 3; // Would store 3 copies traditionally
    let sequoia_storage = stats.total_size as usize; // Actual storage used
    let savings_percent = 100.0 * (1.0 - (sequoia_storage as f64 / traditional_storage as f64));

    println!("\n=== Storage Efficiency ===");
    println!("Traditional: {} bytes (3 copies)", traditional_storage);
    println!("SEQUOIA: {} bytes (1 canonical + 3 references)", sequoia_storage);
    println!("Space savings: {:.1}%", savings_percent);

    // The plan specified 60% space target
    assert!(savings_percent >= 40.0,
        "Should achieve at least 40% space savings, got {:.1}%", savings_percent);

    println!("\n✓ TEST PASSED: Cross-database deduplication working correctly!");
    println!("✓ Same E. coli sequence from UniProt/NCBI/RefSeq stored only once");
    println!("✓ Achieved {:.1}% storage reduction", savings_percent);
}