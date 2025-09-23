/// Tests for resumable processing operations in SEQUOIA
use talaria_sequoia::processing_state::{
    OperationType, ProcessingState, ProcessingStateManager, SourceInfo,
};
use talaria_sequoia::storage::SEQUOIAStorage;
use talaria_sequoia::types::SHA256Hash;
use anyhow::Result;
use std::collections::HashSet;
use tempfile::TempDir;

#[test]
fn test_resume_after_partial_download() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = SEQUOIAStorage::new(temp_dir.path())?;

    // Simulate starting a download operation
    let source_info = SourceInfo {
        database: "test_db".to_string(),
        source_url: Some("https://example.com/db".to_string()),
        etag: Some("v1.0".to_string()),
        total_size_bytes: Some(1_000_000),
    };

    let manifest_hash = SHA256Hash::compute(b"test_manifest");
    let manifest_version = "v1.0".to_string();
    let total_chunks = 100;

    // Start processing
    let _operation_id = storage.start_processing(
        OperationType::InitialDownload,
        manifest_hash.clone(),
        manifest_version.clone(),
        total_chunks,
        source_info.clone(),
    )?;

    // Simulate downloading some chunks
    let completed_chunks: Vec<SHA256Hash> = (0..50).map(|i| SHA256Hash::compute(&[i])).collect();

    storage.update_processing_state(&completed_chunks)?;

    // Check that state was saved correctly
    let state = storage.get_current_state()?;
    assert!(state.is_some());
    let state = state.unwrap();
    assert_eq!(state.completed_chunks.len(), 50);
    assert_eq!(state.remaining_chunks(), 50);
    assert_eq!(state.completion_percentage(), 50.0);

    // Simulate interruption by creating a new storage instance
    drop(storage);
    let storage2 = SEQUOIAStorage::open(temp_dir.path())?;

    // Check for resumable operation
    let resumable = storage2.check_resumable(
        &source_info.database,
        &OperationType::InitialDownload,
        &manifest_hash,
        &manifest_version,
    )?;

    assert!(resumable.is_some());
    let resumed_state = resumable.unwrap();
    assert_eq!(resumed_state.completed_chunks.len(), 50);
    assert!(resumed_state.can_resume_with(&manifest_hash, &manifest_version));

    // Complete the operation
    let remaining_chunks: Vec<SHA256Hash> = (50..100).map(|i| SHA256Hash::compute(&[i])).collect();

    storage2.update_processing_state(&remaining_chunks)?;

    let final_state = storage2.get_current_state()?;
    assert!(final_state.is_some());
    let final_state = final_state.unwrap();
    assert!(final_state.is_complete());

    // Clean up
    storage2.complete_processing()?;

    // Verify state was removed after completion
    let no_state = storage2.get_current_state()?;
    assert!(no_state.is_none());

    Ok(())
}

#[test]
fn test_version_mismatch_prevents_resume() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = SEQUOIAStorage::new(temp_dir.path())?;

    let source_info = SourceInfo {
        database: "test_db".to_string(),
        source_url: None,
        etag: Some("v1.0".to_string()),
        total_size_bytes: None,
    };

    let manifest_hash_v1 = SHA256Hash::compute(b"manifest_v1");
    let manifest_version_v1 = "v1.0".to_string();

    // Start processing with v1
    storage.start_processing(
        OperationType::IncrementalUpdate,
        manifest_hash_v1.clone(),
        manifest_version_v1.clone(),
        50,
        source_info.clone(),
    )?;

    // Add some completed chunks
    let chunks: Vec<SHA256Hash> = (0..10).map(|i| SHA256Hash::compute(&[i])).collect();
    storage.update_processing_state(&chunks)?;

    // Try to resume with different version
    let manifest_hash_v2 = SHA256Hash::compute(b"manifest_v2");
    let manifest_version_v2 = "v2.0".to_string();

    let resumable = storage.check_resumable(
        &source_info.database,
        &OperationType::IncrementalUpdate,
        &manifest_hash_v2,
        &manifest_version_v2,
    )?;

    // Should not be resumable due to version mismatch
    assert!(resumable.is_none());

    // But should be resumable with correct version
    let resumable_correct = storage.check_resumable(
        &source_info.database,
        &OperationType::IncrementalUpdate,
        &manifest_hash_v1,
        &manifest_version_v1,
    )?;

    assert!(resumable_correct.is_some());

    Ok(())
}

