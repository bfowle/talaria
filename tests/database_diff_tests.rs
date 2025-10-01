/// Integration tests for database diff functionality
use std::process::Command;
use tempfile::TempDir;
use std::fs;
use std::path::PathBuf;
use talaria_sequoia::{SequoiaRepository, ChunkManifest, TaxonId, SHA256Hash, SHA256HashExt, ChunkClassification};

/// Helper to run talaria CLI commands
fn run_talaria_command(args: &[&str]) -> Result<String, String> {
    let output = Command::new("cargo")
        .args(&["run", "--release", "--"])
        .args(args)
        .output()
        .map_err(|e| format!("Failed to execute command: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Create a test database with sample chunks
fn create_test_database(path: &str, db_name: &str, num_chunks: usize) -> anyhow::Result<()> {
    let db_path = PathBuf::from(path).join("data").join(db_name);
    fs::create_dir_all(&db_path)?;

    let repo = SequoiaRepository::init(&db_path)?;

    // Create test chunks
    for i in 0..num_chunks {
        let chunk_data = format!("test_chunk_{}", i);
        let sequences: Vec<SHA256Hash> = (0..3)
            .map(|j| SHA256Hash::compute(format!("seq_{}_{}", i, j).as_bytes()))
            .collect();

        let manifest = ChunkManifest {
            chunk_hash: SHA256Hash::compute(chunk_data.as_bytes()),
            sequence_refs: sequences,
            taxon_ids: vec![TaxonId(9606 + i as u32), TaxonId(10090 + i as u32)],
            chunk_type: ChunkClassification::Taxonomic,
            total_size: 1000 * (i + 1),
            sequence_count: 3,
        };

        let data = bincode::serialize(&manifest)?;
        repo.storage.store_chunk(&data, false)?;
    }

    repo.save()?;
    Ok(())
}

#[test]
#[ignore] // Run with: cargo test --test database_diff_tests -- --ignored
fn test_diff_command_basic() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_str().unwrap();

    // Create two test databases
    create_test_database(base_path, "test_db1", 5).unwrap();
    create_test_database(base_path, "test_db2", 7).unwrap();

    // Set TALARIA_DATABASES_DIR to our temp directory
    std::env::set_var("TALARIA_DATABASES_DIR", base_path);

    // Run diff command
    let result = run_talaria_command(&[
        "database",
        "diff",
        "test_db1",
        "test_db2",
        "--all"
    ]);

    match result {
        Ok(output) => {
            // Check that output contains expected sections
            assert!(output.contains("CHUNK-LEVEL ANALYSIS"), "Missing chunk analysis");
            assert!(output.contains("SEQUENCE-LEVEL ANALYSIS"), "Missing sequence analysis");
            assert!(output.contains("TAXONOMY DISTRIBUTION"), "Missing taxonomy analysis");
            assert!(output.contains("STORAGE METRICS"), "Missing storage metrics");

            // Check for specific data
            assert!(output.contains("Total chunks in first:"), "Missing chunk counts");
            assert!(output.contains("Total sequences in first:"), "Missing sequence counts");
            assert!(output.contains("Taxa in first:"), "Missing taxa counts");
        }
        Err(e) => panic!("Command failed: {}", e),
    }

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

#[test]
#[ignore]
fn test_diff_command_summary_mode() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_str().unwrap();

    create_test_database(base_path, "test_db1", 3).unwrap();
    create_test_database(base_path, "test_db2", 3).unwrap();

    std::env::set_var("TALARIA_DATABASES_DIR", base_path);

    let result = run_talaria_command(&[
        "database",
        "diff",
        "test_db1",
        "test_db2",
        "--summary"
    ]);

    match result {
        Ok(output) => {
            assert!(output.contains("DIFF SUMMARY"), "Missing summary header");
            assert!(output.contains("Added:"), "Missing added chunks");
            assert!(output.contains("Removed:"), "Missing removed chunks");
            assert!(output.contains("Change rate:"), "Missing change rate");
        }
        Err(e) => panic!("Command failed: {}", e),
    }

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

#[test]
#[ignore]
fn test_diff_command_export_json() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_str().unwrap();
    let export_file = temp_dir.path().join("diff_output.json");

    create_test_database(base_path, "test_db1", 4).unwrap();
    create_test_database(base_path, "test_db2", 6).unwrap();

    std::env::set_var("TALARIA_DATABASES_DIR", base_path);

    let result = run_talaria_command(&[
        "database",
        "diff",
        "test_db1",
        "test_db2",
        "--export",
        export_file.to_str().unwrap()
    ]);

    match result {
        Ok(_) => {
            // Check that export file was created
            assert!(export_file.exists(), "Export file not created");

            // Read and validate JSON
            let json_content = fs::read_to_string(&export_file).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json_content).unwrap();

            // Check for expected fields
            assert!(parsed.get("chunk_analysis").is_some(), "Missing chunk_analysis in JSON");
            assert!(parsed.get("sequence_analysis").is_some(), "Missing sequence_analysis in JSON");
            assert!(parsed.get("taxonomy_analysis").is_some(), "Missing taxonomy_analysis in JSON");
            assert!(parsed.get("storage_metrics").is_some(), "Missing storage_metrics in JSON");
        }
        Err(e) => panic!("Command failed: {}", e),
    }

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

#[test]
#[ignore]
fn test_diff_identical_databases() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_str().unwrap();

    // Create identical databases
    create_test_database(base_path, "test_db_same", 5).unwrap();

    std::env::set_var("TALARIA_DATABASES_DIR", base_path);

    let result = run_talaria_command(&[
        "database",
        "diff",
        "test_db_same",
        "test_db_same",
        "--all"
    ]);

    match result {
        Ok(output) => {
            // Should show 100% shared for identical databases
            assert!(output.contains("100.0%"), "Should show 100% shared for identical DBs");
            assert!(output.contains("Unique to first:           0"), "Should have 0 unique chunks");
            assert!(output.contains("Unique to second:          0"), "Should have 0 unique chunks");
        }
        Err(e) => panic!("Command failed: {}", e),
    }

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

#[test]
#[ignore]
fn test_diff_with_sequences_flag() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_str().unwrap();

    create_test_database(base_path, "test_db1", 3).unwrap();
    create_test_database(base_path, "test_db2", 4).unwrap();

    std::env::set_var("TALARIA_DATABASES_DIR", base_path);

    let result = run_talaria_command(&[
        "database",
        "diff",
        "test_db1",
        "test_db2",
        "--sequences"
    ]);

    match result {
        Ok(output) => {
            assert!(output.contains("SEQUENCE-LEVEL ANALYSIS"), "Should show sequence analysis");
            assert!(output.contains("Total sequences"), "Should show sequence counts");
            assert!(output.contains("Shared sequences"), "Should show shared sequences");
        }
        Err(e) => panic!("Command failed: {}", e),
    }

    std::env::remove_var("TALARIA_DATABASES_DIR");
}

#[test]
#[ignore]
fn test_diff_with_taxonomy_flag() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_str().unwrap();

    create_test_database(base_path, "test_db1", 2).unwrap();
    create_test_database(base_path, "test_db2", 3).unwrap();

    std::env::set_var("TALARIA_DATABASES_DIR", base_path);

    let result = run_talaria_command(&[
        "database",
        "diff",
        "test_db1",
        "test_db2",
        "--taxonomy"
    ]);

    match result {
        Ok(output) => {
            assert!(output.contains("TAXONOMY DISTRIBUTION"), "Should show taxonomy analysis");
            assert!(output.contains("Taxa in first"), "Should show taxa counts");
            assert!(output.contains("Shared taxa"), "Should show shared taxa");
        }
        Err(e) => panic!("Command failed: {}", e),
    }

    std::env::remove_var("TALARIA_DATABASES_DIR");
}