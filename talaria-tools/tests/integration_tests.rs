#![allow(clippy::type_complexity)]

use talaria_bio::sequence::Sequence;
use talaria_tools::manager::ToolManager;
use talaria_tools::optimizers::{
    blast::BlastOptimizer, generic::GenericOptimizer, kraken::KrakenOptimizer,
    lambda::LambdaOptimizer,
};
use tempfile::TempDir;

/// Helper to create test sequences with various properties
fn create_diverse_sequences() -> Vec<Sequence> {
    vec![
        Sequence::new("ecoli_1".to_string(), b"ATCGATCGATCG".to_vec()).with_taxon(562),
        Sequence::new("ecoli_2".to_string(), b"GCTAGCTAGCTA".to_vec()).with_taxon(562),
        Sequence::new("human_1".to_string(), b"TTAATTAATTAA".to_vec()).with_taxon(9606),
        Sequence::new("human_2".to_string(), b"CCGGCCGGCCGG".to_vec()).with_taxon(9606),
        Sequence::new("unknown_1".to_string(), b"AAAATTTTGGGGCCCC".to_vec()),
        Sequence::new("unknown_2".to_string(), b"TTTTAAAACCCCGGGG".to_vec()),
    ]
}

// ===== Tool Manager Integration Tests =====

#[test]
fn test_tool_manager_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let manager = ToolManager::with_directory(temp_dir.path());

    // Test directory creation
    assert!(temp_dir.path().exists());

    // Test tool directory paths
    let lambda_dir = manager.tool_dir(talaria_tools::types::Tool::Lambda);
    assert!(lambda_dir.to_str().unwrap().contains("lambda"));

    // Test version comparison
    use std::cmp::Ordering;
    assert_eq!(manager.compare_versions("1.0.0", "2.0.0"), Ordering::Less);
    assert_eq!(
        manager.compare_versions("2.0.0", "1.0.0"),
        Ordering::Greater
    );
    assert_eq!(manager.compare_versions("1.0.0", "1.0.0"), Ordering::Equal);
}

#[test]
fn test_tool_manager_creation() {
    // Test default creation
    let manager_result = ToolManager::new();
    assert!(manager_result.is_ok(), "Failed to create ToolManager");

    // Test custom directory creation
    let temp_dir = TempDir::new().unwrap();
    let manager = ToolManager::with_directory(temp_dir.path());

    // Test getting tool directories
    let lambda_dir = manager.tool_dir(talaria_tools::types::Tool::Lambda);
    assert!(lambda_dir.is_absolute());
    assert!(lambda_dir.to_str().unwrap().contains("lambda"));
}

// ===== Optimizer Integration Tests =====

#[test]
fn test_optimizer_workflow_blast() {
    let optimizer = BlastOptimizer::new();
    let mut sequences = create_diverse_sequences();
    let original_count = sequences.len();

    optimizer.optimize_for_blast(&mut sequences);

    // Verify sequences are still all present
    assert_eq!(sequences.len(), original_count);

    // Verify they are sorted by length
    for i in 1..sequences.len() {
        assert!(sequences[i - 1].len() <= sequences[i].len());
    }
}

#[test]
fn test_optimizer_workflow_lambda() {
    let optimizer = LambdaOptimizer::new();
    let mut sequences = create_diverse_sequences();

    optimizer.optimize_for_lambda(&mut sequences);

    // Verify sequences are sorted by taxon
    let mut last_taxon = 0;
    for seq in &sequences {
        let taxon = seq.taxon_id.unwrap_or(0);
        assert!(taxon >= last_taxon, "Sequences not sorted by taxon");
        last_taxon = taxon;
    }

    // Test taxonomy mapping extraction
    let mapping = optimizer.prepare_taxonomy_mapping(&sequences);
    let sequences_with_taxon = sequences.iter().filter(|s| s.taxon_id.is_some()).count();
    assert_eq!(mapping.len(), sequences_with_taxon);
}

