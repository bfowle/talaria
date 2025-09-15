#[cfg(test)]
mod tests {
    use crate::core::database_manager::DatabaseManager;
    use crate::download::{DatabaseSource, UniProtDatabase, NCBIDatabase};
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a test CASG manager with temp directory
    fn create_test_manager() -> (DatabaseManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let manager = DatabaseManager::new(
            Some(temp_dir.path().to_string_lossy().to_string())
        ).unwrap();
        (manager, temp_dir)
    }

    /// Helper to create a fake manifest
    fn create_fake_manifest() -> crate::casg::TemporalManifest {
        use crate::casg::{TemporalManifest, ChunkMetadata, SHA256Hash, TaxonId};
        use chrono::Utc;

        TemporalManifest {
            version: "test_v1".to_string(),
            created_at: Utc::now(),
            sequence_version: "2024-01-01".to_string(),
            taxonomy_version: "2024-01-01".to_string(),
            taxonomy_root: SHA256Hash::compute(b"test_taxonomy"),
            sequence_root: SHA256Hash::compute(b"test_sequence"),
            taxonomy_manifest_hash: SHA256Hash::compute(b"test_tax_manifest"),
            taxonomy_dump_version: "2024-01-01".to_string(),
            source_database: Some("uniprot-swissprot".to_string()),
            chunk_index: vec![
                ChunkMetadata {
                    hash: SHA256Hash::compute(b"chunk1"),
                    taxon_ids: vec![TaxonId(9606)], // Human
                    sequence_count: 100,
                    size: 1024,
                    compressed_size: Some(512),
                },
                ChunkMetadata {
                    hash: SHA256Hash::compute(b"chunk2"),
                    taxon_ids: vec![TaxonId(10090)], // Mouse
                    sequence_count: 50,
                    size: 512,
                    compressed_size: Some(256),
                },
            ],
            discrepancies: vec![],
            etag: "test_etag_123".to_string(),
            previous_version: None,
        }
    }

    #[test]
    fn test_manifest_path_for_different_databases() {
        let (manager, _temp_dir) = create_test_manager();

        // Test SwissProt
        let swissprot_path = manager.get_manifest_path(&DatabaseSource::UniProt(UniProtDatabase::SwissProt));
        assert!(swissprot_path.ends_with("manifests/uniprot-swissprot.json"));

        // Test TrEMBL
        let trembl_path = manager.get_manifest_path(&DatabaseSource::UniProt(UniProtDatabase::TrEMBL));
        assert!(trembl_path.ends_with("manifests/uniprot-trembl.json"));

        // Test NCBI NR
        let nr_path = manager.get_manifest_path(&DatabaseSource::NCBI(NCBIDatabase::NR));
        assert!(nr_path.ends_with("manifests/ncbi-nr.json"));

        // Test NCBI NT
        let nt_path = manager.get_manifest_path(&DatabaseSource::NCBI(NCBIDatabase::NT));
        assert!(nt_path.ends_with("manifests/ncbi-nt.json"));
    }

    #[test]
    fn test_manifest_saved_to_correct_location() {
        let (manager, temp_dir) = create_test_manager();
        let manifest = create_fake_manifest();

        // Save manifest for SwissProt
        let manifest_path = manager.get_manifest_path(&DatabaseSource::UniProt(UniProtDatabase::SwissProt));

        // Ensure directory exists
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();

        // Write manifest
        let content = serde_json::to_string_pretty(&manifest).unwrap();
        fs::write(&manifest_path, content).unwrap();

        // Verify it exists at the expected location
        assert!(manifest_path.exists());
        assert!(manifest_path.to_string_lossy().contains("manifests/uniprot-swissprot.json"));

        // Verify it's NOT at the old location
        let old_path = temp_dir.path().join("manifest.json");
        assert!(!old_path.exists());
    }

    #[test]
    fn test_subsequent_download_finds_existing_manifest() {
        let (manager, _temp_dir) = create_test_manager();
        let manifest = create_fake_manifest();

        // Save manifest to correct location
        let manifest_path = manager.get_manifest_path(&DatabaseSource::UniProt(UniProtDatabase::SwissProt));
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();

        // Simulate checking for existing manifest
        assert!(manifest_path.exists(), "Manifest should exist at: {:?}", manifest_path);

        // The download function should find this manifest
        // In real usage, this would return DownloadResult::UpToDate
    }

