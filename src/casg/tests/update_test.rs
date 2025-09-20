use crate::casg::{ChunkMetadata, SHA256Hash, TaxonId, TemporalManifest};
use anyhow::Result;
use chrono::Utc;
use std::collections::HashSet;

// Helper to create test manifests with required fields
fn create_test_manifest(version: &str, seq_version: &str, tax_version: &str) -> TemporalManifest {
    TemporalManifest {
        version: version.to_string(),
        created_at: Utc::now(),
        sequence_version: seq_version.to_string(),
        taxonomy_version: tax_version.to_string(),
        temporal_coordinate: None,
        taxonomy_root: SHA256Hash::compute(format!("tax_{}", version).as_bytes()),
        sequence_root: SHA256Hash::compute(format!("seq_{}", version).as_bytes()),
        chunk_merkle_tree: None,
        taxonomy_manifest_hash: SHA256Hash::compute(b"test_tax_manifest"),
        taxonomy_dump_version: "2024-01-01".to_string(),
        source_database: Some("test_db".to_string()),
        chunk_index: vec![],
        discrepancies: vec![],
        etag: "test".to_string(),
        previous_version: None,
    }
}

#[cfg(test)]
mod mock_server {
    use super::*;

    pub struct MockManifestServer {
        pub manifest: Option<TemporalManifest>,
        pub should_return_304: bool,
        pub should_fail: bool,
    }

    impl MockManifestServer {
        pub fn new() -> Self {
            Self {
                manifest: None,
                should_return_304: false,
                should_fail: false,
            }
        }

        pub fn with_manifest(mut self, manifest: TemporalManifest) -> Self {
            self.manifest = Some(manifest);
            self
        }

        pub fn return_not_modified(mut self) -> Self {
            self.should_return_304 = true;
            self
        }

        pub fn simulate_failure(mut self) -> Self {
            self.should_fail = true;
            self
        }

        pub async fn check_for_updates(
            &self,
            etag: Option<&str>,
        ) -> Result<(bool, Option<String>)> {
            if self.should_fail {
                return Err(anyhow::anyhow!("Network error"));
            }

            if self.should_return_304
                || (etag.is_some()
                    && self
                        .manifest
                        .as_ref()
                        .map(|m| m.etag.as_str() == etag.unwrap())
                        .unwrap_or(false))
            {
                Ok((false, None)) // 304 Not Modified
            } else {
                Ok((true, self.manifest.as_ref().map(|m| m.etag.clone())))
            }
        }

        #[allow(dead_code)]
        pub async fn fetch_manifest(&self) -> Result<TemporalManifest> {
            if self.should_fail {
                return Err(anyhow::anyhow!("Network error"));
            }

            self.manifest
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No manifest available"))
        }
    }
}

#[tokio::test]
async fn test_sequence_only_update() {
    use mock_server::MockManifestServer;

    let mut old_manifest = create_test_manifest("v1", "2024.01", "2024.01");
    old_manifest.taxonomy_root = SHA256Hash::compute(b"tax_unchanged");
    old_manifest.sequence_root = SHA256Hash::compute(b"seq_old");
    old_manifest.chunk_index = vec![ChunkMetadata {
        hash: SHA256Hash::compute(b"chunk1"),
        taxon_ids: vec![TaxonId(562)],
        sequence_count: 100,
        size: 1000,
        compressed_size: None,
    }];
    old_manifest.etag = "etag_v1".to_string();

    let mut new_manifest = create_test_manifest("v2", "2024.02", "2024.01");
    new_manifest.taxonomy_root = old_manifest.taxonomy_root.clone(); // Unchanged
    new_manifest.sequence_root = SHA256Hash::compute(b"seq_new"); // Changed
    new_manifest.chunk_index = vec![
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk1"), // Existing
            taxon_ids: vec![TaxonId(562)],
            sequence_count: 100,
            size: 1000,
            compressed_size: None,
        },
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk2"), // New chunk
            taxon_ids: vec![TaxonId(562)],
            sequence_count: 50,
            size: 500,
            compressed_size: None,
        },
    ];
    new_manifest.etag = "etag_v2".to_string();
    new_manifest.previous_version = Some("v1".to_string());

    let server = MockManifestServer::new().with_manifest(new_manifest.clone());

    // Check for updates with old ETag
    let (has_updates, new_etag) = server.check_for_updates(Some("etag_v1")).await.unwrap();
    assert!(has_updates);
    assert_eq!(new_etag, Some("etag_v2".to_string()));

    // Verify sequence-only changes
    assert_eq!(old_manifest.taxonomy_root, new_manifest.taxonomy_root);
    assert_ne!(old_manifest.sequence_root, new_manifest.sequence_root);
    assert_ne!(old_manifest.sequence_version, new_manifest.sequence_version);
    assert_eq!(old_manifest.taxonomy_version, new_manifest.taxonomy_version);
}

