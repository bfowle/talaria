/// Integration tests for download workspace discovery and resume functionality
use anyhow::Result;
use serial_test::serial;
use std::fs;
use talaria_core::system::paths;
use talaria_core::{DatabaseSource, NCBIDatabase, UniProtDatabase};
use talaria_herald::download::{
    find_existing_workspace_for_source, get_download_workspace, DownloadState, Stage,
};
use tempfile::TempDir;

struct TestEnv {
    _temp_dir: TempDir,
    original_data_dir: Option<String>,
}

impl TestEnv {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let original = std::env::var("TALARIA_DATA_DIR").ok();
        std::env::set_var("TALARIA_DATA_DIR", temp_dir.path());

        // Enable cache bypass for tests
        talaria_core::system::paths::bypass_cache_for_tests(true);

        Self {
            _temp_dir: temp_dir,
            original_data_dir: original,
        }
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        if let Some(ref original) = self.original_data_dir {
            std::env::set_var("TALARIA_DATA_DIR", original);
        } else {
            std::env::remove_var("TALARIA_DATA_DIR");
        }
        talaria_core::system::paths::bypass_cache_for_tests(false);
    }
}

#[test]
#[serial]
fn test_workspace_discovery_with_complete_download() -> Result<()> {
    let _env = TestEnv::new();
    let source = DatabaseSource::UniProt(UniProtDatabase::UniRef50);

    // Create a complete download workspace
    let downloads_dir = paths::talaria_downloads_dir();
    fs::create_dir_all(&downloads_dir)?;

    let workspace = downloads_dir.join("uniprot_uniref50_20250927_test");
    fs::create_dir_all(&workspace)?;

    // Create state showing download is complete
    let mut state = DownloadState::new(source.clone(), workspace.clone());
    state.stage = Stage::Complete;

    // Add file tracking
    let fasta_gz = workspace.join("uniref50.fasta.gz");
    let fasta = workspace.join("uniref50.fasta");
    fs::write(&fasta_gz, b"compressed data")?;
    fs::write(&fasta, b"decompressed data")?;

    state.files.compressed = Some(fasta_gz);
    state.files.decompressed = Some(fasta);
    state.save(&workspace.join("state.json"))?;

    // Test discovery
    let result = find_existing_workspace_for_source(&source)?;
    assert!(result.is_some(), "Should find existing workspace");

    let (_found_path, found_state) = result.unwrap();
    assert_eq!(found_state.source, source);
    assert_eq!(found_state.stage, Stage::Complete);
    assert!(found_state.files.decompressed.is_some());

    Ok(())
}

#[test]
#[serial]
fn test_workspace_discovery_with_incomplete_download() -> Result<()> {
    let _env = TestEnv::new();
    let source = DatabaseSource::NCBI(NCBIDatabase::NR);

    // Create an incomplete download workspace
    let downloads_dir = paths::talaria_downloads_dir();
    fs::create_dir_all(&downloads_dir)?;

    let workspace = downloads_dir.join("ncbi_nr_20250927_incomplete");
    fs::create_dir_all(&workspace)?;

    // Create state showing download in progress
    let mut state = DownloadState::new(source.clone(), workspace.clone());
    state.stage = Stage::Downloading {
        bytes_done: 5_000_000_000,   // 5GB
        total_bytes: 90_000_000_000, // 90GB
        url: "https://ftp.ncbi.nlm.nih.gov/blast/db/FASTA/nr.gz".to_string(),
    };
    state.save(&workspace.join("state.json"))?;

    // Test discovery
    let result = find_existing_workspace_for_source(&source)?;
    assert!(result.is_some(), "Should find incomplete workspace");

    let (_, found_state) = result.unwrap();
    match found_state.stage {
        Stage::Downloading {
            bytes_done,
            total_bytes,
            ..
        } => {
            assert_eq!(bytes_done, 5_000_000_000);
            assert_eq!(total_bytes, 90_000_000_000);
        }
        _ => panic!("Expected Downloading stage"),
    }

    Ok(())
}

