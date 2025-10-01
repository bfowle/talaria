/// Test concurrent download scenarios to verify workspace isolation and locking
use anyhow::Result;
use serial_test::serial;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Semaphore;

use talaria_core::{DatabaseSource, NCBIDatabase, UniProtDatabase};
use talaria_sequoia::download::workspace::{
    find_resumable_downloads, get_download_workspace, DownloadLock, DownloadState, Stage,
};

// Import the bypass function for tests
use talaria_core::system::paths::bypass_cache_for_tests;

// Static initializer to enable bypass for all tests
use std::sync::Once;
static INIT: Once = Once::new();

fn init_test_env() {
    INIT.call_once(|| {
        bypass_cache_for_tests(true);
    });
}

#[tokio::test]
#[serial]
async fn test_workspace_isolation() -> Result<()> {
    init_test_env();

    // Set up test environment
    let temp_dir = TempDir::new()?;
    let test_data_dir = temp_dir.path().join("test_workspace_isolation");
    std::fs::create_dir_all(&test_data_dir)?;
    std::env::set_var("TALARIA_DATA_DIR", &test_data_dir);

    // Create different database sources
    let source1 = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
    let source2 = DatabaseSource::NCBI(NCBIDatabase::NR);

    // Get workspaces - they should be different
    let workspace1 = get_download_workspace(&source1);
    let workspace2 = get_download_workspace(&source2);

    assert_ne!(workspace1, workspace2, "Workspaces should be isolated");

    // Workspace names should contain the database identifier
    assert!(workspace1.to_str().unwrap().contains("uniprot_swissprot"));
    assert!(workspace2.to_str().unwrap().contains("ncbi_nr"));

    std::env::remove_var("TALARIA_DATA_DIR");
    Ok(())
}

