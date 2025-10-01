use anyhow::Result;
use std::collections::HashSet;
use std::time::Instant;
/// Scale test for cross-database deduplication
use talaria_sequoia::{storage::SequenceStorage, types::DatabaseSource};
use tempfile::TempDir;

#[test]
fn test_cross_database_deduplication_scale() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = SequenceStorage::new(temp_dir.path())?;

    // Common sequences that appear in multiple databases
    let common_sequences = vec![
        (
            "MVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSHGSAQVKGHG",
            "Common protein 1",
        ),
        (
            "MGLSDGEWQLVLNVWGKVEADIPGHGQEVLIRLFKGHPETLEKFDKFKHLKSEDEMKASE",
            "Common protein 2",
        ),
        (
            "MTEYKLVVVGAGGVGKSALTIQLIQNHFVDEYDPTIEDSYRKQVVIDGETCLLDILDTAG",
            "Common protein 3",
        ),
    ];

    // Database-specific sequences
    let uniprot_specific = vec![
        ("MKQLEDKVEELLSKNYHLENEVARLKKLVGER", "UniProt specific 1"),
        ("MFRTKPHPGRHKTSRPSRGSQGSQGSQGSQGSR", "UniProt specific 2"),
    ];

    let ncbi_specific = vec![
        ("MSDNGPQNQRNAPRITFGGPSDSTGSNQNGERSGAR", "NCBI specific 1"),
        (
            "MAFSAEDVLKEYDRRRRMEALLLSLYYPNDRKLLDYKEWSPPRVQVECPKAPVEWNNPPSEK",
            "NCBI specific 2",
        ),
    ];

    let refseq_specific = vec![
        (
            "MKKLLFAIPLVVPFYSHSRYLTEKEREMFAKLGAKPNFYKINQLLGFSVDTARTAACNLIPKDVYESS",
            "RefSeq specific 1",
        ),
        (
            "MSGRGKQGGKARAKAKSRSSRAGLQFPVGRVHRLLRKGNYAERVGAGAPVYLAAVLEYLTAEILELAG",
            "RefSeq specific 2",
        ),
    ];

    let start = Instant::now();
    let mut all_hashes = Vec::new();
    let mut unique_hashes = HashSet::new();

    // Store UniProt sequences
    println!("Storing UniProt sequences...");
    for (seq, desc) in &common_sequences {
        let header = format!(">sp|UNIPROT|{}", desc);
        let hash = storage.store_sequence(
            seq,
            &header,
            DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt),
        )?;
        all_hashes.push(hash.clone());
        unique_hashes.insert(hash);
    }
    for (seq, desc) in &uniprot_specific {
        let header = format!(">sp|UNIPROT|{}", desc);
        let hash = storage.store_sequence(
            seq,
            &header,
            DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt),
        )?;
        all_hashes.push(hash.clone());
        unique_hashes.insert(hash);
    }

    // Store NCBI sequences (including common ones)
    println!("Storing NCBI sequences...");
    for (seq, desc) in &common_sequences {
        let header = format!(">gi|NCBI|{}", desc);
        let hash = storage.store_sequence(
            seq,
            &header,
            DatabaseSource::NCBI(talaria_core::NCBIDatabase::NR),
        )?;
        all_hashes.push(hash.clone());
        unique_hashes.insert(hash);
    }
    for (seq, desc) in &ncbi_specific {
        let header = format!(">gi|NCBI|{}", desc);
        let hash = storage.store_sequence(
            seq,
            &header,
            DatabaseSource::NCBI(talaria_core::NCBIDatabase::NR),
        )?;
        all_hashes.push(hash.clone());
        unique_hashes.insert(hash);
    }

    // Store RefSeq sequences (including common ones)
    println!("Storing RefSeq sequences...");
    for (seq, desc) in &common_sequences {
        let header = format!(">ref|REFSEQ|{}", desc);
        let hash = storage.store_sequence(
            seq,
            &header,
            DatabaseSource::NCBI(talaria_core::NCBIDatabase::RefSeqProtein),
        )?;
        all_hashes.push(hash.clone());
        unique_hashes.insert(hash);
    }
    for (seq, desc) in &refseq_specific {
        let header = format!(">ref|REFSEQ|{}", desc);
        let hash = storage.store_sequence(
            seq,
            &header,
            DatabaseSource::NCBI(talaria_core::NCBIDatabase::RefSeqProtein),
        )?;
        all_hashes.push(hash.clone());
        unique_hashes.insert(hash);
    }

    let elapsed = start.elapsed();

    // Calculate deduplication metrics
    let total_sequences = (common_sequences.len() * 3)
        + uniprot_specific.len()
        + ncbi_specific.len()
        + refseq_specific.len();

    let dedup_ratio = all_hashes.len() as f32 / unique_hashes.len() as f32;
    let storage_saved = 1.0 - (unique_hashes.len() as f32 / total_sequences as f32);

    println!("\n=== Cross-Database Deduplication Results ===");
    println!("Total sequences submitted: {}", total_sequences);
    println!("Total hash references: {}", all_hashes.len());
    println!("Unique sequences stored: {}", unique_hashes.len());
    println!("Deduplication ratio: {:.2}x", dedup_ratio);
    println!("Storage saved: {:.1}%", storage_saved * 100.0);
    println!("Processing time: {:.3}s", elapsed.as_secs_f32());

    // Verify deduplication worked
    assert_eq!(
        all_hashes.len(),
        total_sequences,
        "All sequences should be referenced"
    );
    assert_eq!(
        unique_hashes.len(),
        common_sequences.len()
            + uniprot_specific.len()
            + ncbi_specific.len()
            + refseq_specific.len(),
        "Common sequences should be deduplicated"
    );

    // The deduplication ratio should be significant
    assert!(
        dedup_ratio > 1.3,
        "Expected significant deduplication, got ratio of {:.2}",
        dedup_ratio
    );

    println!("✓ Cross-database deduplication test PASSED");

    Ok(())
}