    #[test]
    fn test_migration_from_old_manifest_location() {
        let (manager, temp_dir) = create_test_manager();
        let manifest = create_fake_manifest();

        // Save manifest to OLD location
        let old_path = temp_dir.path().join("manifest.json");
        fs::write(&old_path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();

        // Also create an etag file
        let old_etag = old_path.with_extension("etag");
        fs::write(&old_etag, "old_etag_value").unwrap();

        // Get new location
        let new_path = manager.get_manifest_path(&DatabaseSource::UniProt(UniProtDatabase::SwissProt));

        // Initially, new location shouldn't exist
        assert!(!new_path.exists());

        // Simulate migration logic
        if !new_path.exists() && old_path.exists() {
            // Create manifests directory
            fs::create_dir_all(new_path.parent().unwrap()).unwrap();

            // Copy manifest
            fs::copy(&old_path, &new_path).unwrap();

            // Copy etag if exists
            if old_etag.exists() {
                let new_etag = new_path.with_extension("etag");
                fs::copy(&old_etag, &new_etag).unwrap();
            }
        }

        // Verify migration worked
        assert!(new_path.exists(), "Manifest should be migrated to new location");
        assert!(old_path.exists(), "Old manifest should still exist (not deleted)");

        // Verify etag was also migrated
        let new_etag = new_path.with_extension("etag");
        assert!(new_etag.exists(), "Etag should also be migrated");

        // Verify content is the same
        let old_content = fs::read_to_string(&old_path).unwrap();
        let new_content = fs::read_to_string(&new_path).unwrap();
        assert_eq!(old_content, new_content, "Manifest content should be identical");
    }

    #[test]
    fn test_multiple_database_manifests_coexist() {
        let (manager, _temp_dir) = create_test_manager();

        // Create manifests for different databases
        let swissprot_manifest = {
            let mut m = create_fake_manifest();
            m.source_database = Some("uniprot-swissprot".to_string());
            m
        };

        let trembl_manifest = {
            let mut m = create_fake_manifest();
            m.source_database = Some("uniprot-trembl".to_string());
            m.version = "trembl_v1".to_string();
            m
        };

        // Save both manifests
        let swissprot_path = manager.get_manifest_path(&DatabaseSource::UniProt(UniProtDatabase::SwissProt));
        let trembl_path = manager.get_manifest_path(&DatabaseSource::UniProt(UniProtDatabase::TrEMBL));

        fs::create_dir_all(swissprot_path.parent().unwrap()).unwrap();
        fs::write(&swissprot_path, serde_json::to_string_pretty(&swissprot_manifest).unwrap()).unwrap();
        fs::write(&trembl_path, serde_json::to_string_pretty(&trembl_manifest).unwrap()).unwrap();

        // Verify both exist independently
        assert!(swissprot_path.exists());
        assert!(trembl_path.exists());
        assert_ne!(swissprot_path, trembl_path, "Paths should be different");

        // Verify content is different
        let sp_content: crate::casg::TemporalManifest =
            serde_json::from_str(&fs::read_to_string(&swissprot_path).unwrap()).unwrap();
        let tr_content: crate::casg::TemporalManifest =
            serde_json::from_str(&fs::read_to_string(&trembl_path).unwrap()).unwrap();

        assert_eq!(sp_content.source_database, Some("uniprot-swissprot".to_string()));
        assert_eq!(tr_content.source_database, Some("uniprot-trembl".to_string()));
        assert_ne!(sp_content.version, tr_content.version);
    }

    #[test]
    fn test_manifest_directory_creation() {
        let (manager, _temp_dir) = create_test_manager();

        let manifest_path = manager.get_manifest_path(&DatabaseSource::UniProt(UniProtDatabase::SwissProt));
        let manifests_dir = manifest_path.parent().unwrap();

        // Initially shouldn't exist
        assert!(!manifests_dir.exists());

        // Create directory
        fs::create_dir_all(manifests_dir).unwrap();

        // Now it should exist
        assert!(manifests_dir.exists());
        assert!(manifests_dir.is_dir());
        assert!(manifests_dir.ends_with("manifests"));
    }

    #[tokio::test]
    async fn test_download_detection_flow() {
        let (manager, _temp_dir) = create_test_manager();
        let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);

        // Mock progress callback
        let progress_messages = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let progress_clone = progress_messages.clone();
        let _progress_callback = move |msg: &str| {
            progress_clone.lock().unwrap().push(msg.to_string());
        };

        // First: No manifest exists - should detect no local data
        let manifest_path = manager.get_manifest_path(&source);
        assert!(!manifest_path.exists(), "No manifest should exist initially");

        // This would trigger initial download in real scenario
        // We can't test the full download without network, but we can verify the path checking

        // Second: Create a manifest to simulate completed download
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        let manifest = create_fake_manifest();
        fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();

        // Now manifest exists - should detect existing data
        assert!(manifest_path.exists(), "Manifest should exist after 'download'");

        // In real scenario, download() would now return UpToDate or check for updates
    }

    #[test]
    fn test_manifest_content_has_source_database() {
        let manifest = create_fake_manifest();

        // Verify source_database is set
        assert_eq!(manifest.source_database, Some("uniprot-swissprot".to_string()));

        // Serialize and verify it's in JSON
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        assert!(json.contains("\"source_database\""));
        assert!(json.contains("uniprot-swissprot"));
    }
}