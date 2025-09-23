use chrono::Utc;
use talaria_utils::database_ref::parse_database_reference;
/// Comprehensive tests for the versioning system
///
/// Tests timestamp-based storage, alias management, version resolution,
/// and integration with database commands
use talaria_utils::version_detector::{DatabaseVersion, VersionDetector, VersionManager};
use tempfile::TempDir;

/// Helper to create a test version with timestamp
fn create_test_version(source: &str, dataset: &str, upstream: Option<&str>) -> DatabaseVersion {
    let mut version = DatabaseVersion::new(source, dataset);

    if let Some(upstream_ver) = upstream {
        version.upstream_version = Some(upstream_ver.to_string());
        version.aliases.upstream.push(upstream_ver.to_string());
    }

    // Add some test metadata
    version
        .metadata
        .insert("sequences".to_string(), "573021".to_string());
    version
        .metadata
        .insert("size_bytes".to_string(), "404750336".to_string());

    version
}

#[test]
fn test_timestamp_format_validation() {
    // Valid timestamps
    assert!(talaria::utils::version_detector::is_timestamp_format(
        "20250915_053033"
    ));
    assert!(talaria::utils::version_detector::is_timestamp_format(
        "20241231_235959"
    ));
    assert!(talaria::utils::version_detector::is_timestamp_format(
        "19990101_000000"
    ));

    // Invalid timestamps
    assert!(!talaria::utils::version_detector::is_timestamp_format(
        "2024_04"
    )); // UniProt format
    assert!(!talaria::utils::version_detector::is_timestamp_format(
        "2024-09-15"
    )); // Date format
    assert!(!talaria::utils::version_detector::is_timestamp_format(
        "latest"
    )); // Alias
    assert!(!talaria::utils::version_detector::is_timestamp_format(
        "20250915"
    )); // No time part
    assert!(!talaria::utils::version_detector::is_timestamp_format(
        "20250915_05303"
    )); // Wrong time length
}

#[test]
fn test_version_alias_categories() {
    let mut version = create_test_version("uniprot", "swissprot", Some("2024_04"));

    // System aliases
    version.add_system_alias("current".to_string());
    version.add_system_alias("stable".to_string());
    assert!(version.aliases.system.contains(&"current".to_string()));
    assert!(version.aliases.system.contains(&"stable".to_string()));

    // Custom aliases
    version.add_custom_alias("paper-2024".to_string());
    version.add_custom_alias("production-v1".to_string());
    assert!(version.aliases.custom.contains(&"paper-2024".to_string()));
    assert!(version
        .aliases
        .custom
        .contains(&"production-v1".to_string()));

    // Upstream aliases are already added
    assert!(version.aliases.upstream.contains(&"2024_04".to_string()));

    // Test matching
    assert!(version.matches("current"));
    assert!(version.matches("stable"));
    assert!(version.matches("paper-2024"));
    assert!(version.matches("2024_04"));
    assert!(version.matches(&version.timestamp));
    assert!(!version.matches("random"));
}

#[test]
fn test_custom_alias_removal() {
    let mut version = create_test_version("ncbi", "nr", None);

    // Add and remove custom aliases
    version.add_custom_alias("test-alias".to_string());
    assert!(version.aliases.custom.contains(&"test-alias".to_string()));

    let removed = version.remove_custom_alias("test-alias");
    assert!(removed);
    assert!(!version.aliases.custom.contains(&"test-alias".to_string()));

    // Can't remove non-existent alias
    let removed = version.remove_custom_alias("non-existent");
    assert!(!removed);
}

#[test]
fn test_version_resolution() {
    let temp_dir = TempDir::new().unwrap();
    let manager = VersionManager::new(temp_dir.path());

    // Create test version structure
    let versions_dir = temp_dir
        .path()
        .join("versions")
        .join("uniprot")
        .join("swissprot");
    std::fs::create_dir_all(&versions_dir).unwrap();

    // Create timestamp directory
    let timestamp = "20250915_053033";
    let version_dir = versions_dir.join(timestamp);
    std::fs::create_dir_all(&version_dir).unwrap();

    // Create version.json
    let version = create_test_version("uniprot", "swissprot", Some("2024_04"));
    let version_json = serde_json::to_string_pretty(&version).unwrap();
    std::fs::write(version_dir.join("version.json"), version_json).unwrap();

    // Create symlinks (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(timestamp, versions_dir.join("current")).unwrap();
        symlink(timestamp, versions_dir.join("2024_04")).unwrap();
        symlink(timestamp, versions_dir.join("paper-2024")).unwrap();
    }

    // Test resolution
    #[cfg(unix)]
    {
        // Should resolve all references to the same timestamp
        let resolved = manager
            .resolve_version("uniprot", "swissprot", "current")
            .unwrap();
        assert_eq!(resolved, timestamp);

        let resolved = manager
            .resolve_version("uniprot", "swissprot", "2024_04")
            .unwrap();
        assert_eq!(resolved, timestamp);

        let resolved = manager
            .resolve_version("uniprot", "swissprot", "paper-2024")
            .unwrap();
        assert_eq!(resolved, timestamp);

        let resolved = manager
            .resolve_version("uniprot", "swissprot", timestamp)
            .unwrap();
        assert_eq!(resolved, timestamp);
    }
}

