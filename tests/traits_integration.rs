/// Integration tests for the trait system
use talaria::casg::format::*;
use talaria::casg::types::ChunkMetadata;
use talaria::core::{resolver::*, version_store::*};
use talaria::storage::index::*;
use tempfile::TempDir;
use std::path::Path;

#[cfg(test)]
mod manifest_format_tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestManifest {
        version: String,
        chunks: Vec<String>,
        size: usize,
    }

    #[test]
    fn test_talaria_format_roundtrip() {
        let manifest = TestManifest {
            version: "1.0.0".to_string(),
            chunks: vec!["chunk1".to_string(), "chunk2".to_string()],
            size: 1024,
        };

        let format = TalariaFormat;

        // Serialize
        let data = serialize(&format, &manifest).unwrap();
        assert!(data.starts_with(TALARIA_MAGIC));

        // Deserialize
        let deserialized: TestManifest = deserialize(&format, &data).unwrap();
        assert_eq!(manifest, deserialized);
    }

    #[test]
    fn test_json_format_roundtrip() {
        let manifest = TestManifest {
            version: "1.0.0".to_string(),
            chunks: vec!["chunk1".to_string()],
            size: 512,
        };

        let format = JsonFormat::new();

        let data = serialize(&format, &manifest).unwrap();
        let deserialized: TestManifest = deserialize(&format, &data).unwrap();
        assert_eq!(manifest, deserialized);
    }

    #[test]
    fn test_format_detector() {
        let tal_path = Path::new("test.tal");
        let format = FormatDetector::detect(tal_path);
        assert_eq!(format.extension(), "tal");

        let json_path = Path::new("test.json");
        let format = FormatDetector::detect(json_path);
        assert_eq!(format.extension(), "json");
    }

    #[test]
    fn test_size_comparison() {
        let manifest = TestManifest {
            version: "1.0.0".to_string(),
            chunks: (0..100).map(|i| format!("chunk_{}", i)).collect(),
            size: 1024 * 1024,
        };

        let tal_size = serialize(&TalariaFormat, &manifest).unwrap().len();
        let json_size = serialize(&JsonFormat::new(), &manifest).unwrap().len();

        // TAL format should be significantly smaller
        assert!(tal_size < json_size);
        println!("TAL: {} bytes, JSON: {} bytes, Ratio: {:.2}%",
                 tal_size, json_size, (tal_size as f64 / json_size as f64) * 100.0);
    }
}

#[cfg(test)]
mod version_store_tests {
    use super::*;
    use talaria::download::{DatabaseSource, UniProtDatabase};
    

    #[tokio::test]
    async fn test_filesystem_version_store() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FilesystemVersionStore::new(temp_dir.path().to_path_buf());
        let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);

        // Create a version
        let version = store.create_version(&source).await.unwrap();
        assert!(!version.id.is_empty());
        assert_eq!(version.id.len(), 15); // Timestamp format

        // List versions
        let versions = store.list_versions(&source, ListOptions::default()).await.unwrap();
        assert_eq!(versions.len(), 1);

        // Test version exists
        assert!(store.version_exists(&source, &version.id).await);
    }

    #[tokio::test]
    async fn test_version_aliases() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = FilesystemVersionStore::new(temp_dir.path().to_path_buf());
        let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);

        // Create a version
        let version = store.create_version(&source).await.unwrap();

        // Set alias
        store.update_alias(&source, "current", &version.id).await.unwrap();

        // Resolve alias
        let resolved = store.resolve_alias(&source, "current").await.unwrap();
        assert_eq!(resolved.id, version.id);

        // List aliases
        let aliases = store.list_aliases(&source).await.unwrap();
        assert_eq!(aliases.get("current"), Some(&version.id));
    }
}

#[cfg(test)]
mod database_resolver_tests {
    use super::*;
    

