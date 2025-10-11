use std::time::Instant;
/// Taxonomy query performance benchmark
///
/// Target from SEQUOIA_REFACTOR_PLAN.md: Query "all bacterial proteins" in <1 second
use talaria_sequoia::{indices::SequenceIndices, taxonomy_filter::TaxonomyFilter, types::*};
use tempfile::TempDir;

#[test]
fn test_taxonomy_query_performance() {
    let temp_dir = TempDir::new().unwrap();
    let indices_path = temp_dir.path();

    let indices = SequenceIndices::new(indices_path).unwrap();

    // Simulate realistic database with taxonomic distribution
    // 1M sequences across different taxa
    println!("Setting up test data: 1M sequences...");
    let setup_start = Instant::now();

    let taxa_distribution = vec![
        (TaxonId(2), 400_000),     // Bacteria - 40%
        (TaxonId(2157), 50_000),   // Archaea - 5%
        (TaxonId(2759), 300_000),  // Eukaryota - 30%
        (TaxonId(10239), 150_000), // Viruses - 15%
        (TaxonId(562), 100_000),   // E. coli (subset of Bacteria) - 10%
    ];

    let mut sequence_count = 0;
    for (taxon_id, count) in &taxa_distribution {
        for i in 0..(*count) {
            let hash = SHA256Hash::compute(format!("seq_{}_{}", taxon_id.0, i).as_bytes());
            let accession = format!("ACC_{:07}", sequence_count);

            indices
                .add_sequence(hash.clone(), Some(accession.clone()), Some(*taxon_id), None)
                .unwrap();

            sequence_count += 1;

            // Also add E. coli sequences to Bacteria
            if *taxon_id == TaxonId(562) {
                indices
                    .add_sequence(
                        hash.clone(),
                        Some(accession.clone()),
                        Some(TaxonId(2)),
                        None,
                    )
                    .unwrap();
            }
        }
    }

    let setup_time = setup_start.elapsed();
    println!("Setup complete in {:?}", setup_time);

    // Test 1: Query all bacterial proteins (cold cache)
    println!("\nTest 1: Query all bacterial proteins (cold)");
    let cold_start = Instant::now();
    let bacterial_sequences = indices.get_by_taxonomy(TaxonId(2));
    let cold_time = cold_start.elapsed();

    println!(
        "  Found {} bacterial sequences in {:?}",
        bacterial_sequences.len(),
        cold_time
    );
    assert!(
        bacterial_sequences.len() >= 400_000,
        "Should find all bacterial sequences"
    );
    assert!(
        cold_time.as_secs() < 1,
        "Cold query should complete in <1 second"
    );

    // Test 2: Query all bacterial proteins (warm cache)
    println!("\nTest 2: Query all bacterial proteins (cached)");
    let warm_start = Instant::now();
    let bacterial_sequences_cached = indices.get_by_taxonomy(TaxonId(2));
    let warm_time = warm_start.elapsed();

    println!(
        "  Found {} bacterial sequences in {:?}",
        bacterial_sequences_cached.len(),
        warm_time
    );
    assert_eq!(bacterial_sequences.len(), bacterial_sequences_cached.len());
    assert!(
        warm_time.as_millis() < 100,
        "Cached query should complete in <100ms"
    );

    // Test 3: Complex boolean filter
    println!("\nTest 3: Complex filter 'Bacteria AND NOT Escherichia'");
    let filter = TaxonomyFilter::parse("Bacteria AND NOT Escherichia").unwrap();

    let complex_start = Instant::now();
    let mut filtered_count = 0;

    // Simulate filtering chunks
    for (taxon_id, count) in &taxa_distribution {
        if filter.matches(&[*taxon_id]) {
            filtered_count += count;
        }
    }

    let complex_time = complex_start.elapsed();
    println!(
        "  Matched {} sequences in {:?}",
        filtered_count, complex_time
    );
    assert!(
        complex_time.as_millis() < 10,
        "Filter evaluation should be <10ms"
    );

    // Test 4: Bloom filter performance
    println!("\nTest 4: Sequence existence check (Bloom filter)");
    let test_hash = SHA256Hash::compute(b"test_sequence");

    let bloom_start = Instant::now();
    for _ in 0..100_000 {
        let _ = indices.sequence_exists(&test_hash);
    }
    let bloom_time = bloom_start.elapsed();

    let ops_per_second = 100_000.0 / bloom_time.as_secs_f64();
    println!(
        "  100K lookups in {:?} ({:.0} ops/sec)",
        bloom_time, ops_per_second
    );
    assert!(ops_per_second > 1_000_000.0, "Should achieve >1M ops/sec");

    // Summary
    println!("\n{}", "=".repeat(50));
    println!("PERFORMANCE SUMMARY");
    println!("{}", "=".repeat(50));
    println!("✓ Cold taxonomy query: {:?} (target: <1s)", cold_time);
    println!("✓ Cached taxonomy query: {:?} (target: <100ms)", warm_time);
    println!("✓ Boolean filter: {:?} (target: <10ms)", complex_time);
    println!(
        "✓ Bloom filter: {:.0} ops/sec (target: >1M)",
        ops_per_second
    );
}

#[test]
fn test_taxonomy_filter_correctness() {
    // Test various boolean expressions
    let test_cases = vec![
        ("Bacteria", vec![TaxonId(2)], true),
        ("Bacteria", vec![TaxonId(562)], false), // E. coli ID, not Bacteria ID
        ("Bacteria OR Escherichia", vec![TaxonId(2)], true),
        ("Bacteria OR Escherichia", vec![TaxonId(561)], true),
        (
            "Bacteria AND Escherichia",
            vec![TaxonId(2), TaxonId(561)],
            true,
        ),
        ("Bacteria AND Escherichia", vec![TaxonId(2)], false),
        ("Bacteria AND NOT Escherichia", vec![TaxonId(2)], true),
        (
            "Bacteria AND NOT Escherichia",
            vec![TaxonId(2), TaxonId(561)],
            false,
        ),
        (
            "(Bacteria OR Archaea) AND NOT Viruses",
            vec![TaxonId(2)],
            true,
        ),
        (
            "(Bacteria OR Archaea) AND NOT Viruses",
            vec![TaxonId(2157)],
            true,
        ),
        (
            "(Bacteria OR Archaea) AND NOT Viruses",
            vec![TaxonId(10239)],
            false,
        ),
    ];

    for (expr, taxon_ids, expected) in test_cases {
        let filter = TaxonomyFilter::parse(expr).unwrap();
        let result = filter.matches(&taxon_ids);
        assert_eq!(
            result, expected,
            "Filter '{}' with {:?} should be {}",
            expr, taxon_ids, expected
        );
    }

    println!("✓ All taxonomy filter expressions evaluated correctly");
}
