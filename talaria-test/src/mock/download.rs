//! Mock download source for testing
//!
//! Provides a test implementation of database downloads that doesn't require network access.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Mock download source that creates test data
pub struct MockDownloadSource;

impl MockDownloadSource {
    /// Get the download URL for a test source
    pub fn get_url() -> String {
        "mock://test/database.fasta.gz".to_string()
    }

    /// Create a mock compressed database file
    pub fn create_compressed_file(path: &Path) -> Result<PathBuf> {
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create a simple gzipped test file
        let test_data = b">test_seq_1\nACGTACGTACGT\n>test_seq_2\nTGCATGCATGCA\n";

        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let gz_path = if path.extension() == Some(std::ffi::OsStr::new("gz")) {
            path.to_path_buf()
        } else {
            path.with_extension("gz")
        };

        let file = fs::File::create(&gz_path)?;
        let mut encoder = GzEncoder::new(file, Compression::fast());
        encoder.write_all(test_data)?;
        encoder.finish()?;

        Ok(gz_path)
    }

    /// Simulate a download by creating test data
    pub async fn download_test_database(dest_path: &Path) -> Result<PathBuf> {
        Self::create_compressed_file(dest_path)
    }

    /// Get metadata for test database
    pub fn get_metadata() -> TestDatabaseMetadata {
        TestDatabaseMetadata {
            size_bytes: 1024, // Small test file
            checksum: Some("test_checksum_12345".to_string()),
            version: "test_v1.0".to_string(),
        }
    }
}

/// Metadata for test database
pub struct TestDatabaseMetadata {
    pub size_bytes: u64,
    pub checksum: Option<String>,
    pub version: String,
}

/// Test helper to create a valid download state
pub fn create_test_download_state(workspace: &Path) -> Result<()> {
    fs::create_dir_all(workspace)?;

    // Create a state.json file
    let state = r#"{
        "id": "test_download_123",
        "source": {"Test": null},
        "workspace": "",
        "stage": {"Initializing": null},
        "checkpoints": [],
        "files": {
            "compressed": null,
            "decompressed": null,
            "temp_files": [],
            "preserve_on_failure": []
        },
        "created_at": "2024-01-01T00:00:00Z"
    }"#;

    fs::write(workspace.join("state.json"), state)?;
    Ok(())
}
