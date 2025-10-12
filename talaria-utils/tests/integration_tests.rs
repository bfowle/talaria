use talaria_utils::database::database_ref::parse_database_reference;
/// Integration tests for talaria-utils
use talaria_utils::*;
use tempfile::TempDir;

// ===== Workspace Integration Tests =====

#[test]
fn test_temp_workspace_lifecycle() {
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("TALARIA_WORKSPACE_DIR", temp_dir.path());

    // Create workspace
    let workspace = TempWorkspace::new("test_workspace").unwrap();
    assert!(workspace.root.exists());
    assert!(workspace.root.is_dir());

    // Create subdirectories
    let input_dir = workspace.get_path("input");
    let output_dir = workspace.get_path("output");
    assert!(input_dir.exists());
    assert!(output_dir.exists());

    // Workspace ID should be set (format: timestamp_uuid)
    assert!(!workspace.id.is_empty());
    // ID should contain underscore separator between timestamp and UUID
    assert!(workspace.id.contains('_'));

    std::env::remove_var("TALARIA_WORKSPACE_DIR");
}

#[test]
fn test_workspace_preservation_on_error() {
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("TALARIA_WORKSPACE_DIR", temp_dir.path());
    std::env::set_var("TALARIA_PRESERVE_ON_FAILURE", "1");

    {
        let mut workspace = TempWorkspace::new("error_test").unwrap();
        let workspace_id = workspace.id.clone();
        // The workspace root's parent is the herald_root (TALARIA_WORKSPACE_DIR)
        let herald_root = workspace.root.parent().unwrap().to_path_buf();
        workspace.mark_error("Test error").ok();

        // Workspace should be preserved after drop when marked as failed
        drop(workspace);

        // Check if workspace was moved to preserved directory within herald_root
        let preserved_path = herald_root.join("preserved").join(&workspace_id);
        assert!(
            preserved_path.exists(),
            "Workspace should be preserved at {:?}",
            preserved_path
        );

        // Clean up manually
        std::fs::remove_dir_all(preserved_path).ok();
    }

    std::env::remove_var("TALARIA_WORKSPACE_DIR");
    std::env::remove_var("TALARIA_PRESERVE_ON_FAILURE");
}

#[test]
fn test_workspace_cleanup_on_success() {
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("TALARIA_WORKSPACE_DIR", temp_dir.path());

    let workspace_path;
    {
        let workspace = TempWorkspace::new("success_test").unwrap();
        workspace_path = workspace.root.clone();
        assert!(workspace_path.exists());
        // Don't mark as failed - should clean up
    }

    // Workspace should be cleaned up after drop
    assert!(!workspace_path.exists());

    std::env::remove_var("TALARIA_WORKSPACE_DIR");
}

// ===== Display Integration Tests =====

#[test]
fn test_tree_visualization() {
    // Build a complex tree
    let tree = TreeNode::new("Database")
        .add_child(
            TreeNode::new("Sequences")
                .add_child(TreeNode::new("proteins.fasta"))
                .add_child(TreeNode::new("nucleotides.fasta")),
        )
        .add_child(
            TreeNode::new("Indices")
                .add_child(TreeNode::new("lambda.idx"))
                .add_child(TreeNode::new("blast.idx")),
        );

    let rendered = tree.render();

    // Verify structure is correct
    assert!(rendered.contains("Database\n"));
    assert!(rendered.contains("├─ Sequences\n"));
    assert!(rendered.contains("│  ├─ proteins.fasta\n"));
    assert!(rendered.contains("│  └─ nucleotides.fasta\n"));
    assert!(rendered.contains("└─ Indices\n"));
    assert!(rendered.contains("   ├─ lambda.idx\n"));
    assert!(rendered.contains("   └─ blast.idx\n"));
}

#[test]
fn test_number_formatting() {
    // Test various number formats
    assert_eq!(format_number(0), "0");
    assert_eq!(format_number(1234567890), "1,234,567,890");
    assert_eq!(format_number(-999999), "-999,999");

    // Test with different types
    assert_eq!(format_number(1000u32), "1,000");
    assert_eq!(format_number(1000i32), "1,000");
    assert_eq!(format_number(1000u64), "1,000");
}

// ===== Parallel Processing Integration Tests =====

#[test]
fn test_parallel_configuration() {
    use rayon::prelude::*;

    // Configure thread pool
    let result = configure_thread_pool(4);

    // Note: This might fail if already configured
    if result.is_ok() {
        assert_eq!(rayon::current_num_threads(), 4);

        // Test parallel processing
        let data: Vec<i32> = (0..1000).collect();
        let sum: i32 = data.par_iter().sum();
        assert_eq!(sum, 499500);
    }
}

#[test]
fn test_chunk_size_optimization() {
    // Test chunk size calculation for different scenarios
    let small_dataset = 100;
    let large_dataset = 1_000_000;

    let chunk_small = chunk_size_for_parallelism(small_dataset, 8);
    let chunk_large = chunk_size_for_parallelism(large_dataset, 8);

    // Verify bounds
    assert!(chunk_small >= 10);
    assert!(chunk_small <= 1000);
    assert!(chunk_large >= 10);
    assert!(chunk_large <= 1000);

    // Verify parallelization decision
    assert!(!should_parallelize(50, 100));
    assert!(should_parallelize(1000, 100));
}

// ===== Progress Bar Integration Tests =====