#[test]
fn test_optimizer_workflow_kraken() {
    let optimizer = KrakenOptimizer::new();
    let mut sequences = create_diverse_sequences();

    optimizer.optimize_for_kraken(&mut sequences);

    // Verify sequences are sorted by (taxon, length)
    let mut last_key = (0u32, 0usize);
    for seq in &sequences {
        let key = (seq.taxon_id.unwrap_or(0), seq.len());
        assert!(key >= last_key, "Sequences not sorted by (taxon, length)");
        last_key = key;
    }
}

#[test]
fn test_optimizer_workflow_generic() {
    let optimizer = GenericOptimizer::new();
    let mut sequences = create_diverse_sequences();

    optimizer.optimize(&mut sequences);

    // Verify sequences are sorted by length (descending)
    for i in 1..sequences.len() {
        assert!(sequences[i - 1].len() >= sequences[i].len());
    }
}

// ===== Cross-Component Integration Tests =====

#[test]
fn test_optimizer_chaining() {
    // Test that optimizers can be chained without breaking invariants
    let mut sequences = create_diverse_sequences();
    let original_ids: Vec<String> = sequences.iter().map(|s| s.id.clone()).collect();

    // Apply multiple optimizers
    let lambda_opt = LambdaOptimizer::new();
    lambda_opt.optimize_for_lambda(&mut sequences);

    let generic_opt = GenericOptimizer::new();
    generic_opt.optimize(&mut sequences);

    // Verify no sequences were lost
    assert_eq!(sequences.len(), original_ids.len());

    // Verify all original IDs are still present
    let current_ids: Vec<String> = sequences.iter().map(|s| s.id.clone()).collect();
    for id in &original_ids {
        assert!(current_ids.contains(id), "Lost sequence: {}", id);
    }
}

#[test]
fn test_empty_sequence_handling() {
    // Test that all components handle empty sequences gracefully
    let mut empty_sequences = Vec::new();

    // Optimizers should handle empty vectors
    let blast_opt = BlastOptimizer::new();
    blast_opt.optimize_for_blast(&mut empty_sequences);
    assert!(empty_sequences.is_empty());

    let lambda_opt = LambdaOptimizer::new();
    lambda_opt.optimize_for_lambda(&mut empty_sequences);
    assert!(empty_sequences.is_empty());

    let kraken_opt = KrakenOptimizer::new();
    kraken_opt.optimize_for_kraken(&mut empty_sequences);
    assert!(empty_sequences.is_empty());

    let generic_opt = GenericOptimizer::new();
    generic_opt.optimize(&mut empty_sequences);
    assert!(empty_sequences.is_empty());

    // Taxonomy mapping should be empty
    let mapping = lambda_opt.prepare_taxonomy_mapping(&empty_sequences);
    assert!(mapping.is_empty());
}

#[test]
fn test_large_dataset_optimization() {
    // Test with a larger dataset to verify performance characteristics
    let mut sequences = Vec::new();

    // Create 1000 sequences with varying properties
    for i in 0..1000 {
        let len = (i % 100) + 1;
        let seq = vec![b'A'; len];
        let taxon = if i % 3 == 0 {
            Some((i % 10) as u32)
        } else {
            None
        };

        let mut sequence = Sequence::new(format!("seq_{}", i), seq);
        if let Some(t) = taxon {
            sequence = sequence.with_taxon(t);
        }
        sequences.push(sequence);
    }

    // Apply all optimizers and verify they complete without panic
    let optimizers: Vec<Box<dyn Fn(&mut Vec<Sequence>)>> = vec![
        Box::new(|s| BlastOptimizer::new().optimize_for_blast(s)),
        Box::new(|s| LambdaOptimizer::new().optimize_for_lambda(s)),
        Box::new(|s| KrakenOptimizer::new().optimize_for_kraken(s)),
        Box::new(|s| GenericOptimizer::new().optimize(s)),
    ];

    for (i, optimizer) in optimizers.iter().enumerate() {
        let mut seq_copy = sequences.clone();
        optimizer(&mut seq_copy);
        assert_eq!(
            seq_copy.len(),
            1000,
            "Optimizer {} changed sequence count",
            i
        );
    }
}

// ===== Workflow Simulation Tests =====

