//! Test helpers for download state management
//!
//! Provides utilities to create and manipulate download states for testing
//! without requiring actual network downloads.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Create a mock download workspace with pre-populated state
pub fn setup_test_download_workspace(workspace: &Path) -> Result<()> {
    // Create workspace directory structure
    fs::create_dir_all(workspace)?;

    // Create a mock compressed file
    let compressed_file = workspace.join("test_database.fasta.gz");
    create_mock_compressed_file(&compressed_file)?;

    // Create a mock decompressed file
    let decompressed_file = workspace.join("test_database.fasta");
    fs::write(&decompressed_file, b">test_seq1\nACGT\n>test_seq2\nTGCA\n")?;

    Ok(())
}

/// Create a mock compressed database file
pub fn create_mock_compressed_file(path: &Path) -> Result<()> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let test_data = b">test_seq1\nACGTACGTACGT\n>test_seq2\nTGCATGCATGCA\n";

    let file = fs::File::create(path)?;
    let mut encoder = GzEncoder::new(file, Compression::fast());
    encoder.write_all(test_data)?;
    encoder.finish()?;

    Ok(())
}

/// Create a completed download state for testing cleanup
pub fn create_completed_download_state(workspace: &Path) -> Result<PathBuf> {
    setup_test_download_workspace(workspace)?;

    // Create state.json indicating completion
    let state_json = r#"{
        "id": "test_download_complete",
        "source": {"UniProt": "SwissProt"},
        "workspace": "",
        "stage": {"Complete": null},
        "checkpoints": [],
        "files": {
            "compressed": null,
            "decompressed": null,
            "temp_files": [],
            "preserve_on_failure": []
        },
        "created_at": "2024-01-01T00:00:00Z"
    }"#;

    let state_path = workspace.join("state.json");
    fs::write(&state_path, state_json)?;
    Ok(state_path)
}

/// Create a download state at a specific stage for testing
pub fn create_download_state_at_stage(workspace: &Path, stage: &str) -> Result<PathBuf> {
    fs::create_dir_all(workspace)?;

    let stage_json = match stage {
        "downloading" => {
            r#"{"Downloading": {"bytes_done": 500, "total_bytes": 1000, "url": "test://url"}}"#
        }
        "verifying" => r#"{"Verifying": {"checksum": "abc123"}}"#,
        "decompressing" => {
            r#"{"Decompressing": {"source_file": "test.gz", "target_file": "test.fasta"}}"#
        }
        "processing" => r#"{"Processing": {"chunks_done": 5, "total_chunks": 10}}"#,
        "complete" => r#"{"Complete": null}"#,
        _ => r#"{"Initializing": null}"#,
    };

    let state_json = format!(
        r#"{{
        "id": "test_download_{}",
        "source": {{"UniProt": "SwissProt"}},
        "workspace": "{}",
        "stage": {},
        "checkpoints": [],
        "files": {{
            "compressed": null,
            "decompressed": null,
            "temp_files": [],
            "preserve_on_failure": []
        }},
        "created_at": "2024-01-01T00:00:00Z"
    }}"#,
        stage,
        workspace.display(),
        stage_json
    );

    let state_path = workspace.join("state.json");
    fs::write(&state_path, state_json)?;
    Ok(state_path)
}