#[test]
fn test_database_reference_with_versions() {
    // Test parsing with version and profile
    let ref1 = parse_database_reference("uniprot/swissprot@2024_04:50-percent").unwrap();
    assert_eq!(ref1.source, "uniprot");
    assert_eq!(ref1.dataset, "swissprot");
    assert_eq!(ref1.version, Some("2024_04".to_string()));
    assert_eq!(ref1.profile, Some("50-percent".to_string()));

    // Test with timestamp version
    let ref2 = parse_database_reference("ncbi/nr@20250915_053033").unwrap();
    assert_eq!(ref2.source, "ncbi");
    assert_eq!(ref2.dataset, "nr");
    assert_eq!(ref2.version, Some("20250915_053033".to_string()));
    assert_eq!(ref2.profile, None);

    // Test with alias
    let ref3 = parse_database_reference("custom/mydb@stable:minimal").unwrap();
    assert_eq!(ref3.source, "custom");
    assert_eq!(ref3.dataset, "mydb");
    assert_eq!(ref3.version, Some("stable".to_string()));
    assert_eq!(ref3.profile, Some("minimal".to_string()));
}

#[test]
fn test_version_listing_with_symlinks() {
    let temp_dir = TempDir::new().unwrap();
    let manager = VersionManager::new(temp_dir.path());

    // Create multiple versions
    let versions_dir = temp_dir
        .path()
        .join("versions")
        .join("uniprot")
        .join("swissprot");
    std::fs::create_dir_all(&versions_dir).unwrap();

    // Version 1 (older)
    let timestamp1 = "20250914_120000";
    let version_dir1 = versions_dir.join(timestamp1);
    std::fs::create_dir_all(&version_dir1).unwrap();

    let mut version1 = create_test_version("uniprot", "swissprot", Some("2024_03"));
    version1.timestamp = timestamp1.to_string();
    version1.created_at = Utc::now() - chrono::Duration::days(1);
    let version1_json = serde_json::to_string_pretty(&version1).unwrap();
    std::fs::write(version_dir1.join("version.json"), version1_json).unwrap();

    // Version 2 (newer)
    let timestamp2 = "20250915_053033";
    let version_dir2 = versions_dir.join(timestamp2);
    std::fs::create_dir_all(&version_dir2).unwrap();

    let mut version2 = create_test_version("uniprot", "swissprot", Some("2024_04"));
    version2.timestamp = timestamp2.to_string();
    let version2_json = serde_json::to_string_pretty(&version2).unwrap();
    std::fs::write(version_dir2.join("version.json"), version2_json).unwrap();

    // Create symlinks
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(timestamp2, versions_dir.join("current")).unwrap();
        symlink(timestamp2, versions_dir.join("latest")).unwrap();
        symlink(timestamp1, versions_dir.join("stable")).unwrap();
    }

    // List versions
    let versions = manager.list_versions("uniprot", "swissprot").unwrap();

    assert_eq!(versions.len(), 2);

    // Check that newer version is first (sorted by date)
    assert_eq!(versions[0].timestamp, timestamp2);
    assert_eq!(versions[1].timestamp, timestamp1);

    // Check system aliases are updated based on symlinks
    #[cfg(unix)]
    {
        assert!(versions[0].aliases.system.contains(&"current".to_string()));
        assert!(versions[0].aliases.system.contains(&"latest".to_string()));
        assert!(versions[1].aliases.system.contains(&"stable".to_string()));
    }
}

#[test]
fn test_protected_alias_handling() {
    // Test that protected aliases cannot be manually created
    let temp_dir = TempDir::new().unwrap();
    let manager = VersionManager::new(temp_dir.path());

    // Create test structure
    let versions_dir = temp_dir
        .path()
        .join("versions")
        .join("uniprot")
        .join("swissprot");
    std::fs::create_dir_all(&versions_dir).unwrap();

    let timestamp = "20250915_053033";
    let version_dir = versions_dir.join(timestamp);
    std::fs::create_dir_all(&version_dir).unwrap();

    #[cfg(unix)]
    {
        // Should fail to create protected aliases manually
        let result = manager.create_alias("uniprot", "swissprot", timestamp, "current");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("protected"));

        // Should succeed for custom aliases
        let result = manager.create_alias("uniprot", "swissprot", timestamp, "my-custom-alias");
        assert!(result.is_ok());
    }
}

#[test]
fn test_upstream_version_detection() {
    let detector = VersionDetector::new();

    // Test UniProt format
    let uniprot_content = b"# Release: 2024_04\n>sp|P12345|PROT_HUMAN Some protein\nMASEQUENCE";
    let version = detector
        .detect_version("uniprot", "swissprot", uniprot_content)
        .unwrap();
    assert_eq!(version.upstream_version, Some("2024_04".to_string()));
    assert!(version.aliases.upstream.contains(&"2024_04".to_string()));

    // Test without detectable version
    let generic_content = b">seq1\nATGC\n>seq2\nGCTA";
    let version = detector
        .detect_version("custom", "mydb", generic_content)
        .unwrap();
    assert_eq!(version.upstream_version, None);
    assert!(version.aliases.upstream.is_empty());
}