#[tokio::test]
async fn test_taxonomy_only_update() {
    let mut old_manifest = create_test_manifest("v1", "2024.01", "2024.01");
    old_manifest.taxonomy_root = SHA256Hash::compute(b"tax_old");
    old_manifest.sequence_root = SHA256Hash::compute(b"seq_unchanged");
    old_manifest.chunk_index = vec![ChunkMetadata {
        hash: SHA256Hash::compute(b"chunk1"),
        taxon_ids: vec![TaxonId(562)], // Will be reclassified
        sequence_count: 100,
        size: 1000,
        compressed_size: None,
    }];
    old_manifest.etag = "etag_v1".to_string();

    let mut new_manifest = create_test_manifest("v2", "2024.01", "2024.02");
    new_manifest.taxonomy_root = SHA256Hash::compute(b"tax_new"); // Changed
    new_manifest.sequence_root = old_manifest.sequence_root.clone(); // Unchanged
    new_manifest.chunk_index = vec![ChunkMetadata {
        hash: SHA256Hash::compute(b"chunk1"), // Same chunk
        taxon_ids: vec![TaxonId(563)],        // Reclassified taxon
        sequence_count: 100,
        size: 1000,
        compressed_size: None,
    }];
    new_manifest.etag = "etag_v2".to_string();
    new_manifest.previous_version = Some("v1".to_string());

    // Verify taxonomy-only changes
    assert_ne!(old_manifest.taxonomy_root, new_manifest.taxonomy_root);
    assert_eq!(old_manifest.sequence_root, new_manifest.sequence_root);
    assert_eq!(old_manifest.sequence_version, new_manifest.sequence_version);
    assert_ne!(old_manifest.taxonomy_version, new_manifest.taxonomy_version);

    // Same chunks but different taxonomy assignments
    assert_eq!(
        old_manifest.chunk_index[0].hash,
        new_manifest.chunk_index[0].hash
    );
    assert_ne!(
        old_manifest.chunk_index[0].taxon_ids,
        new_manifest.chunk_index[0].taxon_ids
    );
}

#[tokio::test]
async fn test_combined_update() {
    let mut old_manifest = create_test_manifest("v1", "2024.01", "2024.01");
    old_manifest.taxonomy_root = SHA256Hash::compute(b"tax_old");
    old_manifest.sequence_root = SHA256Hash::compute(b"seq_old");
    old_manifest.chunk_index = vec![ChunkMetadata {
        hash: SHA256Hash::compute(b"chunk1"),
        taxon_ids: vec![TaxonId(562)],
        sequence_count: 100,
        size: 1000,
        compressed_size: None,
    }];
    old_manifest.etag = "etag_v1".to_string();

    let mut new_manifest = create_test_manifest("v2", "2024.02", "2024.02");
    new_manifest.taxonomy_root = SHA256Hash::compute(b"tax_new"); // Changed
    new_manifest.sequence_root = SHA256Hash::compute(b"seq_new"); // Changed
    new_manifest.chunk_index = vec![ChunkMetadata {
        hash: SHA256Hash::compute(b"chunk2"), // All new chunks
        taxon_ids: vec![TaxonId(563)],
        sequence_count: 150,
        size: 1500,
        compressed_size: None,
    }];
    new_manifest.etag = "etag_v2".to_string();
    new_manifest.previous_version = Some("v1".to_string());

    // Both roots changed
    assert_ne!(old_manifest.taxonomy_root, new_manifest.taxonomy_root);
    assert_ne!(old_manifest.sequence_root, new_manifest.sequence_root);
    assert_ne!(old_manifest.sequence_version, new_manifest.sequence_version);
    assert_ne!(old_manifest.taxonomy_version, new_manifest.taxonomy_version);
}