    #[test]
    fn test_database_reference_parsing() {
        // Simple reference
        let ref1 = DatabaseReference::parse("uniprot/swissprot").unwrap();
        assert_eq!(ref1.source, "uniprot");
        assert_eq!(ref1.dataset, "swissprot");
        assert_eq!(ref1.version, None);
        assert_eq!(ref1.profile, None);

        // With version
        let ref2 = DatabaseReference::parse("uniprot/swissprot@2024_04").unwrap();
        assert_eq!(ref2.version, Some("2024_04".to_string()));

        // With version and profile
        let ref3 = DatabaseReference::parse("uniprot/swissprot@2024_04:50-percent").unwrap();
        assert_eq!(ref3.version, Some("2024_04".to_string()));
        assert_eq!(ref3.profile, Some("50-percent".to_string()));

        // With current version
        let ref4 = DatabaseReference::parse("ncbi/nr@current").unwrap();
        assert_eq!(ref4.version, Some("current".to_string()));
    }

    #[test]
    fn test_database_reference_to_string() {
        let reference = DatabaseReference {
            source: "uniprot".to_string(),
            dataset: "trembl".to_string(),
            version: Some("20250915_053033".to_string()),
            profile: Some("bacteria-only".to_string()),
        };

        assert_eq!(reference.to_string(), "uniprot/trembl@20250915_053033:bacteria-only");
    }

    #[test]
    fn test_standard_resolver() {
        let temp_dir = TempDir::new().unwrap();
        let resolver = StandardDatabaseResolver::new(temp_dir.path().to_path_buf());

        let reference = DatabaseReference {
            source: "uniprot".to_string(),
            dataset: "swissprot".to_string(),
            version: Some("20250915_053033".to_string()),
            profile: None,
        };

        let paths = resolver.resolve_paths(&reference).unwrap();
        assert!(paths.version_dir.ends_with("versions/uniprot/swissprot/20250915_053033"));
        assert!(paths.manifest_path.ends_with("manifest.tal"));
        assert!(paths.chunks_dir.ends_with("chunks"));
    }

    #[test]
    fn test_resolver_validation() {
        let temp_dir = TempDir::new().unwrap();
        let resolver = StandardDatabaseResolver::new(temp_dir.path().to_path_buf());

        // Valid reference
        let valid = DatabaseReference {
            source: "uniprot".to_string(),
            dataset: "swissprot".to_string(),
            version: Some("20250915_053033".to_string()),
            profile: None,
        };
        assert!(resolver.validate(&valid).is_ok());

        // Invalid source
        let invalid_source = DatabaseReference {
            source: "invalid".to_string(),
            dataset: "swissprot".to_string(),
            version: None,
            profile: None,
        };
        assert!(resolver.validate(&invalid_source).is_err());

        // Invalid dataset for source
        let invalid_dataset = DatabaseReference {
            source: "uniprot".to_string(),
            dataset: "invalid".to_string(),
            version: None,
            profile: None,
        };
        assert!(resolver.validate(&invalid_dataset).is_err());
    }

    #[test]
    fn test_resolver_suggestions() {
        let temp_dir = TempDir::new().unwrap();
        let resolver = StandardDatabaseResolver::new(temp_dir.path().to_path_buf());

        let suggestions = resolver.suggest("swiss");
        assert!(suggestions.contains(&"uniprot/swissprot".to_string()));

        let suggestions = resolver.suggest("nr");
        assert!(suggestions.contains(&"ncbi/nr".to_string()));

        let suggestions = resolver.suggest("taxonomy");
        assert!(suggestions.contains(&"ncbi/taxonomy".to_string()));
    }
}

#[cfg(test)]
mod chunk_index_tests {
    use super::*;
    use talaria::casg::types::{SHA256Hash, TaxonId};