#[test]
#[serial]
fn test_workspace_discovery_prioritizes_newest() -> Result<()> {
    let _env = TestEnv::new();
    let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
    let downloads_dir = paths::talaria_downloads_dir();
    fs::create_dir_all(&downloads_dir)?;

    // Create three workspaces with different ages
    for (suffix, stage) in [
        (
            "old",
            Stage::Failed {
                error: "Network error".to_string(),
                recoverable: true,
                failed_at: chrono::Utc::now(),
            },
        ),
        (
            "middle",
            Stage::Processing {
                chunks_done: 100,
                total_chunks: 500,
            },
        ),
        ("newest", Stage::Complete),
    ] {
        let workspace = downloads_dir.join(format!("uniprot_swissprot_20250927_{}", suffix));
        fs::create_dir_all(&workspace)?;

        let mut state = DownloadState::new(source.clone(), workspace.clone());
        state.stage = stage;
        state.save(&workspace.join("state.json"))?;

        // Add small delay to ensure different modification times
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // Should find the newest (Complete) one
    let result = find_existing_workspace_for_source(&source)?;
    assert!(result.is_some());

    let (found_path, found_state) = result.unwrap();
    assert!(found_path.to_string_lossy().contains("newest"));
    assert_eq!(found_state.stage, Stage::Complete);

    Ok(())
}

#[test]
#[serial]
fn test_workspace_discovery_handles_missing_state_file() -> Result<()> {
    let _env = TestEnv::new();
    let source = DatabaseSource::UniProt(UniProtDatabase::TrEMBL);
    let downloads_dir = paths::talaria_downloads_dir();
    fs::create_dir_all(&downloads_dir)?;

    // Create workspace without state.json
    let workspace = downloads_dir.join("uniprot_trembl_20250927_nostate");
    fs::create_dir_all(&workspace)?;

    // Should not find this workspace
    let result = find_existing_workspace_for_source(&source)?;
    assert!(result.is_none());

    Ok(())
}

#[test]
#[serial]
fn test_workspace_discovery_with_processing_stage() -> Result<()> {
    let _env = TestEnv::new();
    let source = DatabaseSource::UniProt(UniProtDatabase::UniRef90);

    let downloads_dir = paths::talaria_downloads_dir();
    fs::create_dir_all(&downloads_dir)?;

    let workspace = downloads_dir.join("uniprot_uniref90_20250927_processing");
    fs::create_dir_all(&workspace)?;

    // Create state in processing stage
    let mut state = DownloadState::new(source.clone(), workspace.clone());
    state.stage = Stage::Processing {
        chunks_done: 250,
        total_chunks: 1000,
    };

    // Create decompressed file
    let fasta = workspace.join("uniref90.fasta");
    fs::write(&fasta, b"sequence data")?;
    state.files.decompressed = Some(fasta);

    state.save(&workspace.join("state.json"))?;

    // Test discovery
    let result = find_existing_workspace_for_source(&source)?;
    assert!(result.is_some());

    let (_, found_state) = result.unwrap();
    match found_state.stage {
        Stage::Processing {
            chunks_done,
            total_chunks,
        } => {
            assert_eq!(chunks_done, 250);
            assert_eq!(total_chunks, 1000);
        }
        _ => panic!("Expected Processing stage"),
    }

    Ok(())
}

#[test]
#[serial]
fn test_workspace_discovery_with_decompressing_stage() -> Result<()> {
    let _env = TestEnv::new();
    let source = DatabaseSource::UniProt(UniProtDatabase::UniRef100);

    let downloads_dir = paths::talaria_downloads_dir();
    fs::create_dir_all(&downloads_dir)?;

    let workspace = downloads_dir.join("uniprot_uniref100_20250927_decomp");
    fs::create_dir_all(&workspace)?;

    let compressed = workspace.join("uniref100.fasta.gz");
    let decompressed = workspace.join("uniref100.fasta");
    fs::write(&compressed, b"compressed")?;

    // Create state in decompressing stage
    let mut state = DownloadState::new(source.clone(), workspace.clone());
    state.stage = Stage::Decompressing {
        source_file: compressed.clone(),
        target_file: decompressed,
    };
    state.files.compressed = Some(compressed);

    state.save(&workspace.join("state.json"))?;

    // Test discovery
    let result = find_existing_workspace_for_source(&source)?;
    assert!(result.is_some());

    let (_, found_state) = result.unwrap();
    match &found_state.stage {
        Stage::Decompressing { source_file, .. } => {
            assert!(source_file.exists());
        }
        _ => panic!("Expected Decompressing stage"),
    }

    Ok(())
}

#[test]
#[serial]
fn test_get_download_workspace_creates_unique_paths() {
    let _env = TestEnv::new();

    let source1 = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
    let workspace1 = get_download_workspace(&source1);

    // Same source should create different workspace (due to unique session ID)
    let workspace2 = get_download_workspace(&source1);
    assert_ne!(workspace1, workspace2);

    // Different source
    let source2 = DatabaseSource::NCBI(NCBIDatabase::Taxonomy);
    let workspace3 = get_download_workspace(&source2);

    assert!(workspace3.to_string_lossy().contains("ncbi_taxonomy"));
    assert!(!workspace3.to_string_lossy().contains("uniprot"));
}
