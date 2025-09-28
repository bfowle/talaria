/// Simple test to verify database diff functionality compiles and works
#[test]
fn test_database_diff_format_bytes() {
    use talaria_sequoia::format_bytes;

    // Test basic byte formatting
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1024), "1.00 KB");
    assert_eq!(format_bytes(1536), "1.50 KB");
    assert_eq!(format_bytes(1048576), "1.00 MB");
    assert_eq!(format_bytes(1073741824), "1.00 GB");

    println!("✓ format_bytes tests passed");
}

#[test]
fn test_database_comparison_struct() {
    use talaria_sequoia::{DatabaseComparison, ChunkAnalysis, SequenceAnalysis, TaxonomyAnalysis, StorageMetrics};

    // Create a mock comparison result
    let comparison = DatabaseComparison {
        chunk_analysis: ChunkAnalysis {
            total_chunks_a: 100,
            total_chunks_b: 150,
            shared_chunks: vec![],
            unique_to_a: vec![],
            unique_to_b: vec![],
            shared_percentage_a: 30.0,
            shared_percentage_b: 20.0,
        },
        sequence_analysis: SequenceAnalysis {
            total_sequences_a: 1000,
            total_sequences_b: 1500,
            shared_sequences: 300,
            unique_to_a: 700,
            unique_to_b: 1200,
            sample_shared_ids: vec!["seq1".to_string(), "seq2".to_string()],
            sample_unique_a_ids: vec!["seq_a1".to_string()],
            sample_unique_b_ids: vec!["seq_b1".to_string()],
            shared_percentage_a: 30.0,
            shared_percentage_b: 20.0,
        },
        taxonomy_analysis: TaxonomyAnalysis {
            total_taxa_a: 50,
            total_taxa_b: 75,
            shared_taxa: vec![],
            unique_to_a: vec![],
            unique_to_b: vec![],
            top_shared_taxa: vec![],
            shared_percentage_a: 60.0,
            shared_percentage_b: 40.0,
        },
        storage_metrics: StorageMetrics {
            size_a_bytes: 1048576,
            size_b_bytes: 2097152,
            dedup_savings_bytes: 524288,
            dedup_ratio_a: 1.5,
            dedup_ratio_b: 1.8,
        },
    };

    // Verify the structure works
    assert_eq!(comparison.chunk_analysis.total_chunks_a, 100);
    assert_eq!(comparison.sequence_analysis.total_sequences_a, 1000);
    assert_eq!(comparison.taxonomy_analysis.total_taxa_a, 50);
    assert_eq!(comparison.storage_metrics.size_a_bytes, 1048576);

    println!("✓ DatabaseComparison struct tests passed");
}

#[test]
fn test_json_serialization() {
    use talaria_sequoia::{DatabaseComparison, ChunkAnalysis, SequenceAnalysis, TaxonomyAnalysis, StorageMetrics};
    use serde_json;

    let comparison = DatabaseComparison {
        chunk_analysis: ChunkAnalysis {
            total_chunks_a: 10,
            total_chunks_b: 20,
            shared_chunks: vec![],
            unique_to_a: vec![],
            unique_to_b: vec![],
            shared_percentage_a: 50.0,
            shared_percentage_b: 25.0,
        },
        sequence_analysis: SequenceAnalysis {
            total_sequences_a: 100,
            total_sequences_b: 200,
            shared_sequences: 50,
            unique_to_a: 50,
            unique_to_b: 150,
            sample_shared_ids: vec![],
            sample_unique_a_ids: vec![],
            sample_unique_b_ids: vec![],
            shared_percentage_a: 50.0,
            shared_percentage_b: 25.0,
        },
        taxonomy_analysis: TaxonomyAnalysis {
            total_taxa_a: 5,
            total_taxa_b: 8,
            shared_taxa: vec![],
            unique_to_a: vec![],
            unique_to_b: vec![],
            top_shared_taxa: vec![],
            shared_percentage_a: 60.0,
            shared_percentage_b: 37.5,
        },
        storage_metrics: StorageMetrics {
            size_a_bytes: 1000,
            size_b_bytes: 2000,
            dedup_savings_bytes: 500,
            dedup_ratio_a: 1.0,
            dedup_ratio_b: 1.0,
        },
    };

    // Test serialization
    let json = serde_json::to_string(&comparison).unwrap();
    assert!(json.contains("\"total_chunks_a\":10"));
    assert!(json.contains("\"total_sequences_a\":100"));

    // Test deserialization
    let _deserialized: DatabaseComparison = serde_json::from_str(&json).unwrap();

    println!("✓ JSON serialization tests passed");
}