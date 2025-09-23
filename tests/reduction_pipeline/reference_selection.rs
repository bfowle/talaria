/// Integration tests for selection algorithms
use talaria_bio::sequence::Sequence;
use talaria_cli::TargetAligner;
use talaria_core::config::Config;
use talaria_core::reducer::Reducer;
use talaria_core::reference_selector::{ReferenceSelectorImpl, SelectionAlgorithm};

#[test]
fn test_both_algorithms_end_to_end() {
    // Create test sequences with varying lengths
    let sequences = vec![
        Sequence::new("seq1".to_string(), vec![65; 150]), // Longest
        Sequence::new("seq2".to_string(), vec![65; 120]),
        Sequence::new("seq3".to_string(), vec![65; 100]),
        Sequence::new("seq4".to_string(), vec![65; 80]),
        Sequence::new("seq5".to_string(), vec![65; 60]),
        Sequence::new("seq6".to_string(), vec![65; 50]), // Minimum length
        Sequence::new("seq7".to_string(), vec![65; 40]), // Too short
    ];

    let target_ratio = 0.3; // Select 30% as references

    // Test with SinglePass algorithm
    let selector_sp =
        ReferenceSelectorImpl::new().with_selection_algorithm(SelectionAlgorithm::SinglePass);
    let result_sp = selector_sp.simple_select_references(sequences.clone(), target_ratio);

    // Test with SimilarityMatrix algorithm
    let selector_sm =
        ReferenceSelectorImpl::new().with_selection_algorithm(SelectionAlgorithm::SimilarityMatrix);
    let result_sm = selector_sm.simple_select_references(sequences.clone(), target_ratio);

    // Verify both produce valid results
    assert!(
        !result_sp.references.is_empty(),
        "SinglePass should select references"
    );
    assert!(
        !result_sm.references.is_empty(),
        "SimilarityMatrix should select references"
    );

    // Both should select approximately the same number of references
    let expected_refs = ((sequences.len() - 1) as f64 * target_ratio) as usize; // -1 for too short
    assert!(result_sp.references.len() > 0 && result_sp.references.len() <= expected_refs + 1);
    assert!(result_sm.references.len() > 0 && result_sm.references.len() <= expected_refs + 1);

    // The longest sequences should be selected as references
    assert!(
        result_sp.references.iter().any(|r| r.id == "seq1"),
        "SinglePass should select longest sequence"
    );
    assert!(
        result_sm.references.iter().any(|r| r.id == "seq1"),
        "SimilarityMatrix should select longest sequence"
    );

    // Too short sequences should be discarded
    assert!(
        result_sp.discarded.contains("seq7"),
        "SinglePass should discard too-short sequences"
    );
    assert!(
        result_sm.discarded.contains("seq7"),
        "SimilarityMatrix should discard too-short sequences"
    );
}

#[test]
fn test_algorithm_with_reducer_pipeline() {
    let config = Config::default();
    let sequences = vec![
        Sequence::new("ref1".to_string(), vec![65; 100]),
        Sequence::new("ref2".to_string(), vec![65; 90]),
        Sequence::new("child1".to_string(), vec![65; 80]),
        Sequence::new("child2".to_string(), vec![65; 70]),
    ];

    // Test SinglePass through reducer
    let mut reducer_sp = Reducer::new(config.clone())
        .with_selection_algorithm(SelectionAlgorithm::SinglePass)
        .with_silent(true);

    let result_sp = reducer_sp.reduce(sequences.clone(), 0.5, TargetAligner::Generic);

    // Should complete without error
    assert!(result_sp.is_ok(), "SinglePass reduction should succeed");
    let (refs_sp, _deltas_sp, _) = result_sp.unwrap();
    assert_eq!(refs_sp.len(), 2, "Should select 50% as references");

    // Test SimilarityMatrix through reducer
    let mut reducer_sm = Reducer::new(config)
        .with_selection_algorithm(SelectionAlgorithm::SimilarityMatrix)
        .with_silent(true);

    let result_sm = reducer_sm.reduce(sequences.clone(), 0.5, TargetAligner::Generic);

    // Should complete without error
    assert!(
        result_sm.is_ok(),
        "SimilarityMatrix reduction should succeed"
    );
    let (refs_sm, _deltas_sm, _) = result_sm.unwrap();
    assert_eq!(refs_sm.len(), 2, "Should select 50% as references");
}

#[test]
fn test_algorithm_consistency() {
    // Test that algorithms maintain consistency across runs
    let sequences: Vec<_> = (0..20)
        .map(|i| Sequence::new(format!("seq{}", i), vec![65; 100 - i * 2]))
        .collect();

    let selector =
        ReferenceSelectorImpl::new().with_selection_algorithm(SelectionAlgorithm::SinglePass);

    // Run the same selection multiple times
    let result1 = selector.simple_select_references(sequences.clone(), 0.3);
    let result2 = selector.simple_select_references(sequences.clone(), 0.3);

    // Results should be deterministic
    assert_eq!(
        result1.references.len(),
        result2.references.len(),
        "Same algorithm should produce same number of references"
    );

    // Check that the same sequences are selected (order might differ)
    let refs1: std::collections::HashSet<_> = result1.references.iter().map(|r| &r.id).collect();
    let refs2: std::collections::HashSet<_> = result2.references.iter().map(|r| &r.id).collect();
    assert_eq!(refs1, refs2, "Same algorithm should select same references");
}

#[test]
fn test_algorithm_invariants() {
    let sequences = vec![
        Sequence::new("seq1".to_string(), vec![65; 100]),
        Sequence::new("seq2".to_string(), vec![65; 90]),
        Sequence::new("seq3".to_string(), vec![65; 80]),
        Sequence::new("seq4".to_string(), vec![65; 70]),
        Sequence::new("seq5".to_string(), vec![65; 60]),
    ];

    for algorithm in [
        SelectionAlgorithm::SinglePass,
        SelectionAlgorithm::SimilarityMatrix,
    ] {
        let selector = ReferenceSelectorImpl::new().with_selection_algorithm(algorithm);
        let result = selector.simple_select_references(sequences.clone(), 0.4);

        // Invariant 1: No duplicate sequences
        let all_ids: Vec<_> = result
            .references
            .iter()
            .map(|r| &r.id)
            .chain(result.children.values().flatten())
            .collect();
        let unique_ids: std::collections::HashSet<_> = all_ids.iter().collect();
        assert_eq!(
            all_ids.len(),
            unique_ids.len(),
            "No sequence should appear multiple times"
        );

        // Invariant 2: References are not in children
        let ref_ids: std::collections::HashSet<_> =
            result.references.iter().map(|r| &r.id).collect();
        for children in result.children.values() {
            for child in children {
                assert!(
                    !ref_ids.contains(&child),
                    "Reference should not appear as child"
                );
            }
        }

        // Invariant 3: All sequences accounted for
        let total =
            result.references.len() + result.children.values().map(|v| v.len()).sum::<usize>();
        assert!(
            total <= sequences.len(),
            "Cannot have more sequences than input"
        );
    }
}