#[test]
fn test_version_display_names() {
    let mut version = create_test_version("uniprot", "swissprot", Some("2024_04"));

    // Should prefer upstream version for display
    assert_eq!(version.display_name(), "2024_04");

    // Without upstream, should use timestamp
    version.upstream_version = None;
    assert_eq!(version.display_name(), &version.timestamp);

    // All aliases should include everything
    version.upstream_version = Some("2024_04".to_string());
    version.add_custom_alias("test".to_string());
    let all = version.all_aliases();
    assert!(all.contains(&"latest".to_string())); // Default system alias
    assert!(all.contains(&"2024_04".to_string())); // Upstream alias
    assert!(all.contains(&"test".to_string())); // Custom alias
}

#[test]
fn test_edge_cases() {
    // Test empty database listing
    let temp_dir = TempDir::new().unwrap();
    let manager = VersionManager::new(temp_dir.path());

    let versions = manager.list_versions("nonexistent", "database").unwrap();
    assert!(versions.is_empty());

    // Test invalid version resolution
    let result = manager.resolve_version("uniprot", "swissprot", "nonexistent");
    assert!(result.is_err());

    // Test version matching with special characters
    let mut version = create_test_version("ncbi", "nr", None);
    version.add_custom_alias("test-with-dashes".to_string());
    version.add_custom_alias("test_with_underscores".to_string());
    assert!(version.matches("test-with-dashes"));
    assert!(version.matches("test_with_underscores"));
}

#[cfg(unix)]
#[test]
fn test_concurrent_version_operations() {
    use std::sync::Arc;
    use std::thread;

    let temp_dir = Arc::new(TempDir::new().unwrap());
    let manager = Arc::new(VersionManager::new(temp_dir.path()));

    // Create initial structure
    let versions_dir = temp_dir.path().join("versions").join("test").join("db");
    std::fs::create_dir_all(&versions_dir).unwrap();

    // Create multiple versions
    let timestamps: Vec<String> = (0..5)
        .map(|i| format!("2025091{}_0{}0000", 5 + i, i))
        .collect();

    for timestamp in &timestamps {
        let version_dir = versions_dir.join(timestamp);
        std::fs::create_dir_all(&version_dir).unwrap();

        let version = create_test_version("test", "db", None);
        let version_json = serde_json::to_string_pretty(&version).unwrap();
        std::fs::write(version_dir.join("version.json"), version_json).unwrap();
    }

    // Concurrent operations
    let handles: Vec<_> = timestamps
        .iter()
        .enumerate()
        .map(|(i, timestamp)| {
            let manager_clone = manager.clone();
            let timestamp_clone = timestamp.clone();

            thread::spawn(move || {
                // Each thread creates a different alias
                let alias = format!("thread-{}", i);
                manager_clone
                    .create_alias("test", "db", &timestamp_clone, &alias)
                    .unwrap();
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all aliases were created
    for (i, timestamp) in timestamps.iter().enumerate() {
        let alias_path = versions_dir.join(format!("thread-{}", i));
        assert!(alias_path.exists());

        let target = std::fs::read_link(&alias_path).unwrap();
        assert_eq!(target.file_name().unwrap().to_str().unwrap(), timestamp);
    }
}

#[test]
fn test_version_metadata_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let _manager = VersionManager::new(temp_dir.path());

    // Create and save version
    let versions_dir = temp_dir
        .path()
        .join("versions")
        .join("uniprot")
        .join("trembl");
    std::fs::create_dir_all(&versions_dir).unwrap();

    let timestamp = "20250916_100000";
    let version_dir = versions_dir.join(timestamp);
    std::fs::create_dir_all(&version_dir).unwrap();

    let mut version = create_test_version("uniprot", "trembl", Some("2024_05"));
    version.timestamp = timestamp.to_string();
    version.add_custom_alias("test-alias".to_string());
    version
        .metadata
        .insert("custom_field".to_string(), "custom_value".to_string());

    // Save
    let version_json = serde_json::to_string_pretty(&version).unwrap();
    let version_file = version_dir.join("version.json");
    std::fs::write(&version_file, version_json).unwrap();

    // Load and verify
    let loaded_json = std::fs::read_to_string(&version_file).unwrap();
    let loaded: DatabaseVersion = serde_json::from_str(&loaded_json).unwrap();

    assert_eq!(loaded.timestamp, timestamp);
    assert_eq!(loaded.upstream_version, Some("2024_05".to_string()));
    assert!(loaded.aliases.custom.contains(&"test-alias".to_string()));
    assert_eq!(
        loaded.metadata.get("custom_field"),
        Some(&"custom_value".to_string())
    );
}
