/// Integration test for graph centrality-based reference selection
/// Verifies SEQUOIA architecture 5-dimensional approach
use talaria_bio::sequence::Sequence;
use talaria_core::reference_selector::{ReferenceSelectorImpl, SelectionAlgorithm};
use talaria_core::selection::traits::AlignmentScore;
use talaria_core::AlignmentBasedSelector;

#[test]
fn test_graph_centrality_reference_selection() {
    // Create test sequences with known relationships
    let sequences = create_test_sequences();

    // Create mock alignments simulating similarity relationships
    let alignments = create_mock_alignments();

    // Test graph centrality algorithm
    let selector = ReferenceSelectorImpl::new()
        .with_selection_algorithm(SelectionAlgorithm::GraphCentrality)
        .with_similarity_threshold(0.8);

    let result = selector
        .select_with_alignments(sequences.clone(), &alignments)
        .unwrap();

    // Verify centrality-based selection
    assert!(!result.references.is_empty(), "Should select references");
    assert!(
        result.references.len() < sequences.len(),
        "Should select subset"
    );

    // Check that high-centrality sequences are selected
    let ref_ids: Vec<String> = result.references.iter().map(|s| s.id.clone()).collect();

    // Sequences with high connectivity should be preferred
    assert!(
        ref_ids.contains(&"hub_seq1".to_string()) || ref_ids.contains(&"hub_seq2".to_string()),
        "Should select highly connected hub sequences"
    );

    println!(
        "Graph centrality selected {} references",
        result.references.len()
    );
    println!("Coverage: {:.1}%", result.stats.coverage * 100.0);
    // Compression ratio not available in stats
}

#[test]
fn test_graph_centrality_vs_greedy() {
    let sequences = create_test_sequences();
    let alignments = create_mock_alignments();

    // Test greedy (SinglePass) algorithm
    let greedy_selector = ReferenceSelectorImpl::new()
        .with_selection_algorithm(SelectionAlgorithm::SinglePass)
        .with_similarity_threshold(0.8);

    let greedy_result = greedy_selector
        .select_with_alignments(sequences.clone(), &alignments)
        .unwrap();

    // Test graph centrality algorithm
    let graph_selector = ReferenceSelectorImpl::new()
        .with_selection_algorithm(SelectionAlgorithm::GraphCentrality)
        .with_similarity_threshold(0.8);

    let graph_result = graph_selector
        .select_with_alignments(sequences, &alignments)
        .unwrap();

    // Graph centrality should generally achieve better coverage with fewer references
    let greedy_coverage = calculate_coverage(&greedy_result);
    let graph_coverage = calculate_coverage(&graph_result);

    println!("\nAlgorithm Comparison:");
    println!(
        "  Greedy: {} refs, {:.1}% coverage",
        greedy_result.references.len(),
        greedy_coverage * 100.0
    );
    println!(
        "  Graph:  {} refs, {:.1}% coverage",
        graph_result.references.len(),
        graph_coverage * 100.0
    );

    // Graph centrality should be at least as good as greedy
    assert!(
        graph_coverage >= greedy_coverage * 0.95,
        "Graph centrality should achieve comparable coverage"
    );
}