#[test]
fn test_reduction_preparation_workflow() {
    // Simulate preparing sequences for reduction
    let mut sequences = create_diverse_sequences();

    // Step 1: Optimize for the aligner (LAMBDA)
    let lambda_opt = LambdaOptimizer::new();
    lambda_opt.optimize_for_lambda(&mut sequences);

    // Step 2: Extract taxonomy mapping
    let mapping = lambda_opt.prepare_taxonomy_mapping(&sequences);

    // Step 3: Group by taxon
    let mut taxon_groups = std::collections::HashMap::new();
    for seq in &sequences {
        let taxon = seq.taxon_id.unwrap_or(0);
        taxon_groups
            .entry(taxon)
            .or_insert_with(Vec::new)
            .push(seq.id.clone());
    }

    // Verify workflow results
    assert!(!mapping.is_empty(), "Should have taxonomy mappings");
    assert!(!taxon_groups.is_empty(), "Should have taxon groups");

    // Verify all sequences with taxons are in the mapping
    for seq in &sequences {
        if let Some(taxon) = seq.taxon_id {
            assert!(
                mapping.iter().any(|(id, t)| id == &seq.id && *t == taxon),
                "Missing mapping for sequence {}",
                seq.id
            );
        }
    }
}

#[test]
fn test_aligner_preparation_workflow() {
    // Test preparing sequences for different aligners
    let sequences = create_diverse_sequences();

    // For BLAST
    let mut blast_sequences = sequences.clone();
    BlastOptimizer::new().optimize_for_blast(&mut blast_sequences);
    // BLAST sequences should be sorted by length ascending
    for i in 1..blast_sequences.len() {
        assert!(blast_sequences[i - 1].len() <= blast_sequences[i].len());
    }

    // For LAMBDA
    let mut lambda_sequences = sequences.clone();
    LambdaOptimizer::new().optimize_for_lambda(&mut lambda_sequences);
    // LAMBDA sequences should be sorted by taxon
    let mut last_taxon = 0;
    for seq in &lambda_sequences {
        let taxon = seq.taxon_id.unwrap_or(0);
        assert!(taxon >= last_taxon);
        last_taxon = taxon;
    }

    // For Kraken
    let mut kraken_sequences = sequences.clone();
    KrakenOptimizer::new().optimize_for_kraken(&mut kraken_sequences);
    // Kraken sequences should be sorted by (taxon, length)
    let mut last_key = (0u32, 0usize);
    for seq in &kraken_sequences {
        let key = (seq.taxon_id.unwrap_or(0), seq.len());
        assert!(key >= last_key);
        last_key = key;
    }
}

// ===== Error Handling Tests =====

#[test]
fn test_tool_manager_custom_directory() {
    let temp_dir = TempDir::new().unwrap();
    let custom_path = temp_dir.path().join("custom_tools");

    // Test with custom directory
    let manager = ToolManager::with_directory(&custom_path);

    // Should use the custom path
    let tool_dir = manager.tool_dir(talaria_tools::types::Tool::Lambda);
    assert!(tool_dir.starts_with(&custom_path));
    assert!(tool_dir.to_str().unwrap().contains("lambda"));
}

#[test]
fn test_sequences_with_special_characters() {
    // Test that all components handle sequences with special characters
    let mut sequences = vec![
        Sequence::new("seq|with|pipes".to_string(), b"ATCG".to_vec()),
        Sequence::new("seq>with>brackets".to_string(), b"GCTA".to_vec()),
        Sequence::new("seq with spaces".to_string(), b"TTAA".to_vec()),
        Sequence::new("seq\twith\ttabs".to_string(), b"CCGG".to_vec()),
    ];

    // All optimizers should handle these without panic
    BlastOptimizer::new().optimize_for_blast(&mut sequences);
    assert_eq!(sequences.len(), 4);

    LambdaOptimizer::new().optimize_for_lambda(&mut sequences);
    assert_eq!(sequences.len(), 4);

    KrakenOptimizer::new().optimize_for_kraken(&mut sequences);
    assert_eq!(sequences.len(), 4);

    GenericOptimizer::new().optimize(&mut sequences);
    assert_eq!(sequences.len(), 4);
}
