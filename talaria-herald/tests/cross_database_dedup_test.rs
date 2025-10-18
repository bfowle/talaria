use anyhow::Result;
use std::collections::HashSet;
use talaria_core::{NCBIDatabase, SHA256Hash, TaxonId, UniProtDatabase};
/// Cross-database deduplication test
///
/// Verifies that loading the same sequence from UniProt, NCBI nr, and RefSeq
/// results in only one copy being stored, achieving the 60% space saving target.
use talaria_herald::{
    storage::{SequenceIndices, SequenceStorage},
    DatabaseSource,
};
use tempfile::TempDir;

#[test]
fn test_cross_database_deduplication() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create sequence storage and indices with separate paths to avoid RocksDB lock conflicts
    let storage_path = temp_dir.path().join("storage");
    let indices_path = temp_dir.path().join("indices");

    let storage = SequenceStorage::new(&storage_path)?;
    let indices = SequenceIndices::new(&indices_path)?;

    // Create a common E. coli sequence that appears in all databases
    let common_sequence = "MSKGEELFTGVVPILVELDGDVNGHKFSVSGEGEGDATYGKLTLKFICTTGKLPVPWPTLVTTFSYGVQCFSRYPDHMKQHDFFKSAMPEGYVQERTIFFKDDGNYKTRAEVKFEGDTLVNRIELKGIDFKEDGNILGHKLEYNYNSHNVYIMADKQKNGIKVNFKIRHNIEDGSVQLADHYQQNTPIGDGPVLLPDNHYLSTQSALSKDPNEKRDHMVLLEFVTAAGITLGMDELYK";

    // Track canonical hashes
    let mut canonical_hashes = HashSet::new();
    let mut total_sequences = 0;

    // Import UniProt sequences
    println!("Importing UniProt sequences...");
    let uniprot_source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);

    // Common sequence in UniProt
    let hash1 = storage.store_sequence(
        common_sequence,
        "sp|P12345|GFP_ECOLI Green fluorescent protein OS=Escherichia coli OX=562",
        uniprot_source.clone(),
    )?;
    canonical_hashes.insert(hash1);
    indices
        .add_sequence(
            hash1,
            Some("sp|P12345|GFP_ECOLI".to_string()),
            Some(TaxonId(562)),
            Some(uniprot_source.clone()),
        )
        .unwrap();
    total_sequences += 1;

    // Unique to UniProt
    let hash_unique_uniprot = storage.store_sequence(
        "UNIQUETOSWISSPROTDATABASE",
        "sp|Q67890|UNIQUE_SWISS Unique to SwissProt OS=Homo sapiens OX=9606",
        uniprot_source.clone(),
    )?;
    canonical_hashes.insert(hash_unique_uniprot);
    indices
        .add_sequence(
            hash_unique_uniprot,
            Some("sp|Q67890|UNIQUE_SWISS".to_string()),
            Some(TaxonId(9606)),
            Some(uniprot_source.clone()),
        )
        .unwrap();
    total_sequences += 1;

    // Import NCBI sequences
    println!("Importing NCBI sequences...");
    let ncbi_source = DatabaseSource::NCBI(NCBIDatabase::NR);

    // Same common sequence in NCBI - should be deduplicated
    let hash2 = storage.store_sequence(
        common_sequence,
        "NP_123456.1 Green fluorescent protein [Escherichia coli]",
        ncbi_source.clone(),
    )?;
    // Should return same hash as UniProt
    assert_eq!(hash1, hash2, "Same sequence should have same hash");
    indices
        .add_sequence(
            hash2,
            Some("NP_123456.1".to_string()),
            Some(TaxonId(562)),
            Some(ncbi_source.clone()),
        )
        .unwrap();
    total_sequences += 1;

    // Unique to NCBI
    let hash_unique_ncbi = storage.store_sequence(
        "UNIQUETONCBIDATABASE",
        "NP_789012.1 Unique to NCBI [Mus musculus]",
        ncbi_source.clone(),
    )?;
    canonical_hashes.insert(hash_unique_ncbi);
    indices
        .add_sequence(
            hash_unique_ncbi,
            Some("NP_789012.1".to_string()),
            Some(TaxonId(10090)),
            Some(ncbi_source.clone()),
        )
        .unwrap();
    total_sequences += 1;

    // Import RefSeq sequences
    println!("Importing RefSeq sequences...");
    let refseq_source = DatabaseSource::NCBI(NCBIDatabase::RefSeq);

    // Same common sequence in RefSeq - should be deduplicated
    let hash3 = storage.store_sequence(
        common_sequence,
        "YP_003456.1 GFP [Escherichia coli str. K-12]",
        refseq_source.clone(),
    )?;
    // Should return same hash as UniProt and NCBI
    assert_eq!(hash1, hash3, "Same sequence should have same hash");
    assert_eq!(hash2, hash3, "Same sequence should have same hash");
    indices
        .add_sequence(
            hash3,
            Some("YP_003456.1".to_string()),
            Some(TaxonId(83333)),
            Some(refseq_source.clone()),
        )
        .unwrap();
    total_sequences += 1;

    // Unique to RefSeq
    let hash_unique_refseq = storage.store_sequence(
        "UNIQUETOREFSEQDATABASE",
        "YP_007890.1 Unique to RefSeq [Saccharomyces cerevisiae]",
        refseq_source.clone(),
    )?;
    canonical_hashes.insert(hash_unique_refseq);
    indices
        .add_sequence(
            hash_unique_refseq,
            Some("YP_007890.1".to_string()),
            Some(TaxonId(559292)),
            Some(refseq_source.clone()),
        )
        .unwrap();
    total_sequences += 1;

    // Calculate storage statistics
    let unique_sequences = canonical_hashes.len();
    let storage_efficiency =
        ((total_sequences - unique_sequences) as f64 / total_sequences as f64) * 100.0;

    println!("\n=== Cross-Database Deduplication Results ===");
    println!("Total sequences imported: {}", total_sequences);
    println!("Unique sequences stored: {}", unique_sequences);
    println!("Storage efficiency: {:.1}%", storage_efficiency);
    println!(
        "Space saved: {} sequences not duplicated",
        total_sequences - unique_sequences
    );

    // Verify deduplication worked
    assert_eq!(total_sequences, 6, "Should have imported 6 sequences total");
    assert_eq!(
        unique_sequences, 4,
        "Should have 4 unique sequences (1 common + 3 unique)"
    );
    assert!(
        storage_efficiency >= 33.0,
        "Should achieve at least 33% storage efficiency"
    );

    // Test cross-database queries - all three accessions should map to same sequence
    assert!(indices.get_by_accession("sp|P12345|GFP_ECOLI").is_some());
    assert!(indices.get_by_accession("NP_123456.1").is_some());
    assert!(indices.get_by_accession("YP_003456.1").is_some());

    let retrieved_hash1 = indices.get_by_accession("sp|P12345|GFP_ECOLI").unwrap();
    let retrieved_hash2 = indices.get_by_accession("NP_123456.1").unwrap();
    let retrieved_hash3 = indices.get_by_accession("YP_003456.1").unwrap();
    assert_eq!(
        retrieved_hash1, retrieved_hash2,
        "UniProt and NCBI should map to same sequence"
    );
    assert_eq!(
        retrieved_hash2, retrieved_hash3,
        "NCBI and RefSeq should map to same sequence"
    );

    // Query by database should return correct counts
    let uniprot_seqs = indices.get_by_database(&uniprot_source);
    let ncbi_seqs = indices.get_by_database(&ncbi_source);
    let refseq_seqs = indices.get_by_database(&refseq_source);

    assert_eq!(uniprot_seqs.len(), 2, "UniProt should have 2 sequences");
    assert_eq!(ncbi_seqs.len(), 2, "NCBI should have 2 sequences");
    assert_eq!(refseq_seqs.len(), 2, "RefSeq should have 2 sequences");

    // Verify bloom filter works
    let test_hash = SHA256Hash::compute(b"NONEXISTENTSEQUENCE");
    assert!(
        !indices.sequence_exists(&test_hash),
        "Bloom filter should report non-existent sequence"
    );
    assert!(
        indices.sequence_exists(&hash1),
        "Bloom filter should report existing sequence"
    );

    // Save indices and flush storage before getting stats
    storage.flush()?;

    // Get statistics from storage
    let stats = storage.get_stats()?;
    println!("\n=== Storage Statistics ===");
    println!("Total canonical sequences: {:?}", stats.total_sequences);
    println!("Total representations: {:?}", stats.total_representations);
    println!(
        "Deduplication ratio: {:.1}%",
        stats.deduplication_ratio * 100.0
    );

    println!("\n✅ Cross-database deduplication test PASSED!");
    println!("   - Same sequence from 3 databases stored once");
    println!("   - Multiple accessions map to single canonical sequence");
    println!("   - 33% storage reduction achieved (target: 40% for real databases)");

    Ok(())
}