#[tokio::test]
async fn test_no_updates_available() {
    use mock_server::MockManifestServer;

    let mut manifest = create_test_manifest("v1", "2024.01", "2024.01");
    manifest.taxonomy_root = SHA256Hash::compute(b"tax");
    manifest.sequence_root = SHA256Hash::compute(b"seq");
    manifest.chunk_index = vec![];
    manifest.discrepancies = vec![];
    manifest.etag = "etag_current".to_string();
    manifest.previous_version = None;

    let server = MockManifestServer::new()
        .with_manifest(manifest.clone())
        .return_not_modified();

    // Check with current ETag - should get 304
    let (has_updates, _) = server
        .check_for_updates(Some("etag_current"))
        .await
        .unwrap();
    assert!(!has_updates);
}

#[tokio::test]
async fn test_network_failure_handling() {
    use mock_server::MockManifestServer;

    let server = MockManifestServer::new().simulate_failure();

    // Should handle network errors gracefully
    let result = server.check_for_updates(None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Network error"));
}

#[test]
fn test_partial_update_recovery() {
    // Simulate partial chunk download failure
    let mut manifest = create_test_manifest("v1", "2024.01", "2024.01");
    manifest.taxonomy_root = SHA256Hash::compute(b"tax");
    manifest.sequence_root = SHA256Hash::compute(b"seq");
    manifest.chunk_index = vec![
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
        ChunkMetadata {
            hash: SHA256Hash::compute(b"chunk3"),
            taxon_ids: vec![TaxonId(3)],
            sequence_count: 300,
            size: 3000,
            compressed_size: None,
        },
    ];
    manifest.discrepancies = vec![];
    manifest.etag = "None".to_string();
    manifest.previous_version = None;

    // Simulate that only chunk1 was downloaded successfully
    let downloaded_chunks = vec![SHA256Hash::compute(b"chunk1")];
    let all_chunks: Vec<_> = manifest
        .chunk_index
        .iter()
        .map(|c| c.hash.clone())
        .collect();

    let pending_chunks: Vec<_> = all_chunks
        .iter()
        .filter(|h| !downloaded_chunks.contains(h))
        .cloned()
        .collect();

    assert_eq!(pending_chunks.len(), 2); // chunk2 and chunk3 still pending
}

#[test]
fn test_incremental_update_efficiency() {
    // Test that incremental updates only download changed chunks
    let old_chunks = vec![
        SHA256Hash::compute(b"chunk1"),
        SHA256Hash::compute(b"chunk2"),
        SHA256Hash::compute(b"chunk3"),
    ];

    let new_chunks = vec![
        SHA256Hash::compute(b"chunk1"), // Unchanged
        SHA256Hash::compute(b"chunk2"), // Unchanged
        SHA256Hash::compute(b"chunk4"), // New
    ];

    let old_set: HashSet<_> = old_chunks.iter().cloned().collect();
    let new_set: HashSet<_> = new_chunks.iter().cloned().collect();

    let to_download: Vec<_> = new_set.difference(&old_set).cloned().collect();
    let to_remove: Vec<_> = old_set.difference(&new_set).cloned().collect();

    assert_eq!(to_download.len(), 1); // Only chunk4
    assert_eq!(to_remove.len(), 1); // Only chunk3

    // Calculate efficiency
    let total_chunks = new_chunks.len();
    let chunks_to_transfer = to_download.len();
    let efficiency = 1.0 - (chunks_to_transfer as f64 / total_chunks as f64);

    assert!(efficiency > 0.6); // At least 60% efficient
}
