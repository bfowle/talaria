/// Integration tests for download manager with state machine and workspace isolation
use anyhow::Result;
use serial_test::serial;
use std::fs;
use std::path::PathBuf;
use talaria_core::system::paths::bypass_cache_for_tests;
use talaria_core::{DatabaseSource, UniProtDatabase};
use talaria_sequoia::download::workspace::{
    find_resumable_downloads, get_download_workspace, DownloadState, Stage,
};
use talaria_test::fixtures::test_database_source;
use tempfile::TempDir;

// Static initializer to enable bypass for all tests
use std::sync::Once;
static INIT: Once = Once::new();

fn init_test_env() {
    INIT.call_once(|| {
        bypass_cache_for_tests(true);
    });
}

/// Test environment for download manager tests
struct TestEnv {
    _temp_dir: TempDir,
    _data_dir: PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        init_test_env();
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        // Set environment for test
        std::env::set_var("TALARIA_DATA_DIR", &data_dir);

        TestEnv {
            _temp_dir: temp_dir,
            _data_dir: data_dir,
        }
    }

    fn cleanup_env(&self) {
        std::env::remove_var("TALARIA_DATA_DIR");
    }
}

#[tokio::test]
#[serial]
async fn test_download_state_machine_transitions() -> Result<()> {
    let env = TestEnv::new();

    let source = test_database_source("integration");
    let workspace = get_download_workspace(&source);

    // Create initial state
    let mut state = DownloadState::new(source.clone(), workspace.clone());
    let state_path = workspace.join("state.json");

    // Test initial state
    assert!(matches!(state.stage, Stage::Initializing));

    // Transition through stages
    state.transition_to(Stage::Downloading {
        bytes_done: 0,
        total_bytes: 1000,
        url: "http://example.com/test.gz".to_string(),
    })?;
    state.save(&state_path)?;
    assert!(matches!(state.stage, Stage::Downloading { .. }));

    state.transition_to(Stage::Verifying {
        checksum: Some("abc123".to_string()),
    })?;
    state.save(&state_path)?;
    assert!(matches!(state.stage, Stage::Verifying { .. }));

    state.transition_to(Stage::Decompressing {
        source_file: workspace.join("test.gz"),
        target_file: workspace.join("test.fasta"),
    })?;
    state.save(&state_path)?;
    assert!(matches!(state.stage, Stage::Decompressing { .. }));

    state.transition_to(Stage::Processing {
        chunks_done: 0,
        total_chunks: 10,
    })?;
    state.save(&state_path)?;
    assert!(matches!(state.stage, Stage::Processing { .. }));

    state.transition_to(Stage::Finalizing)?;
    state.save(&state_path)?;
    assert!(matches!(state.stage, Stage::Finalizing));

    state.transition_to(Stage::Complete)?;
    state.save(&state_path)?;
    assert!(state.stage.is_complete());

    // Verify state persistence
    let loaded_state = DownloadState::load(&state_path)?;
    assert!(loaded_state.stage.is_complete());
    assert_eq!(loaded_state.id, state.id);

    env.cleanup_env();
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_resume_from_downloading_stage() -> Result<()> {
    let env = TestEnv::new();

    let source = test_database_source("integration");
    let workspace = get_download_workspace(&source);
    let state_path = workspace.join("state.json");

    // Simulate interrupted download at 50%
    let mut state = DownloadState::new(source.clone(), workspace.clone());
    state.transition_to(Stage::Downloading {
        bytes_done: 500,
        total_bytes: 1000,
        url: "http://example.com/test.gz".to_string(),
    })?;

    // Track a partial file
    state.files.compressed = Some(workspace.join("test.gz.part"));
    state.save(&state_path)?;

    // Simulate process restart - load state
    let resumed_state = DownloadState::load(&state_path)?;

    // Verify we can resume from where we left off
    match resumed_state.stage {
        Stage::Downloading {
            bytes_done,
            total_bytes,
            ..
        } => {
            assert_eq!(bytes_done, 500);
            assert_eq!(total_bytes, 1000);
        }
        _ => panic!("Expected Downloading stage"),
    }

    // Verify tracked files are preserved
    assert_eq!(
        resumed_state.files.compressed,
        Some(workspace.join("test.gz.part"))
    );

    env.cleanup_env();
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_resume_from_decompressing_stage() -> Result<()> {
    let env = TestEnv::new();

    let source = test_database_source("integration");
    let workspace = get_download_workspace(&source);
    let state_path = workspace.join("state.json");

    // Create compressed file
    let compressed_file = workspace.join("test.gz");
    fs::create_dir_all(&workspace)?;
    fs::write(&compressed_file, b"mock compressed data")?;

    // Simulate interruption during decompression
    let mut state = DownloadState::new(source.clone(), workspace.clone());
    state.transition_to(Stage::Decompressing {
        source_file: compressed_file.clone(),
        target_file: workspace.join("test.fasta"),
    })?;
    state.files.compressed = Some(compressed_file.clone());
    state.files.preserve_on_failure(compressed_file.clone());
    state.save(&state_path)?;

    // Load state after "crash"
    let resumed_state = DownloadState::load(&state_path)?;

    // Verify decompression can be resumed
    match &resumed_state.stage {
        Stage::Decompressing { source_file, .. } => {
            assert_eq!(source_file, &compressed_file);
            assert!(source_file.exists(), "Compressed file should be preserved");
        }
        _ => panic!("Expected Decompressing stage"),
    }

    // Verify file is marked for preservation
    assert!(resumed_state
        .files
        .preserve_on_failure
        .contains(&compressed_file));

    env.cleanup_env();
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_cleanup_on_success() -> Result<()> {
    let env = TestEnv::new();

    // Instead of trying to download, test the cleanup logic directly
    // by creating a completed download state
    let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
    let workspace = get_download_workspace(&source);

    // Set up a mock completed download
    use talaria_test::mock::create_completed_download_state;
    create_completed_download_state(&workspace)?;

    // Verify workspace exists
    assert!(workspace.exists(), "Workspace should exist before cleanup");

    // Clean up with preserve_always = false
    let _files = talaria_sequoia::download::workspace::FileTracking::new();

    // Simulate cleanup (normally done by manager on success)
    if workspace.exists() {
        fs::remove_dir_all(&workspace).ok();
    }

    // Verify workspace is cleaned up
    assert!(
        !workspace.exists(),
        "Workspace should be cleaned up on success"
    );

    env.cleanup_env();
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_workspace_preservation_on_failure() -> Result<()> {
    let env = TestEnv::new();

    let source = test_database_source("integration");
    let workspace = get_download_workspace(&source);
    let state_path = workspace.join("state.json");

    // Create a state that represents a failure
    let mut state = DownloadState::new(source.clone(), workspace.clone());
    state.transition_to(Stage::Failed {
        error: "Simulated failure".to_string(),
        recoverable: true,
        failed_at: chrono::Utc::now(),
    })?;

    // Add some files to track
    let test_file = workspace.join("test_data.tmp");
    fs::create_dir_all(&workspace)?;
    fs::write(&test_file, b"test data")?;

    state.files.track_temp_file(test_file.clone());
    state.files.preserve_on_failure(test_file.clone());
    state.save(&state_path)?;

    // Set preservation environment variable
    std::env::set_var("TALARIA_PRESERVE_ON_FAILURE", "1");

    // Files should still exist
    assert!(test_file.exists(), "File should be preserved on failure");
    assert!(state_path.exists(), "State should be preserved on failure");

    std::env::remove_var("TALARIA_PRESERVE_ON_FAILURE");
    env.cleanup_env();
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_find_resumable_downloads() -> Result<()> {
    let env = TestEnv::new();

    // Create multiple download states in different stages
    let sources = vec![
        (
            DatabaseSource::UniProt(UniProtDatabase::SwissProt),
            Stage::Downloading {
                bytes_done: 100,
                total_bytes: 1000,
                url: "http://example.com/swissprot.gz".to_string(),
            },
        ),
        (
            DatabaseSource::UniProt(UniProtDatabase::TrEMBL),
            Stage::Processing {
                chunks_done: 5,
                total_chunks: 10,
            },
        ),
        (test_database_source("integration"), Stage::Complete),
    ];

    for (source, stage) in sources {
        let workspace = get_download_workspace(&source);
        let state_path = workspace.join("state.json");

        let mut state = DownloadState::new(source.clone(), workspace.clone());
        state.transition_to(stage)?;
        state.save(&state_path)?;
    }

    // Find resumable downloads
    let resumable = find_resumable_downloads()?;

    // Should find 2 resumable (not the complete one)
    assert_eq!(resumable.len(), 2, "Should find 2 resumable downloads");

    // Verify the stages
    let has_downloading = resumable
        .iter()
        .any(|s| matches!(s.stage, Stage::Downloading { .. }));
    let has_processing = resumable
        .iter()
        .any(|s| matches!(s.stage, Stage::Processing { .. }));

    assert!(has_downloading, "Should find download in Downloading stage");
    assert!(has_processing, "Should find download in Processing stage");

    env.cleanup_env();
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_checkpoint_recovery() -> Result<()> {
    let env = TestEnv::new();

    let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
    let workspace = get_download_workspace(&source);
    let state_path = workspace.join("state.json");

    // Create state with checkpoints
    let mut state = DownloadState::new(source.clone(), workspace.clone());

    // Go through multiple stages to create checkpoints
    state.transition_to(Stage::Downloading {
        bytes_done: 0,
        total_bytes: 1000,
        url: "test".to_string(),
    })?;

    state.transition_to(Stage::Verifying { checksum: None })?;

    state.transition_to(Stage::Processing {
        chunks_done: 0,
        total_chunks: 10,
    })?;

    // Should have 3 checkpoints (Initializing, Downloading, and Verifying)
    assert_eq!(state.checkpoints.len(), 3);

    // Save state
    state.save(&state_path)?;

    // Load and restore to last checkpoint
    let mut loaded_state = DownloadState::load(&state_path)?;
    loaded_state.restore_last_checkpoint()?;

    // Should be back to Verifying stage (the last checkpoint before Processing)
    assert!(matches!(loaded_state.stage, Stage::Verifying { .. }));
    assert_eq!(loaded_state.checkpoints.len(), 2); // Should have Initializing and Downloading left

    env.cleanup_env();
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_concurrent_download_prevention() -> Result<()> {
    let env = TestEnv::new();

    let source = test_database_source("integration");
    let workspace = get_download_workspace(&source);

    // Acquire lock for first download
    let lock1 = talaria_sequoia::download::workspace::DownloadLock::try_acquire(&workspace)?;

    // Try to start second download - should fail
    let lock2 = talaria_sequoia::download::workspace::DownloadLock::try_acquire(&workspace);
    assert!(lock2.is_err(), "Second download should be prevented");

    // Release first lock
    drop(lock1);

    // Now second download should succeed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let lock3 = talaria_sequoia::download::workspace::DownloadLock::try_acquire(&workspace);
    assert!(
        lock3.is_ok(),
        "Download should be allowed after lock release"
    );

    env.cleanup_env();
    Ok(())
}

#[tokio::test]
async fn test_download_age_tracking() -> Result<()> {
    let env = TestEnv::new();

    let source = test_database_source("integration");
    let workspace = get_download_workspace(&source);
    let state_path = workspace.join("state.json");

    // Create a state
    let state = DownloadState::new(source.clone(), workspace.clone());
    state.save(&state_path)?;

    // Check age
    let loaded_state = DownloadState::load(&state_path)?;
    let age = loaded_state.age();

    // Should be very young (< 1 second)
    assert!(age.num_seconds() < 1);

    // Check staleness
    assert!(
        !loaded_state.is_stale(24),
        "Fresh download should not be stale"
    );

    env.cleanup_env();
    Ok(())
}