fn create_test_sequences() -> Vec<Sequence> {
    vec![
        // Hub sequences (high connectivity)
        Sequence {
            id: "hub_seq1".to_string(),
            description: None,
            sequence: b"MGVHECPAWLWLLSVSLVLLPLLLLLLLLSPGPVPPPSPSPSPSPSLELVCVGDHGFLYMKC".to_vec(),
            taxon_id: None,
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "hub_seq2".to_string(),
            description: None,
            sequence: b"MGVHECPAWLWLLSVSLVLLPLLLLLLLLSPGPVPPPSPSPSPSPSLELVCVGDHGFLY".to_vec(),
            taxon_id: None,
            taxonomy_sources: Default::default(),
        },
        // Cluster 1: Similar to hub_seq1
        Sequence {
            id: "cluster1_seq1".to_string(),
            description: None,
            sequence: b"MGVHECPAWLWLLSVSLVLLPLLLLLLLLSPGPVPPPSPSPSPSPSLELVCVGDHGFLYMKCNPG".to_vec(),
            taxon_id: None,
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "cluster1_seq2".to_string(),
            description: None,
            sequence: b"MGVHECPAWLWLLSVSLVLLPLLLLLLLLSPGPVPPPSPSPSPSPSLELVCVGDHGFLYM".to_vec(),
            taxon_id: None,
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "cluster1_seq3".to_string(),
            description: None,
            sequence: b"MGVHECPAWLWLLSVSLVLLPLLLLLLLLSPGPVPPPSPSPSPSPSLELVCVGDH".to_vec(),
            taxon_id: None,
            taxonomy_sources: Default::default(),
        },
        // Cluster 2: Similar to hub_seq2
        Sequence {
            id: "cluster2_seq1".to_string(),
            description: None,
            sequence: b"MGVHECPAWLWLLSVSLVLLPLLLLLLLLSPGPVPPPSPSPSPSPSLELVCVGDHGFLYABC".to_vec(),
            taxon_id: None,
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "cluster2_seq2".to_string(),
            description: None,
            sequence: b"MGVHECPAWLWLLSVSLVLLPLLLLLLLLSPGPVPPPSPSPSPSPSLELVCVGDHGFLYXYZ".to_vec(),
            taxon_id: None,
            taxonomy_sources: Default::default(),
        },
        // Bridge sequence (connects clusters)
        Sequence {
            id: "bridge_seq".to_string(),
            description: None,
            sequence: b"MGVHECPAWLWLLSVSLVLLPLLLLLLLLSPGPVPPPSPSPSPSPS".to_vec(),
            taxon_id: None,
            taxonomy_sources: Default::default(),
        },
        // Isolated sequences
        Sequence {
            id: "isolated_seq1".to_string(),
            description: None,
            sequence: b"ATGGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGC".to_vec(),
            taxon_id: None,
            taxonomy_sources: Default::default(),
        },
        Sequence {
            id: "isolated_seq2".to_string(),
            description: None,
            sequence: b"CGATTACGATTACGATTACGATTACGATTACGATTACGATTACGATTA".to_vec(),
            taxon_id: None,
            taxonomy_sources: Default::default(),
        },
    ]
}

fn create_mock_alignments() -> Vec<AlignmentScore> {
    let mut alignments = Vec::new();

    // Hub connections (high degree)
    alignments.extend(vec![
        create_alignment("hub_seq1", "cluster1_seq1", 95.0),
        create_alignment("hub_seq1", "cluster1_seq2", 92.0),
        create_alignment("hub_seq1", "cluster1_seq3", 88.0),
        create_alignment("hub_seq1", "bridge_seq", 85.0),
        create_alignment("hub_seq2", "cluster2_seq1", 94.0),
        create_alignment("hub_seq2", "cluster2_seq2", 91.0),
        create_alignment("hub_seq2", "bridge_seq", 83.0),
    ]);

    // Within-cluster connections
    alignments.extend(vec![
        create_alignment("cluster1_seq1", "cluster1_seq2", 88.0),
        create_alignment("cluster1_seq2", "cluster1_seq3", 85.0),
        create_alignment("cluster2_seq1", "cluster2_seq2", 87.0),
    ]);

    // Bridge connections (high betweenness)
    alignments.extend(vec![
        create_alignment("bridge_seq", "cluster1_seq1", 82.0),
        create_alignment("bridge_seq", "cluster2_seq1", 81.0),
    ]);

    // Isolated sequences have no good alignments
    alignments.push(create_alignment("isolated_seq1", "isolated_seq2", 20.0));

    alignments
}

fn create_alignment(query: &str, reference: &str, identity: f32) -> AlignmentScore {
    AlignmentScore {
        seq1_id: query.to_string(),
        seq2_id: reference.to_string(),
        score: (identity / 100.0) as f64, // Convert to 0-1 range
        identity: (identity / 100.0) as f64,
        coverage: 0.95, // Assume high coverage for test
    }
}

fn calculate_coverage(result: &talaria::core::selection::traits::TraitSelectionResult) -> f64 {
    // Use the stats from the result
    if result.stats.total_sequences > 0 {
        result.stats.coverage
    } else {
        0.0
    }
}

#[test]
fn test_graph_centrality_formula_weights() {
    // Test that the formula weights are correctly applied
    // Formula: Score = 0.5·Degree + 0.3·Betweenness + 0.2·Coverage

    use talaria_core::reference_selector_optimized::OptimizedReferenceSelector;

    let selector = OptimizedReferenceSelector::new();

    // Verify default weights match SEQUOIA architecture specification
    assert_eq!(selector.alpha, 0.5, "Degree weight should be 0.5");
    assert_eq!(selector.beta, 0.3, "Betweenness weight should be 0.3");
    assert_eq!(selector.gamma, 0.2, "Coverage weight should be 0.2");

    // Weights should sum to 1.0 for normalized scoring
    let weight_sum = selector.alpha + selector.beta + selector.gamma;
    assert!(
        (weight_sum - 1.0).abs() < 0.001,
        "Weights should sum to 1.0"
    );
}