#[test]
fn test_large_scale_deduplication() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = SequenceStorage::new(temp_dir.path())?;

    const NUM_SEQUENCES: usize = 10000;
    const OVERLAP_PERCENTAGE: f32 = 0.4; // 40% overlap

    let overlap_count = (NUM_SEQUENCES as f32 * OVERLAP_PERCENTAGE) as usize;
    let unique_count = NUM_SEQUENCES - overlap_count;

    println!(
        "Starting large scale test with {} sequences per database",
        NUM_SEQUENCES
    );
    println!("Overlap: {}%", OVERLAP_PERCENTAGE * 100.0);

    let start = Instant::now();
    let mut all_hashes = Vec::new();
    let mut unique_hashes = HashSet::new();

    // Generate and store sequences for each database
    let databases = [
        DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt),
        DatabaseSource::NCBI(talaria_core::NCBIDatabase::NR),
        DatabaseSource::NCBI(talaria_core::NCBIDatabase::RefSeqProtein),
    ];

    for (db_idx, source) in databases.iter().enumerate() {
        let db_start = Instant::now();

        // Store overlapping sequences
        for i in 0..overlap_count {
            let seq = format!(
                "COMMON{:04}MVLSPADKTNVKAAWGKVGAHAGEYGAEALERMFLSFPTTKTYFPHFDLSH",
                i
            );
            let header = format!(">{}|COMMON_{:06}", source, i);
            let hash = storage.store_sequence(&seq, &header, source.clone())?;
            all_hashes.push(hash.clone());
            unique_hashes.insert(hash);
        }

        // Store database-specific sequences
        for i in 0..unique_count {
            let seq = format!(
                "DB{}_SEQ{:04}MGLSDGEWQLVLNVWGKVEADIPGHGQEVLIRLFKGHPETLEKFD",
                db_idx, i
            );
            let header = format!(">{}|SPECIFIC_{:06}", source, i);
            let hash = storage.store_sequence(&seq, &header, source.clone())?;
            all_hashes.push(hash.clone());
            unique_hashes.insert(hash);
        }

        println!(
            "  {} processed in {:.2}s",
            source,
            db_start.elapsed().as_secs_f32()
        );
    }

    let elapsed = start.elapsed();

    // Calculate metrics
    let total_sequences = NUM_SEQUENCES * 3;
    let expected_unique = overlap_count + (unique_count * 3);
    let dedup_ratio = all_hashes.len() as f32 / unique_hashes.len() as f32;
    let storage_saved = 1.0 - (unique_hashes.len() as f32 / total_sequences as f32);

    println!("\n=== Large Scale Deduplication Results ===");
    println!("Total sequences: {}", total_sequences);
    println!("Expected unique: {}", expected_unique);
    println!("Actual unique: {}", unique_hashes.len());
    println!("Deduplication ratio: {:.2}x", dedup_ratio);
    println!("Storage saved: {:.1}%", storage_saved * 100.0);
    println!("Processing time: {:.2}s", elapsed.as_secs_f32());
    println!(
        "Throughput: {:.0} sequences/second",
        total_sequences as f32 / elapsed.as_secs_f32()
    );

    // Verify results
    assert_eq!(
        unique_hashes.len(),
        expected_unique,
        "Deduplication should match expected"
    );
    assert!(
        dedup_ratio > 1.3,
        "With 40% overlap, expect ratio > 1.3, got {:.2}",
        dedup_ratio
    );

    println!("✓ Large scale deduplication test PASSED");

    Ok(())
}
