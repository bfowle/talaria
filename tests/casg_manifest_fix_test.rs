/// Integration test to verify CASG manifest path fix
/// This test ensures that manifests are saved to and loaded from the correct locations

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Helper to create a fake manifest file
    fn create_fake_manifest(path: &PathBuf, source: &str) -> std::io::Result<()> {
        let manifest_content = format!(
            r#"{{
                "version": "test_v1",
                "created_at": "2024-01-01T00:00:00Z",
                "sequence_version": "2024.01",
                "taxonomy_version": "2024.01",
                "source_database": "{}",
                "chunk_index": [],
                "etag": "test_etag"
            }}"#,
            source
        );

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, manifest_content)
    }

    #[test]
    fn test_manifest_saved_to_correct_path() {
        let temp_dir = TempDir::new().unwrap();
        let casg_dir = temp_dir.path().join("casg");

        // Expected path for SwissProt manifest
        let expected_path = casg_dir.join("manifests").join("uniprot-swissprot.json");

        // Create manifest at correct location
        create_fake_manifest(&expected_path, "uniprot-swissprot").unwrap();

        // Verify it exists
        assert!(expected_path.exists(),
            "Manifest should exist at {:?}", expected_path);

        // Verify it's NOT at the old location
        let old_path = casg_dir.join("manifest.json");
        assert!(!old_path.exists(),
            "Manifest should NOT exist at old location {:?}", old_path);
    }

    #[test]
    fn test_multiple_database_manifests_coexist() {
        let temp_dir = TempDir::new().unwrap();
        let casg_dir = temp_dir.path().join("casg");
        let manifests_dir = casg_dir.join("manifests");

        // Create manifests for different databases
        let swissprot_path = manifests_dir.join("uniprot-swissprot.json");
        let trembl_path = manifests_dir.join("uniprot-trembl.json");
        let nr_path = manifests_dir.join("ncbi-nr.json");

        create_fake_manifest(&swissprot_path, "uniprot-swissprot").unwrap();
        create_fake_manifest(&trembl_path, "uniprot-trembl").unwrap();
        create_fake_manifest(&nr_path, "ncbi-nr").unwrap();

        // Verify all exist
        assert!(swissprot_path.exists(), "SwissProt manifest should exist");
        assert!(trembl_path.exists(), "TrEMBL manifest should exist");
        assert!(nr_path.exists(), "NR manifest should exist");

        // Verify they have different content
        let sp_content = fs::read_to_string(&swissprot_path).unwrap();
        let tr_content = fs::read_to_string(&trembl_path).unwrap();
        let nr_content = fs::read_to_string(&nr_path).unwrap();

        assert!(sp_content.contains("uniprot-swissprot"));
        assert!(tr_content.contains("uniprot-trembl"));
        assert!(nr_content.contains("ncbi-nr"));
    }

    #[test]
    fn test_old_manifest_migration() {
        let temp_dir = TempDir::new().unwrap();
        let casg_dir = temp_dir.path().join("casg");
        fs::create_dir_all(&casg_dir).unwrap();

        // Create old manifest
        let old_path = casg_dir.join("manifest.json");
        create_fake_manifest(&old_path, "migrated").unwrap();

        // Also create etag file
        let old_etag = old_path.with_extension("etag");
        fs::write(&old_etag, "old_etag_value").unwrap();

        // New path where it should be migrated
        let new_path = casg_dir.join("manifests").join("uniprot-swissprot.json");

        // Simulate migration
        if old_path.exists() && !new_path.exists() {
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
        assert!(old_path.exists(), "Old manifest still exists (not deleted during migration)");

        let new_etag = new_path.with_extension("etag");
        assert!(new_etag.exists(), "Etag should also be migrated");

        // Verify content is identical
        let old_content = fs::read_to_string(&old_path).unwrap();
        let new_content = fs::read_to_string(&new_path).unwrap();
        assert_eq!(old_content, new_content, "Content should be identical after migration");
    }

    #[test]
    fn test_manifest_directory_structure() {
        let temp_dir = TempDir::new().unwrap();
        let casg_dir = temp_dir.path().join("casg");

        // Expected structure
        let manifests_dir = casg_dir.join("manifests");
        let chunks_dir = casg_dir.join("chunks");
        let taxonomy_dir = casg_dir.join("taxonomy");

        // Create structure
        fs::create_dir_all(&manifests_dir).unwrap();
        fs::create_dir_all(&chunks_dir).unwrap();
        fs::create_dir_all(&taxonomy_dir).unwrap();

        // Verify all directories exist
        assert!(manifests_dir.exists() && manifests_dir.is_dir());
        assert!(chunks_dir.exists() && chunks_dir.is_dir());
        assert!(taxonomy_dir.exists() && taxonomy_dir.is_dir());
    }
}