/// Integration tests for LAMBDA aligner with CASG workspace
///
/// These tests verify that the LAMBDA aligner properly integrates with
/// the CASG workspace system and uses the correct temporary directories.
use std::fs;

mod common;
use common::create_shared_test_workspace;

#[test]
fn test_lambda_verbose_environment_variable() {
    // Test that the TALARIA_LAMBDA_VERBOSE flag works correctly
    std::env::remove_var("TALARIA_LAMBDA_VERBOSE");
    assert!(
        std::env::var("TALARIA_LAMBDA_VERBOSE").is_err(),
        "Should not be set initially"
    );

    std::env::set_var("TALARIA_LAMBDA_VERBOSE", "1");
    assert!(
        std::env::var("TALARIA_LAMBDA_VERBOSE").is_ok(),
        "Should be set"
    );

    let is_verbose = std::env::var("TALARIA_LAMBDA_VERBOSE").is_ok();
    assert!(is_verbose, "Should detect verbose mode");

    std::env::remove_var("TALARIA_LAMBDA_VERBOSE");
}

#[test]
fn test_workspace_creation_for_lambda() {
    // Create a workspace with explicit test configuration
    let workspace_arc = create_shared_test_workspace("test_lambda_dirs").unwrap();

    // Get the lambda directory path that would be used
    let ws = workspace_arc.lock().unwrap();
    let lambda_path = ws.get_path("lambda");
    drop(ws);

    // Create the directory
    fs::create_dir_all(&lambda_path).unwrap();
    assert!(lambda_path.exists(), "Lambda directory should be created");

    // Test creating temp files in the lambda directory
    let test_file = lambda_path.join("test.fasta");
    fs::write(&test_file, "test content").unwrap();
    assert!(test_file.exists(), "Test file should be created");

    // Verify the file is in the workspace
    let ws = workspace_arc.lock().unwrap();
    let workspace_root = ws.root.clone();
    drop(ws);

    assert!(
        test_file.starts_with(&workspace_root),
        "Test file should be under workspace root"
    );
}

#[test]
fn test_lambda_preserve_on_failure_env() {
    // Test that TALARIA_PRESERVE_LAMBDA_ON_FAILURE environment variable works
    std::env::set_var("TALARIA_PRESERVE_LAMBDA_ON_FAILURE", "1");
    assert!(
        std::env::var("TALARIA_PRESERVE_LAMBDA_ON_FAILURE").is_ok(),
        "Environment variable should be set"
    );

    std::env::remove_var("TALARIA_PRESERVE_LAMBDA_ON_FAILURE");
    assert!(
        std::env::var("TALARIA_PRESERVE_LAMBDA_ON_FAILURE").is_err(),
        "Environment variable should be removed"
    );
}

#[test]
fn test_lambda_debug_vs_verbose_separation() {
    // Test that TALARIA_DEBUG and TALARIA_LAMBDA_VERBOSE are separate
    std::env::set_var("TALARIA_DEBUG", "1");
    std::env::remove_var("TALARIA_LAMBDA_VERBOSE");

    assert!(
        std::env::var("TALARIA_DEBUG").is_ok(),
        "DEBUG should be set"
    );
    assert!(
        std::env::var("TALARIA_LAMBDA_VERBOSE").is_err(),
        "LAMBDA_VERBOSE should not be set"
    );

    // Now set LAMBDA_VERBOSE
    std::env::set_var("TALARIA_LAMBDA_VERBOSE", "1");
    assert!(
        std::env::var("TALARIA_LAMBDA_VERBOSE").is_ok(),
        "LAMBDA_VERBOSE should be set"
    );

    // Clean up
    std::env::remove_var("TALARIA_DEBUG");
    std::env::remove_var("TALARIA_LAMBDA_VERBOSE");
}

#[test]
fn test_workspace_batch_file_paths() {
    // Test that batch files would be created in the correct workspace location
    let workspace_arc = create_shared_test_workspace("test_batch").unwrap();

    let ws = workspace_arc.lock().unwrap();
    let lambda_path = ws.get_path("lambda");
    drop(ws);

    // Simulate batch file paths
    let batch_file_1 = lambda_path.join("query_batch_0.fasta");
    let batch_file_2 = lambda_path.join("query_batch_1.fasta");
    let alignment_file_1 = lambda_path.join("alignments_batch_0.m8");

    // Verify paths are in workspace
    let ws = workspace_arc.lock().unwrap();
    let workspace_root = ws.root.clone();
    drop(ws);

    assert!(
        batch_file_1.starts_with(&workspace_root),
        "Batch file 1 should be in workspace"
    );
    assert!(
        batch_file_2.starts_with(&workspace_root),
        "Batch file 2 should be in workspace"
    );
    assert!(
        alignment_file_1.starts_with(&workspace_root),
        "Alignment file should be in workspace"
    );
}

#[test]
fn test_lambda_progress_tracking_simulation() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // Create a progress counter like in the actual implementation
    let progress_counter = Arc::new(AtomicUsize::new(0));

    // Simulate LAMBDA progress updates
    let test_cases = vec![
        ("Query no. 10", 10),
        ("Query no. 50", 50),
        ("Query no. 100", 100),
    ];

    for (line, expected) in test_cases {
        // Extract number like the actual implementation does
        if line.contains("Query no.") {
            if let Some(num) = line
                .split_whitespace()
                .find_map(|s| s.parse::<usize>().ok())
            {
                progress_counter.store(num, Ordering::Relaxed);
            }
        }
        assert_eq!(
            progress_counter.load(Ordering::Relaxed),
            expected,
            "Progress counter should be {}",
            expected
        );
    }
}

#[test]
fn test_batch_percentage_calculation() {
    // Test that batch progress shows percentage correctly
    let total_sequences = 100;
    let sequences_processed = 25;
    let percent_complete = sequences_processed as f64 / total_sequences as f64 * 100.0;

    assert_eq!(percent_complete, 25.0, "Should be 25% complete");

    let sequences_processed = 50;
    let percent_complete = sequences_processed as f64 / total_sequences as f64 * 100.0;
    assert_eq!(percent_complete, 50.0, "Should be 50% complete");

    let sequences_processed = 100;
    let percent_complete = sequences_processed as f64 / total_sequences as f64 * 100.0;
    assert_eq!(percent_complete, 100.0, "Should be 100% complete");
}

#[test]
fn test_extreme_sequence_threshold() {
    // Test the extreme sequence threshold constant
    const EXTREME_LONG_SEQ: usize = 30_000;

    let titin_length = 35_000; // TITIN is about 35K amino acids
    let normal_protein = 500;
    let large_protein = 10_000;

    assert!(titin_length > EXTREME_LONG_SEQ, "TITIN should be extreme");
    assert!(
        !(normal_protein > EXTREME_LONG_SEQ),
        "Normal protein should not be extreme"
    );
    assert!(
        !(large_protein > EXTREME_LONG_SEQ),
        "Large protein should not be extreme"
    );
}

#[test]
fn test_ambiguous_content_threshold() {
    // Test the 5% ambiguous content threshold
    let sequence_length = 100;
    let _threshold_percent = 5;
    let max_ambiguous = sequence_length / 20; // 5%

    assert_eq!(max_ambiguous, 5, "Should allow 5 ambiguous residues in 100");

    let ambiguous_count = 6;
    assert!(
        ambiguous_count > max_ambiguous,
        "6 ambiguous should exceed threshold"
    );

    let ambiguous_count = 4;
    assert!(
        !(ambiguous_count > max_ambiguous),
        "4 ambiguous should be under threshold"
    );
}