    #[tokio::test]
    async fn test_in_memory_chunk_index() {
        let mut index = InMemoryChunkIndex::new();

        let metadata = ChunkMetadata {
            hash: SHA256Hash::from_hex("a123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef").unwrap(),
            taxon_ids: vec![TaxonId(9606)],
            sequence_count: 100,
            size: 1024 * 1024,
            compressed_size: None,
        };

        // Add chunk
        index.add_chunk(metadata.clone()).await.unwrap();

        // Verify it exists
        assert!(index.exists(&metadata.hash).await);

        // Find by taxon
        let chunks = index.find_by_taxon(TaxonId(9606)).await.unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], metadata.hash);

        // Get stats
        let stats = index.get_stats().await.unwrap();
        assert_eq!(stats.total_chunks, 1);
        assert_eq!(stats.total_size, 1024 * 1024);
    }

    #[tokio::test]
    async fn test_chunk_query() {
        let mut index = InMemoryChunkIndex::new();

        // Add various chunks
        for i in 0..10 {
            let metadata = ChunkMetadata {
                hash: SHA256Hash::from_hex(&format!("{:064x}", i)).unwrap(),
                taxon_ids: vec![TaxonId(if i < 3 { 9606 } else { 562 })],
                sequence_count: 100,
                size: (i + 1) * 1024 * 1024,
                compressed_size: if i % 2 == 0 { Some((i + 1) * 512 * 1024) } else { None },
            };
            index.add_chunk(metadata).await.unwrap();
        }

        // Query for chunks with specific criteria
        let query = ChunkQuery {
            ..Default::default()
        };
        let results = index.query(query).await.unwrap();
        assert_eq!(results.len(), 5);

        // Query for chunks with reference
        let query = ChunkQuery {
            has_reference: Some(true),
            ..Default::default()
        };
        let results = index.query(query).await.unwrap();
        assert_eq!(results.len(), 5);

        // Query by taxon
        let query = ChunkQuery {
            taxon_ids: Some(vec![TaxonId(9606)]),
            ..Default::default()
        };
        let results = index.query(query).await.unwrap();
        assert_eq!(results.len(), 3);
    }
}

#[cfg(test)]
mod integration_workflow_tests {
    use super::*;
    use talaria::download::{DatabaseSource, UniProtDatabase};

    #[tokio::test]
    async fn test_complete_versioning_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();

        // Create components
        let resolver = StandardDatabaseResolver::new(base_path.clone());
        let mut version_store = FilesystemVersionStore::new(base_path.clone());
        let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);

        // Create a new version
        let version = version_store.create_version(&source).await.unwrap();
        println!("Created version: {}", version.id);

        // Set it as current
        version_store.update_alias(&source, "current", &version.id).await.unwrap();

        // Resolve database reference
        let reference = resolver.from_source(&source);
        assert_eq!(reference.source, "uniprot");
        assert_eq!(reference.dataset, "swissprot");

        // Get paths for the database
        let mut ref_with_version = reference.clone();
        ref_with_version.version = Some(version.id.clone());
        let paths = resolver.resolve_paths(&ref_with_version).unwrap();

        // Verify paths exist
        assert!(paths.version_dir.exists());

        // List versions
        let versions = version_store.list_versions(&source, ListOptions {
            newest_first: true,
            ..Default::default()
        }).await.unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].id, version.id);

        // Resolve current alias
        let current = version_store.resolve_alias(&source, "current").await.unwrap();
        assert_eq!(current.id, version.id);
    }

    #[tokio::test]
    async fn test_manifest_format_workflow() {
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct SimpleManifest {
            version: String,
            created_at: String,
            chunks: Vec<String>,
        }

        let manifest = SimpleManifest {
            version: "1.0.0".to_string(),
            created_at: "2025-09-17T10:00:00Z".to_string(),
            chunks: (0..1000).map(|i| format!("chunk_{:04}", i)).collect(),
        };

        // Test all formats
        let formats: Vec<(Box<dyn ManifestFormat>, &str)> = vec![
            (Box::new(TalariaFormat), "tal"),
            (Box::new(JsonFormat::new()), "json"),
            (Box::new(MessagePackFormat), "msgpack"),
        ];

        for (format, ext) in formats {
            let serialized = serialize(format.as_ref(), &manifest).unwrap();
            let deserialized: SimpleManifest = deserialize(format.as_ref(), &serialized).unwrap();
            assert_eq!(manifest, deserialized);

            println!("{} format: {} bytes", ext, serialized.len());

            // Verify extension
            assert_eq!(format.extension(), ext);
        }
    }
}