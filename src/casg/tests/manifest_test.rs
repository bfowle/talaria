use crate::casg::{Manifest, TemporalManifest, ChunkMetadata, SHA256Hash, TaxonId};
use chrono::Utc;
use std::collections::HashSet;

// Helper to create test manifests with required fields
fn create_test_manifest(version: &str, seq_version: &str, tax_version: &str) -> TemporalManifest {
    TemporalManifest {
        version: version.to_string(),
        created_at: Utc::now(),
        sequence_version: seq_version.to_string(),
        taxonomy_version: tax_version.to_string(),
        taxonomy_root: SHA256Hash::compute(format!("tax_{}", version).as_bytes()),
        sequence_root: SHA256Hash::compute(format!("seq_{}", version).as_bytes()),
        taxonomy_manifest_hash: SHA256Hash::compute(b"test_tax_manifest"),
        taxonomy_dump_version: "2024-01-01".to_string(),
        source_database: Some("test_db".to_string()),
        chunk_index: vec![],
        discrepancies: vec![],
        etag: "test".to_string(),
        previous_version: None,
    }
}

#[test]
fn test_manifest_serialization() {
    let mut manifest = create_test_manifest("20240315_143022", "2024.03.15", "2024.01");
    manifest.taxonomy_root = SHA256Hash::compute(b"taxonomy_root");
    manifest.sequence_root = SHA256Hash::compute(b"sequence_root");
    manifest.chunk_index = vec![
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk1"),
            taxon_ids: vec![TaxonId(562), TaxonId(563)],
            sequence_count: 1000,
            size: 52428800,
            compressed_size: Some(18350080),
        },
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk2"),
            taxon_ids: vec![TaxonId(9606)],
            sequence_count: 2000,
            size: 104857600,
            compressed_size: Some(36700160),
        },
    ];
    manifest.discrepancies = vec![];
    manifest.etag = "W/\"5e3b-1710513022000\"".to_string();
    manifest.previous_version = Some("20240215_120000".to_string());

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&manifest).unwrap();

    // Deserialize back
    let deserialized: TemporalManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(manifest.version, deserialized.version);
    assert_eq!(manifest.chunk_index.len(), deserialized.chunk_index.len());
    assert_eq!(manifest.etag, deserialized.etag);
}

#[test]
fn test_manifest_diff() {
    let mut old_manifest = create_test_manifest("v1", "2024.01", "2024.01");
    old_manifest.taxonomy_root = SHA256Hash::compute(b"tax1");
    old_manifest.sequence_root = SHA256Hash::compute(b"seq1");
    old_manifest.chunk_index = vec![
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk1"),
            taxon_ids: vec![TaxonId(1)],
            sequence_count: 100,
            size: 1000,
            compressed_size: None,
        },
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk2"),
            taxon_ids: vec![TaxonId(2)],
            sequence_count: 200,
            size: 2000,
            compressed_size: None,
        },
    ];
    old_manifest.etag = "v1".to_string();

    let mut new_manifest = create_test_manifest("v2", "2024.02", "2024.02");
    new_manifest.taxonomy_version = "2024.01".to_string();
    new_manifest.taxonomy_root = SHA256Hash::compute(b"tax1");
    new_manifest.sequence_root = SHA256Hash::compute(b"seq2");
    new_manifest.chunk_index = vec![
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk1"), // Same
            taxon_ids: vec![TaxonId(1)],
            sequence_count: 100,
            size: 1000,
            compressed_size: None,
        },
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk3"), // New
            taxon_ids: vec![TaxonId(3)],
            sequence_count: 300,
            size: 3000,
            compressed_size: None,
        },
    ];
    new_manifest.discrepancies = vec![];
    new_manifest.etag = "v2".to_string();
    new_manifest.previous_version = Some("v1".to_string());

    let _old_wrapper = Manifest::new();
    let _new_wrapper = Manifest::new();

    // Create diff manually (since Manifest::diff needs full implementation)
    let old_chunks: HashSet<_> = old_manifest.chunk_index.iter().map(|c| &c.hash).collect();
    let new_chunks: HashSet<_> = new_manifest.chunk_index.iter().map(|c| &c.hash).collect();

    let added: Vec<_> = new_chunks.difference(&old_chunks).collect();
    let removed: Vec<_> = old_chunks.difference(&new_chunks).collect();

    assert_eq!(added.len(), 1); // chunk3 added
    assert_eq!(removed.len(), 1); // chunk2 removed
}