#[tokio::test]
async fn test_concurrent_lock_acquisition() -> Result<()> {
    init_test_env();
    let temp_dir = TempDir::new()?;
    let workspace = temp_dir.path().join("test_workspace");

    // First lock should succeed
    let lock1 = DownloadLock::try_acquire(&workspace)?;
    assert!(DownloadLock::is_locked(&workspace));

    // Concurrent lock should fail
    let lock2_result = DownloadLock::try_acquire(&workspace);
    assert!(lock2_result.is_err(), "Second lock should fail");

    // After dropping first lock, new lock should succeed
    drop(lock1);

    // Small delay to ensure lock file is released
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let lock3 = DownloadLock::try_acquire(&workspace);
    assert!(lock3.is_ok(), "Lock should be acquirable after release");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_state_persistence_across_sessions() -> Result<()> {
    init_test_env();

    let temp_dir = TempDir::new()?;
    let test_data_dir = temp_dir.path().join("test_state_persistence");
    std::fs::create_dir_all(&test_data_dir)?;
    std::env::set_var("TALARIA_DATA_DIR", &test_data_dir);

    let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
    let workspace = get_download_workspace(&source);
    let state_path = workspace.join("state.json");

    // Create and save state
    let mut state1 = DownloadState::new(source.clone(), workspace.clone());
    state1.transition_to(talaria_sequoia::download::workspace::Stage::Downloading {
        bytes_done: 1024,
        total_bytes: 2048,
        url: "http://example.com/test.gz".to_string(),
    })?;
    state1.save(&state_path)?;

    // Load state in new "session"
    let state2 = DownloadState::load(&state_path)?;

    // Verify state was preserved
    assert_eq!(state1.id, state2.id);
    assert!(matches!(
        state2.stage,
        talaria_sequoia::download::workspace::Stage::Downloading {
            bytes_done: 1024,
            ..
        }
    ));

    std::env::remove_var("TALARIA_DATA_DIR");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_resumable_downloads_discovery() -> Result<()> {
    init_test_env();

    let temp_dir = TempDir::new()?;
    // Use a unique subdirectory to ensure isolation
    let test_data_dir = temp_dir.path().join("test_resumable_discovery");
    std::fs::create_dir_all(&test_data_dir)?;
    std::env::set_var("TALARIA_DATA_DIR", &test_data_dir);

    // Create multiple download states
    let sources = vec![
        DatabaseSource::UniProt(UniProtDatabase::SwissProt),
        DatabaseSource::UniProt(UniProtDatabase::TrEMBL),
        DatabaseSource::NCBI(NCBIDatabase::NR),
    ];

    for (i, source) in sources.iter().enumerate() {
        let workspace = get_download_workspace(source);
        let state_path = workspace.join("state.json");

        let mut state = DownloadState::new(source.clone(), workspace.clone());

        // Put some in different stages
        match i {
            0 => state.transition_to(Stage::Downloading {
                bytes_done: 100,
                total_bytes: 1000,
                url: format!("http://example.com/db{}.gz", i),
            })?,
            1 => state.transition_to(Stage::Processing {
                chunks_done: 50,
                total_chunks: 100,
            })?,
            2 => state.transition_to(Stage::Complete)?,
            _ => {}
        }

        state.save(&state_path)?;
    }

    // Find resumable downloads
    let resumable = find_resumable_downloads()?;

    // Should find 2 (not the complete one)
    assert_eq!(resumable.len(), 2, "Should find 2 resumable downloads");

    // Verify they're in expected states
    let stages: Vec<_> = resumable.iter().map(|s| s.stage.name()).collect();
    assert!(stages.contains(&"downloading"));
    assert!(stages.contains(&"processing"));
    assert!(!stages.contains(&"complete"));

    std::env::remove_var("TALARIA_DATA_DIR");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_concurrent_downloads_different_databases() -> Result<()> {
    init_test_env();

    let temp_dir = TempDir::new()?;
    let test_data_dir = temp_dir.path().join("test_concurrent_downloads");
    std::fs::create_dir_all(&test_data_dir)?;
    std::env::set_var("TALARIA_DATA_DIR", &test_data_dir);

    // Simulate concurrent downloads
    let sources = vec![
        Arc::new(DatabaseSource::UniProt(UniProtDatabase::SwissProt)),
        Arc::new(DatabaseSource::UniProt(UniProtDatabase::TrEMBL)),
        Arc::new(DatabaseSource::NCBI(NCBIDatabase::Taxonomy)),
    ];

    let semaphore = Arc::new(Semaphore::new(3));
    let mut handles = Vec::new();

    for source in sources {
        let sem = semaphore.clone();
        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            // Get workspace and try to acquire lock
            let workspace = get_download_workspace(&*source);
            let lock = DownloadLock::try_acquire(&workspace);

            // Each should succeed since they're different databases
            assert!(
                lock.is_ok(),
                "Lock acquisition should succeed for {:?}",
                source
            );

            // Simulate some work
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

            // Lock will be released on drop
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await?;
    }

    std::env::remove_var("TALARIA_DATA_DIR");
    Ok(())
}

#[tokio::test]
async fn test_stale_lock_cleanup() -> Result<()> {
    init_test_env();
    let temp_dir = TempDir::new()?;
    let workspace = temp_dir.path().join("test_workspace");
    let lock_path = workspace.join(".lock");

    // Create directory
    std::fs::create_dir_all(&workspace)?;

    // Manually create a stale lock file (with invalid PID)
    std::fs::write(&lock_path, "99999999\nlocalhost\n2020-01-01T00:00:00Z")?;

    // Should be able to acquire lock (stale lock should be removed)
    let lock = DownloadLock::try_acquire(&workspace);
    assert!(
        lock.is_ok(),
        "Should be able to acquire lock after stale lock cleanup"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_workspace_cleanup_preserves_active() -> Result<()> {
    init_test_env();

    let temp_dir = TempDir::new()?;
    // Use a unique subdirectory to ensure isolation
    let test_data_dir = temp_dir.path().join("test_cleanup_preserves");
    std::fs::create_dir_all(&test_data_dir)?;
    std::env::set_var("TALARIA_DATA_DIR", &test_data_dir);

    // Create an active download
    let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
    let workspace = get_download_workspace(&source);
    let state_path = workspace.join("state.json");

    let state = DownloadState::new(source, workspace.clone());
    state.save(&state_path)?;

    // Acquire lock to mark as active
    let _lock = DownloadLock::try_acquire(&workspace)?;

    // Try to clean with short max age
    let cleaned = talaria_sequoia::download::workspace::cleanup_old_workspaces(1)?;

    // Should not clean locked workspace
    assert_eq!(cleaned, 0, "Should not clean active workspace");
    assert!(workspace.exists(), "Active workspace should still exist");

    std::env::remove_var("TALARIA_DATA_DIR");
    Ok(())
}

#[test]
fn test_unique_session_ids() {
    init_test_env();
    // Even for same database, session IDs should be unique
    let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);

    let workspace1 = get_download_workspace(&source);
    // Small delay to ensure timestamp difference
    std::thread::sleep(std::time::Duration::from_millis(1));
    let workspace2 = get_download_workspace(&source);

    // Workspaces should be different due to unique session IDs
    assert_ne!(
        workspace1, workspace2,
        "Each download should get unique workspace"
    );
}