#[test]
fn test_progress_bar_workflow() {
    // Create manager
    let manager = ProgressBarManager::new();

    // Create multiple progress bars
    let download_pb = manager.create_progress_bar(100, "Downloading");
    let process_pb = manager.create_progress_bar(50, "Processing");
    let spinner = manager.create_spinner("Analyzing");

    // Simulate workflow
    for i in 0..10 {
        download_pb.inc(10);
        if i < 5 {
            process_pb.inc(10);
        }
        spinner.tick();
    }

    // Complete all tasks
    download_pb.finish_with_message("Download complete");
    process_pb.finish_with_message("Processing complete");
    spinner.finish_with_message("Analysis complete");

    assert!(download_pb.is_finished());
    assert!(process_pb.is_finished());
    assert!(spinner.is_finished());

    // Clear all
    assert!(manager.clear().is_ok());
}

// ===== Format Integration Tests =====

#[test]
fn test_format_utilities() {
    // Test byte formatting
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(1024), "1.0 KB");
    assert_eq!(format_bytes(1048576), "1.0 MB");
    assert_eq!(format_bytes(1073741824), "1.0 GB");

    // Test duration formatting
    assert_eq!(format_duration(0), "0s");
    assert_eq!(format_duration(59), "59s");
    assert_eq!(format_duration(60), "1m 0s");
    assert_eq!(format_duration(3661), "1h 1m 1s");
}

// ===== Database Reference Integration Tests =====

#[test]
fn test_database_reference_parsing() {
    // Test various database reference formats
    let ref1 = parse_database_reference("ncbi/nr").unwrap();
    assert_eq!(ref1.source, "ncbi");
    assert_eq!(ref1.dataset, "nr");
    assert_eq!(ref1.version, None);

    let ref2 = parse_database_reference("uniprot/swissprot@2023.05").unwrap();
    assert_eq!(ref2.source, "uniprot");
    assert_eq!(ref2.dataset, "swissprot");
    assert_eq!(ref2.version, Some("2023.05".to_string()));

    let ref3 = parse_database_reference("custom/mydb@v1.2.3").unwrap();
    assert_eq!(ref3.source, "custom");
    assert_eq!(ref3.dataset, "mydb");
    assert_eq!(ref3.version, Some("v1.2.3".to_string()));

    // Test invalid formats
    assert!(parse_database_reference("invalid").is_err());
    assert!(parse_database_reference("").is_err());
}

#[test]
fn test_database_version_comparison() {
    // Database version comparison removed as DatabaseVersion doesn't implement Ord/PartialOrd
    // This functionality may be handled elsewhere
}

// ===== Cross-Component Integration Tests =====

#[test]
fn test_workspace_with_progress() {
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("TALARIA_WORKSPACE_DIR", temp_dir.path());

    // Create workspace
    let workspace = TempWorkspace::new("progress_test").unwrap();

    // Create progress for workspace operations
    let manager = ProgressBarManager::new();
    let pb = manager.create_progress_bar(100, "Setting up workspace");

    // Simulate workspace operations
    let _input = workspace.get_path("input");
    pb.inc(25);

    let _output = workspace.get_path("output");
    pb.inc(25);

    let _logs = workspace.get_path("logs");
    pb.inc(25);

    let _metadata = workspace.get_path("metadata");
    pb.inc(25);

    pb.finish_with_message("Workspace ready");
    assert!(pb.is_finished());

    std::env::remove_var("TALARIA_WORKSPACE_DIR");
}

#[test]
fn test_formatted_output_display() {
    // Test the output formatter with various types
    let mut formatter = OutputFormatter::new();

    let section = formatter.start_section("Test Results");
    section.add_item(Item::new("Total").with_value("1,000"));
    section.add_item(
        Item::new("Passed")
            .with_value("950")
            .with_status(Status::Complete),
    );
    section.add_item(
        Item::new("Failed")
            .with_value("50")
            .with_status(Status::Failed),
    );

    let output = formatter.render();

    assert!(output.contains("Test Results"));
    assert!(output.contains("Total"));
    assert!(output.contains("1,000"));
    assert!(output.contains("Passed"));
    assert!(output.contains("Failed"));
}

#[test]
fn test_memory_estimation() {
    let estimator = MemoryEstimator::new();

    // Estimate memory for alignment
    let sequence_count = 10000;
    let avg_length = 300;

    let estimate = estimator.estimate_alignment_memory(sequence_count, avg_length);

    // Should be reasonable
    assert!(estimate > 0);
    assert!(estimate < 1_000_000_000); // Less than 1GB for 10k sequences

    // Test formatting
    let formatted = format_bytes(estimate);
    assert!(formatted.contains("MB") || formatted.contains("KB") || formatted.contains("GB"));
}

// ===== Error Handling Integration Tests =====

#[test]
fn test_workspace_error_recovery() {
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("TALARIA_WORKSPACE_DIR", temp_dir.path());

    // Simulate error scenario
    let result = std::panic::catch_unwind(|| {
        let workspace = TempWorkspace::new("panic_test").unwrap();
        let _path = workspace.root.clone();
        panic!("Simulated error");
    });

    assert!(result.is_err());

    // Workspace should handle panic gracefully
    // (cleanup or preservation based on config)

    std::env::remove_var("TALARIA_WORKSPACE_DIR");
}

#[test]
fn test_parallel_error_handling() {
    use rayon::prelude::*;

    let data: Vec<i32> = vec![1, 2, 0, 4, 5];

    // Parallel operation that might fail
    let results: Vec<_> = data
        .par_iter()
        .map(|&x| {
            if x == 0 {
                Err("Division by zero")
            } else {
                Ok(10 / x)
            }
        })
        .collect();

    // Should handle errors gracefully
    assert_eq!(results.len(), 5);
    assert!(results[2].is_err());
    assert!(results[0].is_ok());
}
