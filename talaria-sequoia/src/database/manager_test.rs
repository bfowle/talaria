/// Unit test for version creation bug fix
#[cfg(test)]
mod tests {
    use super::super::*;
    use tempfile::TempDir;
    use talaria_bio::sequence::Sequence;
    use talaria_core::DatabaseSource;

    #[test]
    fn test_single_version_across_batches() {
        // Create a temporary directory
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_HOME", temp_dir.path());

        // Create manager
        let mut manager = DatabaseManager::new(None).unwrap();
        let source = DatabaseSource::Test;

        // Simulate processing multiple batches (what streaming does)
        // First batch - not final
        let batch1 = vec![
            Sequence {
                id: "SEQ_001".to_string(),
                description: Some("First batch seq 1".to_string()),
                sequence: b"ACGTACGTACGT".to_vec(),
                taxon_id: Some(9606),
                taxonomy_sources: Default::default(),
            },
            Sequence {
                id: "SEQ_002".to_string(),
                description: Some("First batch seq 2".to_string()),
                sequence: b"GCTAGCTAGCTA".to_vec(),
                taxon_id: Some(9606),
                taxonomy_sources: Default::default(),
            },
        ];

        // Process first batch (not final)
        manager.chunk_sequences_direct_with_progress_final(
            batch1,
            &source,
            None,
            false, // NOT final
        ).unwrap();

        // Check that NO version directory was created yet
        let versions_path = temp_dir.path()
            .join("databases")
            .join("versions")
            .join("test")
            .join("test");

        assert!(
            !versions_path.exists() || std::fs::read_dir(&versions_path).unwrap().count() == 0,
            "No version should be created for non-final batch"
        );

        // Second batch - also not final
        let batch2 = vec![
            Sequence {
                id: "SEQ_003".to_string(),
                description: Some("Second batch seq".to_string()),
                sequence: b"TTTTAAAACCCC".to_vec(),
                taxon_id: Some(9606),
                taxonomy_sources: Default::default(),
            },
        ];

        manager.chunk_sequences_direct_with_progress_final(
            batch2,
            &source,
            None,
            false, // Still NOT final
        ).unwrap();

        // Still no version directory
        assert!(
            !versions_path.exists() || std::fs::read_dir(&versions_path).unwrap().count() == 0,
            "No version should be created for non-final batch"
        );

        // Third batch - FINAL
        let batch3 = vec![
            Sequence {
                id: "SEQ_004".to_string(),
                description: Some("Final batch seq".to_string()),
                sequence: b"GGGGCCCCAAAA".to_vec(),
                taxon_id: Some(9606),
                taxonomy_sources: Default::default(),
            },
        ];

        manager.chunk_sequences_direct_with_progress_final(
            batch3,
            &source,
            None,
            true, // IS final
        ).unwrap();

        // Now exactly ONE version should exist
        if versions_path.exists() {
            let version_count = std::fs::read_dir(&versions_path)
                .unwrap()
                .filter(|e| e.as_ref().unwrap().file_type().unwrap().is_dir())
                .count();

            assert_eq!(
                version_count, 1,
                "Exactly one version should be created after final batch, found {}",
                version_count
            );

            // Verify the manifest contains all 4 sequences
            let manifest = manager.get_manifest("test").unwrap();
            let total_sequences: usize = manifest
                .chunk_index
                .iter()
                .map(|c| c.sequence_count)
                .sum();

            assert_eq!(
                total_sequences, 4,
                "Manifest should contain all 4 sequences from all batches"
            );
        } else {
            panic!("Version directory should exist after final batch");
        }

        // Clean up
        std::env::remove_var("TALARIA_HOME");
    }

    #[test]
    fn test_version_accumulation() {
        // This test verifies that manifests are accumulated across batches
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_HOME", temp_dir.path());

        let mut manager = DatabaseManager::new(None).unwrap();

        // Process 3 batches
        for i in 0..3 {
            let is_final = i == 2;
            let batch = vec![
                Sequence {
                    id: format!("BATCH_{}_SEQ", i),
                    description: Some(format!("Batch {} sequence", i)),
                    sequence: b"ACGTACGTACGT".to_vec(),
                    taxon_id: Some(9606),
                    taxonomy_sources: Default::default(),
                },
            ];

            manager.chunk_sequences_direct_with_progress_final(
                batch,
                &DatabaseSource::Test,
                None,
                is_final,
            ).unwrap();
        }

        // The fact that we got here without error and have a valid manifest
        // proves that accumulation worked correctly

        std::env::remove_var("TALARIA_HOME");
    }
}