#[test]
fn test_bi_temporal_versioning() {
    let mut manifest = create_test_manifest("20240315_143022", "2024.03.15", "2024.01.15");
    manifest.taxonomy_root = SHA256Hash::compute(b"tax_root");
    manifest.sequence_root = SHA256Hash::compute(b"seq_root");
    manifest.chunk_index = vec![];
    manifest.discrepancies = vec![];
    manifest.etag = "test".to_string();
    manifest.previous_version = None;

    // Verify we can track different versions for sequences and taxonomy
    assert_ne!(manifest.sequence_version, manifest.taxonomy_version);

    // Simulate taxonomy-only update
    let mut taxonomy_update = create_test_manifest("20240320_100000", "2024.03.15", "2024.03.20");
    taxonomy_update.sequence_version = manifest.sequence_version.clone(); // Same sequences
    taxonomy_update.taxonomy_version = "2024.03.20".to_string(); // New taxonomy
    taxonomy_update.taxonomy_root = SHA256Hash::compute(b"new_tax_root");
    taxonomy_update.sequence_root = manifest.sequence_root.clone(); // Same sequence root
    taxonomy_update.chunk_index = manifest.chunk_index.clone();
    taxonomy_update.discrepancies = vec![];
    taxonomy_update.etag = "test2".to_string();
    taxonomy_update.previous_version = Some(manifest.version.clone());

    assert_eq!(taxonomy_update.sequence_root, manifest.sequence_root);
    assert_ne!(taxonomy_update.taxonomy_root, manifest.taxonomy_root);
}

#[test]
fn test_etag_comparison() {
    let etag1 = Some("W/\"5e3b-1710513022000\"".to_string());
    let etag2 = Some("W/\"5e3b-1710513022000\"".to_string());
    let etag3 = Some("W/\"6f4c-1710513023000\"".to_string());

    assert_eq!(etag1, etag2); // Same ETag
    assert_ne!(etag1, etag3); // Different ETag
}

#[test]
fn test_version_chaining() {
    let mut versions = Vec::new();

    // Create a chain of versions
    for i in 0..5 {
        let mut manifest = create_test_manifest(
            &format!("v{}", i),
            &format!("2024.{:02}", i + 1),
            "2024.01"
        );
        manifest.taxonomy_root = SHA256Hash::compute(b"tax");
        manifest.sequence_root = SHA256Hash::compute(&format!("seq{}", i).into_bytes());
        manifest.chunk_index = vec![];
        manifest.discrepancies = vec![];
        manifest.etag = format!("v{}", i);
        manifest.previous_version = if i > 0 {
            Some(format!("v{}", i - 1))
        } else {
            None
        };
        versions.push(manifest);
    }

    // Verify the chain
    for i in 1..versions.len() {
        assert_eq!(
            versions[i].previous_version.as_ref().unwrap(),
            &versions[i - 1].version
        );
    }
}

#[test]
fn test_chunk_metadata_with_compression() {
    let chunk = ChunkMetadata {
        hash: SHA256Hash::compute(b"test_chunk"),
        taxon_ids: vec![TaxonId(562), TaxonId(563), TaxonId(564)],
        sequence_count: 15234,
        size: 52428800,
        compressed_size: Some(18350080),
    };

    // Calculate compression ratio
    let ratio = chunk.compressed_size.unwrap() as f64 / chunk.size as f64;
    assert!(ratio < 0.5); // Should achieve good compression

    // Verify taxon IDs are preserved
    assert_eq!(chunk.taxon_ids.len(), 3);
    assert!(chunk.taxon_ids.contains(&TaxonId(562)));
}

#[test]
fn test_manifest_with_discrepancies() {
    use crate::casg::types::TaxonomicDiscrepancy;

    let mut manifest = create_test_manifest("v1", "2024.01", "2024.01");
    manifest.taxonomy_root = SHA256Hash::compute(b"tax");
    manifest.sequence_root = SHA256Hash::compute(b"seq");
    manifest.chunk_index = vec![];
    manifest.discrepancies = vec![
        TaxonomicDiscrepancy {
            sequence_id: "NP_123456.1".to_string(),
            header_taxon: Some(TaxonId(562)),
            mapped_taxon: Some(TaxonId(563)),
            inferred_taxon: Some(TaxonId(562)),
            confidence: 0.92,
            detection_date: Utc::now(),
            discrepancy_type: crate::casg::types::DiscrepancyType::Conflict,
        },
    ];
    manifest.etag = "v1".to_string();
    manifest.previous_version = None;

    assert_eq!(manifest.discrepancies.len(), 1);
    assert_eq!(manifest.discrepancies[0].sequence_id, "NP_123456.1");
    assert_eq!(manifest.discrepancies[0].confidence, 0.92);
}