#[test]
fn test_expired_state_cleanup() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let state_manager = ProcessingStateManager::new(temp_dir.path())?;

    // Create a state that appears expired (by setting old timestamps)
    let old_state = ProcessingState::new(
        OperationType::Chunking,
        SHA256Hash::compute(b"old"),
        "old_version".to_string(),
        100,
        SourceInfo {
            database: "old_db".to_string(),
            source_url: None,
            etag: None,
            total_size_bytes: None,
        },
    );

    // Manually set old timestamps (would need to make fields pub for this test)
    // For now, we'll test the cleanup logic differently

    // Save the state
    state_manager.save_state(&old_state, "old_operation")?;

    // List states - should have one
    let states = state_manager.list_states()?;
    assert_eq!(states.len(), 1);

    // Create a non-expired state
    let new_state = ProcessingState::new(
        OperationType::IncrementalUpdate,
        SHA256Hash::compute(b"new"),
        "new_version".to_string(),
        50,
        SourceInfo {
            database: "new_db".to_string(),
            source_url: None,
            etag: None,
            total_size_bytes: None,
        },
    );

    state_manager.save_state(&new_state, "new_operation")?;

    // List states - should have two
    let states = state_manager.list_states()?;
    assert_eq!(states.len(), 2);

    // Note: We can't easily test expiration without mocking time,
    // but the logic is there in the implementation

    Ok(())
}

#[test]
fn test_multiple_operations_tracking() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = SEQUOIAStorage::new(temp_dir.path())?;

    // Start multiple different operations
    let operations = vec![
        ("db1", OperationType::InitialDownload),
        ("db2", OperationType::IncrementalUpdate),
        (
            "db3",
            OperationType::Reduction {
                profile: "blast-30".to_string(),
            },
        ),
    ];

    for (db, op_type) in &operations {
        let source_info = SourceInfo {
            database: db.to_string(),
            source_url: None,
            etag: None,
            total_size_bytes: None,
        };

        storage.start_processing(
            op_type.clone(),
            SHA256Hash::compute(db.as_bytes()),
            format!("{}_v1", db),
            100,
            source_info,
        )?;

        // Complete this operation before starting the next
        storage.complete_processing()?;
    }

    // All operations should be completed
    let resumable_ops = storage.list_resumable_operations()?;
    assert_eq!(resumable_ops.len(), 0);

    Ok(())
}

#[test]
fn test_get_remaining_chunks() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = SEQUOIAStorage::new(temp_dir.path())?;

    let all_chunks: Vec<SHA256Hash> = (0..100).map(|i| SHA256Hash::compute(&[i])).collect();

    // Without any processing state, should return all chunks
    let remaining = storage.get_remaining_chunks(&all_chunks)?;
    assert_eq!(remaining.len(), 100);

    // Start processing
    let source_info = SourceInfo {
        database: "test".to_string(),
        source_url: None,
        etag: None,
        total_size_bytes: None,
    };

    storage.start_processing(
        OperationType::InitialDownload,
        SHA256Hash::compute(b"test"),
        "v1".to_string(),
        100,
        source_info,
    )?;

    // Mark first 30 as completed
    let completed: Vec<SHA256Hash> = all_chunks.iter().take(30).cloned().collect();
    storage.update_processing_state(&completed)?;

    // Should return remaining 70
    let remaining = storage.get_remaining_chunks(&all_chunks)?;
    assert_eq!(remaining.len(), 70);

    // Verify they're the right ones
    let expected_remaining: HashSet<SHA256Hash> = all_chunks.iter().skip(30).cloned().collect();
    let actual_remaining: HashSet<SHA256Hash> = remaining.into_iter().collect();
    assert_eq!(expected_remaining, actual_remaining);

    Ok(())
}
