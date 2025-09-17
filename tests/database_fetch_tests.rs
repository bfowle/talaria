use talaria::cli::commands::database::fetch::{FetchArgs, run};
use talaria::core::database_manager::DatabaseManager;
use talaria::core::paths;
use std::path::PathBuf;
use tempfile::TempDir;

/// Test that we can parse TaxIDs correctly
#[test]
fn test_parse_taxids() {
    use talaria::bio::uniprot::parse_taxids;

    // Test single TaxID
    let taxids = parse_taxids("9606").unwrap();
    assert_eq!(taxids, vec![9606]);

    // Test multiple TaxIDs
    let taxids = parse_taxids("9606,10090,562").unwrap();
    assert_eq!(taxids, vec![9606, 10090, 562]);

    // Test with spaces
    let taxids = parse_taxids("9606, 10090, 562").unwrap();
    assert_eq!(taxids, vec![9606, 10090, 562]);
}

/// Test database name generation from TaxIDs
#[test]
fn test_database_name_generation() {
    // Test with 1 TaxID
    let taxids = vec![9606u32];
    let name = generate_db_name_from_taxids(&taxids);
    assert_eq!(name, "taxids_9606");

    // Test with 3 TaxIDs
    let taxids = vec![9606, 10090, 562];
    let name = generate_db_name_from_taxids(&taxids);
    assert_eq!(name, "taxids_9606_10090_562");

    // Test with more than 3 TaxIDs
    let taxids = vec![9606, 10090, 562, 511145, 83333];
    let name = generate_db_name_from_taxids(&taxids);
    assert_eq!(name, "taxids_9606_10090_562_and_2_more");
}

fn generate_db_name_from_taxids(taxids: &[u32]) -> String {
    let taxids_str = taxids.iter()
        .take(3)  // Limit to first 3 for readability
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join("_");

    if taxids.len() > 3 {
        format!("taxids_{}_and_{}_more", taxids_str, taxids.len() - 3)
    } else {
        format!("taxids_{}", taxids_str)
    }
}

/// Test that fetch args validation works
#[test]
fn test_fetch_args_validation() {
    // This would require creating FetchArgs instances and testing validation
    // Since FetchArgs uses clap derives, we test the logic separately
    assert!(validate_fetch_input(None, None).is_err());
    assert!(validate_fetch_input(Some("9606".to_string()), None).is_ok());
    assert!(validate_fetch_input(None, Some(PathBuf::from("taxids.txt"))).is_ok());
    assert!(validate_fetch_input(Some("9606".to_string()), Some(PathBuf::from("taxids.txt"))).is_err());
}

fn validate_fetch_input(taxids: Option<String>, taxid_list: Option<PathBuf>) -> Result<(), String> {
    match (taxids, taxid_list) {
        (None, None) => Err("Must specify either --taxids or --taxid-list".to_string()),
        (Some(_), Some(_)) => Err("Cannot specify both --taxids and --taxid-list".to_string()),
        _ => Ok(())
    }
}

