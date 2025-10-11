use tempfile::TempDir;
use chrono::Utc;
use std::sync::Arc;
use talaria_sequoia::{
    SequoiaStorage,
    bi_temporal::BiTemporalDatabase,
    temporal::TemporalIndex,
    types::{ChunkMetadata, SHA256Hash, TaxonId},
};
use anyhow::Result;

#[test]
fn test_bi_temporal_basic_query() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = Arc::new(SequoiaStorage::new(temp_dir.path())?);
    
    // Create bi-temporal database
    let mut db = BiTemporalDatabase::new(storage)?;
    
    // Query at current time (should work even with empty database)
    let now = Utc::now();
    let snapshot = db.query_at(now, now)?;
    
    assert_eq!(snapshot.sequence_count(), 0);
    assert_eq!(snapshot.chunks().len(), 0);
    
    Ok(())
}

#[test]
fn test_bi_temporal_different_times() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = Arc::new(SequoiaStorage::new(temp_dir.path())?);
    
    // Create bi-temporal database
    let mut db = BiTemporalDatabase::new(storage.clone())?;

    // Create a temporal index and add some test data
    let rocksdb = storage.sequence_storage.get_rocksdb();
    let mut temporal_index = TemporalIndex::new(temp_dir.path(), rocksdb)?;
    
    // Add a sequence version from January 2024
    let jan_2024 = chrono::DateTime::parse_from_rfc3339("2024-01-15T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    
    // Add a taxonomy version from March 2024  
    let mar_2024 = chrono::DateTime::parse_from_rfc3339("2024-03-15T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    
    // Query with January sequences and March taxonomy
    let snapshot = db.query_at(jan_2024, mar_2024)?;
    
    // Even with no data, this should return a valid snapshot
    assert_eq!(snapshot.sequence_count(), 0);
    
    Ok(())
}

#[test]
fn test_bi_temporal_diff() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = Arc::new(SequoiaStorage::new(temp_dir.path())?);
    
    let mut db = BiTemporalDatabase::new(storage)?;
    
    let time1 = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    
    let time2 = chrono::DateTime::parse_from_rfc3339("2024-06-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    
    // Create coordinates
    use talaria_sequoia::types::BiTemporalCoordinate;
    let coord1 = BiTemporalCoordinate {
        sequence_time: time1,
        taxonomy_time: time1,
    };
    
    let coord2 = BiTemporalCoordinate {
        sequence_time: time2,
        taxonomy_time: time2,
    };
    
    // Compute diff between two time points
    let diff = db.diff(coord1.clone(), coord2.clone())?;
    
    assert_eq!(diff.coord1.sequence_time, coord1.sequence_time);
    assert_eq!(diff.coord2.sequence_time, coord2.sequence_time);
    assert_eq!(diff.sequences_added, 0);
    assert_eq!(diff.sequences_removed, 0);
    
    Ok(())
}

#[test]
fn test_available_coordinates() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = Arc::new(SequoiaStorage::new(temp_dir.path())?);
    
    let db = BiTemporalDatabase::new(storage)?;
    
    // Get available coordinates (should handle empty case)
    let coords = db.get_available_coordinates()?;
    
    // With no data, we expect empty or current time only
    assert!(coords.len() <= 1);
    
    Ok(())
}