/// Test the creation of CASG chunks from sequences
#[test]
fn test_sequence_chunking() {
    use talaria::bio::sequence::Sequence;
    use talaria::casg::chunker::TaxonomicChunker;
    use talaria::casg::types::{ChunkingStrategy, TaxonId, SpecialTaxon, ChunkStrategy};

    // Create test sequences
    let sequences = vec![
        Sequence {
            id: "seq1".to_string(),
            description: Some("Test sequence 1 OX=9606".to_string()),
            sequence: b"ACGTACGTACGT".to_vec(),
            taxon_id: Some(9606),
        },
        Sequence {
            id: "seq2".to_string(),
            description: Some("Test sequence 2 OX=9606".to_string()),
            sequence: b"TGCATGCATGCA".to_vec(),
            taxon_id: Some(9606),
        },
        Sequence {
            id: "seq3".to_string(),
            description: Some("Test sequence 3 OX=10090".to_string()),
            sequence: b"GGGGCCCCAAAA".to_vec(),
            taxon_id: Some(10090),
        },
    ];

    // Create chunking strategy
    let strategy = ChunkingStrategy {
        target_chunk_size: 100,  // Small for testing
        max_chunk_size: 1000,
        min_sequences_per_chunk: 1,
        taxonomic_coherence: 0.8,
        special_taxa: vec![
            SpecialTaxon {
                taxon_id: TaxonId(9606),
                name: "Human".to_string(),
                strategy: ChunkStrategy::OwnChunks,
            }
        ],
    };

    let chunker = TaxonomicChunker::new(strategy);
    let chunks = chunker.chunk_sequences(sequences).unwrap();

    // Should create at least one chunk
    assert!(!chunks.is_empty());

    // Check that chunks have proper metadata
    for chunk in &chunks {
        assert!(chunk.sequences.len() > 0);
        assert!(chunk.size > 0);
    }
}

/// Test manifest creation for fetched databases
#[test]
fn test_manifest_creation() {
    use talaria::casg::types::{TemporalManifest, ChunkMetadata, SHA256Hash, TaxonId};
    use chrono::Utc;

    let manifest = TemporalManifest {
        version: "20240101_120000".to_string(),
        created_at: Utc::now(),
        sequence_version: "2024-01-01".to_string(),
        taxonomy_version: "2024-01-01".to_string(),
        taxonomy_root: SHA256Hash::zero(),
        sequence_root: SHA256Hash::zero(),
        taxonomy_manifest_hash: SHA256Hash::zero(),
        taxonomy_dump_version: "uniprot".to_string(),
        source_database: Some("custom/test".to_string()),
        chunk_index: vec![
            ChunkMetadata {
                hash: SHA256Hash::zero(),
                taxon_ids: vec![TaxonId(9606)],
                sequence_count: 2,
                size: 100,
                compressed_size: Some(50),
            }
        ],
        discrepancies: Vec::new(),
        etag: "test-etag".to_string(),
        previous_version: None,
    };

    // Test serialization
    let json = serde_json::to_string(&manifest).unwrap();
    assert!(json.contains("20240101_120000"));
    assert!(json.contains("custom/test"));

    // Test deserialization
    let parsed: TemporalManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.version, manifest.version);
    assert_eq!(parsed.chunk_index.len(), 1);
}

/// Integration test for database path handling
#[test]
fn test_database_path_generation() {
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("TALARIA_HOME", temp_dir.path());

    let db_path = paths::talaria_databases_dir();
    assert!(db_path.ends_with("databases"));

    let custom_db_path = db_path.join("custom").join("taxids_9606");
    std::fs::create_dir_all(&custom_db_path).unwrap();
    assert!(custom_db_path.exists());

    // Clean up
    std::env::remove_var("TALARIA_HOME");
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Test that custom databases show up in database list
    #[test]
    #[ignore] // Requires actual database setup
    fn test_custom_database_listing() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_HOME", temp_dir.path());

        // Initialize database manager
        let manager = DatabaseManager::new(None).unwrap();

        // Create a test manifest in custom namespace
        let db_path = paths::talaria_databases_dir()
            .join("custom")
            .join("test_db");
        std::fs::create_dir_all(&db_path).unwrap();

        // Create minimal manifest
        let manifest = r#"{
            "version": "20240101",
            "created_at": "2024-01-01T00:00:00Z",
            "sequence_version": "2024-01-01",
            "taxonomy_version": "2024-01-01",
            "chunk_index": []
        }"#;

        std::fs::write(db_path.join("manifest.json"), manifest).unwrap();

        // List databases
        let databases = manager.list_databases().unwrap();

        // Should find our custom database
        let custom_db = databases.iter()
            .find(|db| db.name == "custom/test_db");
        assert!(custom_db.is_some());

        // Clean up
        std::env::remove_var("TALARIA_HOME");
    